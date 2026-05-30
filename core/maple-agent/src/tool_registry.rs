use anyhow::Result;
use maple_llm::embedding::Embedder;
use maple_llm::request::ToolDefinition;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Tool entry with definition and pre-computed embedding
struct ToolEntry {
    definition: ToolDefinition,
    embedding: Vec<f32>,
    tags: Vec<String>,
    category: String,
    use_count: u64,
    last_used: i64,
}

/// ToolRegistry with embedding-based semantic search (RAG)
///
/// When a user query arrives, the registry embeds the query and finds
/// the most relevant tools using cosine similarity, returning only the
/// top-K tools to send to the LLM — reducing token usage and improving
/// tool selection accuracy.
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, ToolEntry>>,
    embedder: Arc<dyn Embedder>,
    default_top_k: usize,
}

impl ToolRegistry {
    pub fn new(embedder: Arc<dyn Embedder>) -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            embedder,
            default_top_k: 10,
        }
    }

    pub fn with_default_top_k(mut self, k: usize) -> Self {
        self.default_top_k = k;
        self
    }

    /// Register a tool with automatic embedding of its description
    pub async fn register(&self, definition: ToolDefinition) -> Result<()> {
        self.register_with_category(definition, "general").await
    }

    /// Register a tool with a category tag
    pub async fn register_with_category(&self, definition: ToolDefinition, category: &str) -> Result<()> {
        let embed_text = format!(
            "{} {} {}",
            definition.name,
            definition.description,
            self.extract_schema_keywords(&definition.parameters)
        );
        let embedding = self.embedder.embed(&embed_text).await?;

        let tags = self.extract_tags(&definition);

        let entry = ToolEntry {
            definition,
            embedding,
            tags,
            category: category.into(),
            use_count: 0,
            last_used: 0,
        };

        self.tools
            .write()
            .await
            .insert(entry.definition.name.clone(), entry);
        Ok(())
    }

    /// Register a tool with a pre-computed embedding (avoids API call)
    pub async fn register_with_embedding(&self, definition: ToolDefinition, embedding: Vec<f32>) {
        self.register_with_embedding_and_category(definition, embedding, "general").await;
    }

    /// Register a tool with pre-computed embedding and category
    pub async fn register_with_embedding_and_category(
        &self,
        definition: ToolDefinition,
        embedding: Vec<f32>,
        category: &str,
    ) {
        let tags = self.extract_tags(&definition);
        let entry = ToolEntry {
            definition,
            embedding,
            tags,
            category: category.into(),
            use_count: 0,
            last_used: 0,
        };
        self.tools
            .write()
            .await
            .insert(entry.definition.name.clone(), entry);
    }

    /// Batch register multiple tools (parallel embedding)
    pub async fn register_batch(&self, definitions: Vec<ToolDefinition>) -> Result<usize> {
        let mut count = 0;
        for def in definitions {
            self.register(def).await?;
            count += 1;
        }
        Ok(count)
    }

    /// Remove a tool by name
    pub async fn unregister(&self, name: &str) -> bool {
        self.tools.write().await.remove(name).is_some()
    }

    /// Get a tool definition by exact name
    pub async fn get(&self, name: &str) -> Option<ToolDefinition> {
        self.tools
            .read()
            .await
            .get(name)
            .map(|e| e.definition.clone())
    }

    /// List all registered tool names
    pub async fn list_names(&self) -> Vec<String> {
        self.tools.read().await.keys().cloned().collect()
    }

    /// List all tool definitions
    pub async fn list_all(&self) -> Vec<ToolDefinition> {
        self.tools
            .read()
            .await
            .values()
            .map(|e| e.definition.clone())
            .collect()
    }

    /// Semantic search: find the top-K most relevant tools for a query
    pub async fn search(&self, query: &str, top_k: Option<usize>) -> Result<Vec<ToolDefinition>> {
        let k = top_k.unwrap_or(self.default_top_k);
        let query_embedding = self.embedder.embed(query).await?;

        let tools = self.tools.read().await;
        let mut scored: Vec<(&str, f32, &ToolDefinition)> = tools
            .values()
            .map(|entry| {
                let score = cosine_similarity(&query_embedding, &entry.embedding);
                (entry.definition.name.as_str(), score, &entry.definition)
            })
            .collect();

        // Boost score for tag matches
        let query_lower = query.to_lowercase();
        for (_, score, def) in &mut scored {
            if let Some(entry) = tools.get(def.name.as_str()) {
                for tag in &entry.tags {
                    if query_lower.contains(tag.as_str()) {
                        *score += 0.15;
                    }
                }
            }
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored
            .into_iter()
            .take(k)
            .map(|(_, _, def)| def.clone())
            .collect())
    }

    /// Search within a specific category
    pub async fn search_in_category(
        &self,
        query: &str,
        category: &str,
        top_k: Option<usize>,
    ) -> Result<Vec<ToolDefinition>> {
        let k = top_k.unwrap_or(self.default_top_k);
        let query_embedding = self.embedder.embed(query).await?;

        let tools = self.tools.read().await;
        let mut scored: Vec<(&str, f32, &ToolDefinition)> = tools
            .values()
            .filter(|e| e.category == category)
            .map(|entry| {
                let score = cosine_similarity(&query_embedding, &entry.embedding);
                (entry.definition.name.as_str(), score, &entry.definition)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored
            .into_iter()
            .take(k)
            .map(|(_, _, def)| def.clone())
            .collect())
    }

    /// Search with usage-based ranking boost
    pub async fn search_with_usage_boost(
        &self,
        query: &str,
        top_k: Option<usize>,
    ) -> Result<Vec<ToolDefinition>> {
        let k = top_k.unwrap_or(self.default_top_k);
        let query_embedding = self.embedder.embed(query).await?;

        let tools = self.tools.read().await;
        let max_count = tools.values().map(|e| e.use_count).max().unwrap_or(1);

        let mut scored: Vec<(&str, f32, &ToolDefinition)> = tools
            .values()
            .map(|entry| {
                let semantic_score = cosine_similarity(&query_embedding, &entry.embedding);
                // Normalize use_count to 0.0-0.2 boost
                let usage_boost = if max_count > 0 {
                    (entry.use_count as f32 / max_count as f32) * 0.2
                } else {
                    0.0
                };
                let score = semantic_score + usage_boost;
                (entry.definition.name.as_str(), score, &entry.definition)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored
            .into_iter()
            .take(k)
            .map(|(_, _, def)| def.clone())
            .collect())
    }

    /// Record tool usage (for ranking boost)
    pub async fn record_usage(&self, tool_name: &str) {
        let mut tools = self.tools.write().await;
        if let Some(entry) = tools.get_mut(tool_name) {
            entry.use_count += 1;
            entry.last_used = chrono::Utc::now().timestamp();
        }
    }

    /// Get tools by category
    pub async fn by_category(&self, category: &str) -> Vec<ToolDefinition> {
        self.tools
            .read()
            .await
            .values()
            .filter(|e| e.category == category)
            .map(|e| e.definition.clone())
            .collect()
    }

    /// Get all categories
    pub async fn categories(&self) -> Vec<String> {
        let mut cats: Vec<String> = self
            .tools
            .read()
            .await
            .values()
            .map(|e| e.category.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        cats.sort();
        cats
    }

    /// Get registry statistics
    pub async fn stats(&self) -> ToolRegistryStats {
        let tools = self.tools.read().await;
        let total = tools.len();
        let categories: usize = tools
            .values()
            .map(|e| &e.category)
            .collect::<std::collections::HashSet<_>>()
            .len();
        let total_uses: u64 = tools.values().map(|e| e.use_count).sum();
        let avg_embedding_dim = if total > 0 {
            tools.values().map(|e| e.embedding.len()).sum::<usize>() / total
        } else {
            0
        };

        ToolRegistryStats {
            total_tools: total,
            categories,
            total_uses,
            avg_embedding_dim,
            default_top_k: self.default_top_k,
        }
    }

    /// Keyword-based tool search (no embedding required)
    ///
    /// Search tools by matching keywords against:
    /// - Tool name
    /// - Tool description
    /// - Tool tags
    /// - Schema property names
    ///
    /// Returns tools sorted by relevance score.
    pub async fn search_by_keyword(&self, query: &str, top_k: Option<usize>) -> Vec<ToolDefinition> {
        let k = top_k.unwrap_or(self.default_top_k);
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let tools = self.tools.read().await;
        let mut scored: Vec<(f32, &ToolDefinition)> = tools
            .values()
            .map(|entry| {
                let mut score = 0.0f32;

                // Match against name
                let name_lower = entry.definition.name.to_lowercase();
                if name_lower.contains(&query_lower) {
                    score += 2.0; // Exact name match is high priority
                }
                for word in &query_words {
                    if name_lower.contains(word) {
                        score += 0.5;
                    }
                }

                // Match against description
                let desc_lower = entry.definition.description.to_lowercase();
                if desc_lower.contains(&query_lower) {
                    score += 1.0;
                }
                for word in &query_words {
                    if desc_lower.contains(word) {
                        score += 0.3;
                    }
                }

                // Match against tags
                for tag in &entry.tags {
                    if query_words.iter().any(|w| tag.contains(w)) {
                        score += 0.4;
                    }
                }

                // Match against schema properties
                let schema_keywords = self.extract_schema_keywords(&entry.definition.parameters);
                let schema_lower = schema_keywords.to_lowercase();
                for word in &query_words {
                    if schema_lower.contains(word) {
                        score += 0.2;
                    }
                }

                (score, &entry.definition)
            })
            .filter(|(score, _)| *score > 0.0)
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored
            .into_iter()
            .take(k)
            .map(|(_, def)| def.clone())
            .collect()
    }

    /// Get all tools as ToolDefinitions (for cases where RAG isn't needed)
    pub async fn all_definitions(&self) -> Vec<ToolDefinition> {
        self.list_all().await
    }

    /// Number of registered tools
    pub async fn len(&self) -> usize {
        self.tools.read().await.len()
    }

    /// Check if registry is empty
    pub async fn is_empty(&self) -> bool {
        self.tools.read().await.is_empty()
    }

    fn extract_tags(&self, definition: &ToolDefinition) -> Vec<String> {
        let mut tags = Vec::new();
        // Extract keywords from name (split on _ and camelCase)
        for part in definition.name.split('_') {
            if !part.is_empty() {
                tags.push(part.to_lowercase());
            }
        }
        // Extract first few words of description
        for word in definition.description.split_whitespace().take(5) {
            let clean = word
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            if clean.len() > 2 {
                tags.push(clean);
            }
        }
        tags
    }

    fn extract_schema_keywords(&self, schema: &serde_json::Value) -> String {
        let mut keywords = Vec::new();
        if let Some(props) = schema.get("properties").and_then(|v| v.as_object()) {
            for (key, val) in props {
                keywords.push(key.clone());
                if let Some(desc) = val.get("description").and_then(|v| v.as_str()) {
                    keywords.push(desc.to_string());
                }
            }
        }
        keywords.join(" ")
    }
}

/// Statistics about the tool registry
#[derive(Debug, Clone)]
pub struct ToolRegistryStats {
    pub total_tools: usize,
    pub categories: usize,
    pub total_uses: u64,
    pub avg_embedding_dim: usize,
    pub default_top_k: usize,
}

/// Cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &b)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[tokio::test]
    async fn test_register_with_category() {
        use maple_llm::embedding::FallbackEmbedder;
        let embedder = Arc::new(FallbackEmbedder::new(64));
        let registry = ToolRegistry::new(embedder);

        let def = ToolDefinition {
            name: "read_file".into(),
            description: "Read a file from disk".into(),
            parameters: serde_json::json!({}),
        };

        registry.register_with_category(def, "filesystem").await.unwrap();

        let names = registry.list_names().await;
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], "read_file");

        let cats = registry.categories().await;
        assert_eq!(cats, vec!["filesystem"]);
    }

    #[tokio::test]
    async fn test_search_in_category() {
        use maple_llm::embedding::FallbackEmbedder;
        let embedder = Arc::new(FallbackEmbedder::new(64));
        let registry = ToolRegistry::new(embedder);

        let def1 = ToolDefinition {
            name: "read_file".into(),
            description: "Read a file".into(),
            parameters: serde_json::json!({}),
        };
        let def2 = ToolDefinition {
            name: "send_email".into(),
            description: "Send an email".into(),
            parameters: serde_json::json!({}),
        };

        registry.register_with_category(def1, "filesystem").await.unwrap();
        registry.register_with_category(def2, "email").await.unwrap();

        let results = registry.search_in_category("read file", "filesystem", Some(10)).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "read_file");
    }

    #[tokio::test]
    async fn test_search_with_usage_boost() {
        use maple_llm::embedding::FallbackEmbedder;
        let embedder = Arc::new(FallbackEmbedder::new(64));
        let registry = ToolRegistry::new(embedder);

        let def1 = ToolDefinition {
            name: "tool_a".into(),
            description: "Tool A for testing".into(),
            parameters: serde_json::json!({}),
        };
        let def2 = ToolDefinition {
            name: "tool_b".into(),
            description: "Tool B for testing".into(),
            parameters: serde_json::json!({}),
        };

        registry.register(def1).await.unwrap();
        registry.register(def2).await.unwrap();

        // Record usage for tool_a
        registry.record_usage("tool_a").await;
        registry.record_usage("tool_a").await;
        registry.record_usage("tool_a").await;

        let results = registry.search_with_usage_boost("testing", Some(10)).await.unwrap();
        assert_eq!(results.len(), 2);
        // tool_a should be ranked higher due to usage boost
        assert_eq!(results[0].name, "tool_a");
    }

    #[tokio::test]
    async fn test_record_usage() {
        use maple_llm::embedding::FallbackEmbedder;
        let embedder = Arc::new(FallbackEmbedder::new(64));
        let registry = ToolRegistry::new(embedder);

        let def = ToolDefinition {
            name: "my_tool".into(),
            description: "A tool".into(),
            parameters: serde_json::json!({}),
        };

        registry.register(def).await.unwrap();
        registry.record_usage("my_tool").await;
        registry.record_usage("my_tool").await;

        let stats = registry.stats().await;
        assert_eq!(stats.total_uses, 2);
    }

    #[tokio::test]
    async fn test_stats() {
        use maple_llm::embedding::FallbackEmbedder;
        let embedder = Arc::new(FallbackEmbedder::new(64));
        let registry = ToolRegistry::new(embedder).with_default_top_k(5);

        let def1 = ToolDefinition {
            name: "tool_a".into(),
            description: "Tool A".into(),
            parameters: serde_json::json!({}),
        };
        let def2 = ToolDefinition {
            name: "tool_b".into(),
            description: "Tool B".into(),
            parameters: serde_json::json!({}),
        };

        registry.register_with_category(def1, "cat1").await.unwrap();
        registry.register_with_category(def2, "cat2").await.unwrap();
        registry.record_usage("tool_a").await;

        let stats = registry.stats().await;
        assert_eq!(stats.total_tools, 2);
        assert_eq!(stats.categories, 2);
        assert_eq!(stats.total_uses, 1);
        assert_eq!(stats.default_top_k, 5);
        assert!(stats.avg_embedding_dim > 0);
    }

    #[tokio::test]
    async fn test_by_category() {
        use maple_llm::embedding::FallbackEmbedder;
        let embedder = Arc::new(FallbackEmbedder::new(64));
        let registry = ToolRegistry::new(embedder);

        let def1 = ToolDefinition {
            name: "read_file".into(),
            description: "Read a file".into(),
            parameters: serde_json::json!({}),
        };
        let def2 = ToolDefinition {
            name: "write_file".into(),
            description: "Write a file".into(),
            parameters: serde_json::json!({}),
        };
        let def3 = ToolDefinition {
            name: "send_email".into(),
            description: "Send email".into(),
            parameters: serde_json::json!({}),
        };

        registry.register_with_category(def1, "filesystem").await.unwrap();
        registry.register_with_category(def2, "filesystem").await.unwrap();
        registry.register_with_category(def3, "email").await.unwrap();

        let fs_tools = registry.by_category("filesystem").await;
        assert_eq!(fs_tools.len(), 2);

        let email_tools = registry.by_category("email").await;
        assert_eq!(email_tools.len(), 1);
        assert_eq!(email_tools[0].name, "send_email");
    }

    #[tokio::test]
    async fn test_categories() {
        use maple_llm::embedding::FallbackEmbedder;
        let embedder = Arc::new(FallbackEmbedder::new(64));
        let registry = ToolRegistry::new(embedder);

        let def1 = ToolDefinition {
            name: "tool_a".into(),
            description: "Tool A".into(),
            parameters: serde_json::json!({}),
        };
        let def2 = ToolDefinition {
            name: "tool_b".into(),
            description: "Tool B".into(),
            parameters: serde_json::json!({}),
        };

        registry.register_with_category(def1, "beta").await.unwrap();
        registry.register_with_category(def2, "alpha").await.unwrap();

        let cats = registry.categories().await;
        assert_eq!(cats, vec!["alpha", "beta"]); // sorted
    }

    #[tokio::test]
    async fn test_search_by_keyword() {
        use maple_llm::embedding::FallbackEmbedder;
        let embedder = Arc::new(FallbackEmbedder::new(64));
        let registry = ToolRegistry::new(embedder);

        let def1 = ToolDefinition {
            name: "read_file".into(),
            description: "Read a file from disk".into(),
            parameters: serde_json::json!({"properties": {"path": {"description": "File path"}}}),
        };
        let def2 = ToolDefinition {
            name: "write_file".into(),
            description: "Write content to a file".into(),
            parameters: serde_json::json!({"properties": {"path": {"description": "File path"}, "content": {"description": "File content"}}}),
        };
        let def3 = ToolDefinition {
            name: "send_email".into(),
            description: "Send an email message".into(),
            parameters: serde_json::json!({"properties": {"to": {"description": "Recipient"}, "subject": {"description": "Email subject"}}}),
        };

        registry.register(def1).await.unwrap();
        registry.register(def2).await.unwrap();
        registry.register(def3).await.unwrap();

        // Search for "file" - should return read_file and write_file
        let results = registry.search_by_keyword("file", Some(10)).await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|t| t.name == "read_file"));
        assert!(results.iter().any(|t| t.name == "write_file"));

        // Search for "email" - should return send_email
        let results = registry.search_by_keyword("email", Some(10)).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "send_email");

        // Search for "read" - should return read_file
        let results = registry.search_by_keyword("read", Some(10)).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "read_file");

        // Search for "path" - should return read_file and write_file (from schema)
        let results = registry.search_by_keyword("path", Some(10)).await;
        assert!(results.len() >= 1, "Expected at least 1 result for 'path', got {}", results.len());
    }

    #[tokio::test]
    async fn test_search_by_keyword_with_limit() {
        use maple_llm::embedding::FallbackEmbedder;
        let embedder = Arc::new(FallbackEmbedder::new(64));
        let registry = ToolRegistry::new(embedder);

        for i in 0..10 {
            let def = ToolDefinition {
                name: format!("tool_{}", i),
                description: format!("Tool number {} for testing", i),
                parameters: serde_json::json!({}),
            };
            registry.register(def).await.unwrap();
        }

        // Search with limit
        let results = registry.search_by_keyword("tool", Some(5)).await;
        assert_eq!(results.len(), 5);

        // Search without limit (default_top_k = 10)
        let results = registry.search_by_keyword("tool", None).await;
        assert_eq!(results.len(), 10);
    }
}
