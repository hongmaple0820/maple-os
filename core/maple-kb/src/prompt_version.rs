use serde::{Deserialize, Serialize};
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVersion {
    pub prompt_ref: String,
    pub version: u32,
    pub content: String,
    pub change_reason: Option<String>,
    pub ab_test_result: Option<String>,
    pub created_at: i64,
}

pub struct PromptVersionManager {
    db: sqlx::SqlitePool,
}

impl PromptVersionManager {
    pub fn new(db: sqlx::SqlitePool) -> Self {
        Self { db }
    }

    pub async fn create_version(&self, prompt_ref: &str, content: &str, reason: Option<&str>) -> Result<u32> {
        let max_row = sqlx::query_as::<_, (i64,)>(
            "SELECT COALESCE(MAX(version), 0) FROM prompt_versions WHERE prompt_ref = ?"
        )
        .bind(prompt_ref)
        .fetch_one(&self.db)
        .await?;

        let new_version = max_row.0 as u32 + 1;
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO prompt_versions (prompt_ref, version, content, change_reason, created_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(prompt_ref)
        .bind(new_version)
        .bind(content)
        .bind(reason)
        .bind(now)
        .execute(&self.db)
        .await?;

        Ok(new_version)
    }

    pub async fn get_latest(&self, prompt_ref: &str) -> Result<Option<PromptVersion>> {
        let row = sqlx::query_as::<_, (String, i64, String, Option<String>, Option<String>, i64)>(
            "SELECT prompt_ref, version, content, change_reason, ab_test_result, created_at FROM prompt_versions WHERE prompt_ref = ? ORDER BY version DESC LIMIT 1"
        )
        .bind(prompt_ref)
        .fetch_optional(&self.db)
        .await?;

        Ok(row.map(|(prompt_ref, version, content, change_reason, ab_test_result, created_at)| {
            PromptVersion {
                prompt_ref,
                version: version as u32,
                content,
                change_reason,
                ab_test_result,
                created_at,
            }
        }))
    }

    pub async fn list_versions(&self, prompt_ref: &str) -> Result<Vec<PromptVersion>> {
        let rows = sqlx::query_as::<_, (String, i64, String, Option<String>, Option<String>, i64)>(
            "SELECT prompt_ref, version, content, change_reason, ab_test_result, created_at FROM prompt_versions WHERE prompt_ref = ? ORDER BY version DESC"
        )
        .bind(prompt_ref)
        .fetch_all(&self.db)
        .await?;

        Ok(rows.into_iter().map(|(prompt_ref, version, content, change_reason, ab_test_result, created_at)| {
            PromptVersion {
                prompt_ref,
                version: version as u32,
                content,
                change_reason,
                ab_test_result,
                created_at,
            }
        }).collect())
    }

    pub async fn resolve_prompt(&self, prompt_ref: &str) -> Result<String> {
        match self.get_latest(prompt_ref).await? {
            Some(pv) => Ok(pv.content),
            None => Ok(prompt_ref.to_string()),
        }
    }

    pub async fn rollback(&self, prompt_ref: &str, target_version: i32) -> Result<u32> {
        let row = sqlx::query_as::<_, (String, i64, String, Option<String>, Option<String>, i64)>(
            "SELECT prompt_ref, version, content, change_reason, ab_test_result, created_at FROM prompt_versions WHERE prompt_ref = ? AND version = ?"
        )
        .bind(prompt_ref)
        .bind(target_version)
        .fetch_optional(&self.db)
        .await?;

        match row {
            Some((_, _, content, _, _, _)) => {
                self.create_version(prompt_ref, &content, Some("rollback")).await
            }
            None => anyhow::bail!("Version {} not found for prompt '{}'", target_version, prompt_ref),
        }
    }
}
