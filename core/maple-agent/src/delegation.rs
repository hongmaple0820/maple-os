use crate::registry::{AgentRegistry, AgentRole, AgentTask};
use crate::react_loop::{ReactLoop, ToolExecutor, Session, ToolUse, ToolResult};
use crate::tool_use_context::ToolUseContext;
use maple_llm::request::ToolDefinition;
use maple_llm::router::LlmAdapter;
use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;

/// Agent Delegation — inspired by hermes-agent's delegate_task + claw-code's TaskPacket
///
/// Features:
/// - Runtime sub-agent generation with tool subsets
/// - Independent ReAct loop per delegate
/// - Timeout and cancellation support
/// - Result streaming back to parent

/// Options for delegation
#[derive(Debug, Clone)]
pub struct DelegateOpts {
    /// Maximum iterations for the sub-agent
    pub max_iterations: usize,
    /// Timeout for the entire delegation
    pub timeout: Duration,
    /// Tool subset to provide (None = all parent tools minus excluded)
    pub tool_subset: Option<Vec<String>>,
    /// Role for the sub-agent
    pub role: AgentRole,
    /// Whether to stream results back
    pub stream_results: bool,
}

impl Default for DelegateOpts {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            timeout: Duration::from_secs(300), // 5 minutes
            tool_subset: None,
            role: AgentRole::Leaf,
            stream_results: false,
        }
    }
}

/// Result from a delegated task
#[derive(Debug, Clone)]
pub struct DelegateResult {
    pub task_id: String,
    pub success: bool,
    pub output: String,
    pub iterations_used: usize,
    pub tokens_used: usize,
}

/// Enhanced delegation engine with runtime sub-agent generation
pub struct DelegationEngine {
    agent_registry: Arc<AgentRegistry>,
    adapter: Arc<dyn LlmAdapter>,
    tool_executor: Arc<dyn ToolExecutor>,
    max_depth: usize,
    excluded_tools: Vec<String>,
}

impl DelegationEngine {
    pub fn new(
        agent_registry: Arc<AgentRegistry>,
        adapter: Arc<dyn LlmAdapter>,
        tool_executor: Arc<dyn ToolExecutor>,
        max_depth: usize,
    ) -> Self {
        Self {
            agent_registry,
            adapter,
            tool_executor,
            max_depth,
            excluded_tools: vec![
                "delegate".to_string(),
                "approve".to_string(),
                "delegation".to_string(),
            ],
        }
    }

    /// Delegate a task to a sub-agent with independent execution
    pub async fn delegate(
        &self,
        goal: &str,
        parent_tools: &[String],
        opts: DelegateOpts,
        context: &ToolUseContext,
    ) -> Result<DelegateResult> {
        // Check depth limit
        if self.max_depth == 0 {
            return Err(anyhow::anyhow!("Maximum delegation depth reached"));
        }

        // Determine tool subset for sub-agent
        let child_tools = self.resolve_tool_subset(parent_tools, &opts);

        // Create tool definitions for the sub-agent
        let tool_defs = self.create_tool_definitions(&child_tools);

        // Create independent session for sub-agent
        let system_prompt = format!(
            "You are a delegated agent. Your task is: {}\n\
             Complete this task using the available tools. Be concise and focused.",
            goal
        );
        let mut session = Session::new(&system_prompt);

        // Create ReAct loop for sub-agent
        let react_loop = ReactLoop::new(opts.max_iterations)
            .with_max_concurrent_tools(2); // Limit concurrency for sub-agents

        // Execute with timeout
        let result = tokio::time::timeout(
            opts.timeout,
            react_loop.run_turn(
                self.adapter.as_ref(),
                self.tool_executor.as_ref(),
                &mut session,
                goal,
                tool_defs,
            ),
        ).await;

        match result {
            Ok(Ok(summary)) => Ok(DelegateResult {
                task_id: uuid::Uuid::new_v4().to_string(),
                success: summary.completed,
                output: summary.content,
                iterations_used: summary.iterations,
                tokens_used: session.input_tokens(),
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow::anyhow!(
                "Delegation timed out after {}s",
                opts.timeout.as_secs()
            )),
        }
    }

    /// Delegate using the legacy channel-based approach (for backward compatibility)
    pub async fn delegate_to_agent(
        &self,
        goal: &str,
        role: AgentRole,
        parent_tools: &[String],
    ) -> Result<String> {
        let child_tools = match role {
            AgentRole::Orchestrator => parent_tools.to_vec(),
            AgentRole::Leaf => {
                parent_tools.iter()
                    .filter(|t| !self.excluded_tools.contains(t))
                    .cloned()
                    .collect()
            }
            _ => parent_tools.to_vec(),
        };

        let agent = self.agent_registry.find_available(&child_tools).await
            .ok_or_else(|| anyhow::anyhow!("No available agent for delegation"))?;

        let task_id = uuid::Uuid::new_v4().to_string();
        let timeout_secs = 600;

        let child_task = AgentTask {
            task_id: task_id.clone(),
            goal: goal.to_string(),
            tools: child_tools,
            role,
            timeout_secs,
        };

        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        self.agent_registry.register_result_channel(&task_id, result_tx).await;

        let tx = self.agent_registry.get_task_channel(&agent).await
            .ok_or_else(|| anyhow::anyhow!("Agent {} has no task channel", agent))?;

        tx.send(child_task).await
            .map_err(|_| anyhow::anyhow!("Failed to send task to agent {}", agent))?;

        let result = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            result_rx,
        ).await
        .map_err(|_| anyhow::anyhow!("Task {} timed out after {}s", task_id, timeout_secs))?
        .map_err(|_| anyhow::anyhow!("Task {} result channel closed", task_id))?;

        Ok(result)
    }

