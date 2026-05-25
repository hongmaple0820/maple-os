use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Working,
    Episodic,
    Semantic,
}

impl MemoryType {
    pub fn as_str(&self) -> &str {
        match self {
            MemoryType::Working => "working",
            MemoryType::Episodic => "episodic",
            MemoryType::Semantic => "semantic",
        }
    }

}

impl std::str::FromStr for MemoryType {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "episodic" => MemoryType::Episodic,
            "semantic" => MemoryType::Semantic,
            _ => MemoryType::Working,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub memory_type: MemoryType,
    pub content: String,
    pub metadata: HashMap<String, String>,
    pub created_at: i64,
    pub access_count: u32,
}

pub struct MemoryStore {
    db: sqlx::SqlitePool,
    working: HashMap<String, MemoryEntry>,
    episodic: HashMap<String, MemoryEntry>,
    semantic: HashMap<String, MemoryEntry>,
}

impl MemoryStore {
    pub fn new(db: sqlx::SqlitePool) -> Self {
        Self {
            db,
            working: HashMap::new(),
            episodic: HashMap::new(),
            semantic: HashMap::new(),
        }
    }

    pub async fn store(&mut self, entry: MemoryEntry) -> Result<()> {
        let metadata_json = serde_json::to_string(&entry.metadata)?;
        sqlx::query(
            "INSERT OR REPLACE INTO memories (id, memory_type, content, metadata, created_at, access_count) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(&entry.id)
        .bind(entry.memory_type.as_str())
        .bind(&entry.content)
        .bind(&metadata_json)
        .bind(entry.created_at)
        .bind(entry.access_count)
        .execute(&self.db)
        .await?;

        let map = match entry.memory_type {
            MemoryType::Working => &mut self.working,
            MemoryType::Episodic => &mut self.episodic,
            MemoryType::Semantic => &mut self.semantic,
        };
        map.insert(entry.id.clone(), entry);
        Ok(())
    }

    pub fn retrieve(&self, memory_type: &MemoryType, id: &str) -> Option<&MemoryEntry> {
        match memory_type {
            MemoryType::Working => self.working.get(id),
            MemoryType::Episodic => self.episodic.get(id),
            MemoryType::Semantic => self.semantic.get(id),
        }
    }

pub async fn search_by_type(&self, memory_type: &MemoryType, keyword: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        let rows = sqlx::query_as::<_, (String, String, String, String, i64, i32)>(
            "SELECT id, memory_type, content, metadata, created_at, access_count FROM memories WHERE memory_type = ? AND content LIKE ? ORDER BY created_at DESC LIMIT ?"
        )
        .bind(memory_type.as_str())
        .bind(format!("%{}%", keyword))
        .bind(limit as i64)
        .fetch_all(&self.db)
        .await?;

        let entries: Vec<MemoryEntry> = rows.into_iter().map(|(id, mt, content, metadata, created_at, access_count)| {
            let metadata_map: HashMap<String, String> = serde_json::from_str(&metadata).unwrap_or_default();
            MemoryEntry { id, memory_type: mt.parse::<MemoryType>().unwrap_or(MemoryType::Working), content, metadata: metadata_map, created_at, access_count: access_count as u32 }
        }).collect();
        Ok(entries)
    }

    pub async fn get(&self, id: &str) -> Result<Option<MemoryEntry>> {
        let row = sqlx::query_as::<_, (String, String, String, String, i64, i32)>(
            "SELECT id, memory_type, content, metadata, created_at, access_count FROM memories WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.db)
        .await?;

        Ok(row.map(|(id, mt, content, metadata, created_at, access_count)| {
            let metadata_map: HashMap<String, String> = serde_json::from_str(&metadata).unwrap_or_default();
            MemoryEntry { id, memory_type: mt.parse::<MemoryType>().unwrap_or(MemoryType::Working), content, metadata: metadata_map, created_at, access_count: access_count as u32 }
        }))
    }

    pub async fn delete(&mut self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM memories WHERE id = ?")
            .bind(id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    pub async fn load_all(&mut self) -> Result<()> {
        let rows = sqlx::query_as::<_, (String, String, String, String, i64, i32)>(
            "SELECT id, memory_type, content, metadata, created_at, access_count FROM memories ORDER BY created_at ASC"
        )
        .fetch_all(&self.db)
        .await?;

        for (id, mt, content, metadata, created_at, access_count) in rows {
            let metadata_map: HashMap<String, String> = serde_json::from_str(&metadata).unwrap_or_default();
            let entry = MemoryEntry {
                id,
                memory_type: mt.parse::<MemoryType>().unwrap_or(MemoryType::Working),
                content,
                metadata: metadata_map,
                created_at,
                access_count: access_count as u32,
            };
            let map = match entry.memory_type {
                MemoryType::Working => &mut self.working,
                MemoryType::Episodic => &mut self.episodic,
                MemoryType::Semantic => &mut self.semantic,
            };
            map.insert(entry.id.clone(), entry);
        }

        tracing::info!(
            working = self.working.len(),
            episodic = self.episodic.len(),
            semantic = self.semantic.len(),
            "Memories loaded from SQLite"
        );
        Ok(())
    }

    pub fn consolidate(&mut self) {
        let working_ids: Vec<String> = self.working.keys().cloned().collect();
        for id in working_ids {
            if let Some(entry) = self.working.remove(&id) {
                let mut episodic_entry = entry.clone();
                episodic_entry.memory_type = MemoryType::Episodic;
                self.episodic.insert(id, episodic_entry);
            }
        }
    }
}
