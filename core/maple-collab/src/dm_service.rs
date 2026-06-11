use serde::{Deserialize, Serialize};
use sqlx::{SqlitePool, Row};
use anyhow::Result;

use crate::group::{GroupManager, GroupType, GroupSettings};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DmType {
    HumanHuman,
    HumanAgent,
    AgentAgent,
}

impl DmType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HumanHuman => "human_human",
            Self::HumanAgent => "human_agent",
            Self::AgentAgent => "agent_agent",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "human_human" => Self::HumanHuman,
            "agent_agent" => Self::AgentAgent,
            _ => Self::HumanAgent,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmToolGrant {
    pub id: String,
    pub dm_group_id: String,
    pub tool_name: String,
    pub granted_by: String,
    pub granted_at: i64,
    pub expires_at: Option<i64>,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2ADelegation {
    pub id: String,
    pub dm_group_id: String,
    pub delegator_id: String,
    pub executor_id: String,
    pub task_id: Option<String>,
    pub prompt: String,
    pub status: DelegationStatus,
    pub result: Option<String>,
    pub visible_to: Vec<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DelegationStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

impl DelegationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

pub struct DmService {
    pool: SqlitePool,
    group_manager: GroupManager,
}

impl DmService {
    pub fn new(pool: SqlitePool, group_manager: GroupManager) -> Self {
        Self { pool, group_manager }
    }

    /// Build a deterministic pair key from two user IDs
    fn pair_key(user_a: &str, user_b: &str) -> String {
        if user_a < user_b {
            format!("{}:{}", user_a, user_b)
        } else {
            format!("{}:{}", user_b, user_a)
        }
    }

    /// Determine DM type from user types
    async fn determine_dm_type(&self, user_a: &str, user_b: &str) -> Result<DmType> {
        let a_type = self.get_user_type(user_a).await?;
        let b_type = self.get_user_type(user_b).await?;
        Ok(match (a_type.as_str(), b_type.as_str()) {
            ("human", "human") => DmType::HumanHuman,
            ("agent", "agent") => DmType::AgentAgent,
            _ => DmType::HumanAgent,
        })
    }

    async fn get_user_type(&self, user_id: &str) -> Result<String> {
        let row = sqlx::query_scalar::<_, String>(
            "SELECT user_type FROM users_v3 WHERE id = ?"
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.unwrap_or_else(|| "human".to_string()))
    }

    /// Find existing DM between two users, or create one (idempotent)
    pub async fn find_or_create(&self, user_a: &str, user_b: &str) -> Result<String> {
        let pair_key = Self::pair_key(user_a, user_b);

        // Check for existing DM
        if let Some(row) = sqlx::query_scalar::<_, String>(
            "SELECT id FROM groups WHERE dm_pair_key = ? AND group_type = 'dm' AND deleted_at IS NULL"
        )
        .bind(&pair_key)
        .fetch_optional(&self.pool)
        .await?
        {
            return Ok(row);
        }

        // Determine DM type
        let dm_type = self.determine_dm_type(user_a, user_b).await?;

        // Create the DM group
        let group = self.group_manager.create_group(
            "", // DM groups have empty name
            None,
            GroupType::Dm,
            user_a, // creator
            &GroupSettings::default(),
        ).await?;
        let group_id = group.id;

        // Set DM-specific fields
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "UPDATE groups SET dm_type = ?, dm_pair_key = ?, member_count = 2, updated_at = ? WHERE id = ?"
        )
        .bind(dm_type.as_str())
        .bind(&pair_key)
        .bind(now)
        .bind(&group_id)
        .execute(&self.pool)
        .await?;

        // Add both members
        for uid in [user_a, user_b] {
            let user_type = self.get_user_type(uid).await?;
            let _ = self.group_manager.add_member(&group_id, uid, &user_type, "member").await;
        }

        Ok(group_id)
    }

    /// Get DM type for a group
    pub async fn get_dm_type(&self, group_id: &str) -> Result<Option<DmType>> {
        let row = sqlx::query_scalar::<_, String>(
            "SELECT dm_type FROM groups WHERE id = ? AND group_type = 'dm'"
        )
        .bind(group_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|s| DmType::from_str(&s)))
    }

    /// List all DMs for a user, ordered by last message time
    pub async fn list_user_dms(&self, user_id: &str) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            r#"SELECT g.id, g.name, g.group_type, g.dm_type, g.dm_pair_key,
                      g.member_count, g.message_count, g.created_at, g.updated_at,
                      gm2.member_id AS other_member_id
               FROM groups g
               INNER JOIN group_members gm1 ON g.id = gm1.group_id AND gm1.member_id = ?
               INNER JOIN group_members gm2 ON g.id = gm2.group_id AND gm2.member_id != ?
               WHERE g.group_type = 'dm' AND g.deleted_at IS NULL
               ORDER BY g.updated_at DESC"#
        )
        .bind(user_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut dms = Vec::new();
        for row in rows {
            let group_id: String = row.get("id");
            let other_id: String = row.get("other_member_id");
            let dm_type: Option<String> = row.get("dm_type");

            dms.push(serde_json::json!({
                "id": group_id,
                "dm_type": dm_type,
                "other_member_id": other_id,
                "member_count": row.get::<i64, _>("member_count"),
                "message_count": row.get::<i64, _>("message_count"),
                "created_at": row.get::<i64, _>("created_at"),
                "updated_at": row.get::<i64, _>("updated_at"),
            }));
        }

        Ok(dms)
    }

    // ── Tool Grants ──

    /// Grant a tool to a DM context
    pub async fn grant_tool(
        &self,
        dm_group_id: &str,
        tool_name: &str,
        granted_by: &str,
        expires_at: Option<i64>,
        scope: Option<&str>,
    ) -> Result<DmToolGrant> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            r#"INSERT INTO dm_tool_grants (id, dm_group_id, tool_name, granted_by, granted_at, expires_at, scope)
               VALUES (?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(dm_group_id, tool_name, granted_by) DO UPDATE SET
               expires_at = excluded.expires_at, scope = excluded.scope"#
        )
        .bind(&id)
        .bind(dm_group_id)
        .bind(tool_name)
        .bind(granted_by)
        .bind(now)
        .bind(expires_at)
        .bind(scope)
        .execute(&self.pool)
        .await?;

        Ok(DmToolGrant {
            id,
            dm_group_id: dm_group_id.to_string(),
            tool_name: tool_name.to_string(),
            granted_by: granted_by.to_string(),
            granted_at: now,
            expires_at,
            scope: scope.map(|s| s.to_string()),
        })
    }

    /// Revoke a tool grant
    pub async fn revoke_tool(&self, dm_group_id: &str, tool_name: &str, granted_by: &str) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM dm_tool_grants WHERE dm_group_id = ? AND tool_name = ? AND granted_by = ?"
        )
        .bind(dm_group_id)
        .bind(tool_name)
        .bind(granted_by)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// List all active tool grants for a DM
    pub async fn list_grants(&self, dm_group_id: &str) -> Result<Vec<DmToolGrant>> {
        let now = chrono::Utc::now().timestamp();
        let rows = sqlx::query(
            "SELECT id, dm_group_id, tool_name, granted_by, granted_at, expires_at, scope
             FROM dm_tool_grants WHERE dm_group_id = ? AND (expires_at IS NULL OR expires_at > ?)"
        )
        .bind(dm_group_id)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| DmToolGrant {
            id: r.get(0),
            dm_group_id: r.get(1),
            tool_name: r.get(2),
            granted_by: r.get(3),
            granted_at: r.get(4),
            expires_at: r.get(5),
            scope: r.get(6),
        }).collect())
    }

    /// Check if a specific tool is granted in a DM
    pub async fn is_tool_granted(&self, dm_group_id: &str, tool_name: &str) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM dm_tool_grants WHERE dm_group_id = ? AND tool_name = ? AND (expires_at IS NULL OR expires_at > ?)"
        )
        .bind(dm_group_id)
        .bind(tool_name)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    // ── A2A Delegations ──

    /// Create an A2A delegation
    pub async fn create_delegation(
        &self,
        dm_group_id: &str,
        delegator_id: &str,
        executor_id: &str,
        prompt: &str,
        task_id: Option<&str>,
        visible_to: &[String],
    ) -> Result<A2ADelegation> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        let visible_json = serde_json::to_string(visible_to)?;

        sqlx::query(
            r#"INSERT INTO a2a_delegations (id, dm_group_id, delegator_id, executor_id, task_id, prompt, status, visible_to, created_at)
               VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?)"#
        )
        .bind(&id)
        .bind(dm_group_id)
        .bind(delegator_id)
        .bind(executor_id)
        .bind(task_id)
        .bind(prompt)
        .bind(&visible_json)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(A2ADelegation {
            id,
            dm_group_id: dm_group_id.to_string(),
            delegator_id: delegator_id.to_string(),
            executor_id: executor_id.to_string(),
            task_id: task_id.map(|s| s.to_string()),
            prompt: prompt.to_string(),
            status: DelegationStatus::Pending,
            result: None,
            visible_to: visible_to.to_vec(),
            created_at: now,
            completed_at: None,
        })
    }

    /// Update delegation status
    pub async fn update_delegation_status(
        &self,
        delegation_id: &str,
        status: DelegationStatus,
        result: Option<&str>,
    ) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let completed_at = if matches!(status, DelegationStatus::Completed | DelegationStatus::Failed) {
            Some(now)
        } else {
            None
        };

        let affected = sqlx::query(
            "UPDATE a2a_delegations SET status = ?, result = ?, completed_at = ? WHERE id = ?"
        )
        .bind(status.as_str())
        .bind(result)
        .bind(completed_at)
        .bind(delegation_id)
        .execute(&self.pool)
        .await?
        .rows_affected();

        Ok(affected > 0)
    }

    /// List delegations visible to a user
    pub async fn list_visible_delegations(&self, user_id: &str) -> Result<Vec<A2ADelegation>> {
        let rows = sqlx::query(
            r#"SELECT id, dm_group_id, delegator_id, executor_id, task_id, prompt, status, result, visible_to, created_at, completed_at
               FROM a2a_delegations
               WHERE delegator_id = ? OR executor_id = ? OR visible_to LIKE ?
               ORDER BY created_at DESC"#
        )
        .bind(user_id)
        .bind(user_id)
        .bind(format!("%{}%", user_id))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| {
            let visible_str: String = r.get(8);
            let visible_to: Vec<String> = serde_json::from_str(&visible_str).unwrap_or_default();
            A2ADelegation {
                id: r.get(0),
                dm_group_id: r.get(1),
                delegator_id: r.get(2),
                executor_id: r.get(3),
                task_id: r.get(4),
                prompt: r.get(5),
                status: DelegationStatus::from_str(r.get::<&str, _>(6)),
                result: r.get(7),
                visible_to,
                created_at: r.get(9),
                completed_at: r.get(10),
            }
        }).collect())
    }

    /// Get a single delegation
    pub async fn get_delegation(&self, delegation_id: &str) -> Result<Option<A2ADelegation>> {
        let row = sqlx::query(
            r#"SELECT id, dm_group_id, delegator_id, executor_id, task_id, prompt, status, result, visible_to, created_at, completed_at
               FROM a2a_delegations WHERE id = ?"#
        )
        .bind(delegation_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let visible_str: String = r.get(8);
            let visible_to: Vec<String> = serde_json::from_str(&visible_str).unwrap_or_default();
            A2ADelegation {
                id: r.get(0),
                dm_group_id: r.get(1),
                delegator_id: r.get(2),
                executor_id: r.get(3),
                task_id: r.get(4),
                prompt: r.get(5),
                status: DelegationStatus::from_str(r.get::<&str, _>(6)),
                result: r.get(7),
                visible_to,
                created_at: r.get(9),
                completed_at: r.get(10),
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();

        sqlx::query("CREATE TABLE users_v3 (id TEXT PRIMARY KEY, name TEXT NOT NULL, user_type TEXT NOT NULL DEFAULT 'human', status TEXT NOT NULL DEFAULT 'offline', platform_role TEXT NOT NULL DEFAULT 'user', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)")
            .execute(&pool).await.unwrap();

        sqlx::query("INSERT INTO users_v3 (id, name, user_type, created_at, updated_at) VALUES ('u1', 'Alice', 'human', 0, 0)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO users_v3 (id, name, user_type, created_at, updated_at) VALUES ('u2', 'Bob', 'human', 0, 0)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO users_v3 (id, name, user_type, created_at, updated_at) VALUES ('a1', 'Coder', 'agent', 0, 0)")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE groups (id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT, avatar_url TEXT, group_type TEXT NOT NULL DEFAULT 'collaboration', dm_type TEXT, dm_pair_key TEXT, owner_id TEXT NOT NULL DEFAULT 'system', settings TEXT NOT NULL DEFAULT '{}', member_count INTEGER NOT NULL DEFAULT 0, message_count INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, archived_at INTEGER, deleted_at INTEGER)")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE group_members (group_id TEXT NOT NULL, member_id TEXT NOT NULL, member_type TEXT NOT NULL DEFAULT 'human', role TEXT NOT NULL DEFAULT 'member', joined_at INTEGER NOT NULL, PRIMARY KEY (group_id, member_id))")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE dm_tool_grants (id TEXT PRIMARY KEY, dm_group_id TEXT NOT NULL, tool_name TEXT NOT NULL, granted_by TEXT NOT NULL, granted_at INTEGER NOT NULL, expires_at INTEGER, scope TEXT, UNIQUE(dm_group_id, tool_name, granted_by))")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE a2a_delegations (id TEXT PRIMARY KEY, dm_group_id TEXT NOT NULL, delegator_id TEXT NOT NULL, executor_id TEXT NOT NULL, task_id TEXT, prompt TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending', result TEXT, visible_to TEXT NOT NULL DEFAULT '[]', created_at INTEGER NOT NULL, completed_at INTEGER)")
            .execute(&pool).await.unwrap();

        pool
    }

    #[tokio::test]
    async fn test_find_or_create_dm() {
        let pool = setup_db().await;
        let gm = GroupManager::new(pool.clone());
        let svc = DmService::new(pool.clone(), gm);

        // First call creates
        let id1 = svc.find_or_create("u1", "u2").await.unwrap();
        assert!(!id1.is_empty());

        // Second call returns same (idempotent)
        let id2 = svc.find_or_create("u2", "u1").await.unwrap();
        assert_eq!(id1, id2);
    }

    #[tokio::test]
    async fn test_dm_type_human_agent() {
        let pool = setup_db().await;
        let gm = GroupManager::new(pool.clone());
        let svc = DmService::new(pool.clone(), gm);

        let id = svc.find_or_create("u1", "a1").await.unwrap();
        let dm_type = svc.get_dm_type(&id).await.unwrap();
        assert_eq!(dm_type, Some(DmType::HumanAgent));
    }

    #[tokio::test]
    async fn test_dm_type_human_human() {
        let pool = setup_db().await;
        let gm = GroupManager::new(pool.clone());
        let svc = DmService::new(pool.clone(), gm);

        let id = svc.find_or_create("u1", "u2").await.unwrap();
        let dm_type = svc.get_dm_type(&id).await.unwrap();
        assert_eq!(dm_type, Some(DmType::HumanHuman));
    }

    #[tokio::test]
    async fn test_tool_grant_and_revoke() {
        let pool = setup_db().await;
        let gm = GroupManager::new(pool.clone());
        let svc = DmService::new(pool.clone(), gm);

        let dm_id = svc.find_or_create("u1", "a1").await.unwrap();

        // Grant
        let grant = svc.grant_tool(&dm_id, "bash", "u1", None, None).await.unwrap();
        assert_eq!(grant.tool_name, "bash");

        // Check
        assert!(svc.is_tool_granted(&dm_id, "bash").await.unwrap());
        assert!(!svc.is_tool_granted(&dm_id, "file_edit").await.unwrap());

        // List
        let grants = svc.list_grants(&dm_id).await.unwrap();
        assert_eq!(grants.len(), 1);

        // Revoke
        assert!(svc.revoke_tool(&dm_id, "bash", "u1").await.unwrap());
        assert!(!svc.is_tool_granted(&dm_id, "bash").await.unwrap());
    }

    #[tokio::test]
    async fn test_a2a_delegation_lifecycle() {
        let pool = setup_db().await;
        let gm = GroupManager::new(pool.clone());
        let svc = DmService::new(pool.clone(), gm);

        let dm_id = svc.find_or_create("a1", "a1").await.unwrap_or_else(|_| "dm1".to_string());

        // Create delegation
        let delegation = svc.create_delegation(
            &dm_id, "a1", "a1", "fix the bug", None, &["u1".to_string()],
        ).await.unwrap();
        assert_eq!(delegation.status, DelegationStatus::Pending);

        // Update status
        assert!(svc.update_delegation_status(&delegation.id, DelegationStatus::Completed, Some("fixed")).await.unwrap());

        // Verify
        let fetched = svc.get_delegation(&delegation.id).await.unwrap().unwrap();
        assert_eq!(fetched.status, DelegationStatus::Completed);
        assert_eq!(fetched.result.as_deref(), Some("fixed"));
    }
}
