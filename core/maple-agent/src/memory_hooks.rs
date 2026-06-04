use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Memory Provider Lifecycle Hooks — pluggable memory system
///
/// Provides:
/// - MemoryProvider trait for custom storage backends
/// - MemoryHook trait for lifecycle interception (before/after save/load)
/// - TTL-based expiration
/// - Built-in hooks: encryption, compression, sync
///
///   Memory entry (compatible with maple_kb::MemoryEntry)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub content: String,
    pub memory_type: String,
    pub scope: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub access_count: u64,
    pub metadata: HashMap<String, String>,
}

/// Query for retrieving memories
#[derive(Debug, Clone)]
pub struct MemoryQuery {
    pub text: Option<String>,
    pub scope: Option<String>,
    pub memory_type: Option<String>,
    pub tags: Vec<String>,
    pub limit: usize,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            text: None,
            scope: None,
            memory_type: None,
            tags: Vec::new(),
            limit: 10,
        }
    }
}

/// Memory provider statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_entries: u64,
    pub by_type: HashMap<String, u64>,
    pub by_scope: HashMap<String, u64>,
    pub total_size_bytes: u64,
}

/// Hook decision
#[derive(Debug, Clone)]
pub enum HookDecision {
    /// Allow the operation
    Allow,
    /// Deny the operation
    Deny { reason: String },
    /// The entry was modified by the hook
    Modified,
}

/// Memory provider trait — pluggable storage backend
#[async_trait]
pub trait MemoryProvider: Send + Sync {
    /// Store a memory record
    async fn store(&self, record: &MemoryRecord) -> Result<String, MemoryError>;
    /// Retrieve memories matching query
    async fn retrieve(&self, query: &MemoryQuery) -> Result<Vec<MemoryRecord>, MemoryError>;
    /// Delete a memory by ID
    async fn delete(&self, id: &str) -> Result<bool, MemoryError>;
    /// Update a memory record
    async fn update(&self, record: &MemoryRecord) -> Result<(), MemoryError>;
    /// Clean up expired memories
    async fn cleanup(&self) -> Result<u64, MemoryError>;
    /// Get statistics
    async fn stats(&self) -> Result<MemoryStats, MemoryError>;
}

/// Memory lifecycle hook trait
#[async_trait]
pub trait MemoryHook: Send + Sync {
    /// Called before storing a memory (can modify or reject)
    async fn before_save(&self, _record: &mut MemoryRecord) -> Result<HookDecision, MemoryError> {
        Ok(HookDecision::Allow)
    }
    /// Called after storing a memory
    async fn after_save(&self, _record: &MemoryRecord) -> Result<(), MemoryError> {
        Ok(())
    }
    /// Called before retrieving memories
    async fn before_load(&self, _query: &mut MemoryQuery) -> Result<(), MemoryError> {
        Ok(())
    }
    /// Called after retrieving memories (can filter/transform)
    async fn after_load(
        &self,
        _records: &mut Vec<MemoryRecord>,
    ) -> Result<(), MemoryError> {
        Ok(())
    }
    /// Called when a memory expires
    async fn on_expire(&self, _record: &MemoryRecord) -> Result<(), MemoryError> {
        Ok(())
    }
}

/// TTL expiration policy
#[derive(Debug, Clone)]
pub struct TtlPolicy {
    /// Default TTL for all memories
    pub default_ttl: Duration,
    /// Per-type TTL overrides
    pub per_type_ttl: HashMap<String, Duration>,
    /// Cleanup interval
    pub cleanup_interval: Duration,
}

impl Default for TtlPolicy {
    fn default() -> Self {
        Self {
            default_ttl: Duration::from_secs(7 * 24 * 3600), // 7 days
            per_type_ttl: HashMap::new(),
            cleanup_interval: Duration::from_secs(3600), // 1 hour
        }
    }
}

/// Memory manager with provider + hooks
pub struct MemoryManagerV2 {
    provider: Arc<dyn MemoryProvider>,
    hooks: Vec<Arc<dyn MemoryHook>>,
    ttl_policy: Option<TtlPolicy>,
}

impl MemoryManagerV2 {
    pub fn new(provider: Arc<dyn MemoryProvider>) -> Self {
        Self {
            provider,
            hooks: Vec::new(),
            ttl_policy: None,
        }
    }

    /// Add a lifecycle hook
    pub fn with_hook(mut self, hook: Arc<dyn MemoryHook>) -> Self {
        self.hooks.push(hook);
        self
    }

    /// Set TTL policy
    pub fn with_ttl(mut self, policy: TtlPolicy) -> Self {
        self.ttl_policy = Some(policy);
        self
    }

    /// Store a memory (through hook chain)
    pub async fn remember(&self, mut record: MemoryRecord) -> Result<String, MemoryError> {
        // Run before_save hooks
        for hook in &self.hooks {
            match hook.before_save(&mut record).await? {
                HookDecision::Allow => {}
                HookDecision::Deny { reason } => {
                    return Err(MemoryError::HookDenied(reason));
                }
                HookDecision::Modified => {}
            }
        }

        let id = self.provider.store(&record).await?;

        // Run after_save hooks
        for hook in &self.hooks {
            hook.after_save(&record).await?;
        }

        Ok(id)
    }

