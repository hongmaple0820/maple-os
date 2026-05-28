use anyhow::Result;
use maple_kb::memory::{MemoryEntry, MemoryStore, MemoryType};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Memory scope — determines lifetime and visibility
#[derive(Debug, Clone, PartialEq)]
pub enum MemoryScope {
    /// Tied to a single session, discarded after session ends
    Session(String),
    /// Persisted across sessions for a specific user
    User(String),
    /// Global memories shared across all users
    Global,
}

/// MemoryManager — orchestrates cross-session memory lifecycle
///
/// Features:
/// - Automatic memory extraction from conversation turns
/// - Relevance-based retrieval (BM25 + recency + access frequency)
/// - Memory injection into session context
/// - Scoped memories (session / user / global)
pub struct MemoryManager {
    store: Arc<Mutex<MemoryStore>>,
    /// Maximum memories to inject into context
    max_inject: usize,
    /// Recency decay factor (0.0-1.0) — higher = more weight on recent
    recency_weight: f64,
    /// Access frequency weight
    frequency_weight: f64,
    /// Relevance score weight (from BM25/keyword match)
    relevance_weight: f64,
}

impl MemoryManager {
    pub fn new(store: Arc<Mutex<MemoryStore>>) -> Self {
        Self {
            store,
            max_inject: 10,
            recency_weight: 0.3,
            frequency_weight: 0.2,
            relevance_weight: 0.5,
        }
    }

    pub fn with_max_inject(mut self, max: usize) -> Self {
        self.max_inject = max;
        self
    }

    /// Store a memory with scope
    pub async fn remember(
        &self,
        scope: &MemoryScope,
        memory_type: MemoryType,
        content: &str,
        tags: Vec<&str>,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        let mut metadata = HashMap::new();
        match scope {
            MemoryScope::Session(sid) => {
                metadata.insert("scope".to_string(), "session".to_string());
                metadata.insert("session_id".to_string(), sid.clone());
            }
            MemoryScope::User(uid) => {
                metadata.insert("scope".to_string(), "user".to_string());
                metadata.insert("user_id".to_string(), uid.clone());
            }
            MemoryScope::Global => {
                metadata.insert("scope".to_string(), "global".to_string());
            }
        }
        for tag in tags {
            metadata.insert(format!("tag_{}", tag), "true".to_string());
        }

        let entry = MemoryEntry {
            id: id.clone(),
            memory_type,
            content: content.to_string(),
            metadata,
            created_at: now,
            access_count: 0,
        };

        let mut store = self.store.lock().await;
        store.store(entry).await?;
        Ok(id)
    }

    /// Retrieve memories relevant to a query, scored by BM25 + recency + frequency
    pub async fn retrieve_relevant(
        &self,
        query: &str,
        scope: Option<&MemoryScope>,
        limit: usize,
    ) -> Result<Vec<ScoredMemory>> {
        let store = self.store.lock().await;

        // Search across all memory types
        let working = store
            .search_by_type(&MemoryType::Working, query, limit * 2)
            .await
            .unwrap_or_default();
        let episodic = store
            .search_by_type(&MemoryType::Episodic, query, limit * 2)
            .await
            .unwrap_or_default();
        let semantic = store
            .search_by_type(&MemoryType::Semantic, query, limit * 2)
            .await
            .unwrap_or_default();

        let mut all: Vec<MemoryEntry> = Vec::new();
        all.extend(working);
        all.extend(episodic);
        all.extend(semantic);

        // Filter by scope if specified
        if let Some(scope) = scope {
            all.retain(|entry| match scope {
                MemoryScope::Session(sid) => {
                    entry.metadata.get("session_id").map_or(false, |s| s == sid)
                }
                MemoryScope::User(uid) => entry.metadata.get("user_id").map_or(false, |s| s == uid),
                MemoryScope::Global => entry.metadata.get("scope").map_or(false, |s| s == "global"),
            });
        }

        let now = chrono::Utc::now().timestamp();
        let max_access = all.iter().map(|e| e.access_count).max().unwrap_or(1).max(1);

        // Score: relevance (keyword match) + recency + frequency
        let mut scored: Vec<ScoredMemory> = all
            .into_iter()
            .map(|entry| {
                let relevance = Self::keyword_relevance(query, &entry.content);
                let age_hours = (now - entry.created_at).max(0) as f64 / 3600.0;
                let recency = 1.0 / (1.0 + age_hours / 24.0); // decay over days
                let frequency = entry.access_count as f64 / max_access as f64;

                let score = self.relevance_weight * relevance
                    + self.recency_weight * recency
                    + self.frequency_weight * frequency;

                ScoredMemory {
                    entry,
                    score,
                    relevance,
                    recency,
                    frequency,
                }
            })
            .filter(|s| s.score > 0.05)
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);

        // Increment access count for retrieved memories
        drop(store);
        {
            let mut store = self.store.lock().await;
            for scored in &scored {
                let _ = store.increment_access(&scored.entry.id).await;
            }
        }

