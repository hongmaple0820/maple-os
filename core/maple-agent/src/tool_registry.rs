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
        };

        self.tools
            .write()
            .await
            .insert(entry.definition.name.clone(), entry);
        Ok(())
    }

    /// Register a tool with a pre-computed embedding (avoids API call)
    pub async fn register_with_embedding(&self, definition: ToolDefinition, embedding: Vec<f32>) {
        let tags = self.extract_tags(&definition);
        let entry = ToolEntry {
            definition,
            embedding,
            tags,
        };
        self.tools
            .write()
            .await
            .insert(entry.definition.name.clone(), entry);
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
}
