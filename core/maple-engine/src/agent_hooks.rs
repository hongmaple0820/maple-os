use crate::event_bus::{EventBus, Event};
use crate::memory_service::{MemoryService, MemoryLayer};
use crate::approval::{ApprovalService, QuorumType, ApprovalUrgency};
use std::sync::Arc;

/// Actions the hook can return to control agent execution flow
#[derive(Debug, Clone)]
pub enum HookAction {
    /// Continue normal execution
    Continue,
    /// Abort with reason
    Abort { reason: String },
    /// Pause execution until approval is resolved
    WaitForApproval { approval_id: String },
    /// Block the tool call
    Block { reason: String },
}

/// Configuration for which tools require approval in a given context
#[derive(Debug, Clone)]
pub struct ToolApprovalConfig {
    pub tool_name: String,
    pub urgency: ApprovalUrgency,
    pub quorum: QuorumType,
    pub timeout_seconds: i64,
}

/// MapleAgentHook injects behavior at key lifecycle points of agent execution.
/// Designed to be used within group chat contexts where agent actions need
/// to be visible, auditable, and optionally gated by human approval.
pub struct MapleAgentHook {
    pub group_id: String,
    pub agent_id: String,
    pub event_bus: Arc<EventBus>,
    pub memory_service: Option<Arc<MemoryService>>,
    pub approval_service: Option<Arc<ApprovalService>>,
    /// Tools that require approval before execution
    pub approval_required_tools: Vec<ToolApprovalConfig>,
    /// Tools that are denied entirely
    pub denied_tools: Vec<String>,
    /// Rate limit: max calls per minute (0 = no limit)
    pub max_calls_per_minute: u32,
}

impl MapleAgentHook {
    pub fn new(
        group_id: String,
        agent_id: String,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self {
            group_id,
            agent_id,
            event_bus,
            memory_service: None,
            approval_service: None,
            approval_required_tools: Vec::new(),
            denied_tools: Vec::new(),
            max_calls_per_minute: 0,
        }
    }

    pub fn with_memory(mut self, svc: Arc<MemoryService>) -> Self {
        self.memory_service = Some(svc);
        self
    }

    pub fn with_approval(mut self, svc: Arc<ApprovalService>) -> Self {
        self.approval_service = Some(svc);
        self
    }

    pub fn with_denied_tools(mut self, tools: Vec<String>) -> Self {
        self.denied_tools = tools;
        self
    }

    pub fn with_approval_tools(mut self, configs: Vec<ToolApprovalConfig>) -> Self {
        self.approval_required_tools = configs;
        self
    }

    pub fn with_rate_limit(mut self, max_per_minute: u32) -> Self {
        self.max_calls_per_minute = max_per_minute;
        self
    }

    // ── Lifecycle Hooks ──

    /// Called before LLM completion. Broadcasts thinking state to group.
    pub async fn on_completion_call(&self) -> HookAction {
        // Broadcast agent thinking state
        let _ = self.event_bus.publish(Event::GroupMessageSent {
            group_id: self.group_id.clone(),
            message_id: uuid::Uuid::new_v4().to_string(),
            sender_id: self.agent_id.clone(),
            content: serde_json::json!({
                "type": "thinking",
                "agent_id": self.agent_id
            }).to_string(),
        }).await;

        HookAction::Continue
    }