        Ok(scored)
    }

    /// Build a memory context string for injection into the session
    pub async fn build_context_injection(
        &self,
        query: &str,
        scope: Option<&MemoryScope>,
    ) -> Result<String> {
        let memories = self
            .retrieve_relevant(query, scope, self.max_inject)
            .await?;
        if memories.is_empty() {
            return Ok(String::new());
        }

        let mut lines = vec![
            "[Relevant Memories from Previous Sessions]".to_string(),
            String::new(),
        ];

        for (i, mem) in memories.iter().enumerate() {
            let type_label = match mem.entry.memory_type {
                MemoryType::Working => "working",
                MemoryType::Episodic => "episodic",
                MemoryType::Semantic => "semantic",
            };
            lines.push(format!(
                "{}. [{}] {} (score: {:.2})",
                i + 1,
                type_label,
                mem.entry.content,
                mem.score
            ));
        }

        lines.push(String::new());
        lines.push("---".to_string());

        Ok(lines.join("\n"))
    }

    /// Extract memories from a conversation turn (user + assistant exchange)
    /// Uses simple heuristic extraction — key decisions, preferences, facts
    pub async fn extract_from_turn(
        &self,
        user_msg: &str,
        assistant_msg: &str,
        scope: &MemoryScope,
    ) -> Result<Vec<String>> {
        let mut memories = Vec::new();

        // Extract user preferences (patterns like "I prefer", "I like", "always", "never")
        let preference_patterns = [
            "i prefer",
            "i like",
            "i want",
            "always",
            "never",
            "don't",
            "do not",
            "please use",
            "use",
            "switch to",
        ];
        let user_lower = user_msg.to_lowercase();
        for pattern in &preference_patterns {
            if user_lower.contains(pattern) {
                // Extract the sentence containing the pattern
                for sentence in user_msg.split('.') {
                    if sentence.to_lowercase().contains(pattern) {
                        let content = format!("User preference: {}", sentence.trim());
                        let id = self
                            .remember(scope, MemoryType::Semantic, &content, vec!["preference"])
                            .await?;
                        memories.push(id);
                        break;
                    }
                }
            }
        }

        // Extract decisions (patterns like "let's use", "we'll go with", "decided")
        let decision_patterns = [
            "let's use",
            "we'll go with",
            "decided",
            "go with",
            "switch to",
        ];
        let assistant_lower = assistant_msg.to_lowercase();
        for pattern in &decision_patterns {
            if assistant_lower.contains(pattern) {
                for sentence in assistant_msg.split('.') {
                    if sentence.to_lowercase().contains(pattern) {
                        let content = format!("Decision: {}", sentence.trim());
                        let id = self
                            .remember(scope, MemoryType::Episodic, &content, vec!["decision"])
                            .await?;
                        memories.push(id);
                        break;
                    }
                }
            }
        }

        // Extract facts (long assistant messages with technical content)
        if assistant_msg.len() > 500 && assistant_msg.contains("```") {
            let content = format!(
                "Technical context from conversation: {}",
                Self::summarize_for_memory(assistant_msg)
            );
            let id = self
                .remember(scope, MemoryType::Working, &content, vec!["technical"])
                .await?;
            memories.push(id);
        }

        Ok(memories)
    }

    /// Simple keyword relevance score (0.0 - 1.0)
    fn keyword_relevance(query: &str, content: &str) -> f64 {
        let query_words: Vec<&str> = query
            .to_lowercase()
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .collect();

        if query_words.is_empty() {
            return 0.0;
        }

        let content_lower = content.to_lowercase();
        let matched = query_words
            .iter()
            .filter(|w| content_lower.contains(**w))
            .count();

        matched as f64 / query_words.len() as f64
    }

    /// Summarize text for memory storage (truncate to key parts)
    fn summarize_for_memory(text: &str) -> String {
        // Take first 200 chars + last 100 chars
        if text.len() <= 300 {
            return text.to_string();
        }
        let prefix = &text[..200];
        let suffix = &text[text.len() - 100..];
        format!("{}...{}", prefix, suffix)
    }
}

/// A memory entry with its computed relevance score
#[derive(Debug, Clone)]
pub struct ScoredMemory {
    pub entry: MemoryEntry,
    pub score: f64,
    pub relevance: f64,
    pub recency: f64,
    pub frequency: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_relevance() {
        let score = MemoryManager::keyword_relevance("rust programming", "I love Rust programming");
        assert!(score > 0.5, "expected >0.5, got {}", score);

        let score = MemoryManager::keyword_relevance("python", "I love Rust programming");
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_keyword_relevance_empty_query() {
        let score = MemoryManager::keyword_relevance("", "some content");
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_summarize_short() {
        let text = "short text";
        assert_eq!(MemoryManager::summarize_for_memory(text), text);
    }

    #[test]
    fn test_summarize_long() {
        let text = "a".repeat(500);
        let summary = MemoryManager::summarize_for_memory(&text);
        assert!(summary.len() < text.len());
        assert!(summary.contains("..."));
    }
}
