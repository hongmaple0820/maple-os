use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{SqlitePool, Row};
use std::sync::Arc;
use maple_engine::event_bus::{EventBus, Event};
use maple_engine::scheduler::{Scheduler, ScheduledJob, next_timestamp_from_cron};

/// Group-level cron job stored in SQLite
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupCronJob {
    pub id: String,
    pub group_id: String,
    pub name: String,
    pub cron_expr: String,
    pub message_template: String,
    pub job_type: String,
    pub target_agent_id: Option<String>,
    pub enabled: bool,
    pub created_by: String,
    pub created_at: i64,
    pub last_run_at: Option<i64>,
    pub next_run_at: i64,
    pub run_count: i32,
}

#[derive(Debug, Deserialize)]
pub struct CreateCronJobRequest {
    pub name: String,
    pub cron_expr: String,
    pub message_template: String,
    pub job_type: Option<String>,
    pub target_agent_id: Option<String>,
    pub enabled: Option<bool>,
}

pub struct GroupCronService {
    pool: SqlitePool,
    scheduler: Arc<Scheduler>,
    event_bus: Arc<EventBus>,
}

impl GroupCronService {
    pub fn new(pool: SqlitePool, scheduler: Arc<Scheduler>, event_bus: Arc<EventBus>) -> Self {
        Self { pool, scheduler, event_bus }
    }

    /// Initialize: load all enabled jobs into the scheduler
    pub async fn init(&self) -> Result<()> {
        let rows = sqlx::query(
            "SELECT id, group_id, name, cron_expr, message_template, job_type,
                    target_agent_id, enabled, created_by, created_at, last_run_at,
                    next_run_at, run_count
             FROM group_cron_jobs WHERE enabled = 1"
        )
        .fetch_all(&self.pool)
        .await?;

        for row in rows {
            let next_run: i64 = row.get("next_run_at");
            let job = ScheduledJob {
                id: row.get::<String, _>("id"),
                workflow_id: row.get::<String, _>("group_id"),
                cron_expr: row.get::<String, _>("cron_expr"),
                timezone: "UTC".to_string(),
                last_run_at: row.get("last_run_at"),
                next_run_at: next_run,
                enabled: true,
            };
            let _ = self.scheduler.add_job(job).await;
        }

        tracing::info!("GroupCronService initialized");
        Ok(())
    }

    /// Create a new group cron job
    pub async fn create_job(
        &self,
        group_id: &str,
        created_by: &str,
        req: CreateCronJobRequest,
    ) -> Result<GroupCronJob> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        let next_run = next_timestamp_from_cron(&req.cron_expr, now)?;
        let job_type = req.job_type.unwrap_or_else(|| "system_broadcast".to_string());
        let enabled = req.enabled.unwrap_or(true);

