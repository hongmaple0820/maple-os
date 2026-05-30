use maple_llm::request::Message;
use maple_llm::token_counter::{SimpleTokenCounter, TokenCounter};
use std::collections::HashMap;
use std::ops::Range;

/// Trident Compaction — 3-stage context compression
///
/// Stage 1 - Supersede: Remove old tool outputs replaced by newer ones
/// Stage 2 - Collapse: Summarize consecutive tool call chains
/// Stage 3 - Cluster: Group messages by topic, summarize each cluster
///
/// Inspired by claw-code's trident compaction strategy.
///
///   Trident compaction configuration
pub struct TridentConfig {
    /// Enable Stage 1: Supersede
    pub enable_supersede: bool,
    /// Enable Stage 2: Collapse
    pub enable_collapse: bool,
    /// Enable Stage 3: Cluster
    pub enable_cluster: bool,
    /// Supersede: content similarity threshold (0.0-1.0) to detect replacement
    pub supersede_similarity_threshold: f64,
    /// Collapse: minimum consecutive tool messages to trigger collapse
    pub collapse_min_chain: usize,
    /// Collapse: max tokens for collapsed summary
    pub collapse_max_tokens: usize,
    /// Cluster: maximum number of clusters
    pub max_clusters: usize,
    /// Cluster: max tokens per cluster summary
    pub max_cluster_tokens: usize,
}

impl Default for TridentConfig {
    fn default() -> Self {
        Self {
            enable_supersede: true,
            enable_collapse: true,
            enable_cluster: true,
            supersede_similarity_threshold: 0.7,
            collapse_min_chain: 3,
            collapse_max_tokens: 500,
            max_clusters: 5,
            max_cluster_tokens: 1000,
        }
    }
}

/// Log of compaction actions for debugging
#[derive(Debug, Clone)]
pub enum CompactionAction {
    Supersede {
        removed_idx: usize,
        kept_idx: usize,
        reason: String,
    },
    Collapse {
        range: Range<usize>,
        message_count: usize,
        summary: String,
    },
    Cluster {
        indices: Vec<usize>,
        summary: String,
    },
}

/// Trident compactor
pub struct TridentCompactor {
    config: TridentConfig,
    token_counter: Box<dyn TokenCounter>,
}

impl TridentCompactor {
    pub fn new(config: TridentConfig) -> Self {
        Self {
            config,
            token_counter: Box::new(SimpleTokenCounter::new()),
        }
    }

    pub fn with_token_counter(mut self, counter: Box<dyn TokenCounter>) -> Self {
        self.token_counter = counter;
        self
    }

    /// Execute all three stages in sequence
    pub fn compact(&self, messages: &[Message]) -> (Vec<Message>, Vec<CompactionAction>) {
        let mut result = messages.to_vec();
        let mut actions = Vec::new();

        if self.config.enable_supersede {
            let (superseded, sup_actions) = self.supersede(&result);
            result = superseded;
            actions.extend(sup_actions);
        }

        if self.config.enable_collapse {
            let (collapsed, col_actions) = self.collapse(&result);
            result = collapsed;
            actions.extend(col_actions);
        }

        if self.config.enable_cluster {
            let (clustered, clu_actions) = self.cluster(&result);
            result = clustered;
            actions.extend(clu_actions);
        }

        (result, actions)
    }

