use crate::error::LlmError;
use crate::request::LlmRequest;
use crate::response::LlmResponse;
use crate::router::LlmAdapter;
use crate::stream::{LlmStream, StreamChunk};
use anyhow::Result;
use async_trait::async_trait;

pub struct OpenAiCompatAdapter {
    client: reqwest::Client,
    base_url: String,
    api_path: String,
    api_key: String,
    model: String,
    pricing: (f64, f64),
    context_length: usize,
}

impl OpenAiCompatAdapter {
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.clone(),
            api_path: "/v1/chat/completions".to_string(),
            api_key,
            model,
            pricing: (0.001, 0.002),
            context_length: 128_000,
        }
    }

    pub fn new_with_path(
        base_url: String,
        api_path: String,
        api_key: String,
        model: String,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            api_path,
            api_key,
            model,
            pricing: (0.001, 0.002),
            context_length: 128_000,
        }
    }

    pub fn with_pricing(mut self, input: f64, output: f64) -> Self {
        self.pricing = (input, output);
        self
    }

    pub fn with_context_length(mut self, len: usize) -> Self {
        self.context_length = len;
        self
    }

    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }

    pub fn with_api_path(mut self, path: String) -> Self {
        self.api_path = path;
        self
    }

    pub fn deepseek(api_key: String) -> Self {
        Self::new(
            "https://api.deepseek.com".to_string(),
            api_key,
            "deepseek-chat".to_string(),
        )
        .with_pricing(0.0014, 0.0028)
        .with_context_length(128_000)
    }

    pub fn qwen(api_key: String) -> Self {
        Self::new(
            "https://dashscope.aliyuncs.com/compatible-mode".to_string(),
            api_key,
            "qwen-plus".to_string(),
        )
        .with_pricing(0.0008, 0.002)
        .with_context_length(128_000)
    }

    pub fn glm(api_key: String) -> Self {
        Self::new_with_path(
            "https://open.bigmodel.cn/api/paas".to_string(),
            "/v4/chat/completions".to_string(),
            api_key,
            "glm-4".to_string(),
        )
        .with_pricing(0.001, 0.001)
        .with_context_length(128_000)
    }

    pub fn openai(api_key: String, model: String) -> Self {
        Self::new("https://api.openai.com".to_string(), api_key, model)
    }

    pub fn google(api_key: String, model: String) -> Self {
        Self::new_with_path(
            "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
            "/chat/completions".to_string(),
            api_key,
            model,
        )
        .with_pricing(0.00125, 0.005)
        .with_context_length(1_000_000)
    }

    pub fn mistral(api_key: String, model: String) -> Self {
        Self::new(
            "https://api.mistral.ai".to_string(),
            api_key,
            model,
        )
        .with_pricing(0.002, 0.006)
        .with_context_length(128_000)
    }

    pub fn cohere(api_key: String, model: String) -> Self {
        Self::new(
            "https://api.cohere.com/v2".to_string(),
            api_key,
            model,
        )
        .with_pricing(0.0015, 0.002)
        .with_context_length(128_000)
    }

    pub fn groq(api_key: String, model: String) -> Self {
        Self::new(
            "https://api.groq.com/openai".to_string(),
            api_key,
            model,
        )
        .with_pricing(0.0005, 0.0008)
        .with_context_length(32_000)
    }

    pub fn together(api_key: String, model: String) -> Self {
        Self::new(
            "https://api.together.xyz".to_string(),
            api_key,
            model,
        )
        .with_pricing(0.0009, 0.0009)
        .with_context_length(32_000)
    }

    pub fn fireworks(api_key: String, model: String) -> Self {
        Self::new(
            "https://api.fireworks.ai/inference/v1".to_string(),
            api_key,
            model,
        )
        .with_pricing(0.0009, 0.0009)
        .with_context_length(32_000)
    }

    pub fn moonshot(api_key: String, model: String) -> Self {
        Self::new(
            "https://api.moonshot.cn".to_string(),
            api_key,
            model,
        )
        .with_pricing(0.006, 0.012)
        .with_context_length(128_000)
    }

    pub fn baichuan(api_key: String, model: String) -> Self {
        Self::new(
            "https://api.baichuan-ai.com".to_string(),
            api_key,
            model,
        )
        .with_pricing(0.001, 0.002)
        .with_context_length(32_000)
    }

    pub fn yi(api_key: String, model: String) -> Self {
        Self::new(
            "https://api.lingyiwanwu.com".to_string(),
            api_key,
            model,
        )
        .with_pricing(0.006, 0.006)
        .with_context_length(200_000)
    }

    pub fn minimax(api_key: String, model: String) -> Self {
        Self::new(
            "https://api.minimax.chat".to_string(),
            api_key,
            model,
        )
        .with_pricing(0.001, 0.001)
        .with_context_length(32_000)
    }

    pub fn stepfun(api_key: String, model: String) -> Self {
        Self::new(
            "https://api.stepfun.com".to_string(),
            api_key,
            model,
        )
        .with_pricing(0.004, 0.008)
        .with_context_length(256_000)
    }

    fn build_messages(&self, req: &LlmRequest) -> Vec<serde_json::Value> {
        req.messages
            .iter()
            .map(|m| {
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
            })
            .collect()
    }

    fn build_tools_json(&self, req: &LlmRequest) -> Option<Vec<serde_json::Value>> {
        req.tools.as_ref().map(|tools| {
            tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect()
        })
    }
}

