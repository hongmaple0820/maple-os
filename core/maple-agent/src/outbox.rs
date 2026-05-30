use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Outbox Pattern — reliable task dispatch with retry and dead-letter queue
///
/// Guarantees at-least-once delivery:
/// - Tasks written to outbox before dispatch
/// - Status tracked through lifecycle (Pending → Dispatched → Completed/Failed)
/// - Automatic retry with configurable backoff
/// - Dead-letter queue for permanently failed tasks
/// - Idempotency via dedup keys

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OutboxStatus {
    /// Task created, waiting to be dispatched
    Pending,
    /// Task dispatched to worker
    Dispatched,
    /// Task completed successfully
    Completed,
    /// Task failed, will be retried
    Failed,
    /// Task moved to dead-letter queue (exhausted retries)
    DeadLettered,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub id: String,
    pub task_type: String,
    pub payload: String,
    pub status: OutboxStatus,
    pub dedup_key: Option<String>,
    pub attempt: u32,
    pub max_attempts: u32,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Outbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxConfig {
    /// Maximum retry attempts before dead-lettering
    pub max_attempts: u32,
    /// Base delay between retries (milliseconds)
    pub retry_base_delay_ms: u64,
    /// Maximum delay between retries (milliseconds)
    pub retry_max_delay_ms: u64,
    /// Maximum entries in dead-letter queue
    pub dead_letter_capacity: usize,
}

impl Default for OutboxConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            retry_base_delay_ms: 1000,
            retry_max_delay_ms: 60000,
            dead_letter_capacity: 100,
        }
    }
}

/// Reliable task outbox
#[derive(Debug)]
pub struct Outbox {
    config: OutboxConfig,
    entries: Vec<OutboxEntry>,
    dedup_keys: HashMap<String, usize>,
    dead_letter: Vec<OutboxEntry>,
}

impl Outbox {
    pub fn new(config: OutboxConfig) -> Self {
        Self {
            config,
            entries: Vec::new(),
            dedup_keys: HashMap::new(),
            dead_letter: Vec::new(),
        }
    }

    /// Add a new task to the outbox
    pub fn enqueue(
        &mut self,
        id: String,
        task_type: String,
        payload: String,
        dedup_key: Option<String>,
    ) -> Result<(), OutboxError> {
        // Check for duplicate
        if let Some(ref key) = dedup_key
            && self.dedup_keys.contains_key(key)
        {
            return Err(OutboxError::DuplicateTask(key.clone()));
        }

        let now = chrono::Utc::now().timestamp();
        let index = self.entries.len();

        if let Some(ref key) = dedup_key {
            self.dedup_keys.insert(key.clone(), index);
        }

        self.entries.push(OutboxEntry {
            id,
            task_type,
            payload,
            status: OutboxStatus::Pending,
            dedup_key,
            attempt: 0,
            max_attempts: self.config.max_attempts,
            last_error: None,
            created_at: now,
            updated_at: now,
        });

        Ok(())
    }

    /// Get all pending tasks ready for dispatch
    pub fn pending(&self) -> Vec<&OutboxEntry> {
        self.entries
            .iter()
            .filter(|e| e.status == OutboxStatus::Pending)
            .collect()
    }