    /// Stage 1: Supersede — remove old tool outputs replaced by newer ones
    ///
    /// Detection: same tool name + same target (file path extracted from arguments)
    /// keeps the latest, removes or summarizes the old one.
    pub fn supersede(&self, messages: &[Message]) -> (Vec<Message>, Vec<CompactionAction>) {
        let mut actions = Vec::new();
        // Track: (tool_name, target_file) -> index of last seen tool result
        let mut tool_targets: HashMap<(String, String), usize> = HashMap::new();
        // Indices to remove (replaced by newer)
        let mut to_remove: Vec<usize> = Vec::new();

        for (i, msg) in messages.iter().enumerate() {
            if msg.role == "tool"
                && let Some(tool_call_id) = &msg.tool_call_id
            {
                // Find the corresponding assistant message with tool_calls
                let (tool_name, target) =
                    self.extract_tool_info(messages, i, tool_call_id);

                if let Some(name) = tool_name {
                    let key = (name.clone(), target.clone());
                    if let Some(&prev_idx) = tool_targets.get(&key) {
                        // Supersede: new output replaces old
                        actions.push(CompactionAction::Supersede {
                            removed_idx: prev_idx,
                            kept_idx: i,
                            reason: format!(
                                "Tool '{}' target '{}' superseded",
                                name, target
                            ),
                        });
                        to_remove.push(prev_idx);
                    }
                    tool_targets.insert(key, i);
                }
            }
        }

        // Build result excluding superseded messages
        let result: Vec<Message> = messages
            .iter()
            .enumerate()
            .filter(|(i, _)| !to_remove.contains(i))
            .map(|(_, m)| m.clone())
            .collect();

        (result, actions)
    }

    /// Stage 2: Collapse — summarize consecutive tool call chains
    ///
    /// A "tool chain" is a sequence of: assistant(tool_calls) → tool(result) → assistant(tool_calls) → tool(result) → ...
    /// When a chain exceeds `collapse_min_chain`, collapse it into a summary message.
    pub fn collapse(&self, messages: &[Message]) -> (Vec<Message>, Vec<CompactionAction>) {
        let mut actions = Vec::new();
        let mut result = Vec::new();
        let mut i = 0;

        while i < messages.len() {
            // Detect start of a tool chain
            if self.is_tool_chain_start(messages, i) {
                let chain_start = i;
                let mut chain_end = i;

                // Find the end of the chain
                while chain_end < messages.len() {
                    if self.is_tool_message(&messages[chain_end])
                        || self.is_assistant_with_tools(&messages[chain_end])
                    {
                        chain_end += 1;
                    } else {
                        break;
                    }
                }

                let chain_len = chain_end - chain_start;
                if chain_len >= self.config.collapse_min_chain {
                    // Collapse the chain
                    let summary = self.summarize_tool_chain(&messages[chain_start..chain_end]);
                    let summary_msg = Message::system(&format!(
                        "[Collapsed tool chain: {} steps]\n{}",
                        chain_len, summary
                    ));

                    actions.push(CompactionAction::Collapse {
                        range: chain_start..chain_end,
                        message_count: chain_len,
                        summary: summary.clone(),
                    });

                    result.push(summary_msg);
                    i = chain_end;
                } else {
                    // Too short, keep as-is
                    for msg in &messages[chain_start..chain_end] {
                        result.push(msg.clone());
                    }
                    i = chain_end;
                }
            } else {
                result.push(messages[i].clone());
                i += 1;
            }
        }

        (result, actions)
    }

    /// Stage 3: Cluster — group messages by topic, summarize each cluster
    ///
    /// Clusters are detected by user message boundaries: each user message starts a new cluster.
    /// Old clusters (beyond max_clusters) are summarized.
    pub fn cluster(&self, messages: &[Message]) -> (Vec<Message>, Vec<CompactionAction>) {
        let clusters = self.detect_clusters(messages);

        if clusters.len() <= self.config.max_clusters {
            // No clustering needed
            return (messages.to_vec(), Vec::new());
        }

        let mut actions = Vec::new();
        let mut result = Vec::new();

        // Keep the first cluster (system + initial context) intact
        if let Some(first) = clusters.first() {
            for msg in &messages[first.clone()] {
                result.push(msg.clone());
            }
        }

        // Summarize old clusters, keep recent ones
        let keep_from = clusters.len().saturating_sub(self.config.max_clusters);
        let mut summary_parts = Vec::new();

        for (idx, cluster_range) in clusters.iter().enumerate() {
            if idx == 0 {
                continue; // Already handled
            }
            if idx < keep_from {
                // Old cluster — summarize
                let cluster_msgs = &messages[cluster_range.clone()];
                let summary = self.summarize_cluster(cluster_msgs, idx);
                summary_parts.push(summary.clone());

                actions.push(CompactionAction::Cluster {
                    indices: cluster_range.clone().collect(),
                    summary,
                });
            } else {
                // Recent cluster — keep intact
                for msg in &messages[cluster_range.clone()] {
                    result.push(msg.clone());
                }
            }
        }

        // Insert cluster summary if any clusters were summarized
        if !summary_parts.is_empty() {
            let combined = format!(
                "[Previous context: {} clusters summarized]\n{}",
                summary_parts.len(),
                summary_parts.join("\n---\n")
            );
            // Insert after first cluster
            let insert_pos = if result.first().is_some_and(|m: &Message| m.role == "system") {
                1
            } else {
                0
            };
            result.insert(insert_pos, Message::system(&combined));
        }

        (result, actions)
    }

