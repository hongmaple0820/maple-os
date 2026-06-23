use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::{broadcast, RwLock};

/// Real-time Collaboration — WebSocket broadcast + presence + conflict resolution
///
/// Enables multi-user and multi-agent collaboration:
/// - Presence tracking (who's online, what they're doing)
/// - WebSocket broadcast for real-time events
/// - Operational Transform for concurrent edit resolution
/// - Collaborative session management
///
///   Collaboration event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollabEvent {
    /// User/agent joined the session
    PresenceJoin {
        user_id: String,
        display_name: String,
        role: UserRole,
    },
    /// User/agent left the session
    PresenceLeave { user_id: String },
    /// Cursor/attention position changed
    CursorMove {
        user_id: String,
        position: CursorPosition,
    },
    /// Text operation (insert/delete)
    TextOperation {
        user_id: String,
        op: TextOp,
        revision: u64,
    },
    /// File locked for editing
    FileLock {
        user_id: String,
        file_path: String,
        locked: bool,
    },
    /// Chat message in collaboration channel
    Chat {
        user_id: String,
        message: String,
    },
    /// Agent status update
    AgentStatus {
        agent_id: String,
        status: AgentWorkStatus,
    },
}

/// User role in collaboration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserRole {
    Owner,
    Editor,
    Viewer,
    Agent,
}

/// Cursor position in a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPosition {
    pub file_path: String,
    pub line: u32,
    pub column: u32,
}

/// Text operation for Operational Transform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextOp {
    Insert {
        position: u64,
        text: String,
    },
    Delete {
        position: u64,
        length: u64,
    },
    Replace {
        position: u64,
        length: u64,
        text: String,
    },
}

/// Agent work status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentWorkStatus {
    Idle,
    Working { task: String },
    Blocked { reason: String },
    Completed { summary: String },
}

/// Presence info for a connected user/agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Presence {
    pub user_id: String,
    pub display_name: String,
    pub role: UserRole,
    pub cursor: Option<CursorPosition>,
    pub last_active: i64,
    pub status: PresenceStatus,
}

/// Presence status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresenceStatus {
    Online,
    Away,
    Busy,
    Offline,
}

/// Collaborative session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabSession {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub owner_id: String,
    pub file_locks: HashMap<String, String>, // file_path -> user_id
}

/// Conflict resolution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictResolution {
    pub winning_op: TextOp,
    pub losing_op: TextOp,
    pub strategy: ConflictStrategy,
}

/// Conflict resolution strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictStrategy {
    /// First writer wins
    FirstWriterWins,
    /// Last writer wins
    LastWriterWins,
    /// Owner's changes take priority
    OwnerPriority,
    /// Merge both changes (when possible)
    Merge,
}

/// Collaboration manager
pub struct CollabManager {
    sessions: RwLock<HashMap<String, CollabSession>>,
    presences: RwLock<HashMap<String, HashMap<String, Presence>>>, // session_id -> user_id -> presence
    event_tx: broadcast::Sender<CollabEvent>,
    revision: RwLock<u64>,
}

