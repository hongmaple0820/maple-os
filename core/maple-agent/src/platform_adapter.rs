use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Platform Adapter Framework — cross-platform messaging abstraction
///
/// Provides a unified interface for messaging platforms:
/// - Adapter trait with capabilities discovery
/// - Registry for managing multiple adapters
/// - Session persistence across platforms
/// - Message normalization and routing
///
///   Platform capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformCapabilities {
    /// Platform name
    pub name: String,
    /// Supports rich text / markdown
    pub rich_text: bool,
    /// Supports file attachments
    pub file_attachments: bool,
    /// Supports reactions (emoji)
    pub reactions: bool,
    /// Supports threads / replies
    pub threads: bool,
    /// Supports voice messages
    pub voice: bool,
    /// Maximum message length
    pub max_message_length: Option<usize>,
    /// Supported attachment MIME types
    pub supported_mime_types: Vec<String>,
    /// Whether platform supports group chats
    pub group_chats: bool,
    /// Whether platform supports direct messages
    pub direct_messages: bool,
}

impl Default for PlatformCapabilities {
    fn default() -> Self {
        Self {
            name: String::new(),
            rich_text: false,
            file_attachments: false,
            reactions: false,
            threads: false,
            voice: false,
            max_message_length: Some(4096),
            supported_mime_types: Vec::new(),
            group_chats: true,
            direct_messages: true,
        }
    }
}

/// Normalized message across all platforms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformMessage {
    pub id: String,
    pub platform: String,
    pub channel_id: String,
    pub sender_id: String,
    pub sender_name: Option<String>,
    pub content: MessageContent,
    pub reply_to: Option<String>,
    pub timestamp: i64,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Message content types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageContent {
    Text(String),
    RichText { text: String, format: TextFormat },
    Image { url: String, caption: Option<String> },
    File { url: String, name: String, mime_type: String },
    Audio { url: String, duration_ms: Option<u64> },
    Interactive { blocks: Vec<InteractiveBlock> },
}

/// Text formatting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextFormat {
    Plain,
    Markdown,
    Html,
}

/// Interactive block (for platforms supporting Block Kit / Card messages)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InteractiveBlock {
    Section { text: String },
    Divider,
    Button { label: String, action_id: String, value: String },
    Select { label: String, options: Vec<SelectOption>, action_id: String },
    Image { url: String, alt_text: String },
}

/// Select option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
}

/// Outbound message to send
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub content: MessageContent,
    pub reply_to: Option<String>,
    pub thread_id: Option<String>,
}

/// Session persistence key
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionKey {
    pub platform: String,
    pub user_id: String,
}

/// Cross-platform session state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformSession {
    pub key: SessionKey,
    pub display_name: Option<String>,
    pub conversation_id: Option<String>,
    pub context: HashMap<String, serde_json::Value>,
    pub last_active: i64,
}

/// Adapter error
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("platform not found: {0}")]
    PlatformNotFound(String),
    #[error("capability not supported: {0}")]
    UnsupportedCapability(String),
    #[error("message too long: {len} > {max}")]
    MessageTooLong { len: usize, max: usize },
    #[error("adapter error: {0}")]
    Internal(String),
}

/// Platform adapter trait
#[async_trait::async_trait]
pub trait PlatformAdapter: Send + Sync {
    /// Platform identifier
    fn platform_id(&self) -> &str;

    /// Platform capabilities
    fn capabilities(&self) -> PlatformCapabilities;

    /// Send a message to a channel
    async fn send_message(
        &self,
        channel_id: &str,
        message: &OutboundMessage,
    ) -> Result<String, AdapterError>;

    /// Send a direct message to a user
    async fn send_direct(
        &self,
        user_id: &str,
        message: &OutboundMessage,
    ) -> Result<String, AdapterError>;

    /// Edit an existing message
    async fn edit_message(
        &self,
        channel_id: &str,
        message_id: &str,
        new_content: &MessageContent,
    ) -> Result<(), AdapterError>;

    /// Delete a message
    async fn delete_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> Result<(), AdapterError>;

    /// Add a reaction to a message
    async fn add_reaction(
        &self,
        channel_id: &str,
        message_id: &str,
        emoji: &str,
    ) -> Result<(), AdapterError> {
        let _ = (channel_id, message_id, emoji);
        Err(AdapterError::UnsupportedCapability("reactions".into()))
    }

