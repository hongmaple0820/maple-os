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
    pub reasoning: bool,
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

pub struct LiveSseStream {
    receiver: tokio::sync::mpsc::Receiver<Result<StreamChunk>>,
}

impl LiveSseStream {
    pub fn new(resp: reqwest::Response, format: SseFormat) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        tokio::spawn(async move {
            let mut buffer = String::new();
            let mut stream = resp.bytes_stream();
            use futures::StreamExt;

            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Err(anyhow::anyhow!("Stream error: {}", e))).await;
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim().to_string();
                    buffer = buffer[pos + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    let parsed = match format {
                        SseFormat::OpenAi => Self::parse_openai_line(&line),
                        SseFormat::Anthropic => Self::parse_anthropic_line(&line),
                    };

                    if let Some(result) = parsed
                        && tx.send(result).await.is_err()
                    {
                        break;
                    }
                }
            }
        });

        Self { receiver: rx }
    }

    fn parse_openai_line(line: &str) -> Option<Result<StreamChunk>> {
        let data = line.strip_prefix("data: ")?;
        if data == "[DONE]" {
            return Some(Ok(StreamChunk {
                delta: String::new(),
                finish_reason: Some("stop".to_string()),
                reasoning: false,
            }));
        }
        let json: serde_json::Value = serde_json::from_str(data).ok()?;
        if let Some(delta) = json["choices"][0]["delta"]["content"].as_str()
            && !delta.is_empty()
        {
            return Some(Ok(StreamChunk {
                delta: delta.to_string(),
                finish_reason: None,
                reasoning: false,
            }));
        }
        if let Some(delta) = json["choices"][0]["delta"]["reasoning_content"].as_str()
            && !delta.is_empty()
        {
            return Some(Ok(StreamChunk {
                delta: delta.to_string(),
                finish_reason: None,
                reasoning: true,
            }));
        }
        if let Some(fr) = json["choices"][0]["finish_reason"].as_str() {
            return Some(Ok(StreamChunk {
                delta: String::new(),
                finish_reason: Some(fr.to_string()),
                reasoning: false,
            }));
        }
        None
    }

    fn parse_anthropic_line(line: &str) -> Option<Result<StreamChunk>> {
        if let Some(_event) = line.strip_prefix("event: ") {
            return None;
        }
        let data = line.strip_prefix("data: ")?;
        let json: serde_json::Value = serde_json::from_str(data).ok()?;

        match json["type"].as_str() {
            Some("content_block_delta") => {
                if let Some(text) = json["delta"]["text"].as_str()
                    && !text.is_empty()
                {
                    return Some(Ok(StreamChunk {
                        delta: text.to_string(),
                        finish_reason: None,
                        reasoning: false,
                    }));
                }
            }
            Some("message_delta") => {
                if let Some(fr) = json["delta"]["stop_reason"].as_str() {
                    return Some(Ok(StreamChunk {
                        delta: String::new(),
                        finish_reason: Some(fr.to_string()),
                        reasoning: false,
                    }));
                }
            }
            _ => {}
        }
        None
    }
}

#[derive(Clone, Copy)]
pub enum SseFormat {
    OpenAi,
    Anthropic,
}

#[async_trait]
impl LlmStream for LiveSseStream {
    async fn next_chunk(&mut self) -> Result<Option<StreamChunk>> {
        match self.receiver.recv().await {
            Some(Ok(chunk)) => Ok(Some(chunk)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }
}