    /// Detect cluster boundaries (each user message starts a new cluster)
    fn detect_clusters(&self, messages: &[Message]) -> Vec<Range<usize>> {
        let mut clusters = Vec::new();
        let mut cluster_start = 0;

        for (i, msg) in messages.iter().enumerate() {
            if msg.role == "user" && i > 0 {
                clusters.push(cluster_start..i);
                cluster_start = i;
            }
        }

        // Last cluster
        if cluster_start < messages.len() {
            clusters.push(cluster_start..messages.len());
        }

        clusters
    }

    /// Summarize a tool chain into a compact description
    fn summarize_tool_chain(&self, messages: &[Message]) -> String {
        let mut tool_names = Vec::new();
        let mut files_involved = Vec::new();
        let mut errors = 0;

        for msg in messages {
            if msg.role == "tool" {
                let name = msg
                    .tool_call_id
                    .as_deref()
                    .unwrap_or("unknown")
                    .to_string();
                if !tool_names.contains(&name) {
                    tool_names.push(name);
                }
                if msg.content.contains("error") || msg.content.contains("Error") {
                    errors += 1;
                }
            }
            if msg.role == "assistant" {
                // Extract file paths from tool calls
                if let Some(ref tool_calls) = msg.tool_calls {
                    for tc in tool_calls {
                        if let Some(args) = tc.get("arguments")
                            && let Some(path) = args.get("path").or_else(|| args.get("file_path"))
                            && let Some(p) = path.as_str()
                            && !files_involved.contains(&p.to_string())
                        {
                            files_involved.push(p.to_string());
                        }
                    }
                }
            }
        }

        let mut summary = format!("Tools used: {}", tool_names.join(", "));
        if !files_involved.is_empty() {
            summary.push_str(&format!("\nFiles: {}", files_involved.join(", ")));
        }
        if errors > 0 {
            summary.push_str(&format!("\nErrors: {}", errors));
        }

        // Truncate to max tokens
        self.truncate_to_tokens(&summary, self.config.collapse_max_tokens)
    }

    /// Summarize a cluster of messages
    fn summarize_cluster(&self, messages: &[Message], cluster_idx: usize) -> String {
        let user_msg = messages
            .iter()
            .find(|m| m.role == "user")
            .map(|m| m.content.as_str())
            .unwrap_or("(no user message)");

        let tool_count = messages.iter().filter(|m| m.role == "tool").count();
        let assistant_msgs = messages.iter().filter(|m| m.role == "assistant").count();

        let summary = format!(
            "Cluster {}: User asked \"{}...\" ({} tool calls, {} assistant responses)",
            cluster_idx,
            &user_msg[..user_msg.len().min(100)],
            tool_count,
            assistant_msgs
        );

        self.truncate_to_tokens(&summary, self.config.max_cluster_tokens)
    }