    /// Get user info
    async fn get_user_info(&self, user_id: &str) -> Result<UserInfo, AdapterError>;

    /// Get channel info
    async fn get_channel_info(&self, channel_id: &str) -> Result<ChannelInfo, AdapterError>;

    /// List channels the bot has access to
    async fn list_channels(&self) -> Result<Vec<ChannelInfo>, AdapterError> {
        Ok(Vec::new())
    }
}

/// User info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub is_bot: bool,
}

/// Channel info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub channel_type: ChannelType,
    pub member_count: Option<usize>,
}

/// Channel type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelType {
    Direct,
    Group,
    Public,
    Private,
}

/// Platform adapter registry
pub struct PlatformRegistry {
    adapters: HashMap<String, Arc<dyn PlatformAdapter>>,
    sessions: HashMap<SessionKey, PlatformSession>,
}

impl PlatformRegistry {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
            sessions: HashMap::new(),
        }
    }

    /// Register a platform adapter
    pub fn register(&mut self, adapter: Arc<dyn PlatformAdapter>) {
        let id = adapter.platform_id().to_string();
        self.adapters.insert(id, adapter);
    }

    /// Get an adapter by platform ID
    pub fn get(&self, platform_id: &str) -> Option<&Arc<dyn PlatformAdapter>> {
        self.adapters.get(platform_id)
    }

    /// Get all registered platform IDs
    pub fn platform_ids(&self) -> Vec<&str> {
        self.adapters.keys().map(|s| s.as_str()).collect()
    }

    /// Get capabilities for a platform
    pub fn capabilities(&self, platform_id: &str) -> Option<PlatformCapabilities> {
        self.adapters.get(platform_id).map(|a| a.capabilities())
    }

    /// Get all capabilities
    pub fn all_capabilities(&self) -> HashMap<String, PlatformCapabilities> {
        self.adapters
            .iter()
            .map(|(id, adapter)| (id.clone(), adapter.capabilities()))
            .collect()
    }

    /// Find adapters that support a specific capability
    pub fn with_capability(&self, check: impl Fn(&PlatformCapabilities) -> bool) -> Vec<&str> {
        self.adapters
            .iter()
            .filter(|(_, adapter)| check(&adapter.capabilities()))
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// Send a message via the appropriate platform adapter
    pub async fn route_message(
        &self,
        platform_id: &str,
        channel_id: &str,
        message: &OutboundMessage,
    ) -> Result<String, AdapterError> {
        let adapter = self
            .adapters
            .get(platform_id)
            .ok_or_else(|| AdapterError::PlatformNotFound(platform_id.into()))?;

        // Validate message length
        if let Some(max) = adapter.capabilities().max_message_length {
            let len = message_content_len(&message.content);
            if len > max {
                return Err(AdapterError::MessageTooLong { len, max });
            }
        }

        adapter.send_message(channel_id, message).await
    }

    /// Update or create a session
    pub fn upsert_session(&mut self, session: PlatformSession) {
        self.sessions.insert(session.key.clone(), session);
    }

    /// Get a session
    pub fn get_session(&self, key: &SessionKey) -> Option<&PlatformSession> {
        self.sessions.get(key)
    }

    /// Get sessions for a user across all platforms
    pub fn user_sessions(&self, user_id: &str) -> Vec<&PlatformSession> {
        self.sessions
            .values()
            .filter(|s| s.key.user_id == user_id)
            .collect()
    }

    /// Number of registered adapters
    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }
}

impl Default for PlatformRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn message_content_len(content: &MessageContent) -> usize {
    match content {
        MessageContent::Text(s) => s.len(),
        MessageContent::RichText { text, .. } => text.len(),
        MessageContent::Image { caption, .. } => caption.as_ref().map_or(0, |c| c.len()),
        MessageContent::File { name, .. } => name.len(),
        MessageContent::Audio { .. } => 0,
        MessageContent::Interactive { blocks } => blocks
            .iter()
            .map(|b| match b {
                InteractiveBlock::Section { text } => text.len(),
                InteractiveBlock::Button { label, .. } => label.len(),
                InteractiveBlock::Select { label, .. } => label.len(),
                InteractiveBlock::Image { alt_text, .. } => alt_text.len(),
                InteractiveBlock::Divider => 0,
            })
            .sum(),
    }
}