    /// Retrieve memories (through hook chain)
    pub async fn retrieve(
        &self,
        mut query: MemoryQuery,
    ) -> Result<Vec<MemoryRecord>, MemoryError> {
        // Run before_load hooks
        for hook in &self.hooks {
            hook.before_load(&mut query).await?;
        }

        let mut records = self.provider.retrieve(&query).await?;

        // Run after_load hooks
        for hook in &self.hooks {
            hook.after_load(&mut records).await?;
        }

        Ok(records)
    }

    /// Delete a memory
    pub async fn forget(&self, id: &str) -> Result<bool, MemoryError> {
        self.provider.delete(id).await
    }

    /// Update a memory
    pub async fn update(&self, record: &MemoryRecord) -> Result<(), MemoryError> {
        self.provider.update(record).await
    }

    /// Trigger cleanup of expired memories
    pub async fn cleanup(&self) -> Result<u64, MemoryError> {
        self.provider.cleanup().await
    }

    /// Get provider statistics
    pub async fn stats(&self) -> Result<MemoryStats, MemoryError> {
        self.provider.stats().await
    }
}

/// Error types
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("storage error: {0}")]
    Storage(String),
    #[error("hook denied: {0}")]
    HookDenied(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

// ===== Built-in Hooks =====

/// Encryption hook — encrypts content before storage, decrypts after load
pub struct EncryptionHook {
    /// XOR key for simple encryption (production should use AES)
    key: Vec<u8>,
}

impl EncryptionHook {
    pub fn new(key: Vec<u8>) -> Self {
        Self { key }
    }

    fn xor_transform(&self, data: &[u8]) -> Vec<u8> {
        data.iter()
            .enumerate()
            .map(|(i, &b)| b ^ self.key[i % self.key.len()])
            .collect()
    }

    fn to_hex(data: &[u8]) -> String {
        data.iter().map(|b| format!("{:02x}", b)).collect()
    }

    fn from_hex(hex: &str) -> Option<Vec<u8>> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
            .collect()
    }
}

#[async_trait]
impl MemoryHook for EncryptionHook {
    async fn before_save(&self, record: &mut MemoryRecord) -> Result<HookDecision, MemoryError> {
        let encrypted = self.xor_transform(record.content.as_bytes());
        record
            .metadata
            .insert("encrypted".into(), Self::to_hex(&encrypted));
        record.content = "[encrypted]".into();
        Ok(HookDecision::Modified)
    }

    async fn after_load(&self, records: &mut Vec<MemoryRecord>) -> Result<(), MemoryError> {
        for record in records.iter_mut() {
            if let Some(encrypted_hex) = record.metadata.get("encrypted")
                && let Some(encrypted) = Self::from_hex(encrypted_hex)
            {
                let decrypted = self.xor_transform(&encrypted);
                record.content = String::from_utf8_lossy(&decrypted).to_string();
                record.metadata.remove("encrypted");
            }
        }
        Ok(())
    }
}

/// Content filter hook — blocks memories with sensitive content
pub struct ContentFilterHook {
    /// Patterns to block
    blocked_patterns: Vec<String>,
}

impl ContentFilterHook {
    pub fn new(patterns: Vec<String>) -> Self {
        Self {
            blocked_patterns: patterns,
        }
    }
}

#[async_trait]
impl MemoryHook for ContentFilterHook {
    async fn before_save(&self, record: &mut MemoryRecord) -> Result<HookDecision, MemoryError> {
        for pattern in &self.blocked_patterns {
            if record.content.to_lowercase().contains(&pattern.to_lowercase()) {
                return Ok(HookDecision::Deny {
                    reason: format!("Content contains blocked pattern: {}", pattern),
                });
            }
        }
        Ok(HookDecision::Allow)
    }
}

/// Size limit hook — truncates oversized content
pub struct SizeLimitHook {
    max_content_length: usize,
}

impl SizeLimitHook {
    pub fn new(max_length: usize) -> Self {
        Self {
            max_content_length: max_length,
        }
    }
}

#[async_trait]
impl MemoryHook for SizeLimitHook {
    async fn before_save(&self, record: &mut MemoryRecord) -> Result<HookDecision, MemoryError> {
        if record.content.len() > self.max_content_length {
            record.content = format!(
                "{}...[truncated]",
                &record.content[..self.max_content_length]
            );
            return Ok(HookDecision::Modified);
        }
        Ok(HookDecision::Allow)
    }
}

/// In-memory provider (for testing and simple use cases)
pub struct InMemoryProvider {
    records: tokio::sync::RwLock<HashMap<String, MemoryRecord>>,
}

