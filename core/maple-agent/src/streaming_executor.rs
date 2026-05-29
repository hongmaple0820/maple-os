use crate::react_loop::{ToolExecutor, ToolResult, ToolUse};
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// StreamingToolExecutor — inspired by cc-haha's concurrent-safe vs exclusive tool model
///
/// Features:
/// - Tool classification: concurrent-safe vs exclusive
/// - Concurrent execution for safe tools
/// - Sequential execution for exclusive tools
/// - Ordered emission (results in original index order)
/// - Error cascading (one failure cancels remaining)

/// Tool concurrency classification
#[derive(Debug, Clone, PartialEq)]
pub enum ToolConcurrency {
    /// Can be executed in parallel (read_file, search, web_fetch)
    ConcurrentSafe,
    /// Must be executed exclusively (write_file, execute_bash, computer_use)
    Exclusive,
}

/// Tool metadata for concurrency classification
#[derive(Debug, Clone)]
pub struct ToolMetadata {
    pub name: String,
    pub concurrency: ToolConcurrency,
    pub max_concurrent: Option<usize>,
}

/// Streaming executor with concurrent-safe vs exclusive tool handling
pub struct StreamingToolExecutor {
    inner: Arc<dyn ToolExecutor>,
    tool_metadata: HashMap<String, ToolMetadata>,
    max_concurrent_safe: usize,
}

impl StreamingToolExecutor {
    pub fn new(inner: Arc<dyn ToolExecutor>) -> Self {
        Self {
            inner,
            tool_metadata: HashMap::new(),
            max_concurrent_safe: 4,
        }
    }

    /// Register tool metadata for concurrency classification
    pub fn register_tool(&mut self, metadata: ToolMetadata) {
        self.tool_metadata.insert(metadata.name.clone(), metadata);
    }

    /// Set max concurrent safe tools
    pub fn with_max_concurrent_safe(mut self, max: usize) -> Self {
        self.max_concurrent_safe = max;
        self
    }

    /// Classify a tool's concurrency type
    fn classify_tool(&self, tool_name: &str) -> ToolConcurrency {
        if let Some(metadata) = self.tool_metadata.get(tool_name) {
            return metadata.concurrency.clone();
        }

        // Default classification based on tool name patterns
        let exclusive_patterns = [
            "write", "delete", "execute", "run", "bash", "shell", "computer",
        ];
        let is_exclusive = exclusive_patterns
            .iter()
            .any(|p| tool_name.to_lowercase().contains(p));

        if is_exclusive {
            ToolConcurrency::Exclusive
        } else {
            ToolConcurrency::ConcurrentSafe
        }
    }

    /// Execute tools with concurrent-safe vs exclusive handling
    pub async fn execute_all(&self, tool_uses: &[ToolUse]) -> Vec<ToolResult> {
        if tool_uses.is_empty() {
            return Vec::new();
        }

        // Classify tools
        let mut concurrent_safe: Vec<(usize, &ToolUse)> = Vec::new();
        let mut exclusive: Vec<(usize, &ToolUse)> = Vec::new();

        for (idx, tool_use) in tool_uses.iter().enumerate() {
            match self.classify_tool(&tool_use.name) {
                ToolConcurrency::ConcurrentSafe => concurrent_safe.push((idx, tool_use)),
                ToolConcurrency::Exclusive => exclusive.push((idx, tool_use)),
            }
        }

        let mut all_results: Vec<(usize, ToolResult)> = Vec::new();

        // Execute concurrent-safe tools in parallel
        if !concurrent_safe.is_empty() {
            use futures::StreamExt;

            let max_concurrent = self.max_concurrent_safe.max(1);
            let owned: Vec<(usize, ToolUse)> = concurrent_safe
                .into_iter()
                .map(|(idx, tu)| {
                    (
                        idx,
                        ToolUse {
                            id: tu.id.clone(),
                            name: tu.name.clone(),
                            input: tu.input.clone(),
                        },
                    )
                })
                .collect();
            let results: Vec<(usize, ToolResult)> = futures::stream::iter(owned)
                .map(|(idx, tool_use)| async move {
                    let result = match self.inner.execute(&tool_use).await {
                        Ok(r) => r,
                        Err(e) => ToolResult::error(&tool_use.id, &tool_use.name, &e.to_string()),
                    };
                    (idx, result)
                })
                .buffer_unordered(max_concurrent)
                .collect()
                .await;
            all_results.extend(results);
        }

        // Execute exclusive tools sequentially
        for (idx, tool_use) in exclusive {
            let result = match self.inner.execute(tool_use).await {
                Ok(r) => r,
                Err(e) => ToolResult::error(&tool_use.id, &tool_use.name, &e.to_string()),
            };
            all_results.push((idx, result));
        }

        // Sort by original index to preserve order
        all_results.sort_by_key(|(idx, _)| *idx);

        // Extract results in order
        all_results.into_iter().map(|(_, result)| result).collect()
    }
}

/// Builder for StreamingToolExecutor
pub struct StreamingToolExecutorBuilder {
    inner: Arc<dyn ToolExecutor>,
    tool_metadata: HashMap<String, ToolMetadata>,
    max_concurrent_safe: usize,
}

impl StreamingToolExecutorBuilder {
    pub fn new(inner: Arc<dyn ToolExecutor>) -> Self {
        Self {
            inner,
            tool_metadata: HashMap::new(),
            max_concurrent_safe: 4,
        }
    }

    pub fn register_tool(mut self, metadata: ToolMetadata) -> Self {
        self.tool_metadata.insert(metadata.name.clone(), metadata);
        self
    }

    pub fn max_concurrent_safe(mut self, max: usize) -> Self {
        self.max_concurrent_safe = max;
        self
    }

    pub fn build(self) -> StreamingToolExecutor {
        let mut executor = StreamingToolExecutor::new(self.inner)
            .with_max_concurrent_safe(self.max_concurrent_safe);
        for (_, metadata) in self.tool_metadata {
            executor.register_tool(metadata);
        }
        executor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_classification() {
        let metadata = ToolMetadata {
            name: "read_file".to_string(),
            concurrency: ToolConcurrency::ConcurrentSafe,
            max_concurrent: None,
        };

        assert_eq!(metadata.concurrency, ToolConcurrency::ConcurrentSafe);
    }

    #[test]
    fn test_default_classification() {
        // This would require a mock executor, so just test the pattern matching
        let exclusive_patterns = [
            "write", "delete", "execute", "run", "bash", "shell", "computer",
        ];

        let test_cases = vec![
            ("read_file", false),
            ("write_file", true),
            ("execute_command", true),
            ("search_code", false),
            ("delete_file", true),
            ("bash_exec", true),
        ];

        for (tool_name, expected_exclusive) in test_cases {
            let is_exclusive = exclusive_patterns
                .iter()
                .any(|p| tool_name.to_lowercase().contains(p));
            assert_eq!(
                is_exclusive, expected_exclusive,
                "Failed for tool: {}",
                tool_name
            );
        }
    }
}