    /// Extract tool name and target from a tool result message
    fn extract_tool_info(
        &self,
        messages: &[Message],
        tool_idx: usize,
        tool_call_id: &str,
    ) -> (Option<String>, String) {
        // Look backwards for the assistant message with matching tool_calls
        for i in (0..tool_idx).rev() {
            if messages[i].role == "assistant" {
                if let Some(ref tool_calls) = messages[i].tool_calls {
                    for tc in tool_calls {
                        if tc["id"].as_str() == Some(tool_call_id) {
                            let name = tc["name"].as_str().map(|s| s.to_string());
                            let target = self.extract_target(tc);
                            return (name, target);
                        }
                    }
                }
                break; // Only look at the immediately preceding assistant message
            }
        }
        (None, String::new())
    }

    /// Extract target file/path from tool call arguments
    fn extract_target(&self, tool_call: &serde_json::Value) -> String {
        let args = &tool_call["arguments"];
        // Try common parameter names
        for key in &["path", "file_path", "filename", "file", "command"] {
            if let Some(val) = args.get(key)
                && let Some(s) = val.as_str()
            {
                return s.to_string();
            }
        }
        // Fallback: use first string argument
        if let Some(obj) = args.as_object() {
            for val in obj.values() {
                if let Some(s) = val.as_str() {
                    return s.to_string();
                }
            }
        }
        String::new()
    }

    fn is_tool_message(&self, msg: &Message) -> bool {
        msg.role == "tool"
    }

    fn is_assistant_with_tools(&self, msg: &Message) -> bool {
        msg.role == "assistant" && msg.tool_calls.is_some()
    }

    fn is_tool_chain_start(&self, messages: &[Message], idx: usize) -> bool {
        // A tool chain starts with an assistant message that has tool_calls,
        // or a tool result message
        if idx >= messages.len() {
            return false;
        }
        self.is_assistant_with_tools(&messages[idx]) || self.is_tool_message(&messages[idx])
    }

    fn truncate_to_tokens(&self, text: &str, max_tokens: usize) -> String {
        let token_count = self.token_counter.count_tokens(text);
        if token_count <= max_tokens {
            return text.to_string();
        }
        // Rough truncation: proportionally cut
        let ratio = max_tokens as f64 / token_count as f64;
        let char_limit = (text.len() as f64 * ratio) as usize;
        if char_limit >= text.len() {
            return text.to_string();
        }
        format!("{}...", &text[..char_limit.saturating_sub(3)])
    }
}