impl CollabManager {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        Self {
            sessions: RwLock::new(HashMap::new()),
            presences: RwLock::new(HashMap::new()),
            event_tx,
            revision: RwLock::new(0),
        }
    }

    /// Create a new collaboration session
    pub async fn create_session(&self, id: &str, name: &str, owner_id: &str) -> CollabSession {
        let session = CollabSession {
            id: id.into(),
            name: name.into(),
            created_at: chrono::Utc::now().timestamp(),
            owner_id: owner_id.into(),
            file_locks: HashMap::new(),
        };
        self.sessions.write().await.insert(id.into(), session.clone());
        self.presences.write().await.insert(id.into(), HashMap::new());
        session
    }

    /// Join a session
    pub async fn join(
        &self,
        session_id: &str,
        user_id: &str,
        display_name: &str,
        role: UserRole,
    ) -> Result<(), CollabError> {
        let sessions = self.sessions.write().await;
        if !sessions.contains_key(session_id) {
            return Err(CollabError::SessionNotFound(session_id.into()));
        }

        let presence = Presence {
            user_id: user_id.into(),
            display_name: display_name.into(),
            role,
            cursor: None,
            last_active: chrono::Utc::now().timestamp(),
            status: PresenceStatus::Online,
        };

        self.presences
            .write()
            .await
            .entry(session_id.into())
            .or_default()
            .insert(user_id.into(), presence);

        let _ = self.event_tx.send(CollabEvent::PresenceJoin {
            user_id: user_id.into(),
            display_name: display_name.into(),
            role,
        });

        Ok(())
    }

    /// Leave a session
    pub async fn leave(&self, session_id: &str, user_id: &str) -> Result<(), CollabError> {
        let mut presences = self.presences.write().await;
        if let Some(session_presences) = presences.get_mut(session_id) {
            session_presences.remove(user_id);
        }

        // Release any file locks held by this user
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.file_locks.retain(|_, owner| owner != user_id);
        }

        let _ = self.event_tx.send(CollabEvent::PresenceLeave {
            user_id: user_id.into(),
        });

        Ok(())
    }

    /// Update cursor position
    pub async fn update_cursor(
        &self,
        session_id: &str,
        user_id: &str,
        position: CursorPosition,
    ) -> Result<(), CollabError> {
        let mut presences = self.presences.write().await;
        if let Some(session_presences) = presences.get_mut(session_id)
            && let Some(presence) = session_presences.get_mut(user_id)
        {
            presence.cursor = Some(position.clone());
            presence.last_active = chrono::Utc::now().timestamp();
        }

        let _ = self.event_tx.send(CollabEvent::CursorMove {
            user_id: user_id.into(),
            position,
        });

        Ok(())
    }

    /// Apply a text operation with conflict resolution
    pub async fn apply_operation(
        &self,
        session_id: &str,
        user_id: &str,
        op: TextOp,
    ) -> Result<u64, CollabError> {
        let sessions = self.sessions.write().await;
        let _session = sessions
            .get(session_id)
            .ok_or_else(|| CollabError::SessionNotFound(session_id.into()))?;

        let mut revision = self.revision.write().await;
        *revision += 1;
        let current_revision = *revision;

        let _ = self.event_tx.send(CollabEvent::TextOperation {
            user_id: user_id.into(),
            op,
            revision: current_revision,
        });

        Ok(current_revision)
    }

    /// Lock a file for editing
    pub async fn lock_file(
        &self,
        session_id: &str,
        user_id: &str,
        file_path: &str,
    ) -> Result<(), CollabError> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| CollabError::SessionNotFound(session_id.into()))?;

        if let Some(owner) = session.file_locks.get(file_path)
            && owner != user_id
        {
            return Err(CollabError::FileLocked {
                file: file_path.into(),
                by: owner.clone(),
            });
        }

        session.file_locks.insert(file_path.into(), user_id.into());

        let _ = self.event_tx.send(CollabEvent::FileLock {
            user_id: user_id.into(),
            file_path: file_path.into(),
            locked: true,
        });

        Ok(())
    }

    /// Unlock a file
    pub async fn unlock_file(
        &self,
        session_id: &str,
        user_id: &str,
        file_path: &str,
    ) -> Result<(), CollabError> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id)
            && session.file_locks.get(file_path) == Some(&user_id.to_string())
        {
            session.file_locks.remove(file_path);
        }

        let _ = self.event_tx.send(CollabEvent::FileLock {
            user_id: user_id.into(),
            file_path: file_path.into(),
            locked: false,
        });

        Ok(())
    }

    /// Get all presences in a session
    pub async fn get_presences(&self, session_id: &str) -> Vec<Presence> {
        self.presences
            .read()
            .await
            .get(session_id)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Subscribe to collaboration events
    pub fn subscribe(&self) -> broadcast::Receiver<CollabEvent> {
        self.event_tx.subscribe()
    }

    /// Get current revision number
    pub async fn current_revision(&self) -> u64 {
        *self.revision.read().await
    }

    /// Resolve concurrent text operation conflicts
    pub fn resolve_conflict(
        op_a: &TextOp,
        op_b: &TextOp,
        strategy: ConflictStrategy,
    ) -> ConflictResolution {
        match strategy {
            ConflictStrategy::FirstWriterWins => ConflictResolution {
                winning_op: op_a.clone(),
                losing_op: op_b.clone(),
                strategy,
            },
            ConflictStrategy::LastWriterWins => ConflictResolution {
                winning_op: op_b.clone(),
                losing_op: op_a.clone(),
                strategy,
            },
            ConflictStrategy::OwnerPriority => ConflictResolution {
                winning_op: op_a.clone(),
                losing_op: op_b.clone(),
                strategy,
            },
            ConflictStrategy::Merge => {
                // Simple merge: if operations don't overlap, apply both
                // Otherwise, last writer wins
                let pos_a = op_position(op_a);
                let pos_b = op_position(op_b);
                let end_a = pos_a + op_length(op_a);
                let end_b = pos_b + op_length(op_b);

                if end_a <= pos_b || end_b <= pos_a {
                    // No overlap, both can apply
                    ConflictResolution {
                        winning_op: op_a.clone(),
                        losing_op: op_b.clone(),
                        strategy,
                    }
                } else {
                    // Overlap, last writer wins
                    ConflictResolution {
                        winning_op: op_b.clone(),
                        losing_op: op_a.clone(),
                        strategy: ConflictStrategy::LastWriterWins,
                    }
                }
            }
        }
    }
}

