use maple_llm::request::{LlmRequest, Message};
use maple_llm::router::LlmAdapter;
use maple_llm::token_counter::{SimpleTokenCounter, TokenCounter};
use std::collections::HashMap;
use std::sync::Arc;

/// Context Compressor — inspired by hermes-agent's head/tail protection + tool output pruning
///
/// Key design:
/// - Token-based budget (not message count)
/// - Head protection: system prompt + first N messages always preserved
/// - Tail protection: recent messages based on token budget, with last user message guaranteed
/// - Tool output 3-pass pruning: deduplicate → summarize → truncate
/// - Structured summary template for compressed context
/// - Iterative summary updates (previous_summary + new turns)
/// - LLM-driven summarization with fallback to heuristic

/// Context compressor configuration
pub struct ContextCompressorConfig {
    /// Maximum context length in tokens
    pub max_context_length: usize,
    /// Threshold to trigger compression (percentage of max_context_length)
    pub threshold_percentage: f32,
    /// Token budget for tail protection (percentage of threshold)
    pub tail_percentage: f32,
    /// Minimum tokens to preserve in tail
    pub min_tail_tokens: usize,
    /// Maximum tokens for summary
    pub max_summary_tokens: usize,
    /// Number of head messages to always preserve (beyond system prompt)
    pub head_message_count: usize,
}

impl Default for ContextCompressorConfig {
    fn default() -> Self {
        Self {
            max_context_length: 128_000,
            threshold_percentage: 0.50,
            tail_percentage: 0.20,
            min_tail_tokens: 2_000,
            max_summary_tokens: 12_000,
            head_message_count: 3,
        }
    }
}

/// Context compressor — manages conversation context within token budget
pub struct ContextCompressor {
    config: ContextCompressorConfig,
    token_counter: Box<dyn TokenCounter>,
    previous_summary: Option<String>,
    llm_adapter: Option<Arc<dyn LlmAdapter>>,
    summary_model: Option<String>,
}

impl ContextCompressor {
    pub fn new(config: ContextCompressorConfig) -> Self {
        Self {
            config,
            token_counter: Box::new(SimpleTokenCounter::new()),
            previous_summary: None,
            llm_adapter: None,
            summary_model: None,
        }
    }

    pub fn with_token_counter(mut self, counter: Box<dyn TokenCounter>) -> Self {
        self.token_counter = counter;
        self
    }

    pub fn with_llm_adapter(mut self, adapter: Arc<dyn LlmAdapter>) -> Self {
        self.llm_adapter = Some(adapter);
        self
    }

    pub fn with_summary_model(mut self, model: String) -> Self {
        self.summary_model = Some(model);
        self
    }

    /// Get the token threshold that triggers compression
    pub fn threshold_tokens(&self) -> usize {
        (self.config.max_context_length as f32 * self.config.threshold_percentage) as usize
    }

    /// Get the token budget for tail protection
    pub fn tail_token_budget(&self) -> usize {
        let threshold = self.threshold_tokens();
        let budget = (threshold as f32 * self.config.tail_percentage) as usize;
        budget.max(self.config.min_tail_tokens)
    }

    /// Check if compression is needed
    pub fn needs_compression(&self, messages: &[Message]) -> bool {
        let total_tokens = self.count_total_tokens(messages);
        total_tokens > self.threshold_tokens()
    }

    /// Compress messages to fit within token budget
    /// Returns compressed messages with system prompt preserved
    pub fn compress(&mut self, messages: &[Message]) -> Vec<Message> {
        if !self.needs_compression(messages) {
            return messages.to_vec();
        }

        // 3-pass tool output pruning before compression
        let pruned = self.prune_tool_outputs(messages);

        let mut compressed = Vec::new();

        // 1. Protect head: system prompt + first N messages
        let head_end = self.protect_head(&pruned, &mut compressed);

        // 2. Protect tail: recent messages within token budget, guaranteeing last user message
        let tail_start = self.protect_tail(&pruned, &mut compressed, head_end);

        // 3. Compress middle section (if any)
        if head_end < tail_start {
            let middle_messages = &pruned[head_end..tail_start];
            let summary = self.generate_structured_summary(middle_messages);

            // Store for iterative updates
            self.previous_summary = Some(summary.clone());

            // Insert summary between head and tail
            compressed.push(Message::system(&format!(
                "[Previous context summary]\n{}",
                summary
            )));
        }

        // 4. Add tail messages
        for msg in &pruned[tail_start..] {
            compressed.push(msg.clone());
        }

        compressed
    }