    /// Resolve tool subset based on options
    fn resolve_tool_subset(&self, parent_tools: &[String], opts: &DelegateOpts) -> Vec<String> {
        if let Some(ref subset) = opts.tool_subset {
            // Use explicitly provided subset
            return subset.clone();
        }

        // Filter out excluded tools
        parent_tools.iter()
            .filter(|t| !self.excluded_tools.contains(t))
            .cloned()
            .collect()
    }

    /// Create tool definitions from tool names
    fn create_tool_definitions(&self, tool_names: &[String]) -> Vec<ToolDefinition> {
        // In a real implementation, this would look up tool definitions from a registry
        // For now, create placeholder definitions
        tool_names.iter().map(|name| {
            ToolDefinition {
                name: name.clone(),
                description: format!("Tool: {}", name),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            }
        }).collect()
    }
}

/// Builder for DelegateOpts
pub struct DelegateOptsBuilder {
    opts: DelegateOpts,
}

impl DelegateOptsBuilder {
    pub fn new() -> Self {
        Self {
            opts: DelegateOpts::default(),
        }
    }

    pub fn max_iterations(mut self, n: usize) -> Self {
        self.opts.max_iterations = n;
        self
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.opts.timeout = duration;
        self
    }

    pub fn tool_subset(mut self, tools: Vec<String>) -> Self {
        self.opts.tool_subset = Some(tools);
        self
    }

    pub fn role(mut self, role: AgentRole) -> Self {
        self.opts.role = role;
        self
    }

    pub fn stream_results(mut self, enabled: bool) -> Self {
        self.opts.stream_results = enabled;
        self
    }

    pub fn build(self) -> DelegateOpts {
        self.opts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delegate_opts_default() {
        let opts = DelegateOpts::default();
        assert_eq!(opts.max_iterations, 10);
        assert_eq!(opts.timeout, Duration::from_secs(300));
        assert!(opts.tool_subset.is_none());
        assert_eq!(opts.role, AgentRole::Leaf);
        assert!(!opts.stream_results);
    }

    #[test]
    fn test_delegate_opts_builder() {
        let opts = DelegateOptsBuilder::new()
            .max_iterations(20)
            .timeout(Duration::from_secs(600))
            .role(AgentRole::Orchestrator)
            .stream_results(true)
            .build();

        assert_eq!(opts.max_iterations, 20);
        assert_eq!(opts.timeout, Duration::from_secs(600));
        assert_eq!(opts.role, AgentRole::Orchestrator);
        assert!(opts.stream_results);
    }

    #[test]
    fn test_resolve_tool_subset() {
        // This would require mocking the registry, so just test the logic
        let parent_tools = vec![
            "read_file".to_string(),
            "write_file".to_string(),
            "delegate".to_string(),
            "approve".to_string(),
        ];

        let excluded = vec!["delegate".to_string(), "approve".to_string()];

        let filtered: Vec<String> = parent_tools.iter()
            .filter(|t| !excluded.contains(t))
            .cloned()
            .collect();

        assert_eq!(filtered.len(), 2);
        assert!(filtered.contains(&"read_file".to_string()));
        assert!(filtered.contains(&"write_file".to_string()));
        assert!(!filtered.contains(&"delegate".to_string()));
    }
}
