//! Execution fact chain — unified recorder for all execution entries.
//!
//! See `docs/execution-fact-chain-spec.md` for the authoritative contract.
//!
//! Every execution entry point (chat send, workflow run, agent run, task
//! dequeue, approval create) MUST call [`ExecutionRecorder::start`] to obtain
//! an `execution_id`, then [`ExecutionRecorder::append`] for each event, and
//! finally [`ExecutionRecorder::done`] / [`ExecutionRecorder::fail`] /
//! [`ExecutionRecorder::cancel`] to close the execution.
//!
//! UI panels read from the same event chain via [`ExecutionRecorder::list_events`]
//! — they MUST NOT maintain private trace state.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Valid values for `execution_events.source`.
pub const SOURCES: &[&str] = &[
    "chat",
    "workflow",
    "task",
    "approval",
    "agent",
    "tool",
    "scheduler",
    "system",
];

/// Valid values for `execution_events.event_type`.
pub const EVENT_TYPES: &[&str] = &[
    "started",
    "delta",
    "tool_call",
    "tool_result",
    "node_started",
    "node_finished",
    "artifact",
    "usage",
    "approval_requested",
    "approval_decided",
    "retry",
    "cancelled",
    "resumed",
    "paused",
    "done",
    "error",
];

/// Valid values for `executions.status`.
pub const EXECUTION_STATUSES: &[&str] = &[
    "pending",
    "running",
    "paused",
    "success",
    "failed",
    "cancelled",
];

/// One row from `execution_events`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionEvent {
    pub id: String,
    pub execution_id: String,
    pub parent_execution_id: Option<String>,
    pub source: String,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub actor: Option<String>,
    pub actor_type: Option<String>,
    pub created_at: i64,
}

/// One row from `executions` (aggregate view).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Execution {
    pub id: String,
    pub parent_execution_id: Option<String>,
    pub source: String,
    pub status: String,
    pub actor: Option<String>,
    pub actor_type: Option<String>,
    pub trigger_type: Option<String>,
    pub trigger_payload: Option<serde_json::Value>,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub error: Option<String>,
    pub event_count: i64,
    pub updated_at: i64,
}

/// Unified recorder for the execution fact chain.
///
/// Thread-safe and cheap to clone (internally `Arc`).
#[derive(Clone)]
pub struct ExecutionRecorder {
    pool: SqlitePool,
}

impl ExecutionRecorder {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Start a new execution. Returns the freshly-generated `execution_id`.
    ///
    /// Writes one row to `executions` (status='running') and one `started`
    /// event to `execution_events`.
    pub async fn start(
        &self,
        source: &str,
        actor: Option<&str>,
        actor_type: Option<&str>,
        trigger_type: &str,
        trigger_payload: serde_json::Value,
        parent_execution_id: Option<&str>,
    ) -> Result<String> {
        Self::validate_source(source)?;
        let execution_id = format!("exec_{}", Uuid::new_v4().simple());
        let event_id = format!("evt_{}", Uuid::new_v4().simple());
        let now = chrono::Utc::now().timestamp();

        // Insert aggregate row
        sqlx::query(
            "INSERT INTO executions
                (id, parent_execution_id, source, status, actor, actor_type,
                 trigger_type, trigger_payload, started_at, completed_at,
                 error, event_count, updated_at)
             VALUES (?, ?, ?, 'running', ?, ?, ?, ?, ?, NULL, NULL, 1, ?)",
        )
        .bind(&execution_id)
        .bind(parent_execution_id)
        .bind(source)
        .bind(actor)
        .bind(actor_type)
        .bind(trigger_type)
        .bind(trigger_payload.to_string())
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        // Insert started event
        let payload = serde_json::json!({
            "entry": source,
            "trigger": trigger_type,
            "trigger_payload": trigger_payload,
        });
        sqlx::query(
            "INSERT INTO execution_events
                (id, execution_id, parent_execution_id, source, event_type,
                 payload, actor, actor_type, created_at)
             VALUES (?, ?, ?, ?, 'started', ?, ?, ?, ?)",
        )
        .bind(&event_id)
        .bind(&execution_id)
        .bind(parent_execution_id)
        .bind(source)
        .bind(payload.to_string())
        .bind(actor)
        .bind(actor_type)
        .bind(now)
        .execute(&self.pool)
        .await?;

        tracing::debug!(
            execution_id = %execution_id,
            source = %source,
            trigger = %trigger_type,
            "execution started"
        );

        Ok(execution_id)
    }