        sqlx::query(
            "INSERT INTO group_cron_jobs
             (id, group_id, name, cron_expr, message_template, job_type,
              target_agent_id, enabled, created_by, created_at, next_run_at, run_count)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)"
        )
        .bind(&id)
        .bind(group_id)
        .bind(&req.name)
        .bind(&req.cron_expr)
        .bind(&req.message_template)
        .bind(&job_type)
        .bind(&req.target_agent_id)
        .bind(enabled)
        .bind(created_by)
        .bind(now)
        .bind(next_run)
        .execute(&self.pool)
        .await?;

        if enabled {
            self.scheduler.add_job(ScheduledJob {
                id: id.clone(),
                workflow_id: group_id.to_string(),
                cron_expr: req.cron_expr.clone(),
                timezone: "UTC".to_string(),
                last_run_at: None,
                next_run_at: next_run,
                enabled: true,
            }).await?;
        }

        Ok(GroupCronJob {
            id, group_id: group_id.to_string(), name: req.name,
            cron_expr: req.cron_expr, message_template: req.message_template,
            job_type, target_agent_id: req.target_agent_id, enabled,
            created_by: created_by.to_string(), created_at: now,
            last_run_at: None, next_run_at: next_run, run_count: 0,
        })
    }

    /// List all cron jobs for a group
    pub async fn list_jobs(&self, group_id: &str) -> Result<Vec<GroupCronJob>> {
        let rows = sqlx::query(
            "SELECT id, group_id, name, cron_expr, message_template, job_type,
                    target_agent_id, enabled, created_by, created_at, last_run_at,
                    next_run_at, run_count
             FROM group_cron_jobs WHERE group_id = ? ORDER BY created_at DESC"
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| GroupCronJob {
            id: r.get("id"), group_id: r.get("group_id"), name: r.get("name"),
            cron_expr: r.get("cron_expr"), message_template: r.get("message_template"),
            job_type: r.get("job_type"), target_agent_id: r.get("target_agent_id"),
            enabled: r.get("enabled"), created_by: r.get("created_by"),
            created_at: r.get("created_at"), last_run_at: r.get("last_run_at"),
            next_run_at: r.get("next_run_at"), run_count: r.get("run_count"),
        }).collect())
    }

    /// Update a cron job
    pub async fn update_job(
        &self, job_id: &str,
        name: Option<&str>, cron_expr: Option<&str>,
        message_template: Option<&str>, enabled: Option<bool>,
    ) -> Result<bool> {
        let existing = sqlx::query("SELECT id, cron_expr, enabled FROM group_cron_jobs WHERE id = ?")
            .bind(job_id).fetch_optional(&self.pool).await?;
        let Some(row) = existing else { return Ok(false) };

        let old_cron: String = row.get("cron_expr");
        let old_enabled: bool = row.get("enabled");
        let new_cron = cron_expr.unwrap_or(&old_cron);
        let new_enabled = enabled.unwrap_or(old_enabled);
        let now = chrono::Utc::now().timestamp();
        let next_run = if cron_expr.is_some() {
            next_timestamp_from_cron(new_cron, now)?
        } else {
            sqlx::query("SELECT next_run_at FROM group_cron_jobs WHERE id = ?")
                .bind(job_id).fetch_one(&self.pool).await?
                .get::<i64, _>("next_run_at")
        };

        sqlx::query(
            "UPDATE group_cron_jobs SET name = COALESCE(?, name), cron_expr = COALESCE(?, cron_expr),
             message_template = COALESCE(?, message_template), enabled = COALESCE(?, enabled),
             next_run_at = ? WHERE id = ?"
        ).bind(name).bind(cron_expr).bind(message_template).bind(enabled)
         .bind(next_run).bind(job_id).execute(&self.pool).await?;

        self.scheduler.remove_job(job_id).await?;
        if new_enabled {
            let group_id: String = sqlx::query("SELECT group_id FROM group_cron_jobs WHERE id = ?")
                .bind(job_id).fetch_one(&self.pool).await?.get("group_id");
            self.scheduler.add_job(ScheduledJob {
                id: job_id.to_string(), workflow_id: group_id,
                cron_expr: new_cron.to_string(), timezone: "UTC".to_string(),
                last_run_at: None, next_run_at: next_run, enabled: true,
            }).await?;
        }
        Ok(true)
    }

    /// Delete a cron job
    pub async fn delete_job(&self, job_id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM group_cron_jobs WHERE id = ?")
            .bind(job_id).execute(&self.pool).await?;
        self.scheduler.remove_job(job_id).await?;
        Ok(result.rows_affected() > 0)
    }

    /// Execute a fired cron job — publish message to group
    pub async fn execute_job(&self, job: &ScheduledJob) -> Result<()> {
        let group_id = &job.workflow_id;
        let now = chrono::Utc::now().timestamp();

        let row = sqlx::query(
            "SELECT message_template, job_type, target_agent_id FROM group_cron_jobs WHERE id = ?"
        ).bind(&job.id).fetch_optional(&self.pool).await?;
        let Some(row) = row else { return Ok(()) };

        let template: String = row.get("message_template");
        let job_type: String = row.get("job_type");
        let target_agent: Option<String> = row.get("target_agent_id");
        let sender_id = target_agent.as_deref().unwrap_or("system");

        self.event_bus.publish(Event::GroupMessageSent {
            group_id: group_id.clone(),
            message_id: uuid::Uuid::new_v4().to_string(),
            sender_id: sender_id.to_string(),
            content: serde_json::json!({
                "type": "cron_trigger",
                "cron_job_id": job.id,
                "job_type": job_type,
                "message": template,
                "target_agent_id": target_agent,
                "fired_at": now,
            }).to_string(),
        }).await;

        sqlx::query(
            "UPDATE group_cron_jobs SET last_run_at = ?, run_count = run_count + 1, next_run_at = ? WHERE id = ?"
        ).bind(now).bind(next_timestamp_from_cron(&job.cron_expr, now)?)
         .bind(&job.id).execute(&self.pool).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn test_cron_expr_parsing() {
        let now = 1717651200; // 2024-06-06 08:00:00 UTC
        let next = next_timestamp_from_cron("0 9 * * *", now).unwrap();
        let dt = chrono::DateTime::from_timestamp(next, 0).unwrap();
        assert_eq!(dt.hour(), 9);
        assert_eq!(dt.minute(), 0);
    }

    #[test]
    fn test_cron_expr_every_5_min() {
        let now = 1717651200;
        let next = next_timestamp_from_cron("*/5 * * * *", now).unwrap();
        let dt = chrono::DateTime::from_timestamp(next, 0).unwrap();
        assert_eq!(dt.minute() % 5, 0); // minute must be divisible by 5
        assert!(next > now); // must be in the future
    }
}
