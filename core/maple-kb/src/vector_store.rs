use crate::retriever::RetrievalResult;
use crate::indexer::DocumentChunk;
use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use sqlx::SqlitePool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkEmbedding {
    pub content: String,
    pub document_id: String,
    pub embedding: Vec<f32>,
}

#[async_trait]
pub trait VectorSearch: Send + Sync {
    async fn upsert(&self, id: &str, chunk: &DocumentChunk, embedding: Vec<f32>);
    async fn search(&self, query_embedding: &[f32], top_k: usize) -> Vec<RetrievalResult>;
    async fn delete_by_document(&self, document_id: &str) -> Result<()>;
    fn count(&self) -> usize;
}

pub struct InMemoryVectorStore {
    chunks: DashMap<String, ChunkEmbedding>,
    db: SqlitePool,
}

impl InMemoryVectorStore {
    pub fn new(db: SqlitePool) -> Self {
        Self {
            chunks: DashMap::new(),
            db,
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
}

#[async_trait]
impl VectorSearch for InMemoryVectorStore {
    async fn upsert(&self, id: &str, chunk: &DocumentChunk, embedding: Vec<f32>) {
        let now = chrono::Utc::now().timestamp();

        let embedding_bytes: Vec<u8> = embedding.iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        let _ = sqlx::query(
            "INSERT OR REPLACE INTO kb_chunks (id, document_id, content, embedding, created_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(id)
        .bind(&chunk.document_id)
        .bind(&chunk.content)
        .bind(&embedding_bytes)
        .bind(now)
        .execute(&self.db)
        .await;

        self.chunks.insert(id.to_string(), ChunkEmbedding {
            content: chunk.content.clone(),
            document_id: chunk.document_id.clone(),
            embedding,
        });

        tracing::debug!(chunk_id = %id, doc_id = %chunk.document_id, "Upserted chunk");
    }

    async fn search(&self, query_embedding: &[f32], top_k: usize) -> Vec<RetrievalResult> {
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

    async fn delete_by_document(&self, document_id: &str) -> Result<()> {
        let keys_to_remove: Vec<String> = self.chunks.iter()
            .filter(|e| e.value().document_id == document_id)
            .map(|e| e.key().clone())
            .collect();

        for key in keys_to_remove {
            self.chunks.remove(&key);
        }

        sqlx::query("DELETE FROM kb_chunks WHERE document_id = ?")
            .bind(document_id)
            .execute(&self.db)
            .await?;

        Ok(())
    }

    fn count(&self) -> usize {
        self.chunks.len()
    }
}

pub struct QdrantVectorStore {
    client: qdrant_client::Qdrant,
    collection_name: String,
    dimension: usize,
    id_map: DashMap<u64, String>,
}

impl QdrantVectorStore {
    pub async fn new(url: &str, collection_name: &str, dimension: usize) -> Result<Self> {
        let client = qdrant_client::Qdrant::from_url(url).build()?;

        let collections = client.list_collections().await?;
        let exists = collections.collections.iter().any(|c| c.name == collection_name);

        if !exists {
            use qdrant_client::qdrant::{CreateCollectionBuilder, Distance, VectorParamsBuilder};
            client
                .create_collection(
                    CreateCollectionBuilder::new(collection_name)
                        .vectors_config(VectorParamsBuilder::new(dimension as u64, Distance::Cosine)),
                )
                .await?;
            tracing::info!("Created Qdrant collection: {} (dim={})", collection_name, dimension);
        }

        Ok(Self {
            client,
            collection_name: collection_name.to_string(),
            dimension,
            id_map: DashMap::new(),
        })
    }
}

#[async_trait]
impl VectorSearch for QdrantVectorStore {
    async fn upsert(&self, id: &str, chunk: &DocumentChunk, embedding: Vec<f32>) {
        use qdrant_client::{Payload, qdrant::{PointStruct, UpsertPointsBuilder}};
        use serde_json::json;

        let payload: Payload = Payload::try_from(json!({
            "content": chunk.content,
            "document_id": chunk.document_id,
            "chunk_index": chunk.index,
        }))
        .unwrap_or_else(|_| Payload::try_from(json!({"content": chunk.content, "document_id": chunk.document_id})).unwrap_or_default());

        let point_id: u64 = hash_id_to_u64(id);
        self.id_map.insert(point_id, id.to_string());

        let point = PointStruct::new(point_id, embedding, payload);

        let _ = self.client
            .upsert_points(UpsertPointsBuilder::new(&self.collection_name, vec![point]).wait(true))
            .await;

        tracing::debug!(chunk_id = %id, doc_id = %chunk.document_id, "Upserted chunk to Qdrant");
    }

    async fn search(&self, query_embedding: &[f32], top_k: usize) -> Vec<RetrievalResult> {
        use qdrant_client::qdrant::{QueryPointsBuilder, SearchParamsBuilder, value::Kind};

        let query_vec: Vec<f32> = query_embedding.to_vec();

        let result = self.client
            .query(
                QueryPointsBuilder::new(&self.collection_name)
                    .query(query_vec)
                    .limit(top_k as u64)
                    .with_payload(true)
                    .params(SearchParamsBuilder::default().exact(false)),
            )
            .await;

        match result {
            Ok(query_result) => {
                query_result.result.into_iter().map(|point| {
                    let content = point.payload.get("content")
                        .and_then(|v| v.kind.as_ref())
                        .and_then(|k| match k {
                            Kind::StringValue(s) => Some(s.clone()),
                            _ => None,
                        })
                        .unwrap_or_default();
                    let document_id = point.payload.get("document_id")
                        .and_then(|v| v.kind.as_ref())
                        .and_then(|k| match k {
                            Kind::StringValue(s) => Some(s.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| "unknown".to_string());
                    let original_id = point.id
                        .and_then(|pid| pid.point_id_options)
                        .map(|opts| match opts {
                            qdrant_client::qdrant::point_id::PointIdOptions::Num(n) => {
                                self.id_map.get(&n).map(|s| s.to_string()).unwrap_or_else(|| n.to_string())
                            }
                            qdrant_client::qdrant::point_id::PointIdOptions::Uuid(s) => s,
                        })
                        .unwrap_or_else(|| hash_id_to_u64(&document_id).to_string());
                    RetrievalResult {
                        id: original_id,
                        content,
                        score: point.score as f64,
                        source: format!("document:{}", document_id),
                    }
                }).collect()
            }
            Err(e) => {
                tracing::warn!("Qdrant search failed: {}", e);
                Vec::new()
            }
        }
    }

    async fn delete_by_document(&self, document_id: &str) -> Result<()> {
        use qdrant_client::qdrant::{Condition, Filter, DeletePointsBuilder};

        let _ = self.client
            .delete_points(
                DeletePointsBuilder::new(&self.collection_name)
                    .points(Filter::all([Condition::matches("document_id", document_id.to_string())]))
                    .wait(true),
            )
            .await;

        Ok(())
    }

    fn count(&self) -> usize {
        self.id_map.len()
    }
}

fn hash_id_to_u64(id: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish()
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
    async fn test_in_memory_vector_store_insert_and_search() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        let store = InMemoryVectorStore::new(pool);
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

        store.upsert("c1", &chunk1, simple_embedding("Rust programming language", 64)).await;
        store.upsert("c2", &chunk2, simple_embedding("Python data science", 64)).await;

        let query = simple_embedding("Rust coding", 64);
        let results = store.search(&query, 5).await;
        assert_eq!(results.len(), 2);
    }
}