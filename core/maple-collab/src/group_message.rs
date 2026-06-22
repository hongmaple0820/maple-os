use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMessage {
    pub id: String,
    pub group_id: String,
    pub sender_id: String,
    pub sender_type: String,
    pub message_type: MessageType,
    pub content: String,
    pub reply_to_id: Option<String>,
    pub thread_root_id: Option<String>,
    pub thread_reply_count: i64,
    pub source_channel: String,
    pub external_message_id: Option<String>,
    pub pinned: bool,
    pub edited_at: Option<i64>,
    pub created_at: i64,
    pub attachment_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    Text,
    Markdown,
    Image,
    File,
    Voice,
    ToolCall,
    ToolResult,
    Thinking,
    ApprovalRequest,
    ApprovalResponse,
    WorkflowRun,
    WorkflowStep,
    WorkflowComplete,
    WorkflowFailed,
    SkillCall,
    SkillResult,
    TaskCreated,
    TaskUpdated,
    TaskCompleted,
    System,
    MemberJoin,
    MemberLeave,
    CronTrigger,
    ExternalMessage,
}

impl MessageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Markdown => "markdown",
            Self::Image => "image",
            Self::File => "file",
            Self::Voice => "voice",
            Self::ToolCall => "tool_call",
            Self::ToolResult => "tool_result",
            Self::Thinking => "thinking",
            Self::ApprovalRequest => "approval_request",
            Self::ApprovalResponse => "approval_response",
            Self::WorkflowRun => "workflow_run",
            Self::WorkflowStep => "workflow_step",
            Self::WorkflowComplete => "workflow_complete",
            Self::WorkflowFailed => "workflow_failed",
            Self::SkillCall => "skill_call",
            Self::SkillResult => "skill_result",
            Self::TaskCreated => "task_created",
            Self::TaskUpdated => "task_updated",
            Self::TaskCompleted => "task_completed",
            Self::System => "system",
            Self::MemberJoin => "member_join",
            Self::MemberLeave => "member_leave",
            Self::CronTrigger => "cron_trigger",
            Self::ExternalMessage => "external_message",
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "markdown" => Self::Markdown,
            "image" => Self::Image,
            "file" => Self::File,
            "voice" => Self::Voice,
            "tool_call" => Self::ToolCall,
            "tool_result" => Self::ToolResult,
            "thinking" => Self::Thinking,
            "approval_request" => Self::ApprovalRequest,
            "approval_response" => Self::ApprovalResponse,
            "workflow_run" => Self::WorkflowRun,
            "workflow_step" => Self::WorkflowStep,
            "workflow_complete" => Self::WorkflowComplete,
            "workflow_failed" => Self::WorkflowFailed,
            "skill_call" => Self::SkillCall,
            "skill_result" => Self::SkillResult,
            "task_created" => Self::TaskCreated,
            "task_updated" => Self::TaskUpdated,
            "task_completed" => Self::TaskCompleted,
            "system" => Self::System,
            "member_join" => Self::MemberJoin,
            "member_leave" => Self::MemberLeave,
            "cron_trigger" => Self::CronTrigger,
            "external_message" => Self::ExternalMessage,
            _ => Self::Text,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePage {
    pub messages: Vec<GroupMessage>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

pub struct GroupMessageManager {
    pool: SqlitePool,
}

impl GroupMessageManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn send_message(
        &self,
        group_id: &str,
        sender_id: &str,
        sender_type: &str,
        message_type: MessageType,
        content: &str,
        reply_to_id: Option<&str>,
        thread_root_id: Option<&str>,
        source_channel: &str,
    ) -> Result<GroupMessage> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO group_messages (id, group_id, sender_id, sender_type, message_type, content, reply_to_id, thread_root_id, source_channel, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(group_id)
        .bind(sender_id)
        .bind(sender_type)
        .bind(message_type.as_str())
        .bind(content)
        .bind(reply_to_id)
        .bind(thread_root_id)
        .bind(source_channel)
        .bind(now)
        .execute(&self.pool)
        .await?;

        // Update group message count + unread counts
        sqlx::query(
            "UPDATE groups SET message_count = message_count + 1, updated_at = ? WHERE id = ?"
        )
        .bind(now)
        .bind(group_id)
        .execute(&self.pool)
        .await?;

        // Update unread counts for all group members except sender
        sqlx::query(
            "INSERT INTO group_unread_counts (group_id, user_id, unread_count)
             SELECT ?, member_id, 1 FROM group_members WHERE group_id = ? AND member_id != ?
             ON CONFLICT(group_id, user_id) DO UPDATE SET unread_count = unread_count + 1"
        )
        .bind(group_id)
        .bind(group_id)
        .bind(sender_id)
        .execute(&self.pool)
        .await?;

        // Increment thread reply count if this is a thread reply
        if let Some(root_id) = thread_root_id {
            sqlx::query(
                "UPDATE group_messages SET thread_reply_count = thread_reply_count + 1 WHERE id = ?"
            )
            .bind(root_id)
            .execute(&self.pool)
            .await?;
        }

        Ok(GroupMessage {
            id,
            group_id: group_id.to_string(),
            sender_id: sender_id.to_string(),
            sender_type: sender_type.to_string(),
            message_type,
            content: content.to_string(),
            reply_to_id: reply_to_id.map(|s| s.to_string()),
            thread_root_id: thread_root_id.map(|s| s.to_string()),
            thread_reply_count: 0,
            source_channel: source_channel.to_string(),
            external_message_id: None,
            pinned: false,
            edited_at: None,
            created_at: now,
            attachment_id: None,
        })
    }

    pub async fn get_messages(&self, group_id: &str, limit: i64, before: Option<i64>) -> Result<MessagePage> {
        let limit_with_extra = limit + 1;
        type MsgRow = (String, String, String, String, String, String, Option<String>, Option<String>, i64, String, Option<String>, i64, Option<i64>, i64, Option<String>);
        let rows = if let Some(before_ts) = before {
            sqlx::query_as::<_, MsgRow>(
                "SELECT m.id, m.group_id, m.sender_id, m.sender_type, m.message_type, m.content, m.reply_to_id, m.thread_root_id, m.thread_reply_count, m.source_channel, m.external_message_id, m.pinned, m.edited_at, m.created_at, a.id as attachment_id
                 FROM group_messages m LEFT JOIN message_attachments a ON a.message_id = m.id
                 WHERE m.group_id = ? AND m.created_at < ? AND m.deleted_at IS NULL
                 ORDER BY m.created_at DESC LIMIT ?"
            )
            .bind(group_id)
            .bind(before_ts)
            .bind(limit_with_extra)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, MsgRow>(
                "SELECT m.id, m.group_id, m.sender_id, m.sender_type, m.message_type, m.content, m.reply_to_id, m.thread_root_id, m.thread_reply_count, m.source_channel, m.external_message_id, m.pinned, m.edited_at, m.created_at, a.id as attachment_id
                 FROM group_messages m LEFT JOIN message_attachments a ON a.message_id = m.id
                 WHERE m.group_id = ? AND m.deleted_at IS NULL
                 ORDER BY m.created_at DESC LIMIT ?"
            )
            .bind(group_id)
            .bind(limit_with_extra)
            .fetch_all(&self.pool)
            .await?
        };

        let has_more = rows.len() as i64 > limit;
        let messages: Vec<GroupMessage> = rows.into_iter()
            .take(limit as usize)
            .map(|r| GroupMessage {
                id: r.0,
                group_id: r.1,
                sender_id: r.2,
                sender_type: r.3,
                message_type: MessageType::parse_str(&r.4),
                content: r.5,
                reply_to_id: r.6,
                thread_root_id: r.7,
                thread_reply_count: r.8,
                source_channel: r.9,
                external_message_id: r.10,
                pinned: r.11 != 0,
                edited_at: r.12,
                created_at: r.13,
                attachment_id: r.14,
            })
            .collect();

        let next_cursor = if has_more {
            messages.last().map(|m| m.created_at.to_string())
        } else {
            None
        };

        Ok(MessagePage { messages, has_more, next_cursor })
    }

    pub async fn get_message(&self, message_id: &str) -> Result<Option<GroupMessage>> {
        let row = sqlx::query_as::<_, (String, String, String, String, String, String, Option<String>, Option<String>, i64, String, Option<String>, i64, Option<i64>, i64, Option<String>)>(
            "SELECT m.id, m.group_id, m.sender_id, m.sender_type, m.message_type, m.content, m.reply_to_id, m.thread_root_id, m.thread_reply_count, m.source_channel, m.external_message_id, m.pinned, m.edited_at, m.created_at, a.id as attachment_id
             FROM group_messages m LEFT JOIN message_attachments a ON a.message_id = m.id
             WHERE m.id = ? AND m.deleted_at IS NULL"
        )
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| GroupMessage {
            id: r.0,
            group_id: r.1,
            sender_id: r.2,
            sender_type: r.3,
            message_type: MessageType::parse_str(&r.4),
            content: r.5,
            reply_to_id: r.6,
            thread_root_id: r.7,
            thread_reply_count: r.8,
            source_channel: r.9,
            external_message_id: r.10,
            pinned: r.11 != 0,
            edited_at: r.12,
            created_at: r.13,
            attachment_id: r.14,
        }))
    }

    pub async fn edit_message(&self, message_id: &str, editor_id: &str, new_content: &str) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();

        // Save edit history
        let existing = sqlx::query_as::<_, (String,)>(
            "SELECT content FROM group_messages WHERE id = ? AND deleted_at IS NULL"
        )
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((old_content,)) = existing {
            let history_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO message_edit_history (id, message_id, old_content, edited_by, edited_at) VALUES (?, ?, ?, ?, ?)"
            )
            .bind(&history_id)
            .bind(message_id)
            .bind(&old_content)
            .bind(editor_id)
            .bind(now)
            .execute(&self.pool)
            .await?;
        }

        let result = sqlx::query(
            "UPDATE group_messages SET content = ?, edited_at = ? WHERE id = ? AND deleted_at IS NULL"
        )
        .bind(new_content)
        .bind(now)
        .bind(message_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_message(&self, message_id: &str) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query(
            "UPDATE group_messages SET deleted_at = ? WHERE id = ? AND deleted_at IS NULL"
        )
        .bind(now)
        .bind(message_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn mark_as_read(&self, group_id: &str, user_id: &str, message_id: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO message_reads (message_id, user_id, read_at) VALUES (?, ?, ?)
             ON CONFLICT(message_id, user_id) DO UPDATE SET read_at = ?"
        )
        .bind(message_id)
        .bind(user_id)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        // Reset unread count
        sqlx::query(
            "UPDATE group_unread_counts SET unread_count = 0, last_read_message_id = ?, last_read_at = ?
             WHERE group_id = ? AND user_id = ?"
        )
        .bind(message_id)
        .bind(now)
        .bind(group_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn add_reaction(&self, message_id: &str, user_id: &str, emoji: &str) -> Result<bool> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        let result = sqlx::query(
            "INSERT OR IGNORE INTO message_reactions (id, message_id, user_id, emoji, created_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(message_id)
        .bind(user_id)
        .bind(emoji)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn remove_reaction(&self, message_id: &str, user_id: &str, emoji: &str) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM message_reactions WHERE message_id = ? AND user_id = ? AND emoji = ?"
        )
        .bind(message_id)
        .bind(user_id)
        .bind(emoji)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn pin_message(&self, message_id: &str, group_id: &str, pinned_by: &str) -> Result<bool> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        let result = sqlx::query(
            "INSERT OR IGNORE INTO pinned_messages (id, message_id, group_id, pinned_by, pinned_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(message_id)
        .bind(group_id)
        .bind(pinned_by)
        .bind(now)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() > 0 {
            sqlx::query("UPDATE group_messages SET pinned = 1 WHERE id = ?")
                .bind(message_id)
                .execute(&self.pool)
                .await?;
        }

        Ok(result.rows_affected() > 0)
    }

    pub async fn unpin_message(&self, message_id: &str) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM pinned_messages WHERE message_id = ?"
        )
        .bind(message_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() > 0 {
            sqlx::query("UPDATE group_messages SET pinned = 0 WHERE id = ?")
                .bind(message_id)
                .execute(&self.pool)
                .await?;
        }

        Ok(result.rows_affected() > 0)
    }

    pub async fn bookmark_message(&self, message_id: &str, user_id: &str, note: Option<&str>) -> Result<bool> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        let result = sqlx::query(
            "INSERT OR IGNORE INTO message_bookmarks (id, message_id, user_id, note, created_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(message_id)
        .bind(user_id)
        .bind(note)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_thread(&self, thread_root_id: &str, limit: i64) -> Result<Vec<GroupMessage>> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, String, Option<String>, Option<String>, i64, String, Option<String>, i64, Option<i64>, i64)>(
            "SELECT id, group_id, sender_id, sender_type, message_type, content, reply_to_id, thread_root_id, thread_reply_count, source_channel, external_message_id, pinned, edited_at, created_at
             FROM group_messages WHERE (id = ? OR thread_root_id = ?) AND deleted_at IS NULL
             ORDER BY created_at ASC LIMIT ?"
        )
        .bind(thread_root_id)
        .bind(thread_root_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| GroupMessage {
            id: r.0,
            group_id: r.1,
            sender_id: r.2,
            sender_type: r.3,
            message_type: MessageType::parse_str(&r.4),
            content: r.5,
            reply_to_id: r.6,
            thread_root_id: r.7,
            thread_reply_count: r.8,
            source_channel: r.9,
            external_message_id: r.10,
            pinned: r.11 != 0,
            edited_at: r.12,
            created_at: r.13,
            attachment_id: None,
        }).collect())
    }

    pub async fn search_messages(&self, group_id: &str, query: &str, limit: i64) -> Result<Vec<GroupMessage>> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, String, Option<String>, Option<String>, i64, String, Option<String>, i64, Option<i64>, i64)>(
            "SELECT m.id, m.group_id, m.sender_id, m.sender_type, m.message_type, m.content, m.reply_to_id, m.thread_root_id, m.thread_reply_count, m.source_channel, m.external_message_id, m.pinned, m.edited_at, m.created_at
             FROM group_messages m
             INNER JOIN group_messages_fts fts ON m.rowid = fts.rowid
             WHERE fts.content MATCH ? AND m.group_id = ? AND m.deleted_at IS NULL
             ORDER BY m.created_at DESC LIMIT ?"
        )
        .bind(query)
        .bind(group_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| GroupMessage {
            id: r.0,
            group_id: r.1,
            sender_id: r.2,
            sender_type: r.3,
            message_type: MessageType::parse_str(&r.4),
            content: r.5,
            reply_to_id: r.6,
            thread_root_id: r.7,
            thread_reply_count: r.8,
            source_channel: r.9,
            external_message_id: r.10,
            pinned: r.11 != 0,
            edited_at: r.12,
            created_at: r.13,
            attachment_id: None,
        }).collect())
    }
}
