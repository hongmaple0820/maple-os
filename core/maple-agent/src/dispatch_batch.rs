use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Semantic-Gated Dispatch Batching — intelligent tool call batching
///
/// Groups related tool calls and dispatches them as batches:
/// - Semantic similarity gating: only batch calls that operate on related context
/// - Time-window batching: collect calls within a configurable window
/// - Priority-aware: urgent calls bypass batching
/// - Deduplication: identical calls are merged

/// Dispatch priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DispatchPriority {
    Low,
    Normal,
    High,
    Critical,
}

/// Pending tool call awaiting dispatch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingCall {
    pub id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub priority: DispatchPriority,
    pub context_hint: String,
    pub created_at: i64,
}

/// Batch of related tool calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchBatch {
    pub id: String,
    pub calls: Vec<PendingCall>,
    pub batch_type: BatchType,
}

/// Type of batch
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchType {
    /// Calls operating on the same file/resource
    SameResource,
    /// Calls with similar semantic context
    SemanticCluster,
    /// Time-window collected calls
    TimeWindow,
    /// Single urgent call (no batching)
    Urgent,
}

/// Batching configuration
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Time window to collect calls before dispatching
    pub window_duration: Duration,
    /// Maximum calls per batch
    pub max_batch_size: usize,
    /// Minimum similarity score (0.0-1.0) to batch together
    pub similarity_threshold: f64,
    /// Whether to deduplicate identical calls
    pub deduplicate: bool,
    /// Priority level that bypasses batching
    pub bypass_priority: DispatchPriority,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            window_duration: Duration::from_millis(50),
            max_batch_size: 10,
            similarity_threshold: 0.3,
            deduplicate: true,
            bypass_priority: DispatchPriority::Critical,
        }
    }
}

/// Dispatch batcher
pub struct DispatchBatcher {
    config: BatchConfig,
    pending: Vec<PendingCall>,
    last_flush: Instant,
}

impl DispatchBatcher {
    pub fn new(config: BatchConfig) -> Self {
        Self {
            config,
            pending: Vec::new(),
            last_flush: Instant::now(),
        }
    }

    /// Add a call to the pending queue
    pub fn enqueue(&mut self, call: PendingCall) -> Option<DispatchBatch> {
        // Critical priority bypasses batching
        if call.priority >= self.config.bypass_priority {
            return Some(DispatchBatch {
                id: format!("batch_{}", call.id),
                calls: vec![call],
                batch_type: BatchType::Urgent,
            });
        }

        // Deduplicate if enabled
        if self.config.deduplicate {
            if self.pending.iter().any(|existing| {
                existing.tool_name == call.tool_name && existing.arguments == call.arguments
            }) {
                return None; // Duplicate, skip
            }
        }

        self.pending.push(call);

        // Check if we should flush
        if self.should_flush() {
            return Some(self.flush());
        }

        None
    }

    /// Force flush all pending calls
    pub fn flush(&mut self) -> DispatchBatch {
        let calls = std::mem::take(&mut self.pending);
        self.last_flush = Instant::now();

        if calls.is_empty() {
            return DispatchBatch {
                id: "batch_empty".into(),
                calls: Vec::new(),
                batch_type: BatchType::TimeWindow,
            };
        }

        // Group by resource similarity
        let batches = self.group_by_semantics(calls);
        if batches.len() == 1 {
            batches.into_iter().next().unwrap()
        } else {
            // Return the largest batch, re-queue others
            let mut sorted = batches;
            sorted.sort_by(|a, b| b.calls.len().cmp(&a.calls.len()));
            let largest = sorted.remove(0);
            for batch in sorted {
                self.pending.extend(batch.calls);
            }
            largest
        }
    }

    /// Check if time window expired or batch is full
    fn should_flush(&self) -> bool {
        self.pending.len() >= self.config.max_batch_size
            || self.last_flush.elapsed() >= self.config.window_duration
    }

    /// Group calls by semantic similarity
    fn group_by_semantics(&self, calls: Vec<PendingCall>) -> Vec<DispatchBatch> {
        if calls.is_empty() {
            return Vec::new();
        }

        let mut groups: Vec<Vec<PendingCall>> = Vec::new();

        for call in calls {
            let mut placed = false;

            // Try to find a matching group
            for group in &mut groups {
                if let Some(first) = group.first() {
                    let similarity = self.text_similarity(&first.context_hint, &call.context_hint);
                    if similarity >= self.config.similarity_threshold
                        || first.tool_name == call.tool_name
                    {
                        group.push(call.clone());
                        placed = true;
                        break;
                    }
                }
            }

            if !placed {
                groups.push(vec![call]);
            }
        }

        groups
            .into_iter()
            .enumerate()
            .map(|(i, calls)| {
                let batch_type = if calls.len() == 1 {
                    BatchType::Urgent
                } else {
                    BatchType::SemanticCluster
                };
                DispatchBatch {
                    id: format!("batch_{}", i),
                    calls,
                    batch_type,
                }
            })
            .collect()
    }

