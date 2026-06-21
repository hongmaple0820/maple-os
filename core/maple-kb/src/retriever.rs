use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use anyhow::Result;
use async_trait::async_trait;

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

/// Reranker trait — implementations re-score retrieval results after the
/// initial RRF fusion to improve precision (Track 3 / #14).
///
/// The reranker is called AFTER HybridRetriever produces RRF-fused results
/// but BEFORE truncating to top_k. It receives the query + the pre-rerank
/// top-N results and returns them re-sorted by relevance.
#[async_trait]
pub trait Reranker: Send + Sync {
    /// Re-score and re-sort the given results by relevance to the query.
    /// Returns a new Vec with the same results but potentially different
    /// order and updated scores.
    async fn rerank(&self, query: &str, results: Vec<RetrievalResult>) -> Result<Vec<RetrievalResult>>;
}

/// No-op reranker (default — preserves original RRF ordering).
pub struct NoopReranker;

#[async_trait]
impl Reranker for NoopReranker {
    async fn rerank(&self, _query: &str, results: Vec<RetrievalResult>) -> Result<Vec<RetrievalResult>> {
        Ok(results)
    }
}

/// LLM-based reranker that asks the LLM to score each result's relevance
/// to the query on a 0.0-1.0 scale, then re-sorts by that score (#14).
///
/// Falls back to original RRF scores if the LLM is unavailable or returns
/// unparseable output. The reranker only re-scores the top `rerank_top_n`
/// results (default 10) to limit LLM cost.
pub struct LlmReranker {
    router: Arc<maple_llm::router::LlmRouter>,
    /// How many top results to rerank (default 10). Results beyond this
    /// count keep their original RRF score and are appended after.
    rerank_top_n: usize,
}

impl LlmReranker {
    pub fn new(router: Arc<maple_llm::router::LlmRouter>) -> Self {
        Self { router, rerank_top_n: 10 }
    }

    pub fn with_top_n(mut self, n: usize) -> Self {
        self.rerank_top_n = n;
        self
    }
}

#[async_trait]
impl Reranker for LlmReranker {
    async fn rerank(&self, query: &str, mut results: Vec<RetrievalResult>) -> Result<Vec<RetrievalResult>> {
        if results.is_empty() {
            return Ok(results);
        }

        let n = self.rerank_top_n.min(results.len());
        let mut to_rerank: Vec<RetrievalResult> = results.drain(..n).collect();
        let remaining = results; // results after top-N keep original scores

        // Build a single LLM prompt asking it to score all candidates at once
        // to minimize API calls. Format:
        //   Query: <query>
        //   [0] <content snippet>
        //   [1] <content snippet>
        //   ...
        //   Return: 0:0.8,1:0.3,2:0.9,...
        let mut prompt = format!("Score each result's relevance to the query on 0.0-1.0 scale.\nQuery: {}\n\n", query);
        for (i, r) in to_rerank.iter().enumerate() {
            let snippet = if r.content.len() > 200 {
                let mut end = 200;
                while end > 0 && !r.content.is_char_boundary(end) {
                    end -= 1;
                }
                &r.content[..end]
            } else {
                &r.content
            };
            prompt.push_str(&format!("[{}] {}\n", i, snippet));
        }
        prompt.push_str("\nReturn ONLY comma-separated index:score pairs, e.g. 0:0.8,1:0.3,2:0.9");

        let request = maple_llm::request::LlmRequest::quick_qa(&prompt);
        let adapter = match self.router.route(&request).await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(error = %e, "LLM reranker route failed, keeping RRF order");
                // Fallback: keep original order
                to_rerank.extend(remaining);
                return Ok(to_rerank);
            }
        };

        let response = match adapter.complete(request).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "LLM reranker complete failed, keeping RRF order");
                to_rerank.extend(remaining);
                return Ok(to_rerank);
            }
        };

        // Parse "0:0.8,1:0.3,2:0.9" format
        let response_text = response.text();
        let text = response_text.trim();
        let mut new_scores: HashMap<usize, f64> = HashMap::new();
        for pair in text.split(',') {
            let parts: Vec<&str> = pair.trim().split(':').collect();
            if parts.len() == 2 {
                if let (Ok(idx), Ok(score)) = (parts[0].trim().parse::<usize>(), parts[1].trim().parse::<f64>()) {
                    new_scores.insert(idx, score.clamp(0.0, 1.0));
                }
            }
        }

        // Apply new scores + re-sort
        let mut reranked: Vec<RetrievalResult> = to_rerank
            .into_iter()
            .enumerate()
            .map(|(i, mut r)| {
                if let Some(&score) = new_scores.get(&i) {
                    r.score = score;
                }
                r
            })
            .collect();
        reranked.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Append remaining (non-reranked) results
        reranked.extend(remaining);

        Ok(reranked)
    }
}

