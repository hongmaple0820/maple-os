use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Streaming Tool Result Partial Cache
///
/// Caches partial tool results during streaming execution:
/// - Incremental result accumulation from streaming tools
/// - TTL-based cache expiration
/// - Size-bounded entries with LRU eviction
/// - Partial result retrieval for long-running tools
///
///   Cached result entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedResult {
    pub tool_call_id: String,
    pub tool_name: String,
    pub content: String,
    pub is_partial: bool,
    pub is_complete: bool,
    pub created_at: i64,
    pub chunk_count: usize,
    pub total_bytes: usize,
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of entries in cache
    pub max_entries: usize,
    /// TTL for cache entries
    pub ttl: Duration,
    /// Maximum bytes per entry
    pub max_entry_bytes: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 100,
            ttl: Duration::from_secs(300), // 5 minutes
            max_entry_bytes: 1024 * 1024,  // 1MB
        }
    }
}

/// Internal cache entry with metadata
struct CacheEntry {
    result: CachedResult,
    last_accessed: Instant,
}

/// Streaming tool result cache
pub struct StreamingResultCache {
    config: CacheConfig,
    entries: HashMap<String, CacheEntry>,
}

impl StreamingResultCache {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            config,
            entries: HashMap::new(),
        }
    }

    /// Start a new partial result
    pub fn begin(&mut self, tool_call_id: &str, tool_name: &str) {
        let entry = CacheEntry {
            result: CachedResult {
                tool_call_id: tool_call_id.into(),
                tool_name: tool_name.into(),
                content: String::new(),
                is_partial: true,
                is_complete: false,
                created_at: chrono::Utc::now().timestamp(),
                chunk_count: 0,
                total_bytes: 0,
            },
            last_accessed: Instant::now(),
        };
        self.entries.insert(tool_call_id.into(), entry);
        self.evict_if_needed();
    }

    /// Append a chunk to an existing partial result
    pub fn append_chunk(&mut self, tool_call_id: &str, chunk: &str) -> Result<(), CacheError> {
        let entry = self
            .entries
            .get_mut(tool_call_id)
            .ok_or_else(|| CacheError::NotFound(tool_call_id.into()))?;

        if entry.result.is_complete {
            return Err(CacheError::AlreadyComplete(tool_call_id.into()));
        }

        let new_size = entry.result.total_bytes + chunk.len();
        if new_size > self.config.max_entry_bytes {
            // Truncate and mark complete
            let remaining = self.config.max_entry_bytes.saturating_sub(entry.result.total_bytes);
            if remaining > 0 {
                entry.result.content.push_str(&chunk[..remaining]);
                entry.result.total_bytes = self.config.max_entry_bytes;
            }
            entry.result.is_partial = false;
            entry.result.is_complete = true;
            entry.result.chunk_count += 1;
            return Ok(());
        }

        entry.result.content.push_str(chunk);
        entry.result.total_bytes = new_size;
        entry.result.chunk_count += 1;
        entry.last_accessed = Instant::now();
        Ok(())
    }

    /// Mark a result as complete
    pub fn complete(&mut self, tool_call_id: &str) -> Result<(), CacheError> {
        let entry = self
            .entries
            .get_mut(tool_call_id)
            .ok_or_else(|| CacheError::NotFound(tool_call_id.into()))?;

        entry.result.is_partial = false;
        entry.result.is_complete = true;
        entry.last_accessed = Instant::now();
        Ok(())
    }

    /// Get a cached result (full or partial)
    pub fn get(&mut self, tool_call_id: &str) -> Option<&CachedResult> {
        let entry = self.entries.get_mut(tool_call_id)?;
        entry.last_accessed = Instant::now();
        Some(&entry.result)
    }

    /// Get the partial content accumulated so far
    pub fn get_partial(&mut self, tool_call_id: &str) -> Option<&str> {
        let entry = self.entries.get_mut(tool_call_id)?;
        entry.last_accessed = Instant::now();
        Some(&entry.result.content)
    }

    /// Check if a result is complete
    pub fn is_complete(&self, tool_call_id: &str) -> bool {
        self.entries
            .get(tool_call_id)
            .map(|e| e.result.is_complete)
            .unwrap_or(false)
    }

    /// Remove expired entries
    pub fn cleanup_expired(&mut self) {
        let now = Instant::now();
        self.entries
            .retain(|_, entry| now.duration_since(entry.last_accessed) < self.config.ttl);
    }

    /// Evict LRU entries if over capacity
    fn evict_if_needed(&mut self) {
        if self.entries.len() <= self.config.max_entries {
            return;
        }

        // Find the least recently used entry
        let oldest_key = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(k, _)| k.clone());

        if let Some(key) = oldest_key {
            self.entries.remove(&key);
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let total_bytes: usize = self.entries.values().map(|e| e.result.total_bytes).sum();
        let partial_count = self.entries.values().filter(|e| e.result.is_partial).count();
        let complete_count = self.entries.values().filter(|e| e.result.is_complete).count();

        CacheStats {
            entry_count: self.entries.len(),
            partial_count,
            complete_count,
            total_bytes,
            max_entries: self.config.max_entries,
        }
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of cached entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for StreamingResultCache {
    fn default() -> Self {
        Self::new(CacheConfig::default())
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub entry_count: usize,
    pub partial_count: usize,
    pub complete_count: usize,
    pub total_bytes: usize,
    pub max_entries: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("entry not found: {0}")]
    NotFound(String),
    #[error("entry already complete: {0}")]
    AlreadyComplete(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_begin_and_append() {
        let mut cache = StreamingResultCache::default();
        cache.begin("call1", "read_file");
        cache.append_chunk("call1", "hello ").unwrap();
        cache.append_chunk("call1", "world").unwrap();

        let result = cache.get("call1").unwrap();
        assert_eq!(result.content, "hello world");
        assert_eq!(result.chunk_count, 2);
        assert!(result.is_partial);
    }

    #[test]
    fn test_complete() {
        let mut cache = StreamingResultCache::default();
        cache.begin("call1", "read_file");
        cache.append_chunk("call1", "data").unwrap();
        cache.complete("call1").unwrap();

        assert!(cache.is_complete("call1"));
        let result = cache.get("call1").unwrap();
        assert!(!result.is_partial);
        assert!(result.is_complete);
    }

    #[test]
    fn test_append_after_complete_errors() {
        let mut cache = StreamingResultCache::default();
        cache.begin("call1", "read_file");
        cache.complete("call1").unwrap();

        let result = cache.append_chunk("call1", "more");
        assert!(matches!(result, Err(CacheError::AlreadyComplete(_))));
    }

    #[test]
    fn test_get_nonexistent() {
        let mut cache = StreamingResultCache::default();
        assert!(cache.get("nope").is_none());
        assert!(!cache.is_complete("nope"));
    }

    #[test]
    fn test_max_entry_bytes() {
        let config = CacheConfig {
            max_entry_bytes: 10,
            ..Default::default()
        };
        let mut cache = StreamingResultCache::new(config);

        cache.begin("call1", "read");
        cache.append_chunk("call1", "0123456789abcdef").unwrap();

        let result = cache.get("call1").unwrap();
        assert!(result.total_bytes <= 10);
        assert!(result.is_complete); // Auto-completed on overflow
    }

    #[test]
    fn test_lru_eviction() {
        let config = CacheConfig {
            max_entries: 2,
            ..Default::default()
        };
        let mut cache = StreamingResultCache::new(config);

        cache.begin("c1", "tool");
        cache.begin("c2", "tool");
        cache.begin("c3", "tool"); // Should evict c1

        assert_eq!(cache.len(), 2);
        assert!(cache.get("c1").is_none());
        assert!(cache.get("c2").is_some());
        assert!(cache.get("c3").is_some());
    }

    #[test]
    fn test_stats() {
        let mut cache = StreamingResultCache::default();
        cache.begin("c1", "tool");
        cache.begin("c2", "tool");
        cache.complete("c2").unwrap();

        let stats = cache.stats();
        assert_eq!(stats.entry_count, 2);
        assert_eq!(stats.partial_count, 1);
        assert_eq!(stats.complete_count, 1);
    }

    #[test]
    fn test_clear() {
        let mut cache = StreamingResultCache::default();
        cache.begin("c1", "tool");
        cache.begin("c2", "tool");

        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_get_partial() {
        let mut cache = StreamingResultCache::default();
        cache.begin("c1", "tool");
        cache.append_chunk("c1", "partial data").unwrap();

        let partial = cache.get_partial("c1").unwrap();
        assert_eq!(partial, "partial data");
    }

    #[test]
    fn test_cache_config_defaults() {
        let config = CacheConfig::default();
        assert_eq!(config.max_entries, 100);
        assert_eq!(config.ttl, Duration::from_secs(300));
        assert_eq!(config.max_entry_bytes, 1024 * 1024);
    }
}
