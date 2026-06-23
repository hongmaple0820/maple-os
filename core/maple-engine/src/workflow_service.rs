use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDef {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub yaml_content: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: String,
    pub workflow_id: String,
    pub workflow_version: i64,
    pub status: String,
    pub input: Option<String>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub group_id: Option<String>,
    pub agent_id: Option<String>,
    pub started_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCheckpoint {
    pub id: i64,
    pub exec_id: String,
    pub node_id: String,
    pub output: String,
    pub context_snapshot: String,
    pub created_at: i64,
}

/// A saved version of a workflow definition (T2-2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowVersion {
    pub id: i64,
    pub workflow_id: String,
    pub version: i64,
    pub yaml_content: String,
    pub saved_by: Option<String>,
    pub change_summary: Option<String>,
    pub created_at: i64,
}

pub struct WorkflowService {
    pool: SqlitePool,
}

impl WorkflowService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // ── Workflow Definitions ──

    pub async fn create_definition(&self, id: &str, name: &str, yaml: &str) -> anyhow::Result<WorkflowDef> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO workflows (id, name, yaml_content, status, created_at, updated_at) VALUES (?, ?, ?, 'draft', ?, ?)"
        )
        .bind(id).bind(name).bind(yaml).bind(now).bind(now)
        .execute(&self.pool).await?;

        Ok(WorkflowDef {
            id: id.to_string(),
            name: name.to_string(),
            version: 1,
            yaml_content: yaml.to_string(),
            status: "draft".to_string(),
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn list_definitions(&self) -> anyhow::Result<Vec<WorkflowDef>> {
        let rows = sqlx::query_as::<_, (String, String, i64, String, String, i64, i64)>(
            "SELECT id, name, version, yaml_content, status, created_at, updated_at FROM workflows ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(|r| WorkflowDef {
            id: r.0, name: r.1, version: r.2, yaml_content: r.3, status: r.4, created_at: r.5, updated_at: r.6,
        }).collect())
    }

    pub async fn get_definition(&self, id: &str) -> anyhow::Result<Option<WorkflowDef>> {
        let row = sqlx::query_as::<_, (String, String, i64, String, String, i64, i64)>(
            "SELECT id, name, version, yaml_content, status, created_at, updated_at FROM workflows WHERE id = ?"
        )
        .bind(id).fetch_optional(&self.pool).await?;

        Ok(row.map(|r| WorkflowDef {
            id: r.0, name: r.1, version: r.2, yaml_content: r.3, status: r.4, created_at: r.5, updated_at: r.6,
        }))
    }

    pub async fn update_definition(&self, id: &str, name: Option<&str>, yaml: Option<&str>, status: Option<&str>) -> anyhow::Result<bool> {
        let now = chrono::Utc::now().timestamp();

        // T2-2: when yaml changes, bump version + save history
        if let Some(y) = yaml {
            // Fetch current version
            let current_version: i64 = sqlx::query_scalar(
                "SELECT version FROM workflows WHERE id = ?"
            ).bind(id).fetch_one(&self.pool).await.unwrap_or(1);

            let new_version = current_version + 1;

            // Save the OLD version to workflow_versions before overwriting
            let old_yaml: String = sqlx::query_scalar(
                "SELECT yaml_content FROM workflows WHERE id = ?"
            ).bind(id).fetch_one(&self.pool).await.unwrap_or_default();

            let _ = sqlx::query(
                "INSERT OR REPLACE INTO workflow_versions (workflow_id, version, yaml_content, saved_by, change_summary, created_at)
                 VALUES (?, ?, ?, 'system', ?, ?)"
            )
            .bind(id)
            .bind(current_version)
            .bind(&old_yaml)
            .bind(format!("version {} saved before update to {}", current_version, new_version))
            .bind(now)
            .execute(&self.pool).await;

            // Also save the NEW version to workflow_versions
            let _ = sqlx::query(
                "INSERT OR REPLACE INTO workflow_versions (workflow_id, version, yaml_content, saved_by, change_summary, created_at)
                 VALUES (?, ?, ?, 'system', ?, ?)"
            )
            .bind(id)
            .bind(new_version)
            .bind(y)
            .bind(format!("version {} saved", new_version))
            .bind(now)
            .execute(&self.pool).await;

            // Update workflows table with new version + yaml
            let mut sets = vec!["updated_at = ?", "version = ?", "yaml_content = ?"];
            let mut values: Vec<String> = vec![now.to_string(), new_version.to_string(), y.to_string()];

            if let Some(n) = name {
                sets.push("name = ?");
                values.push(n.to_string());
            }
            if let Some(s) = status {
                sets.push("status = ?");
                values.push(s.to_string());
            }

            let sql = format!("UPDATE workflows SET {} WHERE id = ?", sets.join(", "));
            let mut query = sqlx::query(&sql);
            for v in &values {
                query = query.bind(v);
            }
            query = query.bind(id);
            let result = query.execute(&self.pool).await?;
            return Ok(result.rows_affected() > 0);
        }

        // Name/status-only update (no version bump)
        let mut sets = vec!["updated_at = ?".to_string()];
        let mut values: Vec<String> = vec![];

        if let Some(n) = name {
            sets.push("name = ?".to_string());
            values.push(n.to_string());
        }
        if let Some(s) = status {
            sets.push("status = ?".to_string());
            values.push(s.to_string());
        }

        let sql = format!("UPDATE workflows SET {} WHERE id = ?", sets.join(", "));
        let mut query = sqlx::query(&sql).bind(now);
        for v in &values {
            query = query.bind(v);
        }
        query = query.bind(id);
        let result = query.execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_definition(&self, id: &str) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM workflows WHERE id = ?")
            .bind(id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    // ── Workflow Versions (T2-2) ──

    /// List all saved versions of a workflow, newest first.
    pub async fn list_versions(&self, workflow_id: &str) -> anyhow::Result<Vec<WorkflowVersion>> {
        let rows = sqlx::query_as::<_, (i64, i64, String, Option<String>, Option<String>, i64)>(
            "SELECT id, version, yaml_content, saved_by, change_summary, created_at
               FROM workflow_versions
              WHERE workflow_id = ?
              ORDER BY version DESC"
        )
        .bind(workflow_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| WorkflowVersion {
            id: r.0,
            workflow_id: workflow_id.to_string(),
            version: r.1,
            yaml_content: r.2,
            saved_by: r.3,
            change_summary: r.4,
            created_at: r.5,
        }).collect())
    }

    /// Get a specific version's full definition.
    pub async fn get_version(&self, workflow_id: &str, version: i64) -> anyhow::Result<Option<WorkflowVersion>> {
        let row = sqlx::query_as::<_, (i64, i64, String, Option<String>, Option<String>, i64)>(
            "SELECT id, version, yaml_content, saved_by, change_summary, created_at
               FROM workflow_versions
              WHERE workflow_id = ? AND version = ?"
        )
        .bind(workflow_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| WorkflowVersion {
            id: r.0,
            workflow_id: workflow_id.to_string(),
            version: r.1,
            yaml_content: r.2,
            saved_by: r.3,
            change_summary: r.4,
            created_at: r.5,
        }))
    }

    /// Rollback the workflow to a previous version.
    /// Copies the old version's yaml_content into the workflows table,
    /// bumps the version counter, and saves the rolled-back content as
    /// a new version entry.
    pub async fn rollback_to_version(&self, workflow_id: &str, target_version: i64) -> anyhow::Result<bool> {
        let target = self.get_version(workflow_id, target_version).await?
            .ok_or_else(|| anyhow::anyhow!("version {} not found for workflow {}", target_version, workflow_id))?;

        let now = chrono::Utc::now().timestamp();
        let current_version: i64 = sqlx::query_scalar(
            "SELECT version FROM workflows WHERE id = ?"
        ).bind(workflow_id).fetch_one(&self.pool).await.unwrap_or(1);

        let new_version = current_version + 1;

        // Update the workflows table with the rolled-back content
        let result = sqlx::query(
            "UPDATE workflows SET yaml_content = ?, version = ?, updated_at = ? WHERE id = ?"
        )
        .bind(&target.yaml_content)
        .bind(new_version)
        .bind(now)
        .bind(workflow_id)
        .execute(&self.pool).await?;

        // Save the rolled-back version to workflow_versions
        let _ = sqlx::query(
            "INSERT OR REPLACE INTO workflow_versions (workflow_id, version, yaml_content, saved_by, change_summary, created_at)
             VALUES (?, ?, ?, 'system', ?, ?)"
        )
        .bind(workflow_id)
        .bind(new_version)
        .bind(&target.yaml_content)
        .bind(format!("rollback to version {}", target_version))
        .bind(now)
        .execute(&self.pool).await;

        Ok(result.rows_affected() > 0)
    }

    // ── Workflow Runs (Executions) ──

    pub async fn create_run(&self, workflow_id: &str, version: i64, input: &str, group_id: Option<&str>, agent_id: Option<&str>) -> anyhow::Result<WorkflowRun> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO workflow_executions (id, workflow_id, workflow_version, status, input, group_id, agent_id, started_at) VALUES (?, ?, ?, 'running', ?, ?, ?, ?)"
        )
        .bind(&id).bind(workflow_id).bind(version).bind(input).bind(group_id).bind(agent_id).bind(now)
        .execute(&self.pool).await?;

        Ok(WorkflowRun {
            id, workflow_id: workflow_id.to_string(), workflow_version: version,
            status: "running".to_string(), input: Some(input.to_string()),
            output: None, error: None, group_id: group_id.map(|s| s.to_string()),
            agent_id: agent_id.map(|s| s.to_string()), started_at: now, completed_at: None,
        })
    }

    pub async fn list_runs(&self, workflow_id: Option<&str>, group_id: Option<&str>, status: Option<&str>, limit: Option<i64>) -> anyhow::Result<Vec<WorkflowRun>> {
        let mut wheres = vec!["1=1".to_string()];
        let mut params: Vec<String> = vec![];

        if let Some(wid) = workflow_id {
            wheres.push("workflow_id = ?".to_string());
            params.push(wid.to_string());
        }
        if let Some(gid) = group_id {
            wheres.push("group_id = ?".to_string());
            params.push(gid.to_string());
        }
        if let Some(s) = status {
            wheres.push("status = ?".to_string());
            params.push(s.to_string());
        }

        let lim = limit.unwrap_or(50);
        let sql = format!(
            "SELECT id, workflow_id, workflow_version, status, input, output, error, group_id, agent_id, started_at, completed_at FROM workflow_executions WHERE {} ORDER BY started_at DESC LIMIT ?",
            wheres.join(" AND ")
        );

        let mut query = sqlx::query_as::<_, (String, String, i64, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, i64, Option<i64>)>(&sql);
        for p in &params {
            query = query.bind(p);
        }
        query = query.bind(lim);
        let rows = query.fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(|r| WorkflowRun {
            id: r.0, workflow_id: r.1, workflow_version: r.2, status: r.3,
            input: r.4, output: r.5, error: r.6, group_id: r.7,
            agent_id: r.8, started_at: r.9, completed_at: r.10,
        }).collect())
    }

    pub async fn get_run(&self, run_id: &str) -> anyhow::Result<Option<WorkflowRun>> {
        let row = sqlx::query_as::<_, (String, String, i64, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, i64, Option<i64>)>(
            "SELECT id, workflow_id, workflow_version, status, input, output, error, group_id, agent_id, started_at, completed_at FROM workflow_executions WHERE id = ?"
        )
        .bind(run_id).fetch_optional(&self.pool).await?;

        Ok(row.map(|r| WorkflowRun {
            id: r.0, workflow_id: r.1, workflow_version: r.2, status: r.3,
            input: r.4, output: r.5, error: r.6, group_id: r.7,
            agent_id: r.8, started_at: r.9, completed_at: r.10,
        }))
    }

    pub async fn update_run_status(&self, run_id: &str, status: &str, output: Option<&str>, error: Option<&str>) -> anyhow::Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let completed_at = if status == "completed" || status == "failed" || status == "cancelled" {
            Some(now)
        } else {
            None
        };

        let result = sqlx::query(
            "UPDATE workflow_executions SET status = ?, output = COALESCE(?, output), error = COALESCE(?, error), completed_at = COALESCE(?, completed_at) WHERE id = ?"
        )
        .bind(status).bind(output).bind(error).bind(completed_at).bind(run_id)
        .execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    // ── Checkpoints ──

    pub async fn record_checkpoint(&self, exec_id: &str, node_id: &str, output: &str, context: &str) -> anyhow::Result<i64> {
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query(
            "INSERT INTO checkpoints (exec_id, node_id, output, context_snapshot, created_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(exec_id).bind(node_id).bind(output).bind(context).bind(now)
        .execute(&self.pool).await?;
        Ok(result.last_insert_rowid())
    }

    pub async fn list_checkpoints(&self, exec_id: &str) -> anyhow::Result<Vec<RunCheckpoint>> {
        let rows = sqlx::query_as::<_, (i64, String, String, String, String, i64)>(
            "SELECT id, exec_id, node_id, output, context_snapshot, created_at FROM checkpoints WHERE exec_id = ? ORDER BY created_at ASC"
        )
        .bind(exec_id).fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(|r| RunCheckpoint {
            id: r.0, exec_id: r.1, node_id: r.2, output: r.3, context_snapshot: r.4, created_at: r.5,
        }).collect())
    }
}
