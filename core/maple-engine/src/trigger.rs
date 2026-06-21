//! Trigger manager for event-driven and message-driven workflow execution
//! (#15, #16).
//!
//! Listens to the EventBus and GroupMessage events. When a message or
//! event matches a registered trigger rule, the manager fires the
//! associated workflow by calling WorkflowService::create_run.
//!
//! Trigger types:
//! - EventTrigger: fires when a specific EventBus event type is published
//!   (e.g., "task.created", "agent.online", "approval.resolved")
//! - MessageTrigger: fires when a group message contains a keyword or
//!   matches a sender/channel pattern
//! - WebhookTrigger: fires when an HTTP webhook is received (handled
//!   separately by the webhook handler in main.rs)

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A trigger rule that maps an event/message pattern to a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerRule {
    pub id: String,
    pub workflow_id: String,
    pub trigger_type: TriggerType,
    pub enabled: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerType {
    /// #15: fires when a specific EventBus event type is published.
    /// Optional filter matches on event payload fields (JSON path → value).
    Event {
        event_type: String,
        /// Optional filter: JSON path → expected value (e.g., {"group_id": "g1"})
        #[serde(default)]
        filter: HashMap<String, serde_json::Value>,
    },
    /// #16: fires when a group message matches keyword/sender/channel.
    Message {
        /// Group ID to watch (empty = all groups)
        #[serde(default)]
        group_id: String,
        /// Keyword that must appear in the message (empty = any message)
        #[serde(default)]
        keyword: String,
        /// Specific sender to watch (empty = any sender)
        #[serde(default)]
        sender_id: String,
    },
}

pub struct TriggerManager {
    pool: SqlitePool,
    rules: Arc<RwLock<Vec<TriggerRule>>>,
}

impl TriggerManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            rules: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Load all enabled trigger rules from DB into memory.
    pub async fn load_rules(&self) -> Result<()> {
        let rows: Vec<(String, String, String, bool, i64)> = sqlx::query_as(
            "SELECT id, workflow_id, trigger_config, enabled, created_at
               FROM workflow_triggers
              WHERE enabled = 1"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut rules = Vec::new();
        for (id, workflow_id, config_json, enabled, created_at) in rows {
            if let Ok(trigger_type) = serde_json::from_str::<TriggerType>(&config_json) {
                rules.push(TriggerRule {
                    id,
                    workflow_id,
                    trigger_type,
                    enabled,
                    created_at,
                });
            }
        }

        let mut w = self.rules.write().await;
        *w = rules;
        tracing::info!(count = w.len(), "Loaded trigger rules");
        Ok(())
    }

    /// Register a new trigger rule. Also persists to DB.
    pub async fn add_rule(&self, rule: TriggerRule) -> Result<()> {
        let config_json = serde_json::to_string(&rule.trigger_type)?;
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO workflow_triggers (id, workflow_id, trigger_config, enabled, created_at)
             VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&rule.id)
        .bind(&rule.workflow_id)
        .bind(&config_json)
        .bind(rule.enabled)
        .bind(now)
        .execute(&self.pool)
        .await?;

        let mut w = self.rules.write().await;
        w.push(rule);
        Ok(())
    }

    /// Remove a trigger rule.
    pub async fn remove_rule(&self, rule_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM workflow_triggers WHERE id = ?")
            .bind(rule_id)
            .execute(&self.pool)
            .await?;

        let mut w = self.rules.write().await;
        w.retain(|r| r.id != rule_id);
        Ok(())
    }

    /// List all trigger rules.
    pub async fn list_rules(&self) -> Vec<TriggerRule> {
        self.rules.read().await.clone()
    }

    /// #15: Check if an EventBus event matches any trigger rule.
    /// Returns list of workflow_ids that should be fired.
    pub async fn match_event(&self, event_type: &str, payload: &serde_json::Value) -> Vec<String> {
        let rules = self.rules.read().await;
        let mut matches = Vec::new();

        for rule in rules.iter() {
            if !rule.enabled {
                continue;
            }
            if let TriggerType::Event { event_type: et, filter } = &rule.trigger_type {
                if et != event_type {
                    continue;
                }
                // Check filter: all keys must match
                let filter_ok = filter.iter().all(|(key, expected)| {
                    payload.get(key).map_or(false, |actual| actual == expected)
                });
                if filter_ok {
                    matches.push(rule.workflow_id.clone());
                }
            }
        }

        matches
    }

    /// #16: Check if a group message matches any trigger rule.
    /// Returns list of (workflow_id, trigger_rule_id) that should be fired.
    pub async fn match_message(
        &self,
        group_id: &str,
        sender_id: &str,
        message: &str,
    ) -> Vec<(String, String)> {
        let rules = self.rules.read().await;
        let mut matches = Vec::new();

        for rule in rules.iter() {
            if !rule.enabled {
                continue;
            }
            if let TriggerType::Message { group_id: rg, keyword, sender_id: rs } = &rule.trigger_type {
                // Group filter: empty = all groups
                if !rg.is_empty() && rg != group_id {
                    continue;
                }
                // Sender filter: empty = any sender
                if !rs.is_empty() && rs != sender_id {
                    continue;
                }
                // Keyword filter: empty = any message
                if !keyword.is_empty() {
                    let msg_lower = message.to_lowercase();
                    let kw_lower = keyword.to_lowercase();
                    if !msg_lower.contains(&kw_lower) {
                        continue;
                    }
                }
                matches.push((rule.workflow_id.clone(), rule.id.clone()));
            }
        }

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup() -> TriggerManager {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE workflow_triggers (
                id TEXT PRIMARY KEY,
                workflow_id TEXT NOT NULL,
                trigger_config TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL
            )"
        ).execute(&pool).await.unwrap();
        TriggerManager::new(pool)
    }

    #[tokio::test]
    async fn test_event_trigger_matches() {
        let mgr = setup().await;
        mgr.add_rule(TriggerRule {
            id: "t1".into(),
            workflow_id: "wf1".into(),
            trigger_type: TriggerType::Event {
                event_type: "task.created".into(),
                filter: HashMap::new(),
            },
            enabled: true,
            created_at: 0,
        }).await.unwrap();

        let matches = mgr.match_event("task.created", &serde_json::json!({})).await;
        assert_eq!(matches, vec!["wf1"]);
    }

    #[tokio::test]
    async fn test_event_trigger_with_filter() {
        let mgr = setup().await;
        let mut filter = HashMap::new();
        filter.insert("group_id".into(), serde_json::json!("g1"));
        mgr.add_rule(TriggerRule {
            id: "t2".into(),
            workflow_id: "wf2".into(),
            trigger_type: TriggerType::Event {
                event_type: "group_message_sent".into(),
                filter,
            },
            enabled: true,
            created_at: 0,
        }).await.unwrap();

        // Matching filter
        let matches = mgr.match_event("group_message_sent", &serde_json::json!({"group_id": "g1"})).await;
        assert_eq!(matches, vec!["wf2"]);

        // Non-matching filter
        let matches = mgr.match_event("group_message_sent", &serde_json::json!({"group_id": "g2"})).await;
        assert!(matches.is_empty());
    }

    #[tokio::test]
    async fn test_event_trigger_wrong_type_no_match() {
        let mgr = setup().await;
        mgr.add_rule(TriggerRule {
            id: "t3".into(),
            workflow_id: "wf3".into(),
            trigger_type: TriggerType::Event {
                event_type: "task.created".into(),
                filter: HashMap::new(),
            },
            enabled: true,
            created_at: 0,
        }).await.unwrap();

        let matches = mgr.match_event("task.updated", &serde_json::json!({})).await;
        assert!(matches.is_empty());
    }

    #[tokio::test]
    async fn test_message_trigger_keyword_match() {
        let mgr = setup().await;
        mgr.add_rule(TriggerRule {
            id: "m1".into(),
            workflow_id: "wf_msg".into(),
            trigger_type: TriggerType::Message {
                group_id: String::new(),
                keyword: "deploy".into(),
                sender_id: String::new(),
            },
            enabled: true,
            created_at: 0,
        }).await.unwrap();

        // Matching keyword
        let matches = mgr.match_message("g1", "u1", "please deploy to prod").await;
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0, "wf_msg");

        // Non-matching keyword
        let matches = mgr.match_message("g1", "u1", "hello world").await;
        assert!(matches.is_empty());
    }

    #[tokio::test]
    async fn test_message_trigger_group_filter() {
        let mgr = setup().await;
        mgr.add_rule(TriggerRule {
            id: "m2".into(),
            workflow_id: "wf_g".into(),
            trigger_type: TriggerType::Message {
                group_id: "specific_group".into(),
                keyword: String::new(),
                sender_id: String::new(),
            },
            enabled: true,
            created_at: 0,
        }).await.unwrap();

        // Matching group
        let matches = mgr.match_message("specific_group", "u1", "any message").await;
        assert_eq!(matches.len(), 1);

        // Wrong group
        let matches = mgr.match_message("other_group", "u1", "any message").await;
        assert!(matches.is_empty());
    }

    #[tokio::test]
    async fn test_message_trigger_sender_filter() {
        let mgr = setup().await;
        mgr.add_rule(TriggerRule {
            id: "m3".into(),
            workflow_id: "wf_s".into(),
            trigger_type: TriggerType::Message {
                group_id: String::new(),
                keyword: String::new(),
                sender_id: "vip_user".into(),
            },
            enabled: true,
            created_at: 0,
        }).await.unwrap();

        // Matching sender
        let matches = mgr.match_message("g1", "vip_user", "hello").await;
        assert_eq!(matches.len(), 1);

        // Wrong sender
        let matches = mgr.match_message("g1", "regular_user", "hello").await;
        assert!(matches.is_empty());
    }

    #[tokio::test]
    async fn test_disabled_rule_does_not_match() {
        let mgr = setup().await;
        mgr.add_rule(TriggerRule {
            id: "d1".into(),
            workflow_id: "wf_disabled".into(),
            trigger_type: TriggerType::Event {
                event_type: "task.created".into(),
                filter: HashMap::new(),
            },
            enabled: false, // disabled
            created_at: 0,
        }).await.unwrap();

        let matches = mgr.match_event("task.created", &serde_json::json!({})).await;
        assert!(matches.is_empty());
    }

    #[tokio::test]
    async fn test_remove_rule() {
        let mgr = setup().await;
        mgr.add_rule(TriggerRule {
            id: "r1".into(),
            workflow_id: "wf1".into(),
            trigger_type: TriggerType::Event {
                event_type: "task.created".into(),
                filter: HashMap::new(),
            },
            enabled: true,
            created_at: 0,
        }).await.unwrap();

        assert_eq!(mgr.list_rules().await.len(), 1);
        mgr.remove_rule("r1").await.unwrap();
        assert_eq!(mgr.list_rules().await.len(), 0);
    }
}
