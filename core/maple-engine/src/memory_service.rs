use serde::{Deserialize, Serialize};
use sqlx::{SqlitePool, Row};
use anyhow::Result;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemory {
    pub id: String,
    pub agent_id: String,
    pub memory_type: MemoryLayer,
    pub content: String,
    pub summary: Option<String>,
    pub source_type: Option<String>,
    pub source_id: Option<String>,
    pub group_id: Option<String>,
    pub relevance_score: f64,
    pub access_count: i64,
    pub last_accessed_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip)]
    pub embedding: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryLayer {
    Working,
    Episodic,
    Semantic,
}

impl MemoryLayer {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Episodic => "episodic",
            Self::Semantic => "semantic",
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "episodic" => Self::Episodic,
            "semantic" => Self::Semantic,
            _ => Self::Working,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub agent_id: String,
    pub query_text: Option<String>,
    pub memory_type: Option<MemoryLayer>,
    pub group_id: Option<String>,
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredMemory {
    pub memory: AgentMemory,
    pub score: f64,
}

const MEMORY_COLUMNS: &str = "id, agent_id, memory_type, content, summary, source_type, source_id, group_id, relevance_score, access_count, last_accessed_at, expires_at, created_at, updated_at, embedding";

fn row_to_memory(row: &sqlx::sqlite::SqliteRow) -> AgentMemory {
    AgentMemory {
        id: row.get(0),
        agent_id: row.get(1),
        memory_type: MemoryLayer::parse_str(row.get::<&str, _>(2)),
        content: row.get(3),
        summary: row.get(4),
        source_type: row.get(5),
        source_id: row.get(6),
        group_id: row.get(7),
        relevance_score: row.get(8),
        access_count: row.get(9),
        last_accessed_at: row.get(10),
        expires_at: row.get(11),
        created_at: row.get(12),
        updated_at: row.get(13),
        embedding: row.get(14),
    }
}

pub struct MemoryService {
    pool: SqlitePool,
    embedder: Option<Arc<dyn maple_llm::embedding::Embedder>>,
}

impl MemoryService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool, embedder: None }
    }

    pub fn with_embedder(mut self, embedder: Arc<dyn maple_llm::embedding::Embedder>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    pub async fn store(
        &self,
        agent_id: &str,
        memory_type: MemoryLayer,
        content: &str,
        summary: Option<&str>,
        source_type: Option<&str>,
        source_id: Option<&str>,
        group_id: Option<&str>,
        ttl_hours: Option<i64>,
    ) -> Result<AgentMemory> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        let expires_at = ttl_hours.map(|h| now + h * 3600);

        // Generate embedding if embedder is available
        let (embedding_bytes, embedding_model) = if let Some(ref embedder) = self.embedder {
            match embedder.embed(content).await {
                Ok(emb) => {
                    let bytes: Vec<u8> = emb.iter().flat_map(|f| f.to_le_bytes()).collect();
                    (Some(bytes), Some(embedder.name().to_string()))
                }
                Err(e) => {
                    tracing::warn!("Failed to generate embedding: {}", e);
                    (None, None)
                }
            }
        } else {
            (None, None)
        };

        sqlx::query(
            "INSERT INTO agent_memories (id, agent_id, memory_type, content, summary, embedding, embedding_model, source_type, source_id, group_id, relevance_score, access_count, expires_at, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0.7, 0, ?, ?, ?)"
        )
        .bind(&id)
        .bind(agent_id)
        .bind(memory_type.as_str())
        .bind(content)
        .bind(summary)
        .bind(&embedding_bytes)
        .bind(&embedding_model)
        .bind(source_type)
        .bind(source_id)
        .bind(group_id)
        .bind(expires_at)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(AgentMemory {
            id,
            agent_id: agent_id.to_string(),
            memory_type,
            content: content.to_string(),
            summary: summary.map(|s| s.to_string()),
            source_type: source_type.map(|s| s.to_string()),
            source_id: source_id.map(|s| s.to_string()),
            group_id: group_id.map(|s| s.to_string()),
            relevance_score: 0.7,
            access_count: 0,
            last_accessed_at: None,
            expires_at,
            created_at: now,
            updated_at: now,
            embedding: embedding_bytes,
        })
    }

    pub async fn get(&self, memory_id: &str) -> Result<Option<AgentMemory>> {
        let sql = format!("SELECT {} FROM agent_memories WHERE id = ?", MEMORY_COLUMNS);
        let row = sqlx::query(&sql)
            .bind(memory_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| row_to_memory(&r)))
    }

    pub async fn search(&self, query: &MemoryQuery) -> Result<Vec<ScoredMemory>> {
        let now = chrono::Utc::now().timestamp();

        // Compute query embedding for vector similarity if available
        let query_embedding: Option<Vec<f32>> = if let (Some(qtext), Some(embedder)) = (&query.query_text, &self.embedder) {
            embedder.embed(qtext).await.ok()
        } else {
            None
        };

        let rows = if let Some(ref qtext) = query.query_text {
            // FTS5 search
            let fts_sql = format!(
                "SELECT {}, m.rowid FROM agent_memories m
                 INNER JOIN agent_memories_fts fts ON m.rowid = fts.rowid
                 WHERE fts.content MATCH ? AND m.agent_id = ?
                 AND (m.expires_at IS NULL OR m.expires_at > ?)
                 ORDER BY fts.rank LIMIT ?",
                "m.id, m.agent_id, m.memory_type, m.content, m.summary, m.source_type, m.source_id, m.group_id, m.relevance_score, m.access_count, m.last_accessed_at, m.expires_at, m.created_at, m.updated_at, m.embedding"
            );
            let fts_rows = sqlx::query(&fts_sql)
                .bind(qtext)
                .bind(&query.agent_id)
                .bind(now)
                .bind(query.limit)
                .fetch_all(&self.pool)
                .await;

            match fts_rows {
                Ok(rows) if !rows.is_empty() => rows,
                _ => {
                    // Fallback: vector-only search if FTS returns nothing
                    if query_embedding.is_some() {
                        let fallback_sql = format!(
                            "SELECT {} FROM agent_memories m
                             WHERE m.agent_id = ? AND m.embedding IS NOT NULL
                             AND (m.expires_at IS NULL OR m.expires_at > ?)
                             ORDER BY m.relevance_score DESC, m.created_at DESC LIMIT ?",
                            MEMORY_COLUMNS
                        );
                        sqlx::query(&fallback_sql)
                            .bind(&query.agent_id)
                            .bind(now)
                            .bind(query.limit * 3) // fetch more for re-ranking
                            .fetch_all(&self.pool)
                            .await?
                    } else {
                        Vec::new()
                    }
                }
            }
        } else {
            let sql = format!(
                "SELECT {} FROM agent_memories m
                 WHERE m.agent_id = ?
                 AND (m.expires_at IS NULL OR m.expires_at > ?)
                 ORDER BY m.relevance_score DESC, m.created_at DESC LIMIT ?",
                MEMORY_COLUMNS
            );
            sqlx::query(&sql)
                .bind(&query.agent_id)
                .bind(now)
                .bind(query.limit)
                .fetch_all(&self.pool)
                .await?
        };

        let mut results: Vec<ScoredMemory> = rows.iter().map(|r| {
            let mem = row_to_memory(r);
            // Compute vector similarity if we have embeddings
            let score = if let (Some(qemb), Some(stored_emb)) = (&query_embedding, &mem.embedding) {
                let stored: Vec<f32> = stored_emb.chunks(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect();
                cosine_similarity(qemb, &stored)
            } else {
                mem.relevance_score
            };
            ScoredMemory { memory: mem, score }
        })
        .filter(|s| {
            if let Some(ref mt) = query.memory_type {
                if s.memory.memory_type != *mt { return false; }
            }
            if let Some(ref gid) = query.group_id {
                if s.memory.group_id.as_deref() != Some(gid) { return false; }
            }
            true
        })
        .collect();

        // Sort by score descending
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(query.limit as usize);

        // Update access count
        for scored in &results {
            let _ = sqlx::query(
                "UPDATE agent_memories SET access_count = access_count + 1, last_accessed_at = ? WHERE id = ?"
            )
            .bind(now)
            .bind(&scored.memory.id)
            .execute(&self.pool)
            .await;
        }

        Ok(results)
    }

    pub async fn list_by_agent(
        &self,
        agent_id: &str,
        memory_type: Option<&MemoryLayer>,
        limit: i64,
    ) -> Result<Vec<AgentMemory>> {
        let now = chrono::Utc::now().timestamp();

        let rows = if let Some(mt) = memory_type {
            sqlx::query(&format!(
                "SELECT {} FROM agent_memories WHERE agent_id = ? AND memory_type = ? AND (expires_at IS NULL OR expires_at > ?) ORDER BY created_at DESC LIMIT ?",
                MEMORY_COLUMNS
            ))
            .bind(agent_id).bind(mt.as_str()).bind(now).bind(limit)
            .fetch_all(&self.pool).await?
        } else {
            sqlx::query(&format!(
                "SELECT {} FROM agent_memories WHERE agent_id = ? AND (expires_at IS NULL OR expires_at > ?) ORDER BY created_at DESC LIMIT ?",
                MEMORY_COLUMNS
            ))
            .bind(agent_id).bind(now).bind(limit)
            .fetch_all(&self.pool).await?
        };

        Ok(rows.iter().map(row_to_memory).collect())
    }

    pub async fn update_summary(&self, memory_id: &str, summary: &str) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query("UPDATE agent_memories SET summary = ?, updated_at = ? WHERE id = ?")
            .bind(summary).bind(now).bind(memory_id)
            .execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete(&self, memory_id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM agent_memories WHERE id = ?")
            .bind(memory_id)
            .execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn cleanup_expired(&self) -> Result<u64> {
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query("DELETE FROM agent_memories WHERE expires_at IS NOT NULL AND expires_at < ?")
            .bind(now)
            .execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    pub async fn stats(&self, agent_id: &str) -> Result<MemoryStatsResult> {
        let now = chrono::Utc::now().timestamp();

        let working: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agent_memories WHERE agent_id = ? AND memory_type = 'working' AND (expires_at IS NULL OR expires_at > ?)"
        ).bind(agent_id).bind(now).fetch_one(&self.pool).await?;

        let episodic: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agent_memories WHERE agent_id = ? AND memory_type = 'episodic' AND (expires_at IS NULL OR expires_at > ?)"
        ).bind(agent_id).bind(now).fetch_one(&self.pool).await?;

        let semantic: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agent_memories WHERE agent_id = ? AND memory_type = 'semantic' AND (expires_at IS NULL OR expires_at > ?)"
        ).bind(agent_id).bind(now).fetch_one(&self.pool).await?;

        Ok(MemoryStatsResult {
            agent_id: agent_id.to_string(),
            working_count: working,
            episodic_count: episodic,
            semantic_count: semantic,
            total_count: working + episodic + semantic,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStatsResult {
    pub agent_id: String,
    pub working_count: i64,
    pub episodic_count: i64,
    pub semantic_count: i64,
    pub total_count: i64,
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();

        sqlx::query("CREATE TABLE users_v3 (id TEXT PRIMARY KEY, name TEXT NOT NULL, user_type TEXT NOT NULL DEFAULT 'human', status TEXT NOT NULL DEFAULT 'offline', platform_role TEXT NOT NULL DEFAULT 'user', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO users_v3 (id, name, created_at, updated_at) VALUES ('agent1', 'Test Agent', 0, 0)")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE groups (id TEXT PRIMARY KEY, name TEXT NOT NULL, group_type TEXT NOT NULL DEFAULT 'collaboration', owner_id TEXT NOT NULL, settings TEXT NOT NULL DEFAULT '{}', member_count INTEGER NOT NULL DEFAULT 0, message_count INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE agent_memories (id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, memory_type TEXT NOT NULL CHECK(memory_type IN ('working', 'episodic', 'semantic')), content TEXT NOT NULL, summary TEXT, embedding BLOB, embedding_model TEXT, source_type TEXT, source_id TEXT, group_id TEXT, relevance_score REAL NOT NULL DEFAULT 0.7, access_count INTEGER NOT NULL DEFAULT 0, last_accessed_at INTEGER, expires_at INTEGER, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE VIRTUAL TABLE agent_memories_fts USING fts5(content, content='agent_memories', content_rowid='rowid')")
            .execute(&pool).await.unwrap();

        pool
    }

    #[tokio::test]
    async fn test_store_and_get() {
        let pool = setup_db().await;
        let svc = MemoryService::new(pool);

        let mem = svc.store("agent1", MemoryLayer::Working, "hello world", None, None, None, None, None).await.unwrap();
        assert_eq!(mem.content, "hello world");
        assert_eq!(mem.memory_type, MemoryLayer::Working);

        let fetched = svc.get(&mem.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, mem.id);
    }

    #[tokio::test]
    async fn test_ttl_expiration() {
        let pool = setup_db().await;
        let svc = MemoryService::new(pool);

        let mem = svc.store("agent1", MemoryLayer::Working, "expired", None, None, None, None, Some(-1)).await.unwrap();

        let query = MemoryQuery {
            agent_id: "agent1".to_string(),
            query_text: None,
            memory_type: None,
            group_id: None,
            limit: 10,
        };
        let results = svc.search(&query).await.unwrap();
        assert!(results.is_empty());

        let fetched = svc.get(&mem.id).await.unwrap();
        assert!(fetched.is_some());
    }

    #[tokio::test]
    async fn test_list_by_agent_and_type() {
        let pool = setup_db().await;
        let svc = MemoryService::new(pool);

        svc.store("agent1", MemoryLayer::Working, "w1", None, None, None, None, None).await.unwrap();
        svc.store("agent1", MemoryLayer::Episodic, "e1", None, None, None, None, None).await.unwrap();
        svc.store("agent1", MemoryLayer::Semantic, "s1", None, None, None, None, None).await.unwrap();
        svc.store("agent2", MemoryLayer::Working, "w2", None, None, None, None, None).await.unwrap();

        let all = svc.list_by_agent("agent1", None, 10).await.unwrap();
        assert_eq!(all.len(), 3);

        let working = svc.list_by_agent("agent1", Some(&MemoryLayer::Working), 10).await.unwrap();
        assert_eq!(working.len(), 1);
        assert_eq!(working[0].content, "w1");
    }

    #[tokio::test]
    async fn test_stats() {
        let pool = setup_db().await;
        let svc = MemoryService::new(pool);

        svc.store("agent1", MemoryLayer::Working, "w1", None, None, None, None, None).await.unwrap();
        svc.store("agent1", MemoryLayer::Working, "w2", None, None, None, None, None).await.unwrap();
        svc.store("agent1", MemoryLayer::Episodic, "e1", None, None, None, None, None).await.unwrap();

        let stats = svc.stats("agent1").await.unwrap();
        assert_eq!(stats.working_count, 2);
        assert_eq!(stats.episodic_count, 1);
        assert_eq!(stats.semantic_count, 0);
        assert_eq!(stats.total_count, 3);
    }

    #[tokio::test]
    async fn test_delete() {
        let pool = setup_db().await;
        let svc = MemoryService::new(pool);

        let mem = svc.store("agent1", MemoryLayer::Working, "delete me", None, None, None, None, None).await.unwrap();
        assert!(svc.delete(&mem.id).await.unwrap());
        assert!(svc.get(&mem.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_update_summary() {
        let pool = setup_db().await;
        let svc = MemoryService::new(pool);

        let mem = svc.store("agent1", MemoryLayer::Episodic, "content", None, None, None, None, None).await.unwrap();
        assert!(svc.update_summary(&mem.id, "new summary").await.unwrap());

        let fetched = svc.get(&mem.id).await.unwrap().unwrap();
        assert_eq!(fetched.summary.as_deref(), Some("new summary"));
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let pool = setup_db().await;
        let svc = MemoryService::new(pool);

        svc.store("agent1", MemoryLayer::Working, "expired", None, None, None, None, Some(-1)).await.unwrap();
        svc.store("agent1", MemoryLayer::Working, "alive", None, None, None, None, Some(24)).await.unwrap();

        let cleaned = svc.cleanup_expired().await.unwrap();
        assert_eq!(cleaned, 1);
    }
}