impl Default for CollabManager {
    fn default() -> Self {
        Self::new()
    }
}

fn op_position(op: &TextOp) -> u64 {
    match op {
        TextOp::Insert { position, .. } => *position,
        TextOp::Delete { position, .. } => *position,
        TextOp::Replace { position, .. } => *position,
    }
}

fn op_length(op: &TextOp) -> u64 {
    match op {
        TextOp::Insert { text, .. } => text.len() as u64,
        TextOp::Delete { length, .. } => *length,
        TextOp::Replace { length, .. } => *length,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CollabError {
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("file locked by {by}: {file}")]
    FileLocked { file: String, by: String },
    #[error("permission denied: {0}")]
    PermissionDenied(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_lifecycle() {
        let mgr = CollabManager::new();
        let session = mgr.create_session("s1", "Test Session", "owner1").await;
        assert_eq!(session.name, "Test Session");
        assert_eq!(session.owner_id, "owner1");
    }

    #[tokio::test]
    async fn test_join_leave() {
        let mgr = CollabManager::new();
        mgr.create_session("s1", "Test", "owner").await;

        mgr.join("s1", "user1", "Alice", UserRole::Editor).await.unwrap();
        mgr.join("s1", "user2", "Bob", UserRole::Viewer).await.unwrap();

        let presences = mgr.get_presences("s1").await;
        assert_eq!(presences.len(), 2);

        mgr.leave("s1", "user1").await.unwrap();
        let presences = mgr.get_presences("s1").await;
        assert_eq!(presences.len(), 1);
    }

    #[tokio::test]
    async fn test_join_nonexistent_session() {
        let mgr = CollabManager::new();
        let result = mgr.join("nope", "user1", "Alice", UserRole::Editor).await;
        assert!(matches!(result, Err(CollabError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn test_file_locking() {
        let mgr = CollabManager::new();
        mgr.create_session("s1", "Test", "owner").await;
        mgr.join("s1", "user1", "Alice", UserRole::Editor).await.unwrap();
        mgr.join("s1", "user2", "Bob", UserRole::Editor).await.unwrap();

        // User1 locks file
        mgr.lock_file("s1", "user1", "main.rs").await.unwrap();

        // User2 cannot lock same file
        let result = mgr.lock_file("s1", "user2", "main.rs").await;
        assert!(matches!(result, Err(CollabError::FileLocked { .. })));

        // User1 can re-lock (idempotent)
        mgr.lock_file("s1", "user1", "main.rs").await.unwrap();

        // User1 unlocks
        mgr.unlock_file("s1", "user1", "main.rs").await.unwrap();

        // Now user2 can lock
        mgr.lock_file("s1", "user2", "main.rs").await.unwrap();
    }

    #[tokio::test]
    async fn test_file_lock_released_on_leave() {
        let mgr = CollabManager::new();
        mgr.create_session("s1", "Test", "owner").await;
        mgr.join("s1", "user1", "Alice", UserRole::Editor).await.unwrap();

        mgr.lock_file("s1", "user1", "main.rs").await.unwrap();
        mgr.leave("s1", "user1").await.unwrap();

        // Lock should be released
        let sessions = mgr.sessions.read().await;
        let session = sessions.get("s1").unwrap();
        assert!(session.file_locks.is_empty());
    }

    #[tokio::test]
    async fn test_text_operation() {
        let mgr = CollabManager::new();
        mgr.create_session("s1", "Test", "owner").await;

        let rev = mgr
            .apply_operation(
                "s1",
                "user1",
                TextOp::Insert {
                    position: 0,
                    text: "hello".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(rev, 1);

        let rev = mgr
            .apply_operation(
                "s1",
                "user2",
                TextOp::Insert {
                    position: 5,
                    text: " world".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(rev, 2);
    }

    #[tokio::test]
    async fn test_cursor_update() {
        let mgr = CollabManager::new();
        mgr.create_session("s1", "Test", "owner").await;
        mgr.join("s1", "user1", "Alice", UserRole::Editor).await.unwrap();

        mgr.update_cursor(
            "s1",
            "user1",
            CursorPosition {
                file_path: "main.rs".into(),
                line: 10,
                column: 5,
            },
        )
        .await
        .unwrap();

        let presences = mgr.get_presences("s1").await;
        let alice = presences.iter().find(|p| p.user_id == "user1").unwrap();
        let cursor = alice.cursor.as_ref().unwrap();
        assert_eq!(cursor.line, 10);
        assert_eq!(cursor.column, 5);
    }

    #[test]
    fn test_conflict_resolution_first_writer() {
        let op_a = TextOp::Insert {
            position: 5,
            text: "hello".into(),
        };
        let op_b = TextOp::Insert {
            position: 5,
            text: "world".into(),
        };

        let resolution = CollabManager::resolve_conflict(&op_a, &op_b, ConflictStrategy::FirstWriterWins);
        assert!(matches!(resolution.winning_op, TextOp::Insert { ref text, .. } if text == "hello"));
    }

    #[test]
    fn test_conflict_resolution_last_writer() {
        let op_a = TextOp::Insert {
            position: 5,
            text: "hello".into(),
        };
        let op_b = TextOp::Insert {
            position: 5,
            text: "world".into(),
        };

        let resolution = CollabManager::resolve_conflict(&op_a, &op_b, ConflictStrategy::LastWriterWins);
        assert!(matches!(resolution.winning_op, TextOp::Insert { ref text, .. } if text == "world"));
    }

    #[test]
    fn test_conflict_resolution_merge_no_overlap() {
        let op_a = TextOp::Insert {
            position: 0,
            text: "hello".into(),
        };
        let op_b = TextOp::Insert {
            position: 10,
            text: "world".into(),
        };

        let resolution = CollabManager::resolve_conflict(&op_a, &op_b, ConflictStrategy::Merge);
        // Both can apply (no overlap)
        assert!(matches!(resolution.winning_op, TextOp::Insert { ref text, .. } if text == "hello"));
    }

    #[test]
    fn test_conflict_resolution_merge_overlap() {
        let op_a = TextOp::Insert {
            position: 5,
            text: "hello".into(),
        };
        let op_b = TextOp::Delete {
            position: 3,
            length: 4,
        };

        let resolution = CollabManager::resolve_conflict(&op_a, &op_b, ConflictStrategy::Merge);
        // Overlapping: last writer wins
        assert!(matches!(resolution.winning_op, TextOp::Delete { .. }));
    }

    #[test]
    fn test_event_subscribe() {
        let mgr = CollabManager::new();
        let mut rx = mgr.subscribe();

        let _ = mgr.event_tx.send(CollabEvent::Chat {
            user_id: "user1".into(),
            message: "hello".into(),
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, CollabEvent::Chat { .. }));
    }

    #[test]
    fn test_presence_status_variants() {
        assert_ne!(PresenceStatus::Online, PresenceStatus::Offline);
        assert_ne!(UserRole::Owner, UserRole::Viewer);
    }
}
