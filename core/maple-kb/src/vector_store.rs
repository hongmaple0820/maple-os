use crate::retriever::RetrievalResult;
use crate::indexer::DocumentChunk;
use crate::bm25::BM25Searcher;
use anyhow::Result;
use dashmap::DashMap;
use sqlx::SqlitePool;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkEmbedding {
    pub content: String,
    pub document_id: String,
    pub embedding: Vec<f32>,
}

pub struct VectorStore {
    chunks: DashMap<String, ChunkEmbedding>,
    db: SqlitePool,
    bm25: Arc<BM25Searcher>,
}

impl VectorStore {
    pub fn new(db: SqlitePool) -> Self {
        Self {
            chunks: DashMap::new(),
            db,
            bm25: Arc::new(BM25Searcher::new()),
        }
    }

    pub async fn init_schema(&self) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS kb_chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB,
                created_at INTEGER NOT NULL
            )"
        )
        .execute(&self.db)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_kb_chunks_document ON kb_chunks(document_id)"
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    pub async fn upsert_chunk(&self, chunk: &DocumentChunk, embedding: Vec<f32>) {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        let embedding_bytes: Vec<u8> = embedding.iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        let _ = sqlx::query(
            "INSERT OR REPLACE INTO kb_chunks (id, document_id, content, embedding, created_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(&chunk.document_id)
        .bind(&chunk.content)
        .bind(&embedding_bytes)
        .bind(now)
        .execute(&self.db)
        .await;

        self.chunks.insert(id.clone(), ChunkEmbedding {
            content: chunk.content.clone(),
            document_id: chunk.document_id.clone(),
            embedding,
        });

        self.bm25.index_chunk(&DocumentChunk {
            id: id.clone(),
            document_id: chunk.document_id.clone(),
            content: chunk.content.clone(),
            index: chunk.index,
            embedding: None,
        });

        tracing::debug!(chunk_id = %id, doc_id = %chunk.document_id, "Upserted chunk");
    }

    pub async fn search(&self, query_embedding: &[f32], top_k: usize) -> Vec<RetrievalResult> {
        let mut scored: Vec<(String, f64, String, String)> = Vec::new();

        for entry in self.chunks.iter() {
            let chunk = entry.value();
            let similarity = cosine_similarity(query_embedding, &chunk.embedding);
            if similarity > 0.0 {
                scored.push((entry.key().clone(), similarity, chunk.content.clone(), chunk.document_id.clone()));
            }
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);

        scored.into_iter().map(|(id, score, content, source)| {
            RetrievalResult {
                id,
                content,
                score,
                source: format!("document:{}", source),
            }
        }).collect()
    }

    pub fn hybrid_search(&self, query: &str, query_embedding: &[f32], top_k: usize) -> Vec<RetrievalResult> {
        let vector_results = self.search_sync(query_embedding, top_k);
        let bm25_results = self.bm25.search(query, top_k);

        let mut combined: Vec<RetrievalResult> = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();

        for r in vector_results {
            seen_ids.insert(r.id.clone());
            combined.push(RetrievalResult {
                id: r.id.clone(),
                content: r.content.clone(),
                score: r.score * 0.7,
                source: r.source.clone(),
            });
        }

        for r in bm25_results {
            if seen_ids.insert(r.id.clone()) {
                combined.push(RetrievalResult {
                    id: r.id.clone(),
                    content: r.content.clone(),
                    score: r.score * 0.3,
                    source: "bm25".to_string(),
                });
            }
        }

        combined.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        combined.truncate(top_k);
        combined
    }

    fn search_sync(&self, query_embedding: &[f32], top_k: usize) -> Vec<RetrievalResult> {
        let mut scored: Vec<(String, f64, String, String)> = Vec::new();

        for entry in self.chunks.iter() {
            let chunk = entry.value();
            let similarity = cosine_similarity(query_embedding, &chunk.embedding);
            if similarity > 0.0 {
                scored.push((entry.key().clone(), similarity, chunk.content.clone(), chunk.document_id.clone()));
            }
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);

        scored.into_iter().map(|(id, score, content, source)| {
            RetrievalResult {
                id,
                content,
                score,
                source: format!("document:{}", source),
            }
        }).collect()
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| (*x as f64) * (*y as f64)).sum();
    let norm_a: f64 = a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

pub fn simple_embedding(text: &str, dim: usize) -> Vec<f32> {
    maple_llm::embedding::simple_embedding(text, dim)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.001);
    }

    #[test]
    fn test_simple_embedding() {
        let emb = simple_embedding("hello world", 64);
        assert_eq!(emb.len(), 64);
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_vector_store_insert_and_search() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        let store = VectorStore::new(pool);
        store.init_schema().await.unwrap();

        let chunk1 = DocumentChunk {
            id: "c1".to_string(),
            document_id: "d1".to_string(),
            content: "Rust programming language".to_string(),
            index: 0,
            embedding: None,
        };
        let chunk2 = DocumentChunk {
            id: "c2".to_string(),
            document_id: "d2".to_string(),
            content: "Python data science".to_string(),
            index: 0,
            embedding: None,
        };

        store.upsert_chunk(&chunk1, simple_embedding("Rust programming language", 64)).await;
        store.upsert_chunk(&chunk2, simple_embedding("Python data science", 64)).await;

        let query = simple_embedding("Rust coding", 64);
        let results = store.search(&query, 5).await;
        assert_eq!(results.len(), 2);
    }
}