    /// Called before a tool is executed. Checks permissions and may require approval.
    pub async fn on_tool_call(&self, tool_name: &str, call_id: &str, args: &str) -> HookAction {
        // Check denied tools
        if self.denied_tools.iter().any(|t| t == tool_name) {
            return HookAction::Block {
                reason: format!("Tool '{}' is not allowed in this context", tool_name),
            };
        }

        // Check if tool requires approval
        if let Some(config) = self.approval_required_tools.iter().find(|c| c.tool_name == tool_name) {
            if let Some(ref approval_svc) = self.approval_service {
                let approval_id = uuid::Uuid::new_v4().to_string();
                let _now = chrono::Utc::now().timestamp();

                let title = format!("Tool approval: {}", tool_name);
                let description = format!("Agent requests permission to use tool '{}' (call_id: {})", tool_name, call_id);
                let context_data = serde_json::json!({
                    "tool_name": tool_name,
                    "call_id": call_id,
                    "args": args
                }).to_string();

                match approval_svc.create_request(
                    &self.group_id,
                    &title,
                    Some(&description),
                    "tool_call",
                    &self.agent_id,
                    config.urgency.clone(),
                    config.quorum.clone(),
                    "any",
                    Some(&context_data),
                ).await {
                    Ok(_) => {
                        let _ = self.event_bus.publish(Event::GroupMessageSent {
                            group_id: self.group_id.clone(),
                            message_id: uuid::Uuid::new_v4().to_string(),
                            sender_id: self.agent_id.clone(),
                            content: serde_json::json!({
                                "type": "approval_request",
                                "approval_id": approval_id,
                                "tool_name": tool_name,
                                "call_id": call_id,
                                "args": args
                            }).to_string(),
                        }).await;

                        return HookAction::WaitForApproval { approval_id };
                    }
                    Err(e) => {
                        tracing::error!("Failed to create approval for tool '{}': {}", tool_name, e);
                        return HookAction::Abort {
                            reason: format!("Failed to create approval: {}", e),
                        };
                    }
                }
            }
        }

        // Publish tool call message to group
        let _ = self.event_bus.publish(Event::GroupMessageSent {
            group_id: self.group_id.clone(),
            message_id: uuid::Uuid::new_v4().to_string(),
            sender_id: self.agent_id.clone(),
            content: serde_json::json!({
                "type": "tool_call",
                "tool_name": tool_name,
                "call_id": call_id,
                "args": args,
                "status": "running"
            }).to_string(),
        }).await;

        HookAction::Continue
    }

    /// Called after a tool returns its result. Records to memory if notable.
    pub async fn on_tool_result(&self, tool_name: &str, call_id: &str, result: &str) -> HookAction {
        // Publish tool result message
        let _ = self.event_bus.publish(Event::GroupMessageSent {
            group_id: self.group_id.clone(),
            message_id: uuid::Uuid::new_v4().to_string(),
            sender_id: self.agent_id.clone(),
            content: serde_json::json!({
                "type": "tool_result",
                "tool_name": tool_name,
                "call_id": call_id,
                "result_preview": if result.len() > 200 { &result[..200] } else { result },
                "status": "success"
            }).to_string(),
        }).await;

        // Store notable results in episodic memory
        if self.is_memorable_result(tool_name, result) {
            if let Some(ref memory_svc) = self.memory_service {
                let content = format!("Tool '{}' result: {}", tool_name, truncate(result, 500));
                let _ = memory_svc.store(
                    &self.agent_id,
                    MemoryLayer::Episodic,
                    &content,
                    Some(&format!("Tool {} execution result", tool_name)),
                    Some("tool_call"),
                    Some(call_id),
                    Some(&self.group_id),
                    None,
                ).await;
            }
        }

        HookAction::Continue
    }

    /// Called after LLM generates a response. Publishes to group chat.
    pub async fn on_completion_response(&self, response: &str) -> HookAction {
        // Publish agent reply to group
        let _ = self.event_bus.publish(Event::GroupMessageSent {
            group_id: self.group_id.clone(),
            message_id: uuid::Uuid::new_v4().to_string(),
            sender_id: self.agent_id.clone(),
            content: response.to_string(),
        }).await;

        // Store in working memory
        if let Some(ref memory_svc) = self.memory_service {
            let _ = memory_svc.store(
                &self.agent_id,
                MemoryLayer::Working,
                response,
                None,
                Some("chat"),
                None,
                Some(&self.group_id),
                Some(24),
            ).await;
        }

        HookAction::Continue
    }

    /// Build memory context for injection into agent prompt
    pub async fn build_memory_context(&self, query: &str) -> Option<String> {
        let memory_svc = self.memory_service.as_ref()?;

        let query_obj = crate::memory_service::MemoryQuery {
            agent_id: self.agent_id.clone(),
            query_text: Some(query.to_string()),
            memory_type: None,
            group_id: Some(self.group_id.clone()),
            limit: 10,
        };

        let results = memory_svc.search(&query_obj).await.ok()?;
        if results.is_empty() {
            return None;
        }

        let mut context = String::from("\n\n## Relevant Memory\n");
        for (i, scored) in results.iter().take(5).enumerate() {
            context.push_str(&format!(
                "{}. [{}] {}\n",
                i + 1,
                scored.memory.memory_type.as_str(),
                truncate(&scored.memory.content, 200)
            ));
        }
        Some(context)
    }