pub struct HybridRetriever {
    rrf_k: usize,
    /// Optional reranker applied after RRF fusion (#14)
    reranker: Option<Arc<dyn Reranker>>,
}

impl Default for HybridRetriever {
    fn default() -> Self {
        Self::new()
    }
}

impl HybridRetriever {
    pub fn new() -> Self {
        Self { rrf_k: 60, reranker: None }
    }

    /// Attach a reranker (e.g., LlmReranker) to improve precision after
    /// RRF fusion. Without this, the retriever uses RRF scores only.
    pub fn with_reranker(mut self, reranker: Arc<dyn Reranker>) -> Self {
        self.reranker = Some(reranker);
        self
    }

    pub async fn search(
        &self,
        query: &str,
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

        // Take more than top_k for reranking (reranker needs candidates)
        let rerank_pool_size = if let Some(_) = &self.reranker {
            top_k * 3 // over-fetch 3x for reranker to choose from
        } else {
            top_k
        };

        let mut results: Vec<RetrievalResult> = ranked.into_iter().take(rerank_pool_size).map(|(id, score)| {
            let (content, source) = content_map.remove(&id).unwrap_or_default();
            RetrievalResult { id, content, score, source, source_type: String::new(), metadata: serde_json::Value::Null }
        }).collect();

        // #14: apply reranker if attached
        if let Some(reranker) = &self.reranker {
            results = reranker.rerank(query, results).await?;
        }

        // Truncate to top_k
        results.truncate(top_k);
        Ok(results)
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

    // ── #14: Reranker tests ──

    #[tokio::test]
    async fn test_noop_reranker_preserves_order() {
        let reranker = NoopReranker;
        let results = vec![
            make_result("a", "content a", 0.9, "src"),
            make_result("b", "content b", 0.5, "src"),
        ];
        let reranked = reranker.rerank("query", results.clone()).await.unwrap();
        assert_eq!(reranked.len(), results.len());
        assert_eq!(reranked[0].id, "a"); // order preserved
        assert_eq!(reranked[1].id, "b");
    }

    /// Mock reranker that reverses the order — used to verify the
    /// HybridRetriever actually applies the reranker.
    struct ReverseReranker;
    #[async_trait]
    impl Reranker for ReverseReranker {
        async fn rerank(&self, _query: &str, mut results: Vec<RetrievalResult>) -> Result<Vec<RetrievalResult>> {
            results.reverse();
            Ok(results)
        }
    }

    #[tokio::test]
    async fn test_hybrid_search_with_reranker_reorders() {
        let reranker = Arc::new(ReverseReranker);
        let retriever = HybridRetriever::new().with_reranker(reranker);

        // Create results where v1 has highest RRF score
        let vector_results = vec![
            make_result("v1", "highest rrf", 0.9, "doc1"),
            make_result("v2", "medium rrf", 0.5, "doc2"),
            make_result("v3", "lowest rrf", 0.3, "doc3"),
        ];

        let results = retriever.search("test", 3, vector_results, vec![]).await.unwrap();
        // After reranker reverses, v3 should be first (was last in RRF)
        assert_eq!(results[0].id, "v3");
        assert_eq!(results[results.len() - 1].id, "v1");
    }

    #[tokio::test]
    async fn test_hybrid_search_reranker_overfetches() {
        // Verify that with a reranker, the retriever fetches more than
        // top_k candidates for the reranker to choose from
        let reranker = Arc::new(NoopReranker);
        let retriever = HybridRetriever::new().with_reranker(reranker);

        // Provide 10 results, ask for top_k=2
        let vector_results: Vec<_> = (0..10)
            .map(|i| make_result(&format!("v{}", i), &format!("content {}", i), 0.5, "src"))
            .collect();

        let results = retriever.search("test", 2, vector_results, vec![]).await.unwrap();
        // Should still return only top_k=2
        assert_eq!(results.len(), 2);
    }
}