    /// 3-pass tool output pruning (inspired by hermes-agent)
    /// Pass 1: Deduplicate by content hash (keep latest)
    /// Pass 2: Summarize old tool outputs to one-liners
    /// Pass 3: Truncate large JSON tool arguments
    fn prune_tool_outputs(&self, messages: &[Message]) -> Vec<Message> {
        let mut pruned: Vec<Message> = Vec::with_capacity(messages.len());
        let mut seen_outputs: HashMap<String, usize> = HashMap::new(); // content hash -> index in pruned

        for msg in messages {
            if msg.role == "tool" {
                let content_hash = format!("{:x}", md5_simple(&msg.content));

                if let Some(&prev_idx) = seen_outputs.get(&content_hash) {
                    // Pass 1: Duplicate found — replace previous with summary one-liner
                    if let Some(prev) = pruned.get_mut(prev_idx) {
                        let tool_id = prev.tool_call_id.as_deref().unwrap_or("unknown");
                        let line_count = prev.content.lines().count();
                        prev.content =
                            format!("[tool {}] ran -> {} lines output", tool_id, line_count);
                        prev.tool_calls = None;
                    }
                    // Keep the latest copy as-is
                    seen_outputs.insert(content_hash, pruned.len());
                    pruned.push(msg.clone());
                } else {
                    // Pass 2: Summarize old tool outputs (if too many messages back)
                    let age = messages.len().saturating_sub(pruned.len());
                    if age > 6 && msg.content.len() > 200 {
                        let tool_id = msg.tool_call_id.as_deref().unwrap_or("unknown");
                        let line_count = msg.content.lines().count();
                        let mut summarized = msg.clone();
                        summarized.content = format!(
                            "[tool {}] {} lines output (summarized)",
                            tool_id, line_count
                        );
                        summarized.tool_calls = None;
                        seen_outputs.insert(content_hash, pruned.len());
                        pruned.push(summarized);
                    } else {
                        seen_outputs.insert(content_hash, pruned.len());
                        pruned.push(msg.clone());
                    }
                }
            } else if msg.role == "assistant" {
                // Pass 3: Truncate large JSON tool call arguments
                if let Some(ref tool_calls) = msg.tool_calls {
                    let needs_truncation = tool_calls
                        .iter()
                        .any(|tc| tc.arguments.to_string().len() > 1000);
                    if needs_truncation {
                        let mut truncated = msg.clone();
                        if let Some(ref mut tcs) = truncated.tool_calls {
                            for tc in tcs.iter_mut() {
                                let arg_str = tc.arguments.to_string();
                                if arg_str.len() > 1000 {
                                    // Keep valid JSON but truncate large values
                                    tc.arguments = truncate_json_values(&tc.arguments, 500);
                                }
                            }
                        }
                        pruned.push(truncated);
                    } else {
                        pruned.push(msg.clone());
                    }
                } else {
                    pruned.push(msg.clone());
                }
            } else {
                pruned.push(msg.clone());
            }
        }

        pruned
    }

    /// Protect head messages (system prompt + first N messages)
    fn protect_head(&self, messages: &[Message], compressed: &mut Vec<Message>) -> usize {
        if messages.is_empty() {
            return 0;
        }

        // Always preserve system prompt
        if messages[0].role == "system" {
            compressed.push(messages[0].clone());
        }

        // Preserve first N non-system messages
        let start = if messages[0].role == "system" { 1 } else { 0 };
        let end = (start + self.config.head_message_count).min(messages.len());

        for msg in &messages[start..end] {
            compressed.push(msg.clone());
        }

        end
    }

