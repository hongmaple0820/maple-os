use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Performance Optimization — caching, token counting optimization, concurrency tuning
///
/// Features:
/// - LRU cache for tool results
/// - Token count caching
/// - Concurrency limiter
/// - Performance metrics collection

/// LRU Cache
pub struct LruCache<K, V> {
    capacity: usize,
    items: HashMap<K, CacheEntry<V>>,
    access_order: Vec<K>,
}

struct CacheEntry<V> {
    value: V,
    inserted_at: Instant,
    ttl: Duration,
}

impl<K: Eq + Hash + Clone, V: Clone> LruCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            items: HashMap::new(),
            access_order: Vec::new(),
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        // TTL is set per entry in this implementation
        self
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        // Check if entry exists and is not expired
        if let Some(entry) = self.items.get(key) {
            if entry.inserted_at.elapsed() < entry.ttl {
                // Move to end of access order (most recently used)
                self.access_order.retain(|k| k != key);
                self.access_order.push(key.clone());
                return Some(&entry.value);
            } else {
                // Entry expired, remove it
                self.items.remove(key);
                self.access_order.retain(|k| k != key);
            }
        }
        None
    }

    pub fn insert(&mut self, key: K, value: V, ttl: Duration) {
        // Remove if already exists
        if self.items.contains_key(&key) {
            self.items.remove(&key);
            self.access_order.retain(|k| k != &key);
        }

        // Evict if at capacity
        while self.items.len() >= self.capacity {
            if let Some(oldest) = self.access_order.first().cloned() {
                self.items.remove(&oldest);
                self.access_order.remove(0);
            }
        }

        // Insert new entry
        self.items.insert(
            key.clone(),
            CacheEntry {
                value,
                inserted_at: Instant::now(),
                ttl,
            },
        );
        self.access_order.push(key);
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.access_order.retain(|k| k != key);
        self.items.remove(key).map(|entry| entry.value)
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.access_order.clear();
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Tool result cache
pub struct ToolResultCache {
    cache: LruCache<String, CachedToolResult>,
}

#[derive(Clone)]
struct CachedToolResult {
    output: serde_json::Value,
    success: bool,
    cached_at: Instant,
    hit_count: u32,
}

impl ToolResultCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: LruCache::new(capacity),
        }
    }

    pub fn get(
        &mut self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Option<(serde_json::Value, bool)> {
        let key = self.build_key(tool_name, input);
        self.cache
            .get(&key)
            .map(|entry| (entry.output.clone(), entry.success))
    }

    pub fn insert(
        &mut self,
        tool_name: &str,
        input: &serde_json::Value,
        output: serde_json::Value,
        success: bool,
        ttl: Duration,
    ) {
        let key = self.build_key(tool_name, input);
        self.cache.insert(
            key,
            CachedToolResult {
                output,
                success,
                cached_at: Instant::now(),
                hit_count: 0,
            },
            ttl,
        );
    }

    fn build_key(&self, tool_name: &str, input: &serde_json::Value) -> String {
        format!("{}:{}", tool_name, input)
    }
}

/// Token count cache
pub struct TokenCountCache {
    cache: LruCache<String, usize>,
}

impl TokenCountCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: LruCache::new(capacity),
        }
    }

    pub fn get_count(&mut self, text: &str) -> Option<usize> {
        self.cache.get(&text.to_string()).copied()
    }

    pub fn insert_count(&mut self, text: &str, count: usize) {
        self.cache
            .insert(text.to_string(), count, Duration::from_secs(300));
    }
}

/// Concurrency limiter
pub struct ConcurrencyLimiter {
    max_concurrent: usize,
    current: Arc<RwLock<usize>>,
}

impl ConcurrencyLimiter {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent,
            current: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn acquire(&self) -> Result<ConcurrencyGuard, String> {
        let mut current = self.current.write().await;
        if *current >= self.max_concurrent {
            return Err("Concurrency limit reached".to_string());
        }
        *current += 1;
        Ok(ConcurrencyGuard {
            current: self.current.clone(),
        })
    }

    pub async fn available(&self) -> usize {
        let current = self.current.read().await;
        self.max_concurrent - *current
    }
}

pub struct ConcurrencyGuard {
    current: Arc<RwLock<usize>>,
}

impl Drop for ConcurrencyGuard {
    fn drop(&mut self) {
        // Note: This is synchronous drop, which is fine for RwLock
        // In async context, we'd need to handle this differently
        if let Ok(mut current) = self.current.try_write() {
            *current = current.saturating_sub(1);
        }
    }
}

/// Performance metrics
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub tool_calls: u64,
    pub tool_call_duration_ms: u64,
    pub llm_calls: u64,
    pub llm_call_duration_ms: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub token_count_cache_hits: u64,
    pub token_count_cache_misses: u64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            tool_calls: 0,
            tool_call_duration_ms: 0,
            llm_calls: 0,
            llm_call_duration_ms: 0,
            cache_hits: 0,
            cache_misses: 0,
            token_count_cache_hits: 0,
            token_count_cache_misses: 0,
        }
    }
}

