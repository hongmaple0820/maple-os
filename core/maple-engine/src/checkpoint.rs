use crate::workflow::WorkflowExecution;
use anyhow::Result;
use sqlx::SqlitePool;
use std::collections::HashMap;
use serde_json::Value;
use uuid::Uuid;

pub struct CheckpointManager {
    db: SqlitePool,
}

impl CheckpointManager {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    pub async fn save(
        &self,
        exec: &WorkflowExecution,
        node_id: &str,
    ) -> Result<()> {
        let output = match exec.context.get(node_id) {
            Some(v) => serde_json::to_string(v)?,
            None => "null".to_string(),
        };
        let context_snapshot = serde_json::to_string(&exec.context)?;
        let exec_id = exec.exec_id.to_string();
        let created_at = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO checkpoints (exec_id, node_id, output, context_snapshot, created_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&exec_id)
        .bind(node_id)
        .bind(&output)
        .bind(&context_snapshot)
        .bind(created_at)
        .execute(&self.db)
        .await?;

        tracing::debug!(
            exec_id = %exec.exec_id,
            node_id = node_id,
            "Checkpoint persisted to SQLite"
        );
        Ok(())
    }

    pub async fn recover(
        &self,
        exec_id: Uuid,
    ) -> Result<Option<WorkflowExecution>> {
        let exec_id_str = exec_id.to_string();

        let row = sqlx::query_as::<_, (String, String, String, String, i64)>(
            "SELECT exec_id, node_id, output, context_snapshot, created_at FROM checkpoints WHERE exec_id = ? ORDER BY created_at DESC LIMIT 1"
        )
        .bind(&exec_id_str)
        .fetch_optional(&self.db)
        .await?;

        match row {
            Some((_exec_id, node_id, _output, context_snapshot, _created_at)) => {
                let context: HashMap<String, Value> =
                    serde_json::from_str(&context_snapshot).unwrap_or_default();

                tracing::info!(
                    exec_id = %exec_id,
                    recovered_after_node = %node_id,
                    context_keys = context.len(),
                    "Recovered workflow execution from checkpoint"
                );

                let mut exec = WorkflowExecution::new(
                    &format!("recovered_from_{}", exec_id),
                    1,
                    Value::Null,
                );
                exec.exec_id = exec_id;
                exec.context = context;
                exec.set_running();

                Ok(Some(exec))
            }
            None => {
                tracing::info!(exec_id = %exec_id, "No checkpoint found for recovery");
                Ok(None)
            }
        }
    }
}
