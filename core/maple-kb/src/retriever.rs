use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    pub id: String,
    pub content: String,
    pub score: f64,
    pub source: String,
}

pub struct HybridRetriever {
    rrf_k: usize,
}

impl HybridRetriever {
    pub fn new() -> Self {
        Self { rrf_k: 60 }
    }

    pub async fn search(
        &self,
        _query: &str,
        top_k: usize,
        vector_results: Vec<RetrievalResult>,
        bm25_results: Vec<RetrievalResult>,
    ) -> Result<Vec<RetrievalResult>> {
        let mut scores: HashMap<String, f64> = HashMap::new();
        let mut content_map: HashMap<String, (String, String)> = HashMap::new();

        for (rank, result) in vector_results.iter().enumerate() {
            *scores.entry(result.id.clone()).or_default() += 1.0 / (self.rrf_k + rank + 1) as f64;
            content_map.entry(result.id.clone()).or_insert_with(|| {
                (result.content.clone(), result.source.clone())
            });
        }

        for (rank, result) in bm25_results.iter().enumerate() {
            *scores.entry(result.id.clone()).or_default() += 1.0 / (self.rrf_k + rank + 1) as f64;
            content_map.entry(result.id.clone()).or_insert_with(|| {
                (result.content.clone(), result.source.clone())
            });
        }

        let mut ranked: Vec<_> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        Ok(ranked.into_iter().take(top_k).map(|(id, score)| {
            let (content, source) = content_map.remove(&id).unwrap_or_default();
            RetrievalResult { id, content, score, source }
        }).collect())
    }
}