/// Mock adapter for testing
pub struct MockAdapter {
    platform_id: String,
    capabilities: PlatformCapabilities,
    sent: std::sync::Mutex<Vec<(String, OutboundMessage)>>,
}

impl MockAdapter {
    pub fn new(platform_id: &str) -> Self {
        Self {
            platform_id: platform_id.into(),
            capabilities: PlatformCapabilities {
                name: platform_id.into(),
                rich_text: true,
                file_attachments: true,
                reactions: true,
                threads: true,
                max_message_length: Some(4096),
                ..Default::default()
            },
            sent: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn with_capabilities(mut self, caps: PlatformCapabilities) -> Self {
        self.capabilities = caps;
        self
    }

    pub fn sent_messages(&self) -> Vec<(String, OutboundMessage)> {
        self.sent.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl PlatformAdapter for MockAdapter {
    fn platform_id(&self) -> &str {
        &self.platform_id
    }

    fn capabilities(&self) -> PlatformCapabilities {
        self.capabilities.clone()
    }

    async fn send_message(
        &self,
        channel_id: &str,
        message: &OutboundMessage,
    ) -> Result<String, AdapterError> {
        let msg_id = format!("msg_{}", uuid::Uuid::new_v4());
        self.sent
            .lock()
            .unwrap()
            .push((channel_id.to_string(), message.clone()));
        Ok(msg_id)
    }

    async fn send_direct(
        &self,
        user_id: &str,
        message: &OutboundMessage,
    ) -> Result<String, AdapterError> {
        let msg_id = format!("dm_{}", uuid::Uuid::new_v4());
        self.sent
            .lock()
            .unwrap()
            .push((user_id.to_string(), message.clone()));
        Ok(msg_id)
    }

    async fn edit_message(
        &self,
        _channel_id: &str,
        _message_id: &str,
        _new_content: &MessageContent,
    ) -> Result<(), AdapterError> {
        Ok(())
    }

    async fn delete_message(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> Result<(), AdapterError> {
        Ok(())
    }

    async fn add_reaction(
        &self,
        _channel_id: &str,
        _message_id: &str,
        _emoji: &str,
    ) -> Result<(), AdapterError> {
        Ok(())
    }

    async fn get_user_info(&self, user_id: &str) -> Result<UserInfo, AdapterError> {
        Ok(UserInfo {
            id: user_id.into(),
            name: format!("User {}", user_id),
            avatar_url: None,
            is_bot: false,
        })
    }

    async fn get_channel_info(&self, channel_id: &str) -> Result<ChannelInfo, AdapterError> {
        Ok(ChannelInfo {
            id: channel_id.into(),
            name: format!("Channel {}", channel_id),
            channel_type: ChannelType::Group,
            member_count: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_adapter(id: &str) -> Arc<dyn PlatformAdapter> {
        Arc::new(MockAdapter::new(id))
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut reg = PlatformRegistry::new();
        assert!(reg.is_empty());

        reg.register(mock_adapter("telegram"));
        reg.register(mock_adapter("discord"));

        assert_eq!(reg.len(), 2);
        assert!(reg.get("telegram").is_some());
        assert!(reg.get("discord").is_some());
        assert!(reg.get("slack").is_none());
    }

    #[test]
    fn test_registry_platform_ids() {
        let mut reg = PlatformRegistry::new();
        reg.register(mock_adapter("telegram"));
        reg.register(mock_adapter("feishu"));

        let mut ids = reg.platform_ids();
        ids.sort();
        assert_eq!(ids, vec!["feishu", "telegram"]);
    }

    #[test]
    fn test_capabilities() {
        let mut reg = PlatformRegistry::new();
        reg.register(mock_adapter("telegram"));

        let caps = reg.capabilities("telegram").unwrap();
        assert_eq!(caps.name, "telegram");
        assert!(caps.rich_text);
        assert!(caps.reactions);
        assert_eq!(caps.max_message_length, Some(4096));
    }

    #[test]
    fn test_all_capabilities() {
        let mut reg = PlatformRegistry::new();
        reg.register(mock_adapter("telegram"));
        reg.register(mock_adapter("slack"));

        let all = reg.all_capabilities();
        assert_eq!(all.len(), 2);
        assert!(all.contains_key("telegram"));
        assert!(all.contains_key("slack"));
    }

    #[test]
    fn test_with_capability() {
        let mut reg = PlatformRegistry::new();
        reg.register(mock_adapter("telegram"));
        reg.register(
            Arc::new(
                MockAdapter::new("sms")
                    .with_capabilities(PlatformCapabilities {
                        name: "sms".into(),
                        rich_text: false,
                        file_attachments: false,
                        reactions: false,
                        threads: false,
                        max_message_length: Some(160),
                        ..Default::default()
                    }),
            ),
        );

        let rich = reg.with_capability(|c| c.rich_text);
        assert_eq!(rich.len(), 1);
        assert_eq!(rich[0], "telegram");

        let all = reg.with_capability(|c| c.direct_messages);
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_route_message() {
        let mut reg = PlatformRegistry::new();
        reg.register(mock_adapter("telegram"));

        let msg = OutboundMessage {
            content: MessageContent::Text("hello".into()),
            reply_to: None,
            thread_id: None,
        };

        let id = reg.route_message("telegram", "ch1", &msg).await.unwrap();
        assert!(id.starts_with("msg_"));
    }

    #[tokio::test]
    async fn test_route_message_platform_not_found() {
        let reg = PlatformRegistry::new();
        let msg = OutboundMessage {
            content: MessageContent::Text("hello".into()),
            reply_to: None,
            thread_id: None,
        };

        let result = reg.route_message("unknown", "ch1", &msg).await;
        assert!(matches!(result, Err(AdapterError::PlatformNotFound(_))));
    }

    #[tokio::test]
    async fn test_route_message_too_long() {
        let mut reg = PlatformRegistry::new();
        reg.register(
            Arc::new(
                MockAdapter::new("sms").with_capabilities(PlatformCapabilities {
                    name: "sms".into(),
                    max_message_length: Some(10),
                    ..Default::default()
                }),
            ),
        );

        let msg = OutboundMessage {
            content: MessageContent::Text("this is a very long message".into()),
            reply_to: None,
            thread_id: None,
        };

        let result = reg.route_message("sms", "ch1", &msg).await;
        assert!(matches!(result, Err(AdapterError::MessageTooLong { .. })));
    }

    #[tokio::test]
    async fn test_mock_adapter_send() {
        let adapter = MockAdapter::new("test");
        let msg = OutboundMessage {
            content: MessageContent::Text("hello".into()),
            reply_to: None,
            thread_id: None,
        };

        let id = adapter.send_message("ch1", &msg).await.unwrap();
        assert!(id.starts_with("msg_"));

        let sent = adapter.sent_messages();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "ch1");
    }

    #[tokio::test]
    async fn test_mock_adapter_reactions() {
        let adapter = MockAdapter::new("test");
        // MockAdapter supports reactions
        adapter
            .add_reaction("ch1", "msg1", "👍")
            .await
            .unwrap();
    }

    #[test]
    fn test_session_management() {
        let mut reg = PlatformRegistry::new();
        let key = SessionKey {
            platform: "telegram".into(),
            user_id: "user1".into(),
        };

        reg.upsert_session(PlatformSession {
            key: key.clone(),
            display_name: Some("Alice".into()),
            conversation_id: Some("conv1".into()),
            context: HashMap::new(),
            last_active: 1000,
        });

        let session = reg.get_session(&key).unwrap();
        assert_eq!(session.display_name.as_deref(), Some("Alice"));

        let user_sessions = reg.user_sessions("user1");
        assert_eq!(user_sessions.len(), 1);
    }

    #[test]
    fn test_message_content_len() {
        assert_eq!(message_content_len(&MessageContent::Text("hello".into())), 5);

        let rich = MessageContent::RichText {
            text: "bold".into(),
            format: TextFormat::Markdown,
        };
        assert_eq!(message_content_len(&rich), 4);

        let img = MessageContent::Image {
            url: "http://img.png".into(),
            caption: Some("photo".into()),
        };
        assert_eq!(message_content_len(&img), 5);
    }

    #[test]
    fn test_channel_type_variants() {
        assert_ne!(ChannelType::Direct, ChannelType::Group);
        assert_ne!(ChannelType::Public, ChannelType::Private);
    }
}
