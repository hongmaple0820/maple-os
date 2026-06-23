use crate::retriever::RetrievalResult;
use crate::indexer::DocumentChunk;

use dashmap::DashMap;
use std::collections::HashMap;

struct ChunkIndex {
    content: String,
    document_id: String,
    term_freqs: HashMap<String, f64>,
    length: usize,
}

pub struct BM25Searcher {
    k1: f64,
    b: f64,
    chunks: DashMap<String, ChunkIndex>,
    doc_count: std::sync::atomic::AtomicUsize,
    df: DashMap<String, usize>,
    avg_dl: std::sync::atomic::AtomicU64,
}

impl Default for BM25Searcher {
    fn default() -> Self {
        Self::new()
    }
}

impl BM25Searcher {
    pub fn new() -> Self {
        Self {
            k1: 1.5,
            b: 0.75,
            chunks: DashMap::new(),
            doc_count: std::sync::atomic::AtomicUsize::new(0),
            df: DashMap::new(),
            avg_dl: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn index_chunk(&self, chunk: &DocumentChunk) {
        let tokens = tokenize(&chunk.content);
        let length = tokens.len();
        let mut term_freqs: HashMap<String, f64> = HashMap::new();

        for token in &tokens {
            *term_freqs.entry(token.clone()).or_default() += 1.0;
        }

        for term in term_freqs.keys() {
            *self.df.entry(term.clone()).or_default() += 1;
        }

        self.chunks.insert(chunk.id.clone(), ChunkIndex {
            content: chunk.content.clone(),
            document_id: chunk.document_id.clone(),
            term_freqs,
            length,
        });

        self.doc_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let count = self.doc_count.load(std::sync::atomic::Ordering::Relaxed);
        let total_len: usize = self.chunks.iter().map(|c| c.length).sum();
        let avg = total_len.checked_div(count).unwrap_or(0);
        self.avg_dl.store(avg as u64, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<RetrievalResult> {
        let query_tokens = tokenize(query);
        let avg_dl = self.avg_dl.load(std::sync::atomic::Ordering::Relaxed) as f64;
        let n = self.doc_count.load(std::sync::atomic::Ordering::Relaxed) as f64;

        let mut scores: Vec<(String, f64, String, String)> = Vec::new();

        for entry in self.chunks.iter() {
            let chunk = entry.value();
            let mut score = 0.0;

            for token in &query_tokens {
                let tf = chunk.term_freqs.get(token).copied().unwrap_or(0.0);
                if tf == 0.0 {
                    continue;
                }

                let df = self.df.get(token).map(|r| *r as f64).unwrap_or(0.0);
                let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();

                let dl = chunk.length as f64;
                let norm = 1.0 - self.b + self.b * (dl / avg_dl.max(1.0));
                let tf_norm = (tf * (self.k1 + 1.0)) / (tf + self.k1 * norm);

                score += idf * tf_norm;
            }

            if score > 0.0 {
                scores.push((entry.key().clone(), score, chunk.content.clone(), chunk.document_id.clone()));
            }
        }

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        scores.truncate(top_k);

        scores.into_iter().map(|(id, score, content, source)| {
            RetrievalResult {
                id,
                content,
                score,
                source: format!("document:{}", source),
                source_type: "bm25".to_string(),
                metadata: serde_json::Value::Null,
            }
        }).collect()
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty() && s.len() > 1)
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("Hello, World! This is a test.");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(tokens.contains(&"this".to_string()));
        assert!(!tokens.contains(&"a".to_string()));
    }

    #[test]
    fn test_bm25_search() {
        let searcher = BM25Searcher::new();

        searcher.index_chunk(&DocumentChunk {
            id: "doc1_0".to_string(),
            document_id: "doc1".to_string(),
            content: "Rust is a systems programming language focused on safety and performance".to_string(),
            index: 0,
            embedding: None,
        });

        searcher.index_chunk(&DocumentChunk {
            id: "doc2_0".to_string(),
            document_id: "doc2".to_string(),
            content: "Python is a popular programming language for data science".to_string(),
            index: 0,
            embedding: None,
        });

        let results = searcher.search("Rust programming language", 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "doc1_0");
    }

    #[test]
    fn test_bm25_empty_query() {
        let searcher = BM25Searcher::new();
        let results = searcher.search("", 5);
        assert!(results.is_empty());
    }
}
