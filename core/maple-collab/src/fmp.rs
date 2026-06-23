use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FmpMessage {
    pub id: String,
    #[serde(rename = "type")]
    pub msg_type: FmpMessageType,
    pub channel: FmpChannel,
    pub sender: FmpSender,
    pub content: FmpContent,
    pub metadata: FmpMetadata,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FmpMessageType {
    Message,
    System,
    ToolCall,
    ToolResult,
    SkillCall,
    SkillResult,
    ApprovalRequest,
    ApprovalResponse,
    Heartbeat,
    Event,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FmpChannel {
    pub channel_id: String,
    pub channel_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FmpSender {
    pub id: String,
    #[serde(rename = "type")]
    pub sender_type: SenderType,
    pub name: String,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SenderType {
    Human,
    Agent,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FmpContent {
    pub text: Option<String>,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FmpMetadata {
    pub reply_to: Option<String>,
    pub thread_id: Option<String>,
    pub tags: Vec<String>,
    pub extra: HashMap<String, String>,
}

impl FmpMessage {
    pub fn new_human_message(channel_id: &str, sender_id: &str, sender_name: &str, text: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            msg_type: FmpMessageType::Message,
            channel: FmpChannel {
                channel_id: channel_id.to_string(),
                channel_type: "workspace".to_string(),
            },
            sender: FmpSender {
                id: sender_id.to_string(),
                sender_type: SenderType::Human,
                name: sender_name.to_string(),
                avatar: None,
            },
            content: FmpContent {
                text: Some(text.to_string()),
                data: None,
            },
            metadata: FmpMetadata {
                reply_to: None,
                thread_id: None,
                tags: Vec::new(),
                extra: HashMap::new(),
            },
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    pub fn new_agent_message(channel_id: &str, agent_id: &str, agent_name: &str, text: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            msg_type: FmpMessageType::Message,
            channel: FmpChannel {
                channel_id: channel_id.to_string(),
                channel_type: "workspace".to_string(),
            },
            sender: FmpSender {
                id: agent_id.to_string(),
                sender_type: SenderType::Agent,
                name: agent_name.to_string(),
                avatar: None,
            },
            content: FmpContent {
                text: Some(text.to_string()),
                data: None,
            },
            metadata: FmpMetadata {
                reply_to: None,
                thread_id: None,
                tags: Vec::new(),
                extra: HashMap::new(),
            },
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}
