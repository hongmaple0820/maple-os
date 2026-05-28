use crate::error::LlmError;
use crate::request::LlmRequest;
use crate::response::LlmResponse;
use crate::router::LlmAdapter;
use crate::stream::{LlmStream, StreamChunk};
use anyhow::Result;
use async_trait::async_trait;

pub struct AnthropicAdapter {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl AnthropicAdapter {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
            base_url: "https://api.anthropic.com".to_string(),
        }
    }

    pub fn claude_3_5_sonnet(api_key: String) -> Self {
        Self::new(api_key, "claude-3-5-sonnet-20241022".to_string())
    }

    pub fn claude_3_opus(api_key: String) -> Self {
        Self::new(api_key, "claude-3-opus-20240229".to_string())
    }

    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }
}

#[async_trait]
impl LlmAdapter for AnthropicAdapter {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse> {
        let messages: Vec<serde_json::Value> = req
            .messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": req.max_tokens.unwrap_or(4096),
        });

        if req.temperature != 0.7 {
            body["temperature"] = serde_json::json!(req.temperature);
        }

        if let Some(ref tools) = req.tools {
            let anthropic_tools: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.parameters,
                    })
                })
                .collect();
            body["tools"] = serde_json::json!(anthropic_tools);
        }

        let resp = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await?;

        if !status.is_success() {
            let llm_err = LlmError::classify(status.as_u16(), &text, "anthropic");
            return Err(anyhow::anyhow!(llm_err));
        }

        let json: serde_json::Value = serde_json::from_str(&text)?;

        let mut content = String::new();
        let mut tool_calls = Vec::new();

        if let Some(blocks) = json["content"].as_array() {
            for block in blocks {
                match block["type"].as_str() {
                    Some("text") => {
                        content.push_str(block["text"].as_str().unwrap_or(""));
                    }
                    Some("tool_use") => {
                        tool_calls.push(serde_json::json!({
                            "id": block["id"],
                            "name": block["name"],
                            "arguments": block["input"],
                        }));
                    }
                    _ => {}
                }
            }
        }

        let input_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as usize;
        let output_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as usize;

        let mut response = LlmResponse::new(content, input_tokens, output_tokens);
        if !tool_calls.is_empty() {
            response = response.with_tool_calls(tool_calls);
        }

        Ok(response)
    }

    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmStream>> {
        let messages: Vec<serde_json::Value> = req
            .messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                })
            })
            .collect();

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": req.max_tokens.unwrap_or(4096),
            "stream": true,
        });

        let resp = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await?;
            let llm_err = LlmError::classify(status.as_u16(), &text, "anthropic");
            return Err(anyhow::anyhow!(llm_err));
        }

        Ok(Box::new(crate::stream::LiveSseStream::new(
            resp,
            crate::stream::SseFormat::Anthropic,
        )))
    }

    fn count_tokens(&self, text: &str) -> usize {
        crate::token_counter::count_tokens(text)
    }

    fn max_context_length(&self) -> usize {
        200_000
    }

    fn cost_per_1k_tokens(&self) -> (f64, f64) {
        (0.003, 0.015)
    }

    fn name(&self) -> &str {
        &self.model
    }

    fn supports_vision(&self) -> bool {
        true
    }

    fn supports_function_calling(&self) -> bool {
        true
    }
}

pub struct AnthropicSseStream {
    chunks: Vec<StreamChunk>,
    position: usize,
}

impl AnthropicSseStream {
    pub fn from_response_body(body: &[u8]) -> Self {
        let mut chunks = Vec::new();
        let text = String::from_utf8_lossy(body);

        let mut current_event = String::new();

        for line in text.lines() {
            let line = line.trim();
            if let Some(event) = line.strip_prefix("event: ") {
                current_event = event.to_string();
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                    match current_event.as_str() {
                        "content_block_delta" => {
                            if let Some(delta_text) = json["delta"]["text"].as_str()
                                && !delta_text.is_empty()
                            {
                                chunks.push(StreamChunk {
                                    delta: delta_text.to_string(),
                                    finish_reason: None,
                                });
                            }
                        }
                        "message_delta" => {
                            if let Some(fr) = json["delta"]["stop_reason"].as_str() {
                                chunks.push(StreamChunk {
                                    delta: String::new(),
                                    finish_reason: Some(fr.to_string()),
                                });
                            }
                        }
                        "message_stop"
                            if chunks.last().is_none_or(|c| c.finish_reason.is_none()) =>
                        {
                            chunks.push(StreamChunk {
                                delta: String::new(),
                                finish_reason: Some("end_turn".to_string()),
                            });
                        }
                        _ => {}
                    }
                }
                current_event.clear();
            }
        }

        Self {
            chunks,
            position: 0,
        }
    }

    pub fn empty() -> Self {
        Self {
            chunks: Vec::new(),
            position: 0,
        }
    }
}

#[async_trait]
impl LlmStream for AnthropicSseStream {
    async fn next_chunk(&mut self) -> Result<Option<StreamChunk>> {
        if self.position < self.chunks.len() {
            let chunk = self.chunks[self.position].clone();
            self.position += 1;
            Ok(Some(chunk))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_sse_parse() {
        let body = b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_01\"}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}\n\nevent: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n";

        let stream = AnthropicSseStream::from_response_body(body);
        assert_eq!(stream.chunks.len(), 3);
        assert_eq!(stream.chunks[0].delta, "Hello");
        assert_eq!(stream.chunks[1].delta, " world");
        assert_eq!(stream.chunks[2].finish_reason, Some("end_turn".to_string()));
    }

    #[test]
    fn test_anthropic_sse_empty() {
        let stream = AnthropicSseStream::empty();
        assert_eq!(stream.chunks.len(), 0);
    }
}
