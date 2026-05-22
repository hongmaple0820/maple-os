use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    fn dimension(&self) -> usize;
    fn name(&self) -> &str;
}

pub struct OllamaEmbedder {
    client: reqwest::Client,
    base_url: String,
    model: String,
    dimension: usize,
}

impl OllamaEmbedder {
    pub fn new(base_url: String, model: String, dimension: usize) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            model,
            dimension,
        }
    }

    pub fn nomic_embed_text() -> Self {
        Self::new(
            "http://127.0.0.1:11434".to_string(),
            "nomic-embed-text".to_string(),
            768,
        )
    }

    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }
}

#[async_trait]
impl Embedder for OllamaEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let body = serde_json::json!({
            "model": self.model,
            "prompt": text,
        });

        let resp = self.client
            .post(format!("{}/api/embeddings", self.base_url))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama embedding error ({}): {}", status, err_text);
        }

        let json: serde_json::Value = resp.json().await?;
        let embedding = json["embedding"].as_array()
            .ok_or_else(|| anyhow::anyhow!("No embedding in Ollama response"))?
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        Ok(embedding)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn name(&self) -> &str {
        &self.model
    }
}

pub fn simple_embedding(text: &str, dim: usize) -> Vec<f32> {
    let mut embedding = vec![0.0f32; dim];
    let bytes = text.as_bytes();

    for (i, &byte) in bytes.iter().cycle().take(dim * 3).enumerate() {
        let idx = i % dim;
        let shift = ((i / dim) % 4) as u8;
        embedding[idx] += ((byte as f32) / 255.0) * (1.0 / (1 << shift) as f32);
    }

    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in embedding.iter_mut() {
            *v /= norm;
        }
    }

    embedding
}

pub struct FallbackEmbedder {
    dimension: usize,
}

impl FallbackEmbedder {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }
}

#[async_trait]
impl Embedder for FallbackEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Ok(simple_embedding(text, self.dimension))
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn name(&self) -> &str {
        "fallback_hash"
    }
}
