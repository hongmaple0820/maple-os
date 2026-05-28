use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Trigger System — inspired by golutra's TriggerBus + TriggerScheduler
///
/// Features:
/// - Event-driven rule evaluation
/// - Deferred trigger stages (Stable, Silence, Debounce, PostReadyTick, ChatPendingForce)
/// - Deduplication by trigger key
/// - Priority queue for scheduling

/// Trigger event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerEvent {
    /// File system change
    FileChanged { path: String },
    /// Git operation
    GitOperation { operation: String, branch: String },
    /// Session state change
    SessionStateChanged { session_id: String, state: String },
    /// Tool execution completed
    ToolExecuted { tool_name: String, success: bool },
    /// Health check result
    HealthCheck { component: String, healthy: bool },
    /// Custom event
    Custom { event_type: String, data: Value },
}

/// Trigger stage for deferred evaluation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TriggerStage {
    /// Immediate evaluation
    Immediate,
    /// Wait for stability (no new events for duration)
    Stable { duration_ms: u64 },
    /// Wait for silence (no events at all)
    Silence { duration_ms: u64 },
    /// Debounce (delay execution)
    Debounce { delay_ms: u64 },
    /// Execute after ready tick
    PostReadyTick,
    /// Force execution even if pending
    ChatPendingForce,
}

/// Trigger rule definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerRule {
    pub id: String,
    pub name: String,
    pub event_type: String,
    pub conditions: Vec<TriggerCondition>,
    pub actions: Vec<TriggerAction>,
    pub stage: TriggerStage,
    pub enabled: bool,
    pub priority: u8,
}

/// Trigger condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerCondition {
    /// Path matches glob pattern
    PathMatches { pattern: String },
    /// Event data contains key-value
    EventContains { key: String, value: Value },
    /// Time since last trigger
    MinInterval { duration_ms: u64 },
    /// Custom condition
    Custom { expression: String },
}

/// Trigger action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerAction {
    /// Execute a tool
    ExecuteTool { tool_name: String, input: Value },
    /// Send a message
    SendMessage { target: String, message: String },
    /// Run a script
    RunScript { script: String },
    /// Custom action
    Custom { action_type: String, data: Value },
}

/// Scheduled trigger entry
#[derive(Debug)]
struct TriggerEntry {
    rule_id: String,
    due_at: Instant,
    event: TriggerEvent,
}

impl PartialEq for TriggerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.due_at == other.due_at
    }
}

impl Eq for TriggerEntry {}

impl PartialOrd for TriggerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TriggerEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse for min-heap (earliest first)
        self.due_at.cmp(&other.due_at).reverse()
    }
}

/// Deduplication key
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct TriggerKey {
    rule_id: String,
    event_type: String,
}

/// Trigger scheduler with deduplication
pub struct TriggerScheduler {
    rules: HashMap<String, TriggerRule>,
    queue: BinaryHeap<Reverse<TriggerEntry>>,
    dedup: HashMap<TriggerKey, Instant>,
    dedup_window: Duration,
}

