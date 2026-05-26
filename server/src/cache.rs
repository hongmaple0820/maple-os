use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};
use dashmap::DashMap;
use tokio::sync::RwLock;

/// 缓存条目
#[derive(Debug, Clone)]
struct CacheEntry<V> {
    value: V,
    inserted_at: Instant,
    ttl: Duration,
}

impl<V> CacheEntry<V> {
    fn new(value: V, ttl: Duration) -> Self {
        Self {
            value,
            inserted_at: Instant::now(),
            ttl,
        }
    }

    fn is_expired(&self) -> bool {
        self.inserted_at.elapsed() > self.ttl
    }
}

/// 通用缓存结构
#[derive(Debug, Clone)]
pub struct Cache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    store: Arc<DashMap<K, CacheEntry<V>>>,
    default_ttl: Duration,
    max_size: usize,
}

impl<K, V> Cache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// 创建新的缓存
    pub fn new(default_ttl: Duration, max_size: usize) -> Self {
        Self {
            store: Arc::new(DashMap::new()),
            default_ttl,
            max_size,
        }
    }

    /// 获取缓存值
    pub fn get(&self, key: &K) -> Option<V> {
        if let Some(entry) = self.store.get(key) {
            if entry.is_expired() {
                drop(entry);
                self.store.remove(key);
                None
            } else {
                Some(entry.value.clone())
            }
        } else {
            None
        }
    }

    /// 插入缓存值
    pub fn insert(&self, key: K, value: V) {
        self.insert_with_ttl(key, value, self.default_ttl);
    }

    /// 插入带自定义TTL的缓存值
    pub fn insert_with_ttl(&self, key: K, value: V, ttl: Duration) {
        // 如果缓存已满，清理过期条目
        if self.store.len() >= self.max_size {
            self.cleanup();
        }
        
        // 如果仍然已满，删除最旧的条目
        if self.store.len() >= self.max_size {
            if let Some(oldest_key) = self.find_oldest_key() {
                self.store.remove(&oldest_key);
            }
        }

        self.store.insert(key, CacheEntry::new(value, ttl));
    }

    /// 删除缓存条目
    pub fn remove(&self, key: &K) -> Option<V> {
        self.store.remove(key).map(|(_, entry)| entry.value)
    }

    /// 清空缓存
    pub fn clear(&self) {
        self.store.clear();
    }

    /// 清理过期条目
    pub fn cleanup(&self) {
        self.store.retain(|_, entry| !entry.is_expired());
    }

    /// 获取缓存大小
    pub fn len(&self) -> usize {
        self.store.len()
    }

    /// 检查缓存是否为空
    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }

    /// 查找最旧的条目
    fn find_oldest_key(&self) -> Option<K> {
        let mut oldest_key = None;
        let mut oldest_time = Instant::now();

        for entry in self.store.iter() {
            if entry.value().inserted_at < oldest_time {
                oldest_time = entry.value().inserted_at;
                oldest_key = Some(entry.key().clone());
            }
        }

        oldest_key
    }

    /// 获取或插入
    pub async fn get_or_insert_with<F, Fut>(&self, key: K, f: F) -> V
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = V>,
    {
        if let Some(value) = self.get(&key) {
            return value;
        }

        let value = f().await;
        self.insert(key.clone(), value.clone());
        value
    }
}

/// 预定义的缓存实例
pub struct AppCache {
    /// 系统配置缓存
    pub config: Cache<String, serde_json::Value>,
    /// 模型列表缓存
    pub models: Cache<String, Vec<serde_json::Value>>,
    /// Agent列表缓存
    pub agents: Cache<String, Vec<serde_json::Value>>,
    /// 知识库搜索结果缓存
    pub kb_search: Cache<String, Vec<serde_json::Value>>,
    /// LLM响应缓存
    pub llm_response: Cache<String, String>,
}

impl AppCache {
    pub fn new() -> Self {
        Self {
            config: Cache::new(Duration::from_secs(300), 100),      // 5分钟TTL
            models: Cache::new(Duration::from_secs(60), 100),       // 1分钟TTL
            agents: Cache::new(Duration::from_secs(30), 100),       // 30秒TTL
            kb_search: Cache::new(Duration::from_secs(120), 1000),  // 2分钟TTL，最多1000条
            llm_response: Cache::new(Duration::from_secs(600), 500), // 10分钟TTL，最多500条
        }
    }

    /// 清理所有缓存
    pub fn cleanup_all(&self) {
        self.config.cleanup();
        self.models.cleanup();
        self.agents.cleanup();
        self.kb_search.cleanup();
        self.llm_response.cleanup();
    }

    /// 清空所有缓存
    pub fn clear_all(&self) {
        self.config.clear();
        self.models.clear();
        self.agents.clear();
        self.kb_search.clear();
        self.llm_response.clear();
    }
}

impl Default for AppCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_cache_basic_operations() {
        let cache = Cache::new(Duration::from_secs(60), 100);
        
        // 插入和获取
        cache.insert("key1".to_string(), "value1".to_string());
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
        
        // 删除
        cache.remove(&"key1".to_string());
        assert_eq!(cache.get(&"key1".to_string()), None);
    }

    #[test]
    fn test_cache_expiration() {
        let cache = Cache::new(Duration::from_millis(10), 100);
        
        cache.insert("key1".to_string(), "value1".to_string());
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
        
        // 等待过期
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(cache.get(&"key1".to_string()), None);
    }

    #[test]
    fn test_cache_max_size() {
        let cache = Cache::new(Duration::from_secs(60), 2);
        
        cache.insert("key1".to_string(), "value1".to_string());
        cache.insert("key2".to_string(), "value2".to_string());
        cache.insert("key3".to_string(), "value3".to_string());
        
        // 缓存应该只包含2个条目
        assert!(cache.len() <= 2);
    }

    #[tokio::test]
    async fn test_cache_get_or_insert() {
        let cache = Cache::new(Duration::from_secs(60), 100);
        
        let value = cache.get_or_insert_with("key1".to_string(), || async {
            "computed_value".to_string()
        }).await;
        
        assert_eq!(value, "computed_value");
        assert_eq!(cache.get(&"key1".to_string()), Some("computed_value".to_string()));
    }
}