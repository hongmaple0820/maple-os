use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub group_type: GroupType,
    pub owner_id: String,
    pub settings: GroupSettings,
    pub dm_pair_key: Option<String>,
    pub dm_type: Option<DmType>,
    pub member_count: i64,
    pub message_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupType {
    Collaboration,
    Project,
    Channel,
    Dm,
}

impl GroupType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Collaboration => "collaboration",
            Self::Project => "project",
            Self::Channel => "channel",
            Self::Dm => "dm",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "project" => Self::Project,
            "channel" => Self::Channel,
            "dm" => Self::Dm,
            _ => Self::Collaboration,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "human_human" => Some(Self::HumanHuman),
            "human_agent" => Some(Self::HumanAgent),
            "agent_agent" => Some(Self::AgentAgent),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSettings {
    pub max_agents: u32,
    pub auto_approve: bool,
    pub knowledge_base_enabled: bool,
    pub default_agent_id: Option<String>,
    pub allow_member_invite: bool,
    pub message_retention_days: Option<u32>,
}

impl Default for GroupSettings {
    fn default() -> Self {
        Self {
            max_agents: 10,
            auto_approve: false,
            knowledge_base_enabled: true,
            default_agent_id: None,
            allow_member_invite: true,
            message_retention_days: None,
        }
    }
}

impl GroupSettings {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn from_json(s: &str) -> Self {
        serde_json::from_str(s).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMember {
    pub group_id: String,
    pub member_id: String,
    pub member_type: String,
    pub role: String,
    pub nickname: Option<String>,
    pub can_approve: bool,
    pub approval_scope: Option<String>,
    pub joined_at: i64,
    pub last_active_at: Option<i64>,
}

pub struct GroupManager {
    pool: SqlitePool,
}

impl GroupManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_group(
        &self,
        name: &str,
        description: Option<&str>,
        group_type: GroupType,
        owner_id: &str,
        settings: &GroupSettings,
    ) -> Result<Group> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        let settings_json = settings.to_json();

        sqlx::query(
            "INSERT INTO groups (id, name, description, group_type, owner_id, settings, member_count, message_count, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 0, 0, ?, ?)"
        )
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(group_type.as_str())
        .bind(owner_id)
        .bind(&settings_json)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        // Auto-add owner as member
        self.add_member(&id, owner_id, "human", "owner").await?;