    /// Protect tail messages within token budget
    /// Guarantees the most recent user message is always included
    fn protect_tail(
        &self,
        messages: &[Message],
        compressed: &mut Vec<Message>,
        head_end: usize,
    ) -> usize {
        let tail_budget = self.tail_token_budget();
        let mut tail_tokens = 0;
        let mut tail_start = messages.len();

        // Find the most recent user message index
        let last_user_idx = messages.iter().rposition(|m| m.role == "user");

        // Walk backwards from end, collecting messages within budget
        for i in (head_end..messages.len()).rev() {
            let msg_tokens = self.count_message_tokens(&messages[i]);
            if tail_tokens + msg_tokens > tail_budget {
                // Even if over budget, ensure last user message is included
                if let Some(user_idx) = last_user_idx {
                    if i <= user_idx && user_idx >= head_end {
                        tail_start = user_idx;
                        break;
                    }
                }
                break;
            }
            tail_tokens += msg_tokens;
            tail_start = i;
        }

        // Guarantee last user message is in tail
        if let Some(user_idx) = last_user_idx {
            if user_idx >= head_end && user_idx < tail_start {
                tail_start = user_idx;
            }
        }

        tail_start
    }

    /// Generate structured summary following hermes-agent's template
    fn generate_structured_summary(&self, messages: &[Message]) -> String {
        // Try LLM-driven summary first
        if let Some(ref adapter) = self.llm_adapter {
            return self.llm_summary(adapter, messages);
        }

        // Fallback to heuristic summary
        self.heuristic_summary(messages)
    }

    /// LLM-driven structured summary
    fn llm_summary(&self, adapter: &dyn LlmAdapter, messages: &[Message]) -> String {
        let conversation_text = messages
            .iter()
            .map(|m| {
                let content = if m.content.len() > 500 {
                    format!("{}...", &m.content[..500])
                } else {
                    m.content.clone()
                };
                format!("{}: {}", m.role, content)
            })
            .collect::<Vec<String>>()
            .join("\n");

        let previous_context = self.previous_summary.as_deref().unwrap_or("");

        let prompt = format!(
            r#"You are a conversation summarizer. Compress the following conversation into a structured summary.

{}
{}

STRUCTURED SUMMARY FORMAT:
## Active Task — The original user request (most critical field)
## Goal
## Constraints & Preferences
## Completed Actions — Numbered list with tool name + result
## Active State — Working directory, branch, test status
## In Progress
## Blocked
## Key Decisions
## Resolved Questions
## Pending User Asks
## Relevant Files
## Remaining Work
## Critical Context

Rules:
- Preserve ALL key decisions, facts, and outcomes
- Include tool names and their results in Completed Actions
- Keep file paths and technical details
- Be concise but complete
- If previous summary exists, merge new information into it"#,
            if previous_context.is_empty() {
                String::new()
            } else {
                format!("PREVIOUS SUMMARY:\n{}", previous_context)
            },
            format!("NEW TURNS TO INCORPORATE:\n{}", conversation_text)
        );

        let model = self
            .summary_model
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let request = LlmRequest::new(prompt, &model);

        // Try synchronous-like call (blocking in async context)
        // In production, this should be async, but for the compressor we use a blocking approach
        let rt = tokio::runtime::Handle::current();
        match rt.block_on(adapter.complete(request)) {
            Ok(response) => {
                let summary = response.text();
                if summary.len() > self.config.max_summary_tokens * 4 {
                    // Truncate if too long
                    let max_chars = self.config.max_summary_tokens * 4;
                    format!("{}...", &summary[..max_chars])
                } else {
                    summary
                }
            }
            Err(_) => self.heuristic_summary(messages),
        }
    }

    /// Heuristic summary (fallback when no LLM available)
    fn summarize_middle(&self, messages: &[Message]) -> String {
        self.heuristic_summary(messages)
    }

