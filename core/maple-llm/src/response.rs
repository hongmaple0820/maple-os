use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub model: Option<String>,
    pub finish_reason: Option<String>,
    pub tool_calls: Option<Vec<serde_json::Value>>,
}

impl LlmResponse {
    pub fn new(content: String, input_tokens: usize, output_tokens: usize) -> Self {
        Self {
            content,
            input_tokens,
            output_tokens,
            model: None,
            finish_reason: None,
            tool_calls: None,
        }
    }

    pub fn text(&self) -> String {
        self.content.clone()
    }

    pub fn total_tokens(&self) -> usize {
        self.input_tokens + self.output_tokens
    }

    pub fn has_tool_calls(&self) -> bool {
        self.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty())
    }

    pub fn with_tool_calls(mut self, calls: Vec<serde_json::Value>) -> Self {
        self.tool_calls = Some(calls);
        self
    }

    pub fn parse_tool_calls(&self) -> Vec<ParsedToolCall> {
        match &self.tool_calls {
            Some(calls) => calls
                .iter()
                .filter_map(|tc| {
                    let id = tc["id"].as_str().unwrap_or("").to_string();
                    if let Some(name) = tc["function"]["name"].as_str() {
                        let arguments = tc["function"]["arguments"].as_str().unwrap_or("{}");
                        let args: serde_json::Value = serde_json::from_str(arguments).ok()?;
                        // Normalize null/empty args to {} — rig null-args pattern
                        let args = if args.is_null() {
                            serde_json::json!({})
                        } else {
                            args
                        };
                        Some(ParsedToolCall {
                            id,
                            name: name.to_string(),
                            arguments: args,
                        })
                    } else if let Some(name) = tc["name"].as_str() {
                        let args = tc["arguments"].clone();
                        // Normalize null/empty args to {} — rig null-args pattern
                        let args = if args.is_null() {
                            serde_json::json!({})
                        } else {
                            args
                        };
                        Some(ParsedToolCall {
                            id,
                            name: name.to_string(),
                            arguments: args,
                        })
                    } else {
                        None
                    }
                })
                .collect(),
            None => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_response_new() {
        let resp = LlmResponse::new("Hello".to_string(), 10, 5);
        assert_eq!(resp.content, "Hello");
        assert_eq!(resp.input_tokens, 10);
        assert_eq!(resp.output_tokens, 5);
        assert_eq!(resp.total_tokens(), 15);
        assert!(!resp.has_tool_calls());
    }

    #[test]
    fn test_parse_tool_calls() {
        let mut resp = LlmResponse::new("".to_string(), 0, 0);
        resp.tool_calls = Some(vec![
            serde_json::json!({
                "id": "call_001",
                "function": {
                    "name": "web_search",
                    "arguments": "{\"query\": \"rust\"}"
                }
            }),
            serde_json::json!({
                "id": "call_002",
                "function": {
                    "name": "echo",
                    "arguments": "{}"
                }
            }),
        ]);

        let calls = resp.parse_tool_calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "call_001");
        assert_eq!(calls[0].name, "web_search");
        assert_eq!(calls[0].arguments["query"], "rust");
        assert_eq!(calls[1].id, "call_002");
        assert_eq!(calls[1].name, "echo");
    }

    #[test]
    fn test_parse_tool_calls_empty() {
        let resp = LlmResponse::new("no tools".to_string(), 0, 0);
        assert!(resp.parse_tool_calls().is_empty());
    }

    #[test]
    fn test_has_tool_calls() {
        let mut resp = LlmResponse::new("".to_string(), 0, 0);
        assert!(!resp.has_tool_calls());
        resp.tool_calls = Some(vec![]);
        assert!(!resp.has_tool_calls());
        resp.tool_calls = Some(vec![
            serde_json::json!({"id": "1", "function": {"name": "x", "arguments": "{}"}}),
        ]);
        assert!(resp.has_tool_calls());
    }

    #[test]
    fn test_parse_tool_calls_null_args() {
        let mut resp = LlmResponse::new("".to_string(), 0, 0);
        resp.tool_calls = Some(vec![serde_json::json!({
            "id": "call_null",
            "function": {
                "name": "no_args_tool",
                "arguments": serde_json::Value::Null
            }
        })]);

        let calls = resp.parse_tool_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].arguments, serde_json::json!({}));
    }
}
