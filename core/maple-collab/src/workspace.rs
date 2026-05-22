use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: String,
    pub members: Vec<WorkspaceMember>,
    pub settings: WorkspaceSettings,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMember {
    pub id: String,
    pub name: String,
    pub member_type: MemberType,
    pub role: WorkspaceRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberType {
    Human,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceRole {
    Owner,
    Admin,
    Member,
    Viewer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSettings {
    pub max_agents: u32,
    pub auto_approve: bool,
    pub knowledge_base_enabled: bool,
}

impl Default for WorkspaceSettings {
    fn default() -> Self {
        Self {
            max_agents: 10,
            auto_approve: false,
            knowledge_base_enabled: true,
        }
    }
}

pub struct WorkspaceManager {
    pool: SqlitePool,
}

impl WorkspaceManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn init_schema(&self) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                owner_id TEXT NOT NULL,
                max_agents INTEGER DEFAULT 10,
                auto_approve INTEGER DEFAULT 0,
                knowledge_base_enabled INTEGER DEFAULT 1,
                created_at INTEGER NOT NULL
            )"
        ).execute(&self.pool).await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS workspace_members (
                workspace_id TEXT NOT NULL,
                member_id TEXT NOT NULL,
                name TEXT NOT NULL,
                member_type TEXT NOT NULL,
                role TEXT NOT NULL,
                PRIMARY KEY (workspace_id, member_id),
                FOREIGN KEY (workspace_id) REFERENCES workspaces(id)
            )"
        ).execute(&self.pool).await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_workspace_members_workspace ON workspace_members(workspace_id)"
        ).execute(&self.pool).await?;

        Ok(())
    }

    pub async fn create_workspace(&self, name: &str, owner_id: &str) -> Result<Workspace> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().timestamp();

        let settings = WorkspaceSettings::default();

        sqlx::query(
            "INSERT INTO workspaces (id, name, description, owner_id, max_agents, auto_approve, knowledge_base_enabled, created_at) VALUES (?, ?, NULL, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(name)
        .bind(owner_id)
        .bind(settings.max_agents as i64)
        .bind(settings.auto_approve as i64)
        .bind(settings.knowledge_base_enabled as i64)
        .bind(created_at)
        .execute(&self.pool)
        .await?;

        let owner_member = WorkspaceMember {
            id: owner_id.to_string(),
            name: "Owner".to_string(),
            member_type: MemberType::Human,
            role: WorkspaceRole::Owner,
        };

        self.add_member(&id, &owner_member).await?;

        Ok(Workspace {
            id,
            name: name.to_string(),
            description: None,
            owner_id: owner_id.to_string(),
            members: vec![owner_member],
            settings,
            created_at,
        })
    }

    pub async fn get_workspace(&self, id: &str) -> Result<Option<Workspace>> {
        let row = sqlx::query_as::<_, (String, String, Option<String>, String, i64, i64, i64, i64)>(
            "SELECT id, name, description, owner_id, max_agents, auto_approve, knowledge_base_enabled, created_at FROM workspaces WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        let (ws_id, name, description, owner_id, max_agents, auto_approve, kb_enabled, created_at) = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        let members = self.load_members(&ws_id).await?;

        Ok(Some(Workspace {
            id: ws_id,
            name,
            description,
            owner_id,
            members,
            settings: WorkspaceSettings {
                max_agents: max_agents as u32,
                auto_approve: auto_approve != 0,
                knowledge_base_enabled: kb_enabled != 0,
            },
            created_at,
        }))
    }

    pub async fn add_member(&self, workspace_id: &str, member: &WorkspaceMember) -> Result<bool> {
        let member_type = match member.member_type {
            MemberType::Human => "human",
            MemberType::Agent => "agent",
        };
        let role = match member.role {
            WorkspaceRole::Owner => "owner",
            WorkspaceRole::Admin => "admin",
            WorkspaceRole::Member => "member",
            WorkspaceRole::Viewer => "viewer",
        };

        let result = sqlx::query(
            "INSERT OR REPLACE INTO workspace_members (workspace_id, member_id, name, member_type, role) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(workspace_id)
        .bind(&member.id)
        .bind(&member.name)
        .bind(member_type)
        .bind(role)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn remove_member(&self, workspace_id: &str, member_id: &str) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM workspace_members WHERE workspace_id = ? AND member_id = ? AND role != 'owner'"
        )
        .bind(workspace_id)
        .bind(member_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn update_settings(&self, workspace_id: &str, settings: &WorkspaceSettings) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE workspaces SET max_agents = ?, auto_approve = ?, knowledge_base_enabled = ? WHERE id = ?"
        )
        .bind(settings.max_agents as i64)
        .bind(settings.auto_approve as i64)
        .bind(settings.knowledge_base_enabled as i64)
        .bind(workspace_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn list_workspaces(&self, user_id: &str) -> Result<Vec<Workspace>> {
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT workspace_id FROM workspace_members WHERE member_id = ?"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut workspaces = Vec::new();
        for (ws_id,) in rows {
            if let Some(ws) = self.get_workspace(&ws_id).await? {
                workspaces.push(ws);
            }
        }
        Ok(workspaces)
    }

    async fn load_members(&self, workspace_id: &str) -> Result<Vec<WorkspaceMember>> {
        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT member_id, name, member_type, role FROM workspace_members WHERE workspace_id = ?"
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|(id, name, member_type, role)| {
            WorkspaceMember {
                id,
                name,
                member_type: match member_type.as_str() {
                    "agent" => MemberType::Agent,
                    _ => MemberType::Human,
                },
                role: match role.as_str() {
                    "owner" => WorkspaceRole::Owner,
                    "admin" => WorkspaceRole::Admin,
                    "member" => WorkspaceRole::Member,
                    "viewer" => WorkspaceRole::Viewer,
                    _ => WorkspaceRole::Member,
                },
            }
        }).collect())
    }
}
