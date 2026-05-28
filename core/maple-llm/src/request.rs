use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    CodeGeneration,
    ContentWriting,
    QuickQa,
    LongDocument,
    ImageUnderstanding,
    DataAnalysis,
    General,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyLevel {
    Public,
    Internal,
    Sensitive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Speed,
    Quality,
    Cost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
}

impl Message {
    pub fn user(content: &str) -> Self {
        Self {
            role: "user".to_string(),
            content: content.to_string(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn assistant(content: &str) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.to_string(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn assistant_with_tool_calls(content: &str, tool_calls: Vec<serde_json::Value>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: if content.is_empty() { " ".to_string() } else { content.to_string() },
            tool_call_id: None,
            tool_calls: Some(tool_calls),
        }
    }

    pub fn system(content: &str) -> Self {
        Self {
            role: "system".to_string(),
            content: content.to_string(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn tool_result(tool_use_id: &str, content: &str, is_error: bool) -> Self {
        Self {
            role: "tool".to_string(),
            content: if is_error {
                format!("Tool error: {}", content)
            } else {
                content.to_string()
            },
            tool_call_id: Some(tool_use_id.to_string()),
            tool_calls: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub messages: Vec<Message>,
    pub task_type: TaskType,
    pub privacy_level: PrivacyLevel,
    pub priority: Priority,
    pub max_tokens: Option<u32>,
    pub temperature: f32,
    pub has_image: bool,
    pub estimated_tokens: usize,
    pub tools: Option<Vec<ToolDefinition>>,
    pub requested_model: String,
}

impl LlmRequest {
    pub fn new(prompt: String, model_route: &str) -> Self {
        Self {
            messages: vec![Message::user(&prompt)],
            task_type: TaskType::General,
            privacy_level: PrivacyLevel::Public,
            priority: Priority::Quality,
            max_tokens: None,
            temperature: 0.7,
            has_image: false,
            estimated_tokens: prompt.len() / 4,
            tools: None,
            requested_model: model_route.to_string(),
        }
    }

    pub fn quick_qa(prompt: &str) -> Self {
        Self {
            messages: vec![Message::user(prompt)],
            task_type: TaskType::QuickQa,
            privacy_level: PrivacyLevel::Public,
            priority: Priority::Speed,
            max_tokens: Some(1024),
            temperature: 0.3,
            has_image: false,
            estimated_tokens: prompt.len() / 4,
            tools: None,
            requested_model: "default".to_string(),
        }
    }

    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = temp;
        self
    }

    pub fn with_privacy(mut self, level: PrivacyLevel) -> Self {
        self.privacy_level = level;
        self
    }

    pub fn with_task_type(mut self, task_type: TaskType) -> Self {
        self.task_type = task_type;
        self
    }

    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_user() {
        let m = Message::user("hello");
        assert_eq!(m.role, "user");
        assert_eq!(m.content, "hello");
        assert!(m.tool_call_id.is_none());
        assert!(m.tool_calls.is_none());
    }

    #[test]
    fn test_message_tool_result() {
        let m = Message::tool_result("call_123", "result data", false);
        assert_eq!(m.role, "tool");
        assert_eq!(m.tool_call_id, Some("call_123".to_string()));
        assert_eq!(m.content, "result data");
    }

    #[test]
    fn test_message_tool_result_error() {
        let m = Message::tool_result("call_456", "something failed", true);
        assert_eq!(m.role, "tool");
        assert_eq!(m.tool_call_id, Some("call_456".to_string()));
        assert!(m.content.starts_with("Tool error:"));
    }

    #[test]
    fn test_message_assistant_with_tool_calls() {
        let m = Message::assistant_with_tool_calls("", vec![
            serde_json::json!({"id": "call_1", "function": {"name": "test", "arguments": "{}"}}),
        ]);
        assert_eq!(m.role, "assistant");
        assert!(m.tool_calls.is_some());
        assert!(m.content == " ");
    }

    #[test]
    fn test_message_serialize() {
        let m = Message::tool_result("call_abc", "ok", false);
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"tool_call_id\""));
        assert!(json.contains("call_abc"));
    }

    #[test]
    fn test_llm_request_new() {
        let req = LlmRequest::new("test prompt".to_string(), "default");
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, "user");
        assert_eq!(req.temperature, 0.7);
        assert!(req.tools.is_none());
    }

    #[test]
    fn test_llm_request_with_tools() {
        let mut req = LlmRequest::new("test".to_string(), "default");
        req.tools = Some(vec![ToolDefinition {
            name: "search".to_string(),
            description: "Search the web".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        }]);
        assert!(req.tools.is_some());
        assert_eq!(req.tools.unwrap().len(), 1);
    }
}
