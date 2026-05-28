use crate::react_loop::Session;
use anyhow::Result;
use maple_llm::request::Message;

pub struct SessionStore {
    db: sqlx::SqlitePool,
}

impl SessionStore {
    pub fn new(db: sqlx::SqlitePool) -> Self {
        Self { db }
    }

    pub async fn load_session(&self, session_id: &str) -> Result<Session> {
        self.load_session_with_limit(session_id, None).await
    }

    /// Load session with optional message limit
    /// If max_messages is set, only the most recent N messages are loaded
    /// System prompt is always preserved
    pub async fn load_session_with_limit(
        &self,
        session_id: &str,
        max_messages: Option<usize>,
    ) -> Result<Session> {
        let rows = sqlx::query_as::<_, (String, String, String)>(
            "SELECT role, content, metadata FROM chat_messages WHERE session_id = ? ORDER BY created_at ASC"
        )
        .bind(session_id)
        .fetch_all(&self.db)
        .await?;

        if rows.is_empty() {
            return Ok(Session::new(
                "You are a helpful assistant. Use the provided tools when needed.",
            ));
        }

        let mut messages = Vec::new();
        for (role, content, metadata) in &rows {
            let meta: Option<serde_json::Value> = serde_json::from_str(metadata).ok();
            let tool_call_id = meta
                .as_ref()
                .and_then(|m| m["tool_call_id"].as_str())
                .map(|s| s.to_string());
            let tool_calls = meta
                .as_ref()
                .and_then(|m| m["tool_calls"].as_array())
                .cloned();

            let msg = Message {
                role: role.clone(),
                content: content.clone(),
                tool_call_id,
                tool_calls,
            };
            messages.push(msg);
        }

        // Apply message window limit if specified
        if let Some(limit) = max_messages {
            if messages.len() > limit {
                // Preserve system prompt (first message if it's system role)
                let system_prompt = if messages.first().map_or(false, |m| m.role == "system") {
                    Some(messages.remove(0))
                } else {
                    None
                };

                // Keep only the most recent messages
                let start = messages.len().saturating_sub(limit);
                messages = messages[start..].to_vec();

                // Re-insert system prompt at the beginning
                if let Some(prompt) = system_prompt {
                    messages.insert(0, prompt);
                }
            }
        }

        let token_count: usize = messages
            .iter()
            .map(|m| maple_llm::token_counter::count_message_tokens(&m.content, &m.role))
            .sum();
        Ok(Session {
            messages,
            input_token_count: token_count,
        })
    }

    pub async fn save_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        tool_call_id: Option<&str>,
        tool_calls: Option<&[serde_json::Value]>,
    ) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        let metadata = serde_json::json!({
            "tool_call_id": tool_call_id,
            "tool_calls": tool_calls,
        });

        sqlx::query(
            "INSERT INTO chat_messages (id, session_id, role, content, metadata, created_at) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(session_id)
        .bind(role)
        .bind(content)
        .bind(metadata.to_string())
        .bind(now)
        .execute(&self.db)
        .await?;

        Ok(())
    }

    pub async fn save_session_messages(&self, session_id: &str, session: &Session) -> Result<()> {
        for msg in &session.messages {
            self.save_message(
                session_id,
                &msg.role,
                &msg.content,
                msg.tool_call_id.as_deref(),
                msg.tool_calls.as_deref(),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn list_sessions(&self, limit: i64) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT session_id, MAX(created_at) as last_active FROM chat_messages GROUP BY session_id ORDER BY last_active DESC LIMIT ?"
        )
        .bind(limit)
        .fetch_all(&self.db)
        .await?;

        Ok(rows
            .iter()
            .map(|(id, ts)| {
                serde_json::json!({
                    "session_id": id,
                    "last_active": ts,
                })
            })
            .collect())
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM chat_messages WHERE session_id = ?")
            .bind(session_id)
            .execute(&self.db)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_session_messages(&self, session_id: &str) -> Result<Vec<SessionMessage>> {
        let rows = sqlx::query_as::<_, (String, String, i64)>(
            "SELECT role, content, created_at FROM chat_messages WHERE session_id = ? ORDER BY created_at ASC"
        )
        .bind(session_id)
        .fetch_all(&self.db)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(role, content, created_at)| SessionMessage {
                role,
                content,
                created_at,
            })
            .collect())
    }
}

pub struct SessionMessage {
    pub role: String,
    pub content: String,
    pub created_at: i64,
}
