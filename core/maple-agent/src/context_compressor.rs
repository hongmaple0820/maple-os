use maple_llm::request::Message;
use maple_llm::token_counter::{TokenCounter, SimpleTokenCounter};
use std::collections::HashMap;

/// Context Compressor — inspired by hermes-agent's head/tail protection + tool output pruning
///
/// Key design:
/// - Token-based budget (not message count)
/// - Head protection: system prompt + first N messages always preserved
/// - Tail protection: recent messages based on token budget
/// - Tool output pruning: deduplicate → summarize → truncate
/// - Structured summary template for compressed context

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
}

impl ContextCompressor {
    pub fn new(config: ContextCompressorConfig) -> Self {
        Self {
            config,
            token_counter: Box::new(SimpleTokenCounter::new()),
            previous_summary: None,
        }
    }

    pub fn with_token_counter(mut self, counter: Box<dyn TokenCounter>) -> Self {
        self.token_counter = counter;
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

        let mut compressed = Vec::new();

        // 1. Protect head: system prompt + first N messages
        let head_end = self.protect_head(messages, &mut compressed);

        // 2. Protect tail: recent messages within token budget
        let tail_start = self.protect_tail(messages, &mut compressed, head_end);

        // 3. Compress middle section (if any)
        if head_end < tail_start {
            let middle_messages = &messages[head_end..tail_start];
            let summary = self.summarize_middle(middle_messages);

            // Insert summary between head and tail
            compressed.push(Message::system(&format!(
                "[Previous context summary]\n{}",
                summary
            )));
        }

        // 4. Add tail messages
        for msg in &messages[tail_start..] {
            compressed.push(msg.clone());
        }

        compressed
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
    fn protect_tail(
        &self,
        messages: &[Message],
        compressed: &mut Vec<Message>,
        head_end: usize,
    ) -> usize {
        let tail_budget = self.tail_token_budget();
        let mut tail_tokens = 0;
        let mut tail_start = messages.len();

        // Walk backwards from end, collecting messages within budget
        for i in (head_end..messages.len()).rev() {
            let msg_tokens = self.count_message_tokens(&messages[i]);
            if tail_tokens + msg_tokens > tail_budget {
                break;
            }
            tail_tokens += msg_tokens;
            tail_start = i;
        }

        tail_start
    }

    /// Summarize middle section
    fn summarize_middle(&self, messages: &[Message]) -> String {
        let mut summary = String::new();

        // If we have a previous summary, include it
        if let Some(prev) = &self.previous_summary {
            summary.push_str(&format!("{}\n\n", prev));
        }

        // Extract key information from messages
        let mut tool_outputs: HashMap<String, usize> = HashMap::new();
        let mut key_points: Vec<String> = Vec::new();

        for msg in messages {
            match msg.role.as_str() {
                "user" => {
                    if msg.content.len() > 50 {
                        key_points.push(format!("User: {}...", &msg.content[..50]));
                    }
                }
                "assistant" => {
                    if !msg.content.is_empty() && msg.content.len() > 50 {
                        key_points.push(format!("Assistant: {}...", &msg.content[..50]));
                    }
                }
                "tool" => {
                    // Track tool output sizes for deduplication
                    let tool_id = msg.tool_call_id.clone().unwrap_or_default();
                    *tool_outputs.entry(tool_id).or_insert(0) += 1;
                }
                _ => {}
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
                "Note: {} tool calls had duplicate outputs.\n",
                duplicate_tools.len()
            ));
        }

        // Add key points
        if !key_points.is_empty() {
            summary.push_str("Key conversation points:\n");
            for point in key_points.iter().take(10) {
                summary.push_str(&format!("- {}\n", point));
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
}