        Ok(Group {
            id,
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
            avatar_url: None,
            group_type,
            owner_id: owner_id.to_string(),
            settings: settings.clone(),
            dm_pair_key: None,
            dm_type: None,
            member_count: 1,
            message_count: 0,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn get_group(&self, id: &str) -> Result<Option<Group>> {
        let row = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String, String, String, Option<String>, Option<String>, i64, i64, i64, i64)>(
            "SELECT id, name, description, avatar_url, group_type, owner_id, settings, dm_pair_key, dm_type, member_count, message_count, created_at, updated_at
             FROM groups WHERE id = ? AND deleted_at IS NULL"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            Group {
                id: r.0,
                name: r.1,
                description: r.2,
                avatar_url: r.3,
                group_type: GroupType::from_str(&r.4),
                owner_id: r.5,
                settings: GroupSettings::from_json(&r.6),
                dm_pair_key: r.7,
                dm_type: r.8.and_then(|s| DmType::from_str(&s)),
                member_count: r.9,
                message_count: r.10,
                created_at: r.11,
                updated_at: r.12,
            }
        }))
    }

    pub async fn list_groups(&self, user_id: &str) -> Result<Vec<Group>> {
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT g.id FROM groups g
             INNER JOIN group_members gm ON g.id = gm.group_id
             WHERE gm.member_id = ? AND g.deleted_at IS NULL
             ORDER BY g.updated_at DESC"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut groups = Vec::new();
        for (group_id,) in rows {
            if let Some(g) = self.get_group(&group_id).await? {
                groups.push(g);
            }
        }
        Ok(groups)
    }

    pub async fn add_member(&self, group_id: &str, member_id: &str, member_type: &str, role: &str) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query(
            "INSERT OR IGNORE INTO group_members (group_id, member_id, member_type, role, joined_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(group_id)
        .bind(member_id)
        .bind(member_type)
        .bind(role)
        .bind(now)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() > 0 {
            sqlx::query("UPDATE groups SET member_count = member_count + 1, updated_at = ? WHERE id = ?")
                .bind(now)
                .bind(group_id)
                .execute(&self.pool)
                .await?;
        }

        Ok(result.rows_affected() > 0)
    }

    pub async fn remove_member(&self, group_id: &str, member_id: &str) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query(
            "DELETE FROM group_members WHERE group_id = ? AND member_id = ? AND role != 'owner'"
        )
        .bind(group_id)
        .bind(member_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() > 0 {
            sqlx::query("UPDATE groups SET member_count = member_count - 1, updated_at = ? WHERE id = ?")
                .bind(now)
                .bind(group_id)
                .execute(&self.pool)
                .await?;
        }

        Ok(result.rows_affected() > 0)
    }

    pub async fn update_member_role(&self, group_id: &str, member_id: &str, role: &str) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE group_members SET role = ? WHERE group_id = ? AND member_id = ?"
        )
        .bind(role)
        .bind(group_id)
        .bind(member_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn list_members(&self, group_id: &str) -> Result<Vec<GroupMember>> {
        let rows = sqlx::query_as::<_, (String, String, String, String, Option<String>, i64, Option<String>, i64, Option<i64>)>(
            "SELECT group_id, member_id, member_type, role, nickname, can_approve, approval_scope, joined_at, last_active_at
             FROM group_members WHERE group_id = ? ORDER BY role, joined_at"
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| GroupMember {
            group_id: r.0,
            member_id: r.1,
            member_type: r.2,
            role: r.3,
            nickname: r.4,
            can_approve: r.5 != 0,
            approval_scope: r.6,
            joined_at: r.7,
            last_active_at: r.8,
        }).collect())
    }

    pub async fn is_member(&self, group_id: &str, member_id: &str) -> Result<bool> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT member_id FROM group_members WHERE group_id = ? AND member_id = ?"
        )
        .bind(group_id)
        .bind(member_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.is_some())
    }

    pub async fn get_member_role(&self, group_id: &str, member_id: &str) -> Result<Option<String>> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT role FROM group_members WHERE group_id = ? AND member_id = ?"
        )
        .bind(group_id)
        .bind(member_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.0))
    }

    pub async fn find_or_create_dm(&self, user_a: &str, user_b: &str, dm_type: DmType) -> Result<Group> {
        let (id_a, id_b) = if user_a < user_b {
            (user_a, user_b)
        } else {
            (user_b, user_a)
        };
        let pair_key = format!("{}:{}", id_a, id_b);

        // Try to find existing DM
        let existing = sqlx::query_as::<_, (String,)>(
            "SELECT id FROM groups WHERE dm_pair_key = ? AND group_type = 'dm' AND deleted_at IS NULL"
        )
        .bind(&pair_key)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((group_id,)) = existing {
            if let Some(group) = self.get_group(&group_id).await? {
                return Ok(group);
            }
        }

        // Create new DM group
        let settings = GroupSettings::default();
        let group = self.create_group(
            &format!("DM: {} & {}", id_a, id_b),
            None,
            GroupType::Dm,
            user_a,
            &settings,
        ).await?;

        // Set DM metadata
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "UPDATE groups SET dm_pair_key = ?, dm_type = ?, updated_at = ? WHERE id = ?"
        )
        .bind(&pair_key)
        .bind(dm_type.as_str())
        .bind(now)
        .bind(&group.id)
        .execute(&self.pool)
        .await?;

        // Add second member
        self.add_member(&group.id, user_b, "human", "member").await?;

        self.get_group(&group.id).await?.ok_or_else(|| anyhow::anyhow!("DM group not found after creation"))
    }

    pub async fn update_settings(&self, group_id: &str, settings: &GroupSettings) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query(
            "UPDATE groups SET settings = ?, updated_at = ? WHERE id = ?"
        )
        .bind(settings.to_json())
        .bind(now)
        .bind(group_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_group(&self, group_id: &str) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query(
            "UPDATE groups SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL"
        )
        .bind(now)
        .bind(now)
        .bind(group_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}