    /// Mark a task as dispatched
    pub fn mark_dispatched(&mut self, id: &str) -> Result<(), OutboxError> {
        let entry = self.find_mut(id)?;
        if entry.status != OutboxStatus::Pending {
            return Err(OutboxError::InvalidStatusTransition {
                from: entry.status,
                to: OutboxStatus::Dispatched,
            });
        }
        entry.status = OutboxStatus::Dispatched;
        entry.attempt += 1;
        entry.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    /// Mark a task as completed
    pub fn mark_completed(&mut self, id: &str) -> Result<(), OutboxError> {
        let entry = self.find_mut(id)?;
        if entry.status != OutboxStatus::Dispatched {
            return Err(OutboxError::InvalidStatusTransition {
                from: entry.status,
                to: OutboxStatus::Completed,
            });
        }
        entry.status = OutboxStatus::Completed;
        entry.updated_at = chrono::Utc::now().timestamp();
        // Remove dedup key on completion
        if let Some(ref key) = entry.dedup_key.clone() {
            self.dedup_keys.remove(key);
        }
        Ok(())
    }

    /// Mark a task as failed; retry or dead-letter
    pub fn mark_failed(&mut self, id: &str, error: String) -> Result<(), OutboxError> {
        let entry = self.find_mut(id)?;
        if entry.status != OutboxStatus::Dispatched {
            return Err(OutboxError::InvalidStatusTransition {
                from: entry.status,
                to: OutboxStatus::Failed,
            });
        }

        entry.last_error = Some(error);
        entry.updated_at = chrono::Utc::now().timestamp();

        if entry.attempt >= entry.max_attempts {
            // Move to dead-letter queue
            entry.status = OutboxStatus::DeadLettered;
            let dead_entry = entry.clone();
            if self.dead_letter.len() >= self.config.dead_letter_capacity {
                self.dead_letter.remove(0); // Evict oldest
            }
            self.dead_letter.push(dead_entry);
        } else {
            // Retry: reset to pending
            entry.status = OutboxStatus::Pending;
        }

        Ok(())
    }

    /// Get retry delay for a task (exponential backoff)
    pub fn retry_delay_ms(&self, id: &str) -> Option<u64> {
        let entry = self.entries.iter().find(|e| e.id == id)?;
        if entry.status != OutboxStatus::Pending || entry.attempt == 0 {
            return None;
        }
        let delay = self.config.retry_base_delay_ms * 2u64.pow(entry.attempt - 1);
        Some(delay.min(self.config.retry_max_delay_ms))
    }

    /// Get dead-letter queue
    pub fn dead_letter(&self) -> &[OutboxEntry] {
        &self.dead_letter
    }

    /// Get counts by status
    pub fn counts(&self) -> HashMap<OutboxStatus, usize> {
        let mut counts = HashMap::new();
        for entry in &self.entries {
            *counts.entry(entry.status).or_insert(0) += 1;
        }
        counts
    }

    /// Get total number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn find_mut(&mut self, id: &str) -> Result<&mut OutboxEntry, OutboxError> {
        self.entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| OutboxError::TaskNotFound(id.to_string()))
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum OutboxError {
    #[error("task not found: {0}")]
    TaskNotFound(String),
    #[error("invalid status transition from {from:?} to {to:?}")]
    InvalidStatusTransition {
        from: OutboxStatus,
        to: OutboxStatus,
    },
    #[error("duplicate task: {0}")]
    DuplicateTask(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> OutboxConfig {
        OutboxConfig {
            max_attempts: 3,
            retry_base_delay_ms: 100,
            retry_max_delay_ms: 5000,
            dead_letter_capacity: 5,
        }
    }

    #[test]
    fn test_enqueue_and_pending() {
        let mut outbox = Outbox::new(test_config());
        outbox
            .enqueue("t1".into(), "task".into(), "{}".into(), None)
            .unwrap();

        let pending = outbox.pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "t1");
    }

    #[test]
    fn test_full_lifecycle() {
        let mut outbox = Outbox::new(test_config());
        outbox
            .enqueue("t1".into(), "task".into(), "{}".into(), None)
            .unwrap();

        outbox.mark_dispatched("t1").unwrap();
        assert_eq!(outbox.pending().len(), 0);

        outbox.mark_completed("t1").unwrap();
        assert_eq!(outbox.pending().len(), 0);
    }

    #[test]
    fn test_retry_on_failure() {
        let mut outbox = Outbox::new(test_config());
        outbox
            .enqueue("t1".into(), "task".into(), "{}".into(), None)
            .unwrap();

        // Fail once → retry
        outbox.mark_dispatched("t1").unwrap();
        outbox.mark_failed("t1", "error".into()).unwrap();
        assert_eq!(outbox.pending().len(), 1);
        assert_eq!(outbox.entries[0].attempt, 1);
    }

    #[test]
    fn test_dead_letter_after_max_attempts() {
        let mut outbox = Outbox::new(test_config());
        outbox
            .enqueue("t1".into(), "task".into(), "{}".into(), None)
            .unwrap();

        // Fail 3 times (max_attempts)
        for _ in 0..3 {
            outbox.mark_dispatched("t1").unwrap();
            outbox.mark_failed("t1", "error".into()).unwrap();
        }

        assert_eq!(outbox.dead_letter().len(), 1);
        assert_eq!(outbox.entries[0].status, OutboxStatus::DeadLettered);
    }

    #[test]
    fn test_dedup_key() {
        let mut outbox = Outbox::new(test_config());
        outbox
            .enqueue(
                "t1".into(),
                "task".into(),
                "{}".into(),
                Some("key1".into()),
            )
            .unwrap();

        let result = outbox.enqueue(
            "t2".into(),
            "task".into(),
            "{}".into(),
            Some("key1".into()),
        );
        assert!(matches!(result, Err(OutboxError::DuplicateTask(_))));
    }

    #[test]
    fn test_dedup_key_released_on_complete() {
        let mut outbox = Outbox::new(test_config());
        outbox
            .enqueue(
                "t1".into(),
                "task".into(),
                "{}".into(),
                Some("key1".into()),
            )
            .unwrap();

        outbox.mark_dispatched("t1").unwrap();
        outbox.mark_completed("t1").unwrap();

        // Same key can be reused
        outbox
            .enqueue(
                "t2".into(),
                "task".into(),
                "{}".into(),
                Some("key1".into()),
            )
            .unwrap();
    }

    #[test]
    fn test_invalid_status_transition() {
        let mut outbox = Outbox::new(test_config());
        outbox
            .enqueue("t1".into(), "task".into(), "{}".into(), None)
            .unwrap();

        // Can't complete a pending task
        let result = outbox.mark_completed("t1");
        assert!(matches!(
            result,
            Err(OutboxError::InvalidStatusTransition { .. })
        ));
    }

    #[test]
    fn test_retry_delay_exponential() {
        let mut outbox = Outbox::new(test_config());
        outbox
            .enqueue("t1".into(), "task".into(), "{}".into(), None)
            .unwrap();

        // First dispatch + fail
        outbox.mark_dispatched("t1").unwrap();
        outbox.mark_failed("t1", "err".into()).unwrap();

        let delay = outbox.retry_delay_ms("t1").unwrap();
        assert_eq!(delay, 100); // base * 2^0 = 100

        // Second dispatch + fail
        outbox.mark_dispatched("t1").unwrap();
        outbox.mark_failed("t1", "err".into()).unwrap();

        let delay = outbox.retry_delay_ms("t1").unwrap();
        assert_eq!(delay, 200); // base * 2^1 = 200
    }

    #[test]
    fn test_counts() {
        let mut outbox = Outbox::new(test_config());
        outbox
            .enqueue("t1".into(), "task".into(), "{}".into(), None)
            .unwrap();
        outbox
            .enqueue("t2".into(), "task".into(), "{}".into(), None)
            .unwrap();

        let counts = outbox.counts();
        assert_eq!(counts.get(&OutboxStatus::Pending), Some(&2));
    }

    #[test]
    fn test_dead_letter_capacity() {
        let config = OutboxConfig {
            dead_letter_capacity: 2,
            max_attempts: 1,
            ..Default::default()
        };
        let mut outbox = Outbox::new(config);

        for i in 0..3 {
            outbox
                .enqueue(format!("t{}", i), "task".into(), "{}".into(), None)
                .unwrap();
            outbox.mark_dispatched(&format!("t{}", i)).unwrap();
            outbox
                .mark_failed(&format!("t{}", i), "err".into())
                .unwrap();
        }

        assert_eq!(outbox.dead_letter().len(), 2); // capped at capacity
    }
}
