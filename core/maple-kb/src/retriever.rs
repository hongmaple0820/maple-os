use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    pub id: String,
    pub content: String,
    pub score: f64,
    pub source: String,
    #[serde(default)]
    pub source_type: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

pub struct HybridRetriever {
    rrf_k: usize,
}

impl Default for HybridRetriever {
    fn default() -> Self {
        Self::new()
    }
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
            RetrievalResult { id, content, score, source, source_type: String::new(), metadata: serde_json::Value::Null }
        }).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(id: &str, content: &str, score: f64, source: &str) -> RetrievalResult {
        RetrievalResult {
            id: id.to_string(),
            content: content.to_string(),
            score,
            source: source.to_string(),
            source_type: String::new(),
            metadata: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn test_hybrid_search_merges_results() {
        let retriever = HybridRetriever::new();
        let vector_results = vec![
            make_result("v1", "vec content 1", 0.9, "doc1"),
            make_result("v2", "vec content 2", 0.8, "doc2"),
        ];
        let bm25_results = vec![
            make_result("b1", "bm25 content 1", 0.7, "doc3"),
            make_result("v1", "vec content 1", 0.6, "doc1"),
        ];

        let results = retriever.search("test", 5, vector_results, bm25_results).await.unwrap();
        assert!(results.len() >= 2);
        assert!(results.iter().any(|r| r.id == "v1"));
        assert!(results.iter().any(|r| r.id == "b1"));
    }

    #[tokio::test]
    async fn test_hybrid_search_top_k() {
        let retriever = HybridRetriever::new();
        let results = vec![
            make_result("1", "c", 0.5, "s"),
            make_result("2", "c", 0.5, "s"),
            make_result("3", "c", 0.5, "s"),
        ];
        let output = retriever.search("test", 2, results, vec![]).await.unwrap();
        assert_eq!(output.len(), 2);
    }
}