impl TriggerScheduler {
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
            queue: BinaryHeap::new(),
            dedup: HashMap::new(),
            dedup_window: Duration::from_secs(5),
        }
    }

    pub fn with_dedup_window(mut self, window: Duration) -> Self {
        self.dedup_window = window;
        self
    }

    /// Register a trigger rule
    pub fn register_rule(&mut self, rule: TriggerRule) {
        self.rules.insert(rule.id.clone(), rule);
    }

    /// Process an event and schedule triggers
    pub fn process_event(&mut self, event: &TriggerEvent) -> Vec<String> {
        let event_type = self.get_event_type(event);
        let mut triggered_rules = Vec::new();

        for (_, rule) in &self.rules {
            if !rule.enabled {
                continue;
            }

            if rule.event_type != event_type {
                continue;
            }

            // Check conditions
            if !self.check_conditions(&rule.conditions, event) {
                continue;
            }

            // Check deduplication
            let key = TriggerKey {
                rule_id: rule.id.clone(),
                event_type: event_type.clone(),
            };

            if let Some(last_trigger) = self.dedup.get(&key) {
                if last_trigger.elapsed() < self.dedup_window {
                    continue;
                }
            }

            // Schedule trigger
            let due_at = match &rule.stage {
                TriggerStage::Immediate => Instant::now(),
                TriggerStage::Stable { duration_ms } => {
                    Instant::now() + Duration::from_millis(*duration_ms)
                }
                TriggerStage::Silence { duration_ms } => {
                    Instant::now() + Duration::from_millis(*duration_ms)
                }
                TriggerStage::Debounce { delay_ms } => {
                    Instant::now() + Duration::from_millis(*delay_ms)
                }
                TriggerStage::PostReadyTick => Instant::now() + Duration::from_millis(100),
                TriggerStage::ChatPendingForce => Instant::now(),
            };

            self.queue.push(Reverse(TriggerEntry {
                rule_id: rule.id.clone(),
                due_at,
                event: event.clone(),
            }));

            self.dedup.insert(key, Instant::now());
            triggered_rules.push(rule.id.clone());
        }

        triggered_rules
    }

    /// Get due triggers
    pub fn get_due_triggers(&mut self) -> Vec<(String, TriggerEvent)> {
        let now = Instant::now();
        let mut due = Vec::new();

        while let Some(Reverse(entry)) = self.queue.peek() {
            if entry.due_at <= now {
                let entry = self.queue.pop().unwrap().0;
                due.push((entry.rule_id, entry.event));
            } else {
                break;
            }
        }

        due
    }

    /// Get all registered rules
    pub fn get_rules(&self) -> Vec<&TriggerRule> {
        self.rules.values().collect()
    }

    /// Enable/disable a rule
    pub fn set_rule_enabled(&mut self, rule_id: &str, enabled: bool) {
        if let Some(rule) = self.rules.get_mut(rule_id) {
            rule.enabled = enabled;
        }
    }

    fn get_event_type(&self, event: &TriggerEvent) -> String {
        match event {
            TriggerEvent::FileChanged { .. } => "file_changed".to_string(),
            TriggerEvent::GitOperation { .. } => "git_operation".to_string(),
            TriggerEvent::SessionStateChanged { .. } => "session_state_changed".to_string(),
            TriggerEvent::ToolExecuted { .. } => "tool_executed".to_string(),
            TriggerEvent::HealthCheck { .. } => "health_check".to_string(),
            TriggerEvent::Custom { event_type, .. } => event_type.clone(),
        }
    }

    fn check_conditions(&self, conditions: &[TriggerCondition], event: &TriggerEvent) -> bool {
        for condition in conditions {
            match condition {
                TriggerCondition::PathMatches { pattern } => {
                    if let TriggerEvent::FileChanged { path } = event {
                        if !self.match_glob(pattern, path) {
                            return false;
                        }
                    }
                }
                TriggerCondition::EventContains { key, value } => {
                    // Simple key-value check
                    let event_json = serde_json::to_value(event).unwrap_or_default();
                    if event_json[key] != *value {
                        return false;
                    }
                }
                TriggerCondition::MinInterval { duration_ms } => {
                    // This would need last trigger tracking per rule
                    // For now, allow all
                    let _ = duration_ms;
                }
                TriggerCondition::Custom { expression } => {
                    // Custom condition evaluation would be implemented here
                    let _ = expression;
                }
            }
        }
        true
    }

    fn match_glob(&self, pattern: &str, path: &str) -> bool {
        // Simple glob matching (could be enhanced with glob crate)
        if pattern == "*" {
            return true;
        }
        if pattern.ends_with("/*") {
            let prefix = &pattern[..pattern.len() - 2];
            return path.starts_with(prefix);
        }
        pattern == path
    }
}

/// Trigger bus for event distribution
pub struct TriggerBus {
    sender: mpsc::Sender<TriggerEvent>,
    receiver: mpsc::Receiver<TriggerEvent>,
}

impl TriggerBus {
    pub fn new(buffer_size: usize) -> Self {
        let (sender, receiver) = mpsc::channel(buffer_size);
        Self { sender, receiver }
    }