/// Performance monitor
pub struct PerformanceMonitor {
    metrics: Arc<RwLock<PerformanceMetrics>>,
    start_time: Instant,
}

impl PerformanceMonitor {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(PerformanceMetrics::default())),
            start_time: Instant::now(),
        }
    }

    pub async fn record_tool_call(&self, duration: Duration) {
        let mut metrics = self.metrics.write().await;
        metrics.tool_calls += 1;
        metrics.tool_call_duration_ms += duration.as_millis() as u64;
    }

    pub async fn record_llm_call(&self, duration: Duration) {
        let mut metrics = self.metrics.write().await;
        metrics.llm_calls += 1;
        metrics.llm_call_duration_ms += duration.as_millis() as u64;
    }

    pub async fn record_cache_hit(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.cache_hits += 1;
    }

    pub async fn record_cache_miss(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.cache_misses += 1;
    }

    pub async fn record_token_count_cache_hit(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.token_count_cache_hits += 1;
    }

    pub async fn record_token_count_cache_miss(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.token_count_cache_misses += 1;
    }

    pub async fn get_metrics(&self) -> PerformanceMetrics {
        self.metrics.read().await.clone()
    }

    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub async fn get_summary(&self) -> PerformanceSummary {
        let metrics = self.metrics.read().await;
        let uptime = self.start_time.elapsed();

        PerformanceSummary {
            uptime,
            tool_calls_per_second: metrics.tool_calls as f64 / uptime.as_secs_f64(),
            avg_tool_call_duration_ms: if metrics.tool_calls > 0 {
                metrics.tool_call_duration_ms as f64 / metrics.tool_calls as f64
            } else {
                0.0
            },
            llm_calls_per_second: metrics.llm_calls as f64 / uptime.as_secs_f64(),
            avg_llm_call_duration_ms: if metrics.llm_calls > 0 {
                metrics.llm_call_duration_ms as f64 / metrics.llm_calls as f64
            } else {
                0.0
            },
            cache_hit_rate: if metrics.cache_hits + metrics.cache_misses > 0 {
                metrics.cache_hits as f64 / (metrics.cache_hits + metrics.cache_misses) as f64
            } else {
                0.0
            },
            token_count_cache_hit_rate: if metrics.token_count_cache_hits
                + metrics.token_count_cache_misses
                > 0
            {
                metrics.token_count_cache_hits as f64
                    / (metrics.token_count_cache_hits + metrics.token_count_cache_misses) as f64
            } else {
                0.0
            },
        }
    }
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Performance summary
#[derive(Debug, Clone)]
pub struct PerformanceSummary {
    pub uptime: Duration,
    pub tool_calls_per_second: f64,
    pub avg_tool_call_duration_ms: f64,
    pub llm_calls_per_second: f64,
    pub avg_llm_call_duration_ms: f64,
    pub cache_hit_rate: f64,
    pub token_count_cache_hit_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_cache() {
        let mut cache = LruCache::new(2);
        cache.insert("a", 1, Duration::from_secs(60));
        cache.insert("b", 2, Duration::from_secs(60));

        assert_eq!(cache.get(&"a"), Some(&1));
        assert_eq!(cache.get(&"b"), Some(&2));

        cache.insert("c", 3, Duration::from_secs(60));
        assert_eq!(cache.get(&"a"), None); // Evicted
        assert_eq!(cache.get(&"c"), Some(&3));
    }

    #[test]
    fn test_lru_cache_ttl() {
        let mut cache = LruCache::new(10);
        cache.insert("key", "value", Duration::from_millis(100));

        // Should exist immediately
        assert!(cache.get(&"key").is_some());

        // Wait for TTL to expire
        std::thread::sleep(Duration::from_millis(150));
        assert!(cache.get(&"key").is_none());
    }

    #[test]
    fn test_tool_result_cache() {
        let mut cache = ToolResultCache::new(100);
        let input = serde_json::json!({"path": "test.txt"});

        cache.insert(
            "read_file",
            &input,
            serde_json::json!({"content": "hello"}),
            true,
            Duration::from_secs(60),
        );

        let result = cache.get("read_file", &input);
        assert!(result.is_some());
        let (output, success) = result.unwrap();
        assert!(success);
        assert_eq!(output["content"], "hello");
    }

    #[tokio::test]
    async fn test_concurrency_limiter() {
        let limiter = ConcurrencyLimiter::new(2);

        let _guard1 = limiter.acquire().await.unwrap();
        let _guard2 = limiter.acquire().await.unwrap();

        assert!(limiter.acquire().await.is_err());
        assert_eq!(limiter.available().await, 0);

        drop(_guard1);
        assert_eq!(limiter.available().await, 1);
    }

    #[tokio::test]
    async fn test_performance_monitor() {
        let monitor = PerformanceMonitor::new();

        monitor.record_tool_call(Duration::from_millis(100)).await;
        monitor.record_cache_hit().await;

        let metrics = monitor.get_metrics().await;
        assert_eq!(metrics.tool_calls, 1);
        assert_eq!(metrics.cache_hits, 1);
    }
}
