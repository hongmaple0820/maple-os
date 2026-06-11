use serde::{Deserialize, Serialize};
use sqlx::{SqlitePool, Row};
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: String,
    pub group_id: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskV3 {
    pub id: String,
    pub project_id: Option<String>,
    pub group_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskV3Status,
    pub priority: TaskPriority,
    pub assignee_id: Option<String>,
    pub assignee_type: Option<String>,
    pub creator_id: String,
    pub parent_task_id: Option<String>,
    pub source_message_id: Option<String>,
    pub due_date: Option<i64>,
    pub estimated_hours: Option<f64>,
    pub actual_hours: Option<f64>,
    pub tags: Option<String>,
    pub metadata: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskV3Status {
    Backlog,
    Todo,
    InProgress,
    Review,
    Done,
    Cancelled,
}

impl TaskV3Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::Todo => "todo",
            Self::InProgress => "in_progress",
            Self::Review => "review",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "todo" => Self::Todo,
            "in_progress" => Self::InProgress,
            "review" => Self::Review,
            "done" => Self::Done,
            "cancelled" => Self::Cancelled,
            _ => Self::Backlog,
        }
    }

    pub fn valid_transitions(&self) -> Vec<TaskV3Status> {
        match self {
            Self::Backlog => vec![Self::Todo, Self::Cancelled],
            Self::Todo => vec![Self::InProgress, Self::Backlog, Self::Cancelled],
            Self::InProgress => vec![Self::Review, Self::Todo, Self::Cancelled],
            Self::Review => vec![Self::Done, Self::InProgress, Self::Cancelled],
            Self::Done => vec![],
            Self::Cancelled => vec![Self::Backlog],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Low,
    Medium,
    High,
    Urgent,
    Critical,
}

impl TaskPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Urgent => "urgent",
            Self::Critical => "critical",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "high" => Self::High,
            "urgent" => Self::Urgent,
            "critical" => Self::Critical,
            "low" => Self::Low,
            _ => Self::Medium,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusChange {
    pub id: String,
    pub task_id: String,
    pub old_status: String,
    pub new_status: String,
    pub changed_by: String,
    pub changed_at: i64,
    pub reason: Option<String>,
}

const TASK_COLUMNS: &str = "id, project_id, group_id, title, description, status, priority, assignee_id, assignee_type, creator_id, parent_task_id, source_message_id, due_date, estimated_hours, actual_hours, tags, metadata, created_at, updated_at, completed_at";

fn row_to_task(row: &sqlx::sqlite::SqliteRow) -> TaskV3 {
    TaskV3 {
        id: row.get(0),
        project_id: row.get(1),
        group_id: row.get(2),
        title: row.get(3),
        description: row.get(4),
        status: TaskV3Status::from_str(row.get::<&str, _>(5)),
        priority: TaskPriority::from_str(row.get::<&str, _>(6)),
        assignee_id: row.get(7),
        assignee_type: row.get(8),
        creator_id: row.get(9),
        parent_task_id: row.get(10),
        source_message_id: row.get(11),
        due_date: row.get(12),
        estimated_hours: row.get(13),
        actual_hours: row.get(14),
        tags: row.get(15),
        metadata: row.get(16),
        created_at: row.get(17),
        updated_at: row.get(18),
        completed_at: row.get(19),
    }
}

pub struct TaskService {
    pool: SqlitePool,
}

impl TaskService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_project(
        &self,
        name: &str,
        description: Option<&str>,
        owner_id: &str,
        group_id: Option<&str>,
    ) -> Result<Project> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO projects (id, name, description, owner_id, group_id, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 'active', ?, ?)"
        )
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(owner_id)
        .bind(group_id)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(Project {
            id,
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
            owner_id: owner_id.to_string(),
            group_id: group_id.map(|s| s.to_string()),
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn create_task(
        &self,
        title: &str,
        description: Option<&str>,
        creator_id: &str,
        project_id: Option<&str>,
        group_id: Option<&str>,
        priority: TaskPriority,
        assignee_id: Option<&str>,
        source_message_id: Option<&str>,
        parent_task_id: Option<&str>,
    ) -> Result<TaskV3> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO tasks_v3 (id, project_id, group_id, title, description, status, priority,
             assignee_id, assignee_type, creator_id, parent_task_id, source_message_id,
             created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 'backlog', ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(project_id)
        .bind(group_id)
        .bind(title)
        .bind(description)
        .bind(priority.as_str())
        .bind(assignee_id)
        .bind(assignee_id.map(|_| "human"))
        .bind(creator_id)
        .bind(parent_task_id)
        .bind(source_message_id)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(TaskV3 {
            id,
            project_id: project_id.map(|s| s.to_string()),
            group_id: group_id.map(|s| s.to_string()),
            title: title.to_string(),
            description: description.map(|s| s.to_string()),
            status: TaskV3Status::Backlog,
            priority,
            assignee_id: assignee_id.map(|s| s.to_string()),
            assignee_type: assignee_id.map(|_| "user".to_string()),
            creator_id: creator_id.to_string(),
            parent_task_id: parent_task_id.map(|s| s.to_string()),
            source_message_id: source_message_id.map(|s| s.to_string()),
            due_date: None,
            estimated_hours: None,
            actual_hours: None,
            tags: None,
            metadata: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
        })
    }

    pub async fn get_task(&self, task_id: &str) -> Result<Option<TaskV3>> {
        let sql = format!(
            "SELECT {} FROM tasks_v3 WHERE id = ? AND deleted_at IS NULL",
            TASK_COLUMNS
        );
        let row = sqlx::query(&sql)
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| row_to_task(&r)))
    }

    pub async fn transition_task(
        &self,
        task_id: &str,
        new_status: TaskV3Status,
        changed_by: &str,
        reason: Option<&str>,
    ) -> Result<TaskV3> {
        let task = self.get_task(task_id).await?
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id))?;

        if !task.status.valid_transitions().contains(&new_status) {
            anyhow::bail!(
                "Invalid transition: {} -> {}",
                task.status.as_str(),
                new_status.as_str()
            );
        }

        let now = chrono::Utc::now().timestamp();
        let old_status = task.status.as_str().to_string();
        let completed_at = if new_status == TaskV3Status::Done { Some(now) } else { None };

        sqlx::query(
            "UPDATE tasks_v3 SET status = ?, updated_at = ?, completed_at = COALESCE(?, completed_at) WHERE id = ?"
        )
        .bind(new_status.as_str())
        .bind(now)
        .bind(completed_at)
        .bind(task_id)
        .execute(&self.pool)
        .await?;

        let history_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO task_status_history (id, task_id, old_status, new_status, changed_by, changed_at, reason)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&history_id)
        .bind(task_id)
        .bind(&old_status)
        .bind(new_status.as_str())
        .bind(changed_by)
        .bind(now)
        .bind(reason)
        .execute(&self.pool)
        .await?;

        let mut updated = task;
        updated.status = new_status;
        updated.updated_at = now;
        updated.completed_at = completed_at;
        Ok(updated)
    }

    pub async fn list_tasks(
        &self,
        group_id: Option<&str>,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<TaskV3>> {
        let sql = if group_id.is_some() && status.is_some() {
            format!(
                "SELECT {} FROM tasks_v3 WHERE deleted_at IS NULL AND group_id = ? AND status = ?
                 ORDER BY CASE priority WHEN 'critical' THEN 0 WHEN 'urgent' THEN 1 WHEN 'high' THEN 2 WHEN 'medium' THEN 3 ELSE 4 END, created_at DESC LIMIT ?",
                TASK_COLUMNS
            )
        } else if group_id.is_some() {
            format!(
                "SELECT {} FROM tasks_v3 WHERE deleted_at IS NULL AND group_id = ?
                 ORDER BY CASE priority WHEN 'critical' THEN 0 WHEN 'urgent' THEN 1 WHEN 'high' THEN 2 WHEN 'medium' THEN 3 ELSE 4 END, created_at DESC LIMIT ?",
                TASK_COLUMNS
            )
        } else {
            format!(
                "SELECT {} FROM tasks_v3 WHERE deleted_at IS NULL
                 ORDER BY CASE priority WHEN 'critical' THEN 0 WHEN 'urgent' THEN 1 WHEN 'high' THEN 2 WHEN 'medium' THEN 3 ELSE 4 END, created_at DESC LIMIT ?",
                TASK_COLUMNS
            )
        };

        let query = sqlx::query(&sql);
        let query = if let Some(gid) = group_id { query.bind(gid) } else { query };
        let query = if let Some(st) = status { query.bind(st) } else { query };
        let query = query.bind(limit);

        let rows = query.fetch_all(&self.pool).await?;
        Ok(rows.iter().map(row_to_task).collect())
    }

    pub async fn add_comment(
        &self,
        task_id: &str,
        user_id: &str,
        content: &str,
        source_message_id: Option<&str>,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO task_comments_v3 (id, task_id, user_id, content, source_message_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(task_id)
        .bind(user_id)
        .bind(content)
        .bind(source_message_id)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    pub async fn get_status_history(&self, task_id: &str) -> Result<Vec<StatusChange>> {
        let rows = sqlx::query(
            "SELECT id, task_id, old_status, new_status, changed_by, changed_at, reason
             FROM task_status_history WHERE task_id = ? ORDER BY changed_at ASC"
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| StatusChange {
            id: r.get(0),
            task_id: r.get(1),
            old_status: r.get(2),
            new_status: r.get(3),
            changed_by: r.get(4),
            changed_at: r.get(5),
            reason: r.get(6),
        }).collect())
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
        sqlx::query("INSERT INTO users_v3 (id, name, created_at, updated_at) VALUES ('user1', 'Test User', 0, 0)")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE groups (id TEXT PRIMARY KEY, name TEXT NOT NULL, group_type TEXT NOT NULL DEFAULT 'collaboration', owner_id TEXT NOT NULL, settings TEXT NOT NULL DEFAULT '{}', member_count INTEGER NOT NULL DEFAULT 0, message_count INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO groups (id, name, owner_id, created_at, updated_at) VALUES ('g1', 'Test Group', 'user1', 0, 0)")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE group_messages (id TEXT PRIMARY KEY, group_id TEXT NOT NULL, sender_id TEXT NOT NULL, sender_type TEXT NOT NULL, message_type TEXT NOT NULL, content TEXT NOT NULL, thread_reply_count INTEGER NOT NULL DEFAULT 0, source_channel TEXT NOT NULL DEFAULT 'api', pinned INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL)")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE tasks_v3 (id TEXT PRIMARY KEY, project_id TEXT, group_id TEXT NOT NULL, title TEXT NOT NULL, description TEXT, status TEXT NOT NULL DEFAULT 'todo', priority TEXT NOT NULL DEFAULT 'medium', assignee_id TEXT, assignee_type TEXT, creator_id TEXT NOT NULL, parent_task_id TEXT, source_message_id TEXT, due_date INTEGER, estimated_hours REAL, actual_hours REAL, tags TEXT, metadata TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, completed_at INTEGER, deleted_at INTEGER)")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE task_status_history (id TEXT PRIMARY KEY, task_id TEXT NOT NULL, old_status TEXT NOT NULL, new_status TEXT NOT NULL, changed_by TEXT NOT NULL, reason TEXT, changed_at INTEGER NOT NULL)")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE task_comments_v3 (id TEXT PRIMARY KEY, task_id TEXT NOT NULL, user_id TEXT NOT NULL, content TEXT NOT NULL, source_message_id TEXT, created_at INTEGER NOT NULL)")
            .execute(&pool).await.unwrap();

        pool
    }

    #[tokio::test]
    async fn test_create_and_get_task() {
        let pool = setup_db().await;
        let svc = TaskService::new(pool);

        let task = svc.create_task(
            "Test Task", Some("desc"), "user1", None, Some("g1"),
            TaskPriority::Medium, None, None, None,
        ).await.unwrap();

        assert_eq!(task.title, "Test Task");
        assert_eq!(task.status.as_str(), "backlog");
        assert_eq!(task.priority.as_str(), "medium");

        let fetched = svc.get_task(&task.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, task.id);
    }

    #[tokio::test]
    async fn test_transition_valid() {
        let pool = setup_db().await;
        let svc = TaskService::new(pool);

        let task = svc.create_task(
            "Task", None, "user1", None, Some("g1"), TaskPriority::Medium, None, None, None,
        ).await.unwrap();

        // backlog -> todo -> in_progress -> review -> done
        let updated = svc.transition_task(&task.id, TaskV3Status::Todo, "user1", None).await.unwrap();
        assert_eq!(updated.status.as_str(), "todo");

        let updated = svc.transition_task(&task.id, TaskV3Status::InProgress, "user1", None).await.unwrap();
        assert_eq!(updated.status.as_str(), "in_progress");

        let updated = svc.transition_task(&task.id, TaskV3Status::Review, "user1", None).await.unwrap();
        assert_eq!(updated.status.as_str(), "review");

        let updated = svc.transition_task(&task.id, TaskV3Status::Done, "user1", None).await.unwrap();
        assert_eq!(updated.status.as_str(), "done");
    }

    #[tokio::test]
    async fn test_transition_invalid() {
        let pool = setup_db().await;
        let svc = TaskService::new(pool);

        let task = svc.create_task(
            "Task", None, "user1", None, Some("g1"), TaskPriority::Medium, None, None, None,
        ).await.unwrap();

        // backlog -> done is invalid (must go through todo first)
        let result = svc.transition_task(&task.id, TaskV3Status::Done, "user1", None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_tasks_by_group() {
        let pool = setup_db().await;
        let svc = TaskService::new(pool);

        svc.create_task("A", None, "user1", None, Some("g1"), TaskPriority::Low, None, None, None).await.unwrap();
        svc.create_task("B", None, "user1", None, Some("g1"), TaskPriority::High, None, None, None).await.unwrap();
        svc.create_task("C", None, "user1", None, Some("g2"), TaskPriority::Medium, None, None, None).await.unwrap();

        let tasks = svc.list_tasks(Some("g1"), None, 10).await.unwrap();
        assert_eq!(tasks.len(), 2);
        // high priority should come first
        assert_eq!(tasks[0].priority.as_str(), "high");
    }

    #[tokio::test]
    async fn test_status_history() {
        let pool = setup_db().await;
        let svc = TaskService::new(pool);

        let task = svc.create_task(
            "Task", None, "user1", None, Some("g1"), TaskPriority::Medium, None, None, None,
        ).await.unwrap();

        svc.transition_task(&task.id, TaskV3Status::Todo, "user1", None).await.unwrap();
        svc.transition_task(&task.id, TaskV3Status::InProgress, "user1", None).await.unwrap();
        svc.transition_task(&task.id, TaskV3Status::Review, "user1", Some("ready")).await.unwrap();

        let history = svc.get_status_history(&task.id).await.unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].new_status, "todo");
        assert_eq!(history[1].new_status, "in_progress");
        assert_eq!(history[2].new_status, "review");
        assert_eq!(history[2].reason.as_deref(), Some("ready"));
    }

    #[tokio::test]
    async fn test_add_comment() {
        let pool = setup_db().await;
        let svc = TaskService::new(pool);

        let task = svc.create_task(
            "Task", None, "user1", None, Some("g1"), TaskPriority::Medium, None, None, None,
        ).await.unwrap();

        let comment_id = svc.add_comment(&task.id, "user1", "Looks good", None).await.unwrap();
        assert!(!comment_id.is_empty());
    }

    #[tokio::test]
    async fn test_valid_transitions() {
        assert!(TaskV3Status::Backlog.valid_transitions().contains(&TaskV3Status::Todo));
        assert!(TaskV3Status::Backlog.valid_transitions().contains(&TaskV3Status::Cancelled));
        assert_eq!(TaskV3Status::Done.valid_transitions(), &[] as &[TaskV3Status]);
        assert!(TaskV3Status::Todo.valid_transitions().contains(&TaskV3Status::InProgress));
        assert!(TaskV3Status::InProgress.valid_transitions().contains(&TaskV3Status::Review));
    }
}