    /// Append an event to an existing execution.
    ///
    /// `payload` shape MUST conform to the schema for `event_type` (see
    /// `docs/execution-fact-chain-spec.md` §3). The recorder does not validate
    /// the payload shape at runtime in production builds — callers are
    /// responsible. In debug builds a string-length sanity check is applied.
    pub async fn append(
        &self,
        execution_id: &str,
        source: &str,
        event_type: &str,
        payload: serde_json::Value,
        actor: Option<&str>,
        actor_type: Option<&str>,
    ) -> Result<String> {
        Self::validate_source(source)?;
        Self::validate_event_type(event_type)?;
        Self::validate_payload_size(&payload)?;

        let event_id = format!("evt_{}", Uuid::new_v4().simple());
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO execution_events
                (id, execution_id, parent_execution_id, source, event_type,
                 payload, actor, actor_type, created_at)
             VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&event_id)
        .bind(execution_id)
        .bind(source)
        .bind(event_type)
        .bind(payload.to_string())
        .bind(actor)
        .bind(actor_type)
        .bind(now)
        .execute(&self.pool)
        .await?;

        // Bump event_count + updated_at on the aggregate row.
        sqlx::query(
            "UPDATE executions
                SET event_count = event_count + 1,
                    updated_at = ?
              WHERE id = ?",
        )
        .bind(now)
        .bind(execution_id)
        .execute(&self.pool)
        .await?;