    /// Simple text similarity (Jaccard on words)
    fn text_similarity(&self, a: &str, b: &str) -> f64 {
        let words_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
        let words_b: std::collections::HashSet<&str> = b.split_whitespace().collect();

        if words_a.is_empty() && words_b.is_empty() {
            return 1.0;
        }

        let intersection = words_a.intersection(&words_b).count();
        let union = words_a.union(&words_b).count();

        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }

    /// Number of pending calls
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Check if there are pending calls
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

impl Default for DispatchBatcher {
    fn default() -> Self {
        Self::new(BatchConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_call(id: &str, tool: &str, context: &str) -> PendingCall {
        PendingCall {
            id: id.into(),
            tool_name: tool.into(),
            arguments: serde_json::json!({}),
            priority: DispatchPriority::Normal,
            context_hint: context.into(),
            created_at: 0,
        }
    }

    #[test]
    fn test_single_call_no_batch() {
        let mut batcher = DispatchBatcher::default();
        let result = batcher.enqueue(make_call("c1", "read", "file.rs"));
        assert!(result.is_none()); // Not enough for batch yet
        assert_eq!(batcher.pending_count(), 1);
    }

    #[test]
    fn test_batch_full_triggers_flush() {
        let config = BatchConfig {
            max_batch_size: 3,
            window_duration: Duration::from_secs(10),
            deduplicate: false,
            ..Default::default()
        };
        let mut batcher = DispatchBatcher::new(config);

        batcher.enqueue(make_call("c1", "read", "file.rs"));
        batcher.enqueue(make_call("c2", "read", "file.rs"));
        let batch = batcher.enqueue(make_call("c3", "read", "file.rs"));

        assert!(batch.is_some());
        let batch = batch.unwrap();
        assert_eq!(batch.calls.len(), 3);
    }

    #[test]
    fn test_critical_bypasses_batching() {
        let mut batcher = DispatchBatcher::default();

        let call = PendingCall {
            id: "urgent".into(),
            tool_name: "deploy".into(),
            arguments: serde_json::json!({}),
            priority: DispatchPriority::Critical,
            context_hint: "production deploy".into(),
            created_at: 0,
        };

        let batch = batcher.enqueue(call);
        assert!(batch.is_some());
        assert_eq!(batch.unwrap().batch_type, BatchType::Urgent);
        assert_eq!(batcher.pending_count(), 0);
    }

    #[test]
    fn test_deduplication() {
        let config = BatchConfig {
            deduplicate: true,
            max_batch_size: 10,
            window_duration: Duration::from_secs(10),
            ..Default::default()
        };
        let mut batcher = DispatchBatcher::new(config);

        batcher.enqueue(make_call("c1", "read", "file.rs"));
        let result = batcher.enqueue(make_call("c2", "read", "file.rs"));

        // Second call with same tool+args is deduped
        assert!(result.is_none());
        assert_eq!(batcher.pending_count(), 1);
    }

    #[test]
    fn test_no_dedup_when_disabled() {
        let config = BatchConfig {
            deduplicate: false,
            max_batch_size: 10,
            window_duration: Duration::from_secs(10),
            ..Default::default()
        };
        let mut batcher = DispatchBatcher::new(config);

        batcher.enqueue(make_call("c1", "read", "file.rs"));
        batcher.enqueue(make_call("c2", "read", "file.rs"));

        assert_eq!(batcher.pending_count(), 2);
    }

    #[test]
    fn test_flush_empty() {
        let mut batcher = DispatchBatcher::default();
        let batch = batcher.flush();
        assert!(batch.calls.is_empty());
    }

    #[test]
    fn test_flush_pending() {
        let mut batcher = DispatchBatcher::default();
        batcher.enqueue(make_call("c1", "read", "a.rs"));
        batcher.enqueue(make_call("c2", "read", "a.rs"));

        let batch = batcher.flush();
        assert!(!batch.calls.is_empty());
        assert_eq!(batcher.pending_count(), 0);
    }

    #[test]
    fn test_text_similarity() {
        let batcher = DispatchBatcher::default();

        assert_eq!(batcher.text_similarity("hello world", "hello world"), 1.0);
        assert_eq!(batcher.text_similarity("hello world", "foo bar"), 0.0);

        let sim = batcher.text_similarity("read file.rs", "write file.rs");
        assert!(sim > 0.0 && sim < 1.0);
    }

    #[test]
    fn test_batch_type_variants() {
        assert_ne!(BatchType::Urgent, BatchType::TimeWindow);
        assert_ne!(BatchType::SameResource, BatchType::SemanticCluster);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(DispatchPriority::Critical > DispatchPriority::High);
        assert!(DispatchPriority::High > DispatchPriority::Normal);
        assert!(DispatchPriority::Normal > DispatchPriority::Low);
    }
}