    // ── Helpers ──

    fn is_memorable_result(&self, tool_name: &str, result: &str) -> bool {
        matches!(tool_name, "file_read" | "file_edit" | "bash" | "web_search" | "database_query")
            || result.len() > 1000
    }
}

fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..max_len]
    }
}

// ── AgentHookService: DB-persisted hook CRUD ──────────────────

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHookRecord {
    pub id: String,
    pub group_id: String,
    pub agent_id: String,
    pub event_types: String,       // JSON array
    pub condition_expr: Option<String>,
    pub action_type: String,
    pub action_config: String,     // JSON object
    pub enabled: bool,
    pub priority: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateHookRequest {
    pub agent_id: String,
    pub event_types: Vec<String>,
    pub condition_expr: Option<String>,
    pub action_type: String,
    pub action_config: serde_json::Value,
    pub priority: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookLogRecord {
    pub id: String,
    pub hook_id: String,
    pub event_type: String,
    pub event_data: Option<String>,
    pub status: String,
    pub result: Option<String>,
    pub error: Option<String>,
    pub executed_at: i64,
}

pub struct AgentHookService {
    pool: SqlitePool,
}

impl AgentHookService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_hook(&self, group_id: &str, req: &CreateHookRequest) -> anyhow::Result<AgentHookRecord> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        let event_types = serde_json::to_string(&req.event_types)?;
        let action_config = serde_json::to_string(&req.action_config)?;

        sqlx::query(
            "INSERT INTO agent_hooks (id, group_id, agent_id, event_types, condition_expr, action_type, action_config, enabled, priority, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)"
        )
        .bind(&id).bind(group_id).bind(&req.agent_id).bind(&event_types)
        .bind(&req.condition_expr).bind(&req.action_type).bind(&action_config)
        .bind(req.priority.unwrap_or(0)).bind(now).bind(now)
        .execute(&self.pool).await?;

        Ok(AgentHookRecord {
            id, group_id: group_id.to_string(), agent_id: req.agent_id.clone(),
            event_types, condition_expr: req.condition_expr.clone(),
            action_type: req.action_type.clone(), action_config,
            enabled: true, priority: req.priority.unwrap_or(0),
            created_at: now, updated_at: now,
        })
    }

    pub async fn list_hooks(&self, group_id: &str) -> anyhow::Result<Vec<AgentHookRecord>> {
        let rows = sqlx::query_as::<_, (String, String, String, String, Option<String>, String, String, bool, i64, i64, i64)>(
            "SELECT id, group_id, agent_id, event_types, condition_expr, action_type, action_config, enabled, priority, created_at, updated_at
             FROM agent_hooks WHERE group_id = ? ORDER BY priority DESC, created_at DESC"
        ).bind(group_id).fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(|r| AgentHookRecord {
            id: r.0, group_id: r.1, agent_id: r.2, event_types: r.3,
            condition_expr: r.4, action_type: r.5, action_config: r.6,
            enabled: r.7, priority: r.8, created_at: r.9, updated_at: r.10,
        }).collect())
    }

    pub async fn get_hook(&self, hook_id: &str) -> anyhow::Result<Option<AgentHookRecord>> {
        let row = sqlx::query_as::<_, (String, String, String, String, Option<String>, String, String, bool, i64, i64, i64)>(
            "SELECT id, group_id, agent_id, event_types, condition_expr, action_type, action_config, enabled, priority, created_at, updated_at
             FROM agent_hooks WHERE id = ?"
        ).bind(hook_id).fetch_optional(&self.pool).await?;

        Ok(row.map(|r| AgentHookRecord {
            id: r.0, group_id: r.1, agent_id: r.2, event_types: r.3,
            condition_expr: r.4, action_type: r.5, action_config: r.6,
            enabled: r.7, priority: r.8, created_at: r.9, updated_at: r.10,
        }))
    }

    pub async fn update_hook(&self, hook_id: &str, updates: &serde_json::Value) -> anyhow::Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let mut sets = vec!["updated_at = ?".to_string()];
        let mut values: Vec<String> = vec![];

        if let Some(v) = updates.get("event_types") {
            sets.push("event_types = ?".to_string());
            values.push(serde_json::to_string(v)?);
        }
        if let Some(v) = updates.get("condition_expr") {
            sets.push("condition_expr = ?".to_string());
            values.push(v.as_str().unwrap_or("").to_string());
        }
        if let Some(v) = updates.get("action_type") {
            sets.push("action_type = ?".to_string());
            values.push(v.as_str().unwrap_or("").to_string());
        }
        if let Some(v) = updates.get("action_config") {
            sets.push("action_config = ?".to_string());
            values.push(serde_json::to_string(v)?);
        }
        if let Some(v) = updates.get("enabled") {
            sets.push("enabled = ?".to_string());
            values.push(if v.as_bool().unwrap_or(true) { "1" } else { "0" }.to_string());
        }
        if let Some(v) = updates.get("priority") {
            sets.push("priority = ?".to_string());
            values.push(v.as_i64().unwrap_or(0).to_string());
        }

        let sql = format!("UPDATE agent_hooks SET {} WHERE id = ?", sets.join(", "));
        let mut query = sqlx::query(&sql).bind(now);
        for v in &values {
            query = query.bind(v);
        }
        query = query.bind(hook_id);
        let result = query.execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_hook(&self, hook_id: &str) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM agent_hooks WHERE id = ?")
            .bind(hook_id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn record_log(
        &self, hook_id: &str, event_type: &str, event_data: Option<&str>,
        status: &str, result: Option<&str>, error: Option<&str>,
    ) -> anyhow::Result<HookLogRecord> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO agent_hook_logs (id, hook_id, event_type, event_data, status, result, error, executed_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id).bind(hook_id).bind(event_type).bind(event_data)
        .bind(status).bind(result).bind(error).bind(now)
        .execute(&self.pool).await?;

        Ok(HookLogRecord {
            id, hook_id: hook_id.to_string(), event_type: event_type.to_string(),
            event_data: event_data.map(|s| s.to_string()), status: status.to_string(),
            result: result.map(|s| s.to_string()), error: error.map(|s| s.to_string()),
            executed_at: now,
        })
    }

    pub async fn list_logs(&self, hook_id: &str, limit: i64) -> anyhow::Result<Vec<HookLogRecord>> {
        let rows = sqlx::query_as::<_, (String, String, String, Option<String>, String, Option<String>, Option<String>, i64)>(
            "SELECT id, hook_id, event_type, event_data, status, result, error, executed_at
             FROM agent_hook_logs WHERE hook_id = ? ORDER BY executed_at DESC LIMIT ?"
        ).bind(hook_id).bind(limit).fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(|r| HookLogRecord {
            id: r.0, hook_id: r.1, event_type: r.2, event_data: r.3,
            status: r.4, result: r.5, error: r.6, executed_at: r.7,
        }).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hook() -> MapleAgentHook {
        MapleAgentHook::new(
            "group1".to_string(),
            "agent1".to_string(),
            Arc::new(EventBus::new()),
        )
    }

    #[test]
    fn test_denied_tool_blocks() {
        let hook = make_hook().with_denied_tools(vec!["rm".to_string(), "drop_table".to_string()]);
        assert!(hook.denied_tools.contains(&"rm".to_string()));
    }

    #[test]
    fn test_is_memorable_result() {
        let hook = make_hook();
        assert!(hook.is_memorable_result("file_read", "some content"));
        assert!(hook.is_memorable_result("bash", "output"));
        assert!(!hook.is_memorable_result("think", "short"));
        assert!(hook.is_memorable_result("unknown_tool", &"x".repeat(1001)));
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hello");
    }

    #[tokio::test]
    async fn test_on_completion_call_publishes() {
        let hook = make_hook();
        let action = hook.on_completion_call().await;
        assert!(matches!(action, HookAction::Continue));
    }

    #[tokio::test]
    async fn test_on_tool_call_denied() {
        let hook = make_hook().with_denied_tools(vec!["rm".to_string()]);
        let action = hook.on_tool_call("rm", "call1", "{}").await;
        assert!(matches!(action, HookAction::Block { .. }));
    }

    #[tokio::test]
    async fn test_on_tool_call_allowed() {
        let hook = make_hook();
        let action = hook.on_tool_call("bash", "call1", "{}").await;
        assert!(matches!(action, HookAction::Continue));
    }

    #[tokio::test]
    async fn test_on_completion_response() {
        let hook = make_hook();
        let action = hook.on_completion_response("Hello!").await;
        assert!(matches!(action, HookAction::Continue));
    }
}
