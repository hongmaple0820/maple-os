use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait LlmStream: Send + Sync {
    async fn next_chunk(&mut self) -> Result<Option<StreamChunk>>;
}

#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub delta: String,
    pub finish_reason: Option<String>,
}

pub struct OpenAiStream {
    chunks: Vec<StreamChunk>,
    position: usize,
}

impl OpenAiStream {
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
            });
        }

        Self { chunks, position: 0 }
    }

    pub fn empty() -> Self {
        Self {
            chunks: Vec::new(),
            position: 0,
        }
    }
}

#[async_trait]
impl LlmStream for OpenAiStream {
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