        Ok(event_id)
    }

    /// Mark the execution as successfully completed.
    pub async fn done(&self, execution_id: &str, output_summary: &str) -> Result<()> {
        let event_id = format!("evt_{}", Uuid::new_v4().simple());
        let now = chrono::Utc::now().timestamp();

        let payload = serde_json::json!({
            "output_summary": output_summary,
        });

        sqlx::query(
            "INSERT INTO execution_events
                (id, execution_id, parent_execution_id, source, event_type,
                 payload, actor, actor_type, created_at)
             VALUES (?, ?, NULL, 'system', 'done', ?, NULL, 'system', ?)",
        )
        .bind(&event_id)
        .bind(execution_id)
        .bind(payload.to_string())
        .bind(now)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "UPDATE executions
                SET status = 'success',
                    completed_at = ?,
                    updated_at = ?,
                    event_count = event_count + 1
              WHERE id = ?",
        )
        .bind(now)
        .bind(now)
        .bind(execution_id)
        .execute(&self.pool)
        .await?;

        tracing::debug!(execution_id = %execution_id, "execution done");
        Ok(())
    }

    /// Mark the execution as failed.
    pub async fn fail(
        &self,
        execution_id: &str,
        error: &str,
        recoverable: bool,
    ) -> Result<()> {
        let event_id = format!("evt_{}", Uuid::new_v4().simple());
        let now = chrono::Utc::now().timestamp();

        let payload = serde_json::json!({
            "error_type": "generic",
            "message": error,
            "stack": null,
            "recoverable": recoverable,
        });

        sqlx::query(
            "INSERT INTO execution_events
                (id, execution_id, parent_execution_id, source, event_type,
                 payload, actor, actor_type, created_at)
             VALUES (?, ?, NULL, 'system', 'error', ?, NULL, 'system', ?)",
        )
        .bind(&event_id)
        .bind(execution_id)
        .bind(payload.to_string())
        .bind(now)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "UPDATE executions
                SET status = 'failed',
                    completed_at = ?,
                    updated_at = ?,
                    error = ?,
                    event_count = event_count + 1
              WHERE id = ?",
        )
        .bind(now)
        .bind(now)
        .bind(error)
        .bind(execution_id)
        .execute(&self.pool)
        .await?;

        tracing::warn!(
            execution_id = %execution_id,
            error = %error,
            recoverable,
            "execution failed"
        );
        Ok(())
    }

    /// Mark the execution as cancelled.
    pub async fn cancel(&self, execution_id: &str, actor: &str, reason: &str) -> Result<()> {
        let event_id = format!("evt_{}", Uuid::new_v4().simple());
        let now = chrono::Utc::now().timestamp();

        let payload = serde_json::json!({
            "reason": reason,
            "actor": actor,
        });

        sqlx::query(
            "INSERT INTO execution_events
                (id, execution_id, parent_execution_id, source, event_type,
                 payload, actor, actor_type, created_at)
             VALUES (?, ?, NULL, 'system', 'cancelled', ?, ?, 'human', ?)",
        )
        .bind(&event_id)
        .bind(execution_id)
        .bind(payload.to_string())
        .bind(actor)
        .bind(now)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "UPDATE executions
                SET status = 'cancelled',
                    completed_at = ?,
                    updated_at = ?,
                    event_count = event_count + 1
              WHERE id = ?",
        )
        .bind(now)
        .bind(now)
        .bind(execution_id)
        .execute(&self.pool)
        .await?;

        tracing::info!(
            execution_id = %execution_id,
            actor = %actor,
            reason = %reason,
            "execution cancelled"
        );
        Ok(())
    }

    /// Pause the execution (e.g. waiting for approval).
    pub async fn pause(
        &self,
        execution_id: &str,
        reason: &str,
        resume_token: Option<&str>,
    ) -> Result<()> {
        let payload = serde_json::json!({
            "reason": reason,
            "resume_token": resume_token,
        });
        self.append(execution_id, "system", "paused", payload, None, None)
            .await?;

        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "UPDATE executions SET status = 'paused', updated_at = ? WHERE id = ?",
        )
        .bind(now)
        .bind(execution_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Resume a paused execution.
    pub async fn resume(
        &self,
        execution_id: &str,
        reason: &str,
        actor: Option<&str>,
    ) -> Result<()> {
        let payload = serde_json::json!({
            "reason": reason,
            "actor": actor,
        });
        self.append(
            execution_id,
            "system",
            "resumed",
            payload,
            actor,
            actor.map(|_| "human"),
        )
        .await?;

        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "UPDATE executions SET status = 'running', updated_at = ? WHERE id = ?",
        )
        .bind(now)
        .bind(execution_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ============================================================
    // Query API (used by HTTP handlers)
    // ============================================================

    /// Get the aggregate view of an execution.
    pub async fn get_execution(&self, execution_id: &str) -> Result<Option<Execution>> {
        let row = sqlx::query_as::<_, ExecutionRow>(
            "SELECT id, parent_execution_id, source, status, actor, actor_type,
                    trigger_type, trigger_payload, started_at, completed_at,
                    error, event_count, updated_at
               FROM executions
              WHERE id = ?",
        )
        .bind(execution_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    /// List all events for an execution, in chronological order.
    pub async fn list_events(&self, execution_id: &str) -> Result<Vec<ExecutionEvent>> {
        let rows = sqlx::query_as::<_, EventRow>(
            "SELECT id, execution_id, parent_execution_id, source, event_type,
                    payload, actor, actor_type, created_at
               FROM execution_events
              WHERE execution_id = ?
              ORDER BY created_at ASC, rowid ASC",
        )
        .bind(execution_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }
}

// ============================================================
// Internal helpers
// ============================================================

impl ExecutionRecorder {
    fn validate_source(source: &str) -> Result<()> {
        if !SOURCES.contains(&source) {
            anyhow::bail!("invalid source '{source}'; must be one of {SOURCES:?}");
        }
        Ok(())
    }

    fn validate_event_type(event_type: &str) -> Result<()> {
        if !EVENT_TYPES.contains(&event_type) {
            anyhow::bail!("invalid event_type '{event_type}'; must be one of {EVENT_TYPES:?}");
        }
        Ok(())
    }

    fn validate_payload_size(payload: &serde_json::Value) -> Result<()> {
        // Reject payloads larger than 64KB to prevent event bloat — see
        // spec §13 "anti-patterns: payload > 64KB".
        let serialized = payload.to_string();
        if serialized.len() > 65_536 {
            anyhow::bail!(
                "execution event payload too large: {} bytes (max 65536); \
                 use an artifact reference instead",
                serialized.len()
            );
        }
        Ok(())
    }
}

// ============================================================
// sqlx row types (intermediate, convert to public types)
// ============================================================

#[derive(sqlx::FromRow)]
struct EventRow {
    id: String,
    execution_id: String,
    parent_execution_id: Option<String>,
    source: String,
    event_type: String,
    payload: String,
    actor: Option<String>,
    actor_type: Option<String>,
    created_at: i64,
}

impl From<EventRow> for ExecutionEvent {
    fn from(r: EventRow) -> Self {
        let payload: serde_json::Value =
            serde_json::from_str(&r.payload).unwrap_or(serde_json::Value::Null);
        Self {
            id: r.id,
            execution_id: r.execution_id,
            parent_execution_id: r.parent_execution_id,
            source: r.source,
            event_type: r.event_type,
            payload,
            actor: r.actor,
            actor_type: r.actor_type,
            created_at: r.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ExecutionRow {
    id: String,
    parent_execution_id: Option<String>,
    source: String,
    status: String,
    actor: Option<String>,
    actor_type: Option<String>,
    trigger_type: Option<String>,
    trigger_payload: Option<String>,
    started_at: i64,
    completed_at: Option<i64>,
    error: Option<String>,
    event_count: i64,
    updated_at: i64,
}

impl From<ExecutionRow> for Execution {
    fn from(r: ExecutionRow) -> Self {
        let trigger_payload = r
            .trigger_payload
            .and_then(|s| serde_json::from_str(&s).ok());
        Self {
            id: r.id,
            parent_execution_id: r.parent_execution_id,
            source: r.source,
            status: r.status,
            actor: r.actor,
            actor_type: r.actor_type,
            trigger_type: r.trigger_type,
            trigger_payload,
            started_at: r.started_at,
            completed_at: r.completed_at,
            error: r.error,
            event_count: r.event_count,
            updated_at: r.updated_at,
        }
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        // Run the migration inline (mirrors db.rs::run_v3_migration_014 + 015)
        sqlx::query(
            "CREATE TABLE execution_events (
                id TEXT PRIMARY KEY,
                execution_id TEXT NOT NULL,
                parent_execution_id TEXT,
                source TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                actor TEXT,
                actor_type TEXT,
                created_at INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE executions (
                id TEXT PRIMARY KEY,
                parent_execution_id TEXT,
                source TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'running',
                actor TEXT,
                actor_type TEXT,
                trigger_type TEXT,
                trigger_payload TEXT,
                started_at INTEGER NOT NULL,
                completed_at INTEGER,
                error TEXT,
                event_count INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn start_creates_execution_and_started_event() {
        let pool = setup_pool().await;
        let rec = ExecutionRecorder::new(pool);

        let exec_id = rec
            .start(
                "chat",
                Some("user_1"),
                Some("human"),
                "manual",
                serde_json::json!({"message": "hi"}),
                None,
            )
            .await
            .unwrap();

        assert!(exec_id.starts_with("exec_"));

        let exec = rec.get_execution(&exec_id).await.unwrap().unwrap();
        assert_eq!(exec.source, "chat");
        assert_eq!(exec.status, "running");
        assert_eq!(exec.actor.as_deref(), Some("user_1"));
        assert_eq!(exec.event_count, 1);

        let events = rec.list_events(&exec_id).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "started");
        assert_eq!(events[0].source, "chat");
    }

    #[tokio::test]
    async fn append_increments_event_count() {
        let pool = setup_pool().await;
        let rec = ExecutionRecorder::new(pool);

        let exec_id = rec
            .start("workflow", None, None, "manual", serde_json::json!({}), None)
            .await
            .unwrap();

        rec.append(
            &exec_id,
            "workflow",
            "node_started",
            serde_json::json!({"node_id": "n1"}),
            None,
            None,
        )
        .await
        .unwrap();

        rec.append(
            &exec_id,
            "workflow",
            "node_finished",
            serde_json::json!({"node_id": "n1", "status": "success"}),
            None,
            None,
        )
        .await
        .unwrap();

        let exec = rec.get_execution(&exec_id).await.unwrap().unwrap();
        assert_eq!(exec.event_count, 3); // started + 2 appends

        let events = rec.list_events(&exec_id).await.unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event_type, "started");
        assert_eq!(events[1].event_type, "node_started");
        assert_eq!(events[2].event_type, "node_finished");
    }

    #[tokio::test]
    async fn done_sets_status_success() {
        let pool = setup_pool().await;
        let rec = ExecutionRecorder::new(pool);

        let exec_id = rec
            .start("chat", Some("u1"), Some("human"), "manual", serde_json::json!({}), None)
            .await
            .unwrap();

        rec.done(&exec_id, "all good").await.unwrap();

        let exec = rec.get_execution(&exec_id).await.unwrap().unwrap();
        assert_eq!(exec.status, "success");
        assert!(exec.completed_at.is_some());

        let events = rec.list_events(&exec_id).await.unwrap();
        let last = events.last().unwrap();
        assert_eq!(last.event_type, "done");
        assert_eq!(last.source, "system");
    }

    #[tokio::test]
    async fn fail_sets_status_failed_and_error() {
        let pool = setup_pool().await;
        let rec = ExecutionRecorder::new(pool);

        let exec_id = rec
            .start("task", None, None, "cron", serde_json::json!({}), None)
            .await
            .unwrap();

        rec.fail(&exec_id, "tool timeout", true).await.unwrap();

        let exec = rec.get_execution(&exec_id).await.unwrap().unwrap();
        assert_eq!(exec.status, "failed");
        assert_eq!(exec.error.as_deref(), Some("tool timeout"));

        let events = rec.list_events(&exec_id).await.unwrap();
        let last = events.last().unwrap();
        assert_eq!(last.event_type, "error");
        let payload = &last.payload;
        assert_eq!(payload["recoverable"], true);
        assert_eq!(payload["message"], "tool timeout");
    }

    #[tokio::test]
    async fn cancel_sets_status_cancelled() {
        let pool = setup_pool().await;
        let rec = ExecutionRecorder::new(pool);

        let exec_id = rec
            .start("workflow", Some("u1"), Some("human"), "manual", serde_json::json!({}), None)
            .await
            .unwrap();

        rec.cancel(&exec_id, "u1", "user cancelled").await.unwrap();

        let exec = rec.get_execution(&exec_id).await.unwrap().unwrap();
        assert_eq!(exec.status, "cancelled");
    }

    #[tokio::test]
    async fn pause_and_resume_transition_status() {
        let pool = setup_pool().await;
        let rec = ExecutionRecorder::new(pool);

        let exec_id = rec
            .start("workflow", None, None, "manual", serde_json::json!({}), None)
            .await
            .unwrap();

        rec.pause(&exec_id, "waiting_approval", Some("tok123"))
            .await
            .unwrap();
        let paused = rec.get_execution(&exec_id).await.unwrap().unwrap();
        assert_eq!(paused.status, "paused");

        rec.resume(&exec_id, "approval_granted", Some("u1"))
            .await
            .unwrap();
        let resumed = rec.get_execution(&exec_id).await.unwrap().unwrap();
        assert_eq!(resumed.status, "running");

        let events = rec.list_events(&exec_id).await.unwrap();
        let types: Vec<_> = events.iter().map(|e| e.event_type.as_str()).collect();
        assert!(types.contains(&"paused"));
        assert!(types.contains(&"resumed"));
    }

    #[tokio::test]
    async fn invalid_source_rejected() {
        let pool = setup_pool().await;
        let rec = ExecutionRecorder::new(pool);

        let err = rec
            .start(
                "invalid_source",
                None,
                None,
                "manual",
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid source"));
    }

    #[tokio::test]
    async fn invalid_event_type_rejected() {
        let pool = setup_pool().await;
        let rec = ExecutionRecorder::new(pool);

        let exec_id = rec
            .start("chat", None, None, "manual", serde_json::json!({}), None)
            .await
            .unwrap();

        let err = rec
            .append(
                &exec_id,
                "chat",
                "totally_made_up_event",
                serde_json::json!({}),
                None,
                None,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid event_type"));
    }

    #[tokio::test]
    async fn oversized_payload_rejected() {
        let pool = setup_pool().await;
        let rec = ExecutionRecorder::new(pool);

        let exec_id = rec
            .start("chat", None, None, "manual", serde_json::json!({}), None)
            .await
            .unwrap();

        // Build a payload > 64KB
        let big_string = "x".repeat(70_000);
        let err = rec
            .append(
                &exec_id,
                "chat",
                "delta",
                serde_json::json!({"token": big_string}),
                None,
                None,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("payload too large"));
    }

    #[tokio::test]
    async fn parent_execution_id_links_nested_executions() {
        let pool = setup_pool().await;
        let rec = ExecutionRecorder::new(pool);

        let parent_id = rec
            .start("workflow", Some("u1"), Some("human"), "manual", serde_json::json!({}), None)
            .await
            .unwrap();

        let child_id = rec
            .start(
                "agent",
                Some("agent_1"),
                Some("agent"),
                "delegated",
                serde_json::json!({"goal": "subtask"}),
                Some(&parent_id),
            )
            .await
            .unwrap();

        let child = rec.get_execution(&child_id).await.unwrap().unwrap();
        assert_eq!(child.parent_execution_id.as_deref(), Some(parent_id.as_str()));
        assert_eq!(child.source, "agent");

        let child_events = rec.list_events(&child_id).await.unwrap();
        assert_eq!(child_events[0].parent_execution_id.as_deref(), Some(parent_id.as_str()));
    }
}