#[async_trait]
impl LlmAdapter for OpenAiCompatAdapter {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse> {
        let messages = self.build_messages(&req);
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": req.max_tokens.unwrap_or(4096),
            "temperature": req.temperature,
        });

        if let Some(tools) = self.build_tools_json(&req) {
            body["tools"] = serde_json::Value::Array(tools);
        }

        let resp = self
            .client
            .post(format!("{}{}", self.base_url, self.api_path))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await?;

        if !status.is_success() {
            let llm_err = LlmError::classify(status.as_u16(), &text, &self.model);
            return Err(anyhow::anyhow!(llm_err));
        }

        let json: serde_json::Value = serde_json::from_str(&text)?;
        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let input_tokens = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as usize;
        let output_tokens = json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as usize;

        let mut response = LlmResponse::new(content, input_tokens, output_tokens);
        response.finish_reason = json["choices"][0]["finish_reason"]
            .as_str()
            .map(|s| s.to_string());
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
            "max_tokens": req.max_tokens.unwrap_or(4096),
            "temperature": req.temperature,
            "stream": true,
        });

        let resp = self
            .client
            .post(format!("{}{}", self.base_url, self.api_path))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let llm_err = LlmError::classify(status.as_u16(), &text, &self.model);
            return Err(anyhow::anyhow!(llm_err));
        }

        Ok(Box::new(crate::stream::LiveSseStream::new(
            resp,
            crate::stream::SseFormat::OpenAi,
        )))
    }

    fn count_tokens(&self, text: &str) -> usize {
        crate::token_counter::count_tokens(text)
    }

    fn max_context_length(&self) -> usize {
        self.context_length
    }

    fn cost_per_1k_tokens(&self) -> (f64, f64) {
        self.pricing
    }

    fn name(&self) -> &str {
        &self.model
    }

    fn supports_vision(&self) -> bool {
        self.model.contains("vision") || self.model.contains("4o")
    }
}

pub struct OpenAiSseStream {
    chunks: Vec<StreamChunk>,
    position: usize,
}

impl OpenAiSseStream {
    pub fn from_response(resp: reqwest::Response) -> Self {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime for SSE parsing");

        rt.block_on(async {
            let bytes = match resp.bytes().await {
                Ok(b) => b,
                Err(_) => return Self::empty(),
            };
            Self::from_response_body(&bytes)
        })
    }

    pub fn from_response_body(body: &[u8]) -> Self {
        let mut chunks = Vec::new();
        let mut finish_reason = None;
        let text = String::from_utf8_lossy(body);

        for line in text.lines() {
            let line = line.trim();
            if !line.starts_with("data: ") {
                continue;
            }
            let data = &line[6..];
            if data == "[DONE]" {
                break;
            }
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(delta) = json["choices"][0]["delta"]["content"].as_str()
                    && !delta.is_empty()
                {
                    chunks.push(StreamChunk {
                        delta: delta.to_string(),
                        finish_reason: None,
                        reasoning: false,
                    });
                }
                if let Some(fr) = json["choices"][0]["finish_reason"].as_str() {
                    finish_reason = Some(fr.to_string());
                }
            }
        }

        if let Some(fr) = finish_reason {
            chunks.push(StreamChunk {
                delta: String::new(),
                finish_reason: Some(fr),
                reasoning: false,
            });
        }

        Self {
            chunks,
            position: 0,
        }
    }

    fn empty() -> Self {
        Self {
            chunks: Vec::new(),
            position: 0,
        }
    }
}

#[async_trait]
impl LlmStream for OpenAiSseStream {
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