    /// Publish an event
    pub async fn publish(
        &self,
        event: TriggerEvent,
    ) -> Result<(), mpsc::error::SendError<TriggerEvent>> {
        self.sender.send(event).await
    }

    /// Subscribe to events (returns a clone of the sender for publishing)
    pub fn get_sender(&self) -> mpsc::Sender<TriggerEvent> {
        self.sender.clone()
    }

    /// Receive next event
    pub async fn receive(&mut self) -> Option<TriggerEvent> {
        self.receiver.recv().await
    }
}

/// Builder for TriggerRule
pub struct TriggerRuleBuilder {
    rule: TriggerRule,
}

impl TriggerRuleBuilder {
    pub fn new(id: &str, name: &str, event_type: &str) -> Self {
        Self {
            rule: TriggerRule {
                id: id.to_string(),
                name: name.to_string(),
                event_type: event_type.to_string(),
                conditions: Vec::new(),
                actions: Vec::new(),
                stage: TriggerStage::Immediate,
                enabled: true,
                priority: 0,
            },
        }
    }

    pub fn condition(mut self, condition: TriggerCondition) -> Self {
        self.rule.conditions.push(condition);
        self
    }

    pub fn action(mut self, action: TriggerAction) -> Self {
        self.rule.actions.push(action);
        self
    }

    pub fn stage(mut self, stage: TriggerStage) -> Self {
        self.rule.stage = stage;
        self
    }

    pub fn priority(mut self, priority: u8) -> Self {
        self.rule.priority = priority;
        self
    }

    pub fn build(self) -> TriggerRule {
        self.rule
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_scheduler_immediate() {
        let mut scheduler = TriggerScheduler::new();
        let rule = TriggerRuleBuilder::new("test", "Test Rule", "file_changed")
            .action(TriggerAction::ExecuteTool {
                tool_name: "lint".to_string(),
                input: serde_json::json!({}),
            })
            .build();

        scheduler.register_rule(rule);

        let event = TriggerEvent::FileChanged {
            path: "src/main.rs".to_string(),
        };

        let triggered = scheduler.process_event(&event);
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0], "test");

        let due = scheduler.get_due_triggers();
        assert_eq!(due.len(), 1);
    }

    #[test]
    fn test_trigger_deduplication() {
        let mut scheduler = TriggerScheduler::new().with_dedup_window(Duration::from_secs(5));

        let rule = TriggerRuleBuilder::new("test", "Test Rule", "file_changed").build();
        scheduler.register_rule(rule);

        let event = TriggerEvent::FileChanged {
            path: "src/main.rs".to_string(),
        };

        // First trigger should work
        let triggered = scheduler.process_event(&event);
        assert_eq!(triggered.len(), 1);

        // Second trigger within dedup window should be skipped
        let triggered = scheduler.process_event(&event);
        assert_eq!(triggered.len(), 0);
    }

    #[test]
    fn test_trigger_conditions() {
        let mut scheduler = TriggerScheduler::new();
        let rule = TriggerRuleBuilder::new("test", "Test Rule", "file_changed")
            .condition(TriggerCondition::PathMatches {
                pattern: "src/*".to_string(),
            })
            .build();

        scheduler.register_rule(rule);

        // Matching path
        let event = TriggerEvent::FileChanged {
            path: "src/main.rs".to_string(),
        };
        let triggered = scheduler.process_event(&event);
        assert_eq!(triggered.len(), 1);

        // Non-matching path
        let event = TriggerEvent::FileChanged {
            path: "docs/README.md".to_string(),
        };
        let triggered = scheduler.process_event(&event);
        assert_eq!(triggered.len(), 0);
    }

    #[test]
    fn test_trigger_bus() {
        let bus = TriggerBus::new(10);
        let sender = bus.get_sender();

        let event = TriggerEvent::Custom {
            event_type: "test".to_string(),
            data: serde_json::json!({"key": "value"}),
        };

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            sender.send(event).await.unwrap();
            let received = bus.receiver.recv().await.unwrap();
            assert!(matches!(received, TriggerEvent::Custom { .. }));
        });
    }
}
