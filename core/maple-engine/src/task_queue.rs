use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub task_type: String,
    pub priority: i32,
    pub payload: serde_json::Value,
    pub status: TaskStatus,
    pub retry_count: i32,
    pub max_retries: i32,
    pub next_run_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub error_message: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    DeadLetter,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Running => "running",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
            TaskStatus::DeadLetter => "dead_letter",
        }
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "pending" => TaskStatus::Pending,
            "running" => TaskStatus::Running,
            "completed" => TaskStatus::Completed,
            "failed" => TaskStatus::Failed,
            "dead_letter" => TaskStatus::DeadLetter,
            _ => TaskStatus::Pending,
        })
    }
}

pub struct TaskQueueService {
    pool: SqlitePool,
    notify: Arc<Notify>,
    retry_backoff_base_secs: i64,
}

impl TaskQueueService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            notify: Arc::new(Notify::new()),
            retry_backoff_base_secs: 30,
        }
    }

    pub fn with_backoff(pool: SqlitePool, backoff_secs: i64) -> Self {
        Self {
            pool,
            notify: Arc::new(Notify::new()),
            retry_backoff_base_secs: backoff_secs,
        }
    }

    pub async fn init_schema(&self) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS task_queue (
                id TEXT PRIMARY KEY,
                task_type TEXT NOT NULL,
                priority INTEGER NOT NULL DEFAULT 0,
                payload TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                retry_count INTEGER NOT NULL DEFAULT 0,
                max_retries INTEGER NOT NULL DEFAULT 3,
                next_run_at INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                error_message TEXT,
                agent_id TEXT
            )"
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_task_queue_status_priority
             ON task_queue(status, priority DESC, next_run_at)"
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_task_queue_type
             ON task_queue(task_type, status)"
        )
        .execute(&self.pool)
        .await?;

        tracing::info!("Task queue schema initialized");
        Ok(())
    }

    pub async fn enqueue(
        &self,
        task_type: &str,
        priority: i32,
        payload: serde_json::Value,
        max_retries: i32,
        delay_secs: i64,
        agent_id: Option<&str>,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        let next_run_at = now + delay_secs;

        sqlx::query(
            "INSERT INTO task_queue (id, task_type, priority, payload, status, retry_count, max_retries, next_run_at, created_at, updated_at, agent_id)
             VALUES (?, ?, ?, ?, 'pending', 0, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(task_type)
        .bind(priority)
        .bind(serde_json::to_string(&payload)?)
        .bind(max_retries)
        .bind(next_run_at)
        .bind(now)
        .bind(now)
        .bind(agent_id)
        .execute(&self.pool)
        .await?;

        tracing::debug!("Task enqueued: id={}, type={}, priority={}", id, task_type, priority);
        self.notify.notify_one();
        Ok(id)
    }

    pub async fn dequeue(&self) -> Result<Option<Task>> {
        let now = chrono::Utc::now().timestamp();

        let row = sqlx::query_as::<_, (String, String, i32, String, String, i32, i32, i64, i64, i64, Option<String>, Option<String>)>(
            "SELECT id, task_type, priority, payload, status, retry_count, max_retries, next_run_at, created_at, updated_at, error_message, agent_id
             FROM task_queue
             WHERE status = 'pending' AND next_run_at <= ?
             ORDER BY priority DESC, next_run_at ASC
             LIMIT 1"
        )
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;

        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        let task_id = row.0.clone();
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "UPDATE task_queue SET status = 'running', updated_at = ? WHERE id = ? AND status = 'pending'"
        )
        .bind(now)
        .bind(&task_id)
        .execute(&self.pool)
        .await?;

        Ok(Some(Task {
            id: row.0,
            task_type: row.1,
            priority: row.2,
            payload: serde_json::from_str(&row.3)?,
            status: TaskStatus::Running,
            retry_count: row.5,
            max_retries: row.6,
            next_run_at: row.7,
            created_at: row.8,
            updated_at: now,
            error_message: row.10,
            agent_id: row.11,
        }))
    }

    pub async fn complete(&self, task_id: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "UPDATE task_queue SET status = 'completed', updated_at = ? WHERE id = ?"
        )
        .bind(now)
        .bind(task_id)
        .execute(&self.pool)
        .await?;

        tracing::debug!("Task completed: id={}", task_id);
        Ok(())
    }

    pub async fn fail(&self, task_id: &str, error: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();

        let task = self.get_by_id(task_id).await?;

        let task = match task {
            Some(t) => t,
            None => return Ok(()),
        };

        if task.retry_count >= task.max_retries {
            sqlx::query(
                "UPDATE task_queue SET status = 'dead_letter', error_message = ?, updated_at = ? WHERE id = ?"
            )
            .bind(error)
            .bind(now)
            .bind(task_id)
            .execute(&self.pool)
            .await?;

            tracing::warn!("Task moved to dead letter queue: id={}, retries={}", task_id, task.retry_count);
        } else {
            let new_retry_count = task.retry_count + 1;
            let backoff = self.retry_backoff_base_secs * 2i64.pow(new_retry_count as u32);
            let next_run_at = now + backoff;

            sqlx::query(
                "UPDATE task_queue SET status = 'pending', retry_count = ?, error_message = ?, next_run_at = ?, updated_at = ? WHERE id = ?"
            )
            .bind(new_retry_count)
            .bind(error)
            .bind(next_run_at)
            .bind(now)
            .bind(task_id)
            .execute(&self.pool)
            .await?;

            tracing::debug!("Task retry scheduled: id={}, retry={}, next_run_at={}", task_id, new_retry_count, next_run_at);
            self.notify.notify_one();
        }

        Ok(())
    }

    pub async fn get_by_id(&self, task_id: &str) -> Result<Option<Task>> {
        let row = sqlx::query_as::<_, (String, String, i32, String, String, i32, i32, i64, i64, i64, Option<String>, Option<String>)>(
            "SELECT id, task_type, priority, payload, status, retry_count, max_retries, next_run_at, created_at, updated_at, error_message, agent_id
             FROM task_queue WHERE id = ?"
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Task {
            id: r.0,
            task_type: r.1,
            priority: r.2,
            payload: serde_json::from_str(&r.3).unwrap_or(serde_json::Value::Null),
            status: r.4.parse::<TaskStatus>().unwrap_or(TaskStatus::Pending),
            retry_count: r.5,
            max_retries: r.6,
            next_run_at: r.7,
            created_at: r.8,
            updated_at: r.9,
            error_message: r.10,
            agent_id: r.11,
        }))
    }

    pub async fn list_pending(&self, limit: i32) -> Result<Vec<Task>> {
        let now = chrono::Utc::now().timestamp();
        let rows = sqlx::query_as::<_, (String, String, i32, String, String, i32, i32, i64, i64, i64, Option<String>, Option<String>)>(
            "SELECT id, task_type, priority, payload, status, retry_count, max_retries, next_run_at, created_at, updated_at, error_message, agent_id
             FROM task_queue
             WHERE status = 'pending' AND next_run_at <= ?
             ORDER BY priority DESC, next_run_at ASC
             LIMIT ?"
        )
        .bind(now)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| Task {
            id: r.0,
            task_type: r.1,
            priority: r.2,
            payload: serde_json::from_str(&r.3).unwrap_or(serde_json::Value::Null),
            status: TaskStatus::Pending,
            retry_count: r.5,
            max_retries: r.6,
            next_run_at: r.7,
            created_at: r.8,
            updated_at: r.9,
            error_message: r.10,
            agent_id: r.11,
        }).collect())
    }

    pub async fn list_dead_letter(&self, limit: i32) -> Result<Vec<Task>> {
        let rows = sqlx::query_as::<_, (String, String, i32, String, String, i32, i32, i64, i64, i64, Option<String>, Option<String>)>(
            "SELECT id, task_type, priority, payload, status, retry_count, max_retries, next_run_at, created_at, updated_at, error_message, agent_id
             FROM task_queue
             WHERE status = 'dead_letter'
             ORDER BY updated_at DESC
             LIMIT ?"
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| Task {
            id: r.0,
            task_type: r.1,
            priority: r.2,
            payload: serde_json::from_str(&r.3).unwrap_or(serde_json::Value::Null),
            status: TaskStatus::DeadLetter,
            retry_count: r.5,
            max_retries: r.6,
            next_run_at: r.7,
            created_at: r.8,
            updated_at: r.9,
            error_message: r.10,
            agent_id: r.11,
        }).collect())
    }

    pub async fn requeue_dead_letter(&self, task_id: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "UPDATE task_queue SET status = 'pending', retry_count = 0, error_message = NULL, next_run_at = ?, updated_at = ? WHERE id = ? AND status = 'dead_letter'"
        )
        .bind(now)
        .bind(now)
        .bind(task_id)
        .execute(&self.pool)
        .await?;

        self.notify.notify_one();
        Ok(())
    }

    pub async fn stats(&self) -> Result<TaskQueueStats> {
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM task_queue WHERE status = 'pending'"
        )
        .fetch_one(&self.pool)
        .await?;

        let running: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM task_queue WHERE status = 'running'"
        )
        .fetch_one(&self.pool)
        .await?;

        let completed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM task_queue WHERE status = 'completed'"
        )
        .fetch_one(&self.pool)
        .await?;

        let failed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM task_queue WHERE status = 'failed'"
        )
        .fetch_one(&self.pool)
        .await?;

        let dead_letter: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM task_queue WHERE status = 'dead_letter'"
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(TaskQueueStats {
            pending,
            running,
            completed,
            failed,
            dead_letter,
        })
    }

    pub fn notify(&self) -> Arc<Notify> {
        self.notify.clone()
    }
}

#[derive(Debug, Serialize)]
pub struct TaskQueueStats {
    pub pending: i64,
    pub running: i64,
    pub completed: i64,
    pub failed: i64,
    pub dead_letter: i64,
}