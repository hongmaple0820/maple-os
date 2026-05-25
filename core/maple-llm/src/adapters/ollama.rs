use crate::router::LlmAdapter;
use crate::request::LlmRequest;
use crate::response::LlmResponse;
use crate::stream::LlmStream;
use async_trait::async_trait;
use anyhow::Result;

pub struct OllamaAdapter {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

impl OllamaAdapter {
    pub fn new(model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "http://127.0.0.1:11434".to_string(),
            model,
        }
    }

    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }

    pub fn qwen_7b() -> Self {
        Self::new("qwen2.5:7b".to_string())
    }

    pub fn qwen_14b() -> Self {
        Self::new("qwen2.5:14b".to_string())
    }

    pub fn llama_8b() -> Self {
        Self::new("llama3.1:8b".to_string())
    }

    pub fn deepseek_coder() -> Self {
        Self::new("deepseek-coder:6.7b".to_string())
    }

    fn build_messages(&self, req: &LlmRequest) -> Vec<serde_json::Value> {
        req.messages.iter().map(|m| {
            let mut msg = serde_json::json!({
                "role": m.role,
                "content": m.content,
            });
            if let Some(ref tool_call_id) = m.tool_call_id {
                msg["tool_call_id"] = serde_json::Value::String(tool_call_id.clone());
            }
            if let Some(ref tool_calls) = m.tool_calls {
                msg["tool_calls"] = serde_json::Value::Array(tool_calls.clone());
            }
            msg
        }).collect()
    }

    fn build_tools_json(&self, req: &LlmRequest) -> Option<Vec<serde_json::Value>> {
        req.tools.as_ref().map(|tools| {
            tools.iter().map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            }).collect()
        })
    }
}

#[async_trait]
impl LlmAdapter for OllamaAdapter {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse> {
        let messages = self.build_messages(&req);
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": false,
        });

        if let Some(tools) = self.build_tools_json(&req) {
            body["tools"] = serde_json::Value::Array(tools);
        }

        let resp = self.client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await?;

        if !status.is_success() {
            anyhow::bail!("Ollama API error ({}): {}", status, text);
        }

        let json: serde_json::Value = serde_json::from_str(&text)?;
        let content = json["choices"][0]["message"]["content"]
            .as_str().unwrap_or("").to_string();
        let input_tokens = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as usize;
        let output_tokens = json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as usize;

        let mut response = LlmResponse::new(content, input_tokens, output_tokens);
        response.finish_reason = json["choices"][0]["finish_reason"]
            .as_str().map(|s| s.to_string());
        response.model = json["model"].as_str().map(|s| s.to_string());

        if let Some(tool_calls) = json["choices"][0]["message"]["tool_calls"].as_array() {
            response.tool_calls = Some(tool_calls.clone());
        }

        Ok(response)
    }

    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmStream>> {
        let messages = self.build_messages(&req);
        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
        });

        let resp = self.client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama stream error ({}): {}", status, text);
        }

        Ok(Box::new(crate::stream::LiveSseStream::new(resp, crate::stream::SseFormat::OpenAi)))
    }

    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }

    fn max_context_length(&self) -> usize {
        32_768
    }

    fn cost_per_1k_tokens(&self) -> (f64, f64) {
        (0.0, 0.0)
    }

    fn name(&self) -> &str {
        &self.model
    }

    fn supports_function_calling(&self) -> bool {
        true
    }
}