impl InMemoryProvider {
    pub fn new() -> Self {
        Self {
            records: tokio::sync::RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MemoryProvider for InMemoryProvider {
    async fn store(&self, record: &MemoryRecord) -> Result<String, MemoryError> {
        let id = record.id.clone();
        let mut records = self.records.write().await;
        records.insert(id.clone(), record.clone());
        Ok(id)
    }

    async fn retrieve(&self, query: &MemoryQuery) -> Result<Vec<MemoryRecord>, MemoryError> {
        let records = self.records.read().await;
        let mut results: Vec<MemoryRecord> = records
            .values()
            .filter(|r| {
                if let Some(ref scope) = query.scope
                    && r.scope != *scope
                {
                    return false;
                }
                if let Some(ref mt) = query.memory_type
                    && r.memory_type != *mt
                {
                    return false;
                }
                if !query.tags.is_empty()
                    && !query.tags.iter().any(|t| r.tags.contains(t))
                {
                    return false;
                }
                if let Some(ref text) = query.text
                    && !r.content.to_lowercase().contains(&text.to_lowercase())
                {
                    return false;
                }
                true
            })
            .cloned()
            .collect();

        results.truncate(query.limit);
        Ok(results)
    }

    async fn delete(&self, id: &str) -> Result<bool, MemoryError> {
        let mut records = self.records.write().await;
        Ok(records.remove(id).is_some())
    }

    async fn update(&self, record: &MemoryRecord) -> Result<(), MemoryError> {
        let mut records = self.records.write().await;
        if records.contains_key(&record.id) {
            records.insert(record.id.clone(), record.clone());
            Ok(())
        } else {
            Err(MemoryError::NotFound(record.id.clone()))
        }
    }

    async fn cleanup(&self) -> Result<u64, MemoryError> {
        Ok(0) // In-memory doesn't expire
    }

    async fn stats(&self) -> Result<MemoryStats, MemoryError> {
        let records = self.records.read().await;
        let mut by_type = HashMap::new();
        let mut by_scope = HashMap::new();
        let mut total_size = 0u64;

        for r in records.values() {
            *by_type.entry(r.memory_type.clone()).or_insert(0u64) += 1;
            *by_scope.entry(r.scope.clone()).or_insert(0u64) += 1;
            total_size += r.content.len() as u64;
        }

        Ok(MemoryStats {
            total_entries: records.len() as u64,
            by_type,
            by_scope,
            total_size_bytes: total_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_record(id: &str, content: &str) -> MemoryRecord {
        MemoryRecord {
            id: id.into(),
            content: content.into(),
            memory_type: "working".into(),
            scope: "global".into(),
            tags: vec![],
            created_at: 1000,
            updated_at: 1000,
            access_count: 0,
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_in_memory_store_retrieve() {
        let provider = InMemoryProvider::new();
        let record = test_record("1", "hello world");
        provider.store(&record).await.unwrap();

        let query = MemoryQuery {
            text: Some("hello".into()),
            ..Default::default()
        };
        let results = provider.retrieve(&query).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "hello world");
    }

    #[tokio::test]
    async fn test_in_memory_delete() {
        let provider = InMemoryProvider::new();
        provider.store(&test_record("1", "hello")).await.unwrap();
        assert!(provider.delete("1").await.unwrap());
        assert!(!provider.delete("1").await.unwrap());
    }

    #[tokio::test]
    async fn test_manager_with_hooks() {
        let provider = Arc::new(InMemoryProvider::new());
        let manager = MemoryManagerV2::new(provider)
            .with_hook(Arc::new(SizeLimitHook::new(50)));

        let record = test_record("1", "short");
        manager.remember(record).await.unwrap();

        let long_record = test_record("2", &"x".repeat(100));
        manager.remember(long_record).await.unwrap();

        let records = manager
            .retrieve(MemoryQuery {
                text: Some("x".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].content.contains("truncated"));
    }

    #[tokio::test]
    async fn test_content_filter() {
        let provider = Arc::new(InMemoryProvider::new());
        let manager = MemoryManagerV2::new(provider).with_hook(Arc::new(
            ContentFilterHook::new(vec!["password".into(), "secret".into()]),
        ));

        let record = test_record("1", "my password is 123");
        let result = manager.remember(record).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_size_limit_hook() {
        let hook = SizeLimitHook::new(10);
        let mut record = test_record("1", "this is a very long content");
        hook.before_save(&mut record).await.unwrap();
        assert!(record.content.contains("truncated"));
        assert!(record.content.len() <= 25); // "this is a "...[truncated]"
    }

    #[tokio::test]
    async fn test_stats() {
        let provider = InMemoryProvider::new();
        provider.store(&test_record("1", "hello")).await.unwrap();
        provider.store(&test_record("2", "world")).await.unwrap();

        let stats = provider.stats().await.unwrap();
        assert_eq!(stats.total_entries, 2);
    }

    #[tokio::test]
    async fn test_retrieve_by_scope() {
        let provider = InMemoryProvider::new();
        let mut r1 = test_record("1", "global mem");
        r1.scope = "global".into();
        let mut r2 = test_record("2", "user mem");
        r2.scope = "user:alice".into();
        provider.store(&r1).await.unwrap();
        provider.store(&r2).await.unwrap();

        let results = provider
            .retrieve(&MemoryQuery {
                scope: Some("global".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].scope, "global");
    }
}