    fn heuristic_summary(&self, messages: &[Message]) -> String {
        let mut summary = String::new();

        // If we have a previous summary, include it for iterative updates
        if let Some(prev) = &self.previous_summary {
            summary.push_str(&format!("{}\n\n", prev));
        }

        // Extract key information from messages
        let mut tool_outputs: HashMap<String, usize> = HashMap::new();
        let mut key_points: Vec<String> = Vec::new();
        let mut completed_actions: Vec<String> = Vec::new();
        let mut relevant_files: Vec<String> = Vec::new();

        for msg in messages {
            match msg.role.as_str() {
                "user" => {
                    if msg.content.len() > 50 {
                        key_points.push(format!("User request: {}...", &msg.content[..50]));
                    } else {
                        key_points.push(format!("User request: {}", msg.content));
                    }
                }
                "assistant" => {
                    if !msg.content.is_empty() {
                        // Extract file paths mentioned
                        for word in msg.content.split_whitespace() {
                            if word.contains('/') || word.contains('\\') {
                                if word.len() > 3 && word.len() < 200 {
                                    relevant_files.push(word.to_string());
                                }
                            }
                        }
                        if msg.content.len() > 100 {
                            key_points.push(format!("Assistant: {}...", &msg.content[..100]));
                        }
                    }
                }
                "tool" => {
                    let tool_id = msg.tool_call_id.clone().unwrap_or_default();
                    *tool_outputs.entry(tool_id.clone()).or_insert(0) += 1;
                    let line_count = msg.content.lines().count();
                    completed_actions
                        .push(format!("[tool {}] {} lines output", tool_id, line_count));
                }
                _ => {}
            }
        }

        // Build structured summary
        if !key_points.is_empty() {
            summary.push_str("## Key Conversation Points\n");
            for point in key_points.iter().take(10) {
                summary.push_str(&format!("- {}\n", point));
            }
        }

        if !completed_actions.is_empty() {
            summary.push_str("\n## Completed Actions\n");
            for (i, action) in completed_actions.iter().take(15).enumerate() {
                summary.push_str(&format!("{}. {}\n", i + 1, action));
            }
        }

        // Deduplicate tool outputs
        let duplicate_tools: Vec<String> = tool_outputs
            .iter()
            .filter(|(_, count)| **count > 1)
            .map(|(id, _)| id.clone())
            .collect();

        if !duplicate_tools.is_empty() {
            summary.push_str(&format!(
                "\n## Note: {} tool calls had duplicate outputs\n",
                duplicate_tools.len()
            ));
        }

        // Deduplicate and add relevant files
        relevant_files.sort();
        relevant_files.dedup();
        if !relevant_files.is_empty() {
            summary.push_str("\n## Relevant Files\n");
            for file in relevant_files.iter().take(10) {
                summary.push_str(&format!("- {}\n", file));
            }
        }

        // Truncate if too long
        let max_chars = self.config.max_summary_tokens * 4; // Approximate
        if summary.len() > max_chars {
            summary.truncate(max_chars);
            summary.push_str("...");
        }

        summary
    }

    /// Count tokens in a single message
    fn count_message_tokens(&self, msg: &Message) -> usize {
        let mut tokens = self.token_counter.count_tokens(&msg.content);
        if let Some(tool_calls) = &msg.tool_calls {
            for tc in tool_calls {
                tokens += self.token_counter.count_tokens(&tc.to_string());
            }
        }
        tokens
    }

    /// Count total tokens in all messages
    fn count_total_tokens(&self, messages: &[Message]) -> usize {
        messages.iter().map(|m| self.count_message_tokens(m)).sum()
    }
}

/// Simple MD5-like hash for content deduplication (not cryptographic)
fn md5_simple(input: &str) -> u64 {
    let mut hash: u64 = 5381;
    for byte in input.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
}