/// Simple hash for content deduplication (DJB2 variant)
pub fn md5_simple(data: &str) -> u64 {
    let mut hash: u64 = 5381;
    for byte in data.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_call_msg(id: &str, name: &str, args: serde_json::Value) -> Message {
        let mut msg = Message::assistant("calling tool");
        msg.tool_calls = Some(vec![serde_json::json!({
            "id": id,
            "name": name,
            "arguments": args
        })]);
        msg
    }

    fn tool_result_msg(tool_call_id: &str, content: &str) -> Message {
        Message {
            role: "tool".into(),
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
            thinking: None,
        }
    }

    #[test]
    fn test_supersede_same_file() {
        let compactor = TridentCompactor::new(TridentConfig::default());

        let messages = vec![
            Message::user("edit file"),
            tool_call_msg("call_1", "write_file", serde_json::json!({"path": "src/main.rs"})),
            tool_result_msg("call_1", "written old content"),
            Message::user("actually rewrite it"),
            tool_call_msg("call_2", "write_file", serde_json::json!({"path": "src/main.rs"})),
            tool_result_msg("call_2", "written new content"),
        ];

        let (result, actions) = compactor.supersede(&messages);
        // Old write_file result should be removed
        assert_eq!(result.len(), 5);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], CompactionAction::Supersede { .. }));
    }

    #[test]
    fn test_supersede_different_files() {
        let compactor = TridentCompactor::new(TridentConfig::default());

        let messages = vec![
            tool_call_msg("call_1", "write_file", serde_json::json!({"path": "a.rs"})),
            tool_result_msg("call_1", "content a"),
            tool_call_msg("call_2", "write_file", serde_json::json!({"path": "b.rs"})),
            tool_result_msg("call_2", "content b"),
        ];

        let (result, actions) = compactor.supersede(&messages);
        // Different files, no supersede
        assert_eq!(result.len(), 4);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_collapse_long_chain() {
        let compactor = TridentCompactor::new(TridentConfig {
            collapse_min_chain: 3,
            ..Default::default()
        });

        let messages = vec![
            Message::user("do stuff"),
            tool_call_msg("c1", "read_file", serde_json::json!({"path": "a.rs"})),
            tool_result_msg("c1", "file content a"),
            tool_call_msg("c2", "read_file", serde_json::json!({"path": "b.rs"})),
            tool_result_msg("c2", "file content b"),
            tool_call_msg("c3", "read_file", serde_json::json!({"path": "c.rs"})),
            tool_result_msg("c3", "file content c"),
            Message::assistant("done"),
        ];

        let (result, actions) = compactor.collapse(&messages);
        // Chain of 6 tool messages should be collapsed
        assert!(result.len() < messages.len());
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], CompactionAction::Collapse { .. }));
    }

    #[test]
    fn test_collapse_short_chain_kept() {
        let compactor = TridentCompactor::new(TridentConfig {
            collapse_min_chain: 5,
            ..Default::default()
        });

        let messages = vec![
            tool_call_msg("c1", "read", serde_json::json!({})),
            tool_result_msg("c1", "content"),
            tool_call_msg("c2", "read", serde_json::json!({})),
            tool_result_msg("c2", "content"),
        ];

        let (result, actions) = compactor.collapse(&messages);
        // Chain too short, kept as-is
        assert_eq!(result.len(), 4);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_cluster_detection() {
        let compactor = TridentCompactor::new(TridentConfig {
            max_clusters: 2,
            ..Default::default()
        });

        let messages = vec![
            Message::system("sys"),
            Message::user("task 1"),
            Message::assistant("doing 1"),
            Message::user("task 2"),
            Message::assistant("doing 2"),
            Message::user("task 3"),
            Message::assistant("doing 3"),
        ];

        let clusters = compactor.detect_clusters(&messages);
        // system(0) + 3 user messages = 4 clusters: [0..1, 1..3, 3..5, 5..7]
        assert_eq!(clusters.len(), 4);
    }

    #[test]
    fn test_cluster_summarizes_old() {
        let compactor = TridentCompactor::new(TridentConfig {
            max_clusters: 2,
            ..Default::default()
        });

        let messages = vec![
            Message::system("sys"),
            Message::user("old task"),
            Message::assistant("old result"),
            Message::user("recent task 1"),
            Message::assistant("result 1"),
            Message::user("recent task 2"),
            Message::assistant("result 2"),
        ];

        let (result, actions) = compactor.cluster(&messages);
        // Old cluster should be summarized
        assert!(actions.len() > 0);
        // Result should have summary + recent clusters
        assert!(result.iter().any(|m| m.content.contains("summarized")));
    }

    #[test]
    fn test_full_compact() {
        let compactor = TridentCompactor::new(TridentConfig {
            collapse_min_chain: 2,
            max_clusters: 2,
            ..Default::default()
        });

        let messages = vec![
            Message::system("You are helpful"),
            Message::user("task 1"),
            tool_call_msg("c1", "read", serde_json::json!({"path": "a.rs"})),
            tool_result_msg("c1", "content a"),
            tool_call_msg("c2", "read", serde_json::json!({"path": "b.rs"})),
            tool_result_msg("c2", "content b"),
            Message::user("task 2"),
            Message::assistant("result 2"),
            Message::user("task 3"),
            Message::assistant("result 3"),
        ];

        let (result, _actions) = compactor.compact(&messages);
        // Should be compressed
        assert!(result.len() <= messages.len());
    }

    #[test]
    fn test_md5_simple() {
        let h1 = md5_simple("hello world");
        let h2 = md5_simple("hello world");
        let h3 = md5_simple("hello worle");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }
}