/// Truncate large JSON values while keeping valid JSON
fn truncate_json_values(value: &serde_json::Value, max_len: usize) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => {
            if s.len() > max_len {
                serde_json::Value::String(format!("{}...[truncated]", &s[..max_len]))
            } else {
                value.clone()
            }
        }
        serde_json::Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (k, v) in map {
                new_map.insert(k.clone(), truncate_json_values(v, max_len));
            }
            serde_json::Value::Object(new_map)
        }
        serde_json::Value::Array(arr) => {
            let new_arr: Vec<serde_json::Value> = arr
                .iter()
                .map(|v| truncate_json_values(v, max_len))
                .collect();
            serde_json::Value::Array(new_arr)
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_messages() -> Vec<Message> {
        vec![
            Message::system("You are a helpful assistant."),
            Message::user("Hello, how are you?"),
            Message::assistant("I'm doing well, thank you!"),
            Message::user("What is Rust?"),
            Message::assistant("Rust is a systems programming language."),
            Message::user("Tell me more about ownership."),
            Message::assistant("Ownership is Rust's key feature for memory safety."),
        ]
    }

    #[test]
    fn test_no_compression_needed() {
        let config = ContextCompressorConfig {
            max_context_length: 128_000,
            threshold_percentage: 0.50,
            ..Default::default()
        };
        let mut compressor = ContextCompressor::new(config);
        let messages = create_test_messages();

        assert!(!compressor.needs_compression(&messages));
        let compressed = compressor.compress(&messages);
        assert_eq!(compressed.len(), messages.len());
    }

    #[test]
    fn test_compression_triggered() {
        let config = ContextCompressorConfig {
            max_context_length: 100, // Very small to trigger compression
            threshold_percentage: 0.50,
            tail_percentage: 0.20,
            min_tail_tokens: 10,
            head_message_count: 1,
            ..Default::default()
        };
        let mut compressor = ContextCompressor::new(config);
        let messages = create_test_messages();

        assert!(compressor.needs_compression(&messages));
        let compressed = compressor.compress(&messages);

        // Should have: system prompt + head message + summary + tail messages
        assert!(compressed.len() < messages.len());
        assert_eq!(compressed[0].role, "system");
    }

    #[test]
    fn test_head_protection() {
        let config = ContextCompressorConfig {
            head_message_count: 2,
            ..Default::default()
        };
        let compressor = ContextCompressor::new(config);
        let messages = create_test_messages();
        let mut compressed = Vec::new();

        let head_end = compressor.protect_head(&messages, &mut compressed);

        // System prompt + 2 messages
        assert_eq!(compressed.len(), 3);
        assert_eq!(head_end, 3);
    }

    #[test]
    fn test_token_counting() {
        let counter = SimpleTokenCounter::new();
        assert_eq!(counter.count_tokens("hello world"), 2); // 11 chars / 4 = 2
        assert_eq!(counter.count_tokens(""), 0);
    }

    #[test]
    fn test_tail_guarantees_last_user_message() {
        let config = ContextCompressorConfig {
            max_context_length: 100,
            threshold_percentage: 0.50,
            tail_percentage: 0.10,
            min_tail_tokens: 5,
            head_message_count: 1,
            ..Default::default()
        };
        let mut compressor = ContextCompressor::new(config);
        let messages = create_test_messages();

        let compressed = compressor.compress(&messages);
        // The last user message should be in the compressed output
        let has_last_user = compressed
            .iter()
            .any(|m| m.role == "user" && m.content.contains("ownership"));
        assert!(has_last_user, "Last user message must be preserved in tail");
    }

    #[test]
    fn test_tool_output_deduplication() {
        let config = ContextCompressorConfig {
            max_context_length: 100,
            threshold_percentage: 0.50,
            tail_percentage: 0.20,
            min_tail_tokens: 5,
            head_message_count: 0,
            ..Default::default()
        };
        let compressor = ContextCompressor::new(config);

        let messages = vec![
            Message::system("test"),
            Message::user("run tool"),
            Message::assistant("running"),
            {
                let mut m = Message::tool_result("call_1", "same output content", false);
                m
            },
            Message::user("run again"),
            Message::assistant("running again"),
            {
                let mut m = Message::tool_result("call_1", "same output content", false);
                m
            },
        ];

        let pruned = compressor.prune_tool_outputs(&messages);
        // The first duplicate should be summarized
        let tool_msgs: Vec<&Message> = pruned.iter().filter(|m| m.role == "tool").collect();
        assert_eq!(tool_msgs.len(), 2);
        // First one should be summarized
        assert!(
            tool_msgs[0].content.contains("summarized")
                || tool_msgs[0].content.contains("lines output")
        );
    }

    #[test]
    fn test_structured_summary_format() {
        let config = ContextCompressorConfig::default();
        let compressor = ContextCompressor::new(config);

        let messages = vec![
            Message::user("Please fix the authentication bug in src/auth.rs"),
            Message::assistant("I'll look into the auth bug. Let me check the file."),
            Message::tool_result("call_1", "Found the issue: missing null check", false),
            Message::assistant("Found it. The issue is a missing null check. Fixing now."),
        ];

        let summary = compressor.heuristic_summary(&messages);
        assert!(summary.contains("## Key Conversation Points"));
        assert!(summary.contains("## Completed Actions"));
    }
}
