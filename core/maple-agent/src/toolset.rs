use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Toolset — named groups of tools for composition
///
/// Inspired by hermes-agent's toolset system:
/// - Named tool groups (e.g., "file_ops", "web_tools", "code_exec")
/// - Composable: merge multiple toolsets
/// - Webhook-safe subsets: restrict which tools can be called via webhooks
/// - Role-based tool assignment

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Toolset {
    /// Unique toolset identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Description of this toolset
    pub description: String,
    /// Tool names in this toolset
    pub tools: HashSet<String>,
    /// Whether this toolset is webhook-safe (can be triggered externally)
    pub webhook_safe: bool,
    /// Tags for categorization
    pub tags: Vec<String>,
}

impl Toolset {
    /// Create a new toolset
    pub fn new(id: &str, name: &str, description: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            tools: HashSet::new(),
            webhook_safe: false,
            tags: Vec::new(),
        }
    }

    /// Add a tool to this toolset
    pub fn with_tool(mut self, tool_name: &str) -> Self {
        self.tools.insert(tool_name.to_string());
        self
    }

    /// Add multiple tools
    pub fn with_tools(mut self, tool_names: &[&str]) -> Self {
        for name in tool_names {
            self.tools.insert(name.to_string());
        }
        self
    }

    /// Mark as webhook-safe
    pub fn webhook_safe(mut self) -> Self {
        self.webhook_safe = true;
        self
    }

    /// Add tags
    pub fn with_tags(mut self, tags: &[&str]) -> Self {
        self.tags.extend(tags.iter().map(|t| t.to_string()));
        self
    }

    /// Check if a tool is in this toolset
    pub fn contains(&self, tool_name: &str) -> bool {
        self.tools.contains(tool_name)
    }

    /// Get the number of tools
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

/// Registry of toolsets with composition capabilities
#[derive(Debug, Clone, Default)]
pub struct ToolsetRegistry {
    toolsets: HashMap<String, Toolset>,
}

impl ToolsetRegistry {
    pub fn new() -> Self {
        Self {
            toolsets: HashMap::new(),
        }
    }

    /// Register a toolset
    pub fn register(&mut self, toolset: Toolset) {
        self.toolsets.insert(toolset.id.clone(), toolset);
    }

    /// Get a toolset by ID
    pub fn get(&self, id: &str) -> Option<&Toolset> {
        self.toolsets.get(id)
    }

    /// Get all toolsets
    pub fn all(&self) -> Vec<&Toolset> {
        self.toolsets.values().collect()
    }

    /// Get webhook-safe toolsets only
    pub fn webhook_safe(&self) -> Vec<&Toolset> {
        self.toolsets.values().filter(|t| t.webhook_safe).collect()
    }

    /// Compose multiple toolsets into a union of tools
    pub fn compose(&self, toolset_ids: &[&str]) -> HashSet<String> {
        let mut tools = HashSet::new();
        for id in toolset_ids {
            if let Some(toolset) = self.toolsets.get(*id) {
                tools.extend(toolset.tools.iter().cloned());
            }
        }
        tools
    }

    /// Compose with intersection (only tools present in ALL sets)
    pub fn compose_intersection(&self, toolset_ids: &[&str]) -> HashSet<String> {
        if toolset_ids.is_empty() {
            return HashSet::new();
        }

        let mut result: Option<HashSet<String>> = None;
        for id in toolset_ids {
            if let Some(toolset) = self.toolsets.get(*id) {
                result = Some(match result {
                    None => toolset.tools.clone(),
                    Some(existing) => existing.intersection(&toolset.tools).cloned().collect(),
                });
            }
        }
        result.unwrap_or_default()
    }

    /// Find toolsets containing a specific tool
    pub fn find_toolsets_with(&self, tool_name: &str) -> Vec<&Toolset> {
        self.toolsets
            .values()
            .filter(|t| t.tools.contains(tool_name))
            .collect()
    }

    /// Get all unique tools across all toolsets
    pub fn all_tools(&self) -> HashSet<String> {
        self.toolsets
            .values()
            .flat_map(|t| t.tools.iter().cloned())
            .collect()
    }

    /// Remove a toolset
    pub fn remove(&mut self, id: &str) -> Option<Toolset> {
        self.toolsets.remove(id)
    }
}

/// Builder for creating toolsets with a fluent API
pub struct ToolsetBuilder {
    registry: ToolsetRegistry,
}

impl ToolsetBuilder {
    pub fn new() -> Self {
        Self {
            registry: ToolsetRegistry::new(),
        }
    }

    pub fn add(mut self, toolset: Toolset) -> Self {
        self.registry.register(toolset);
        self
    }

    pub fn build(self) -> ToolsetRegistry {
        self.registry
    }
}

/// Predefined toolset categories
pub mod presets {
    use super::*;

    /// File operation tools
    pub fn file_ops() -> Toolset {
        Toolset::new("file_ops", "File Operations", "Read, write, and manage files")
            .with_tools(&["read_file", "write_file", "list_files", "delete_file", "move_file", "copy_file"])
            .with_tags(&["filesystem", "io"])
    }

    /// Web/network tools
    pub fn web_tools() -> Toolset {
        Toolset::new("web_tools", "Web Tools", "HTTP requests and web browsing")
            .with_tools(&["http_request", "web_search", "web_fetch", "browser_navigate"])
            .webhook_safe()
            .with_tags(&["network", "web"])
    }

    /// Code execution tools
    pub fn code_exec() -> Toolset {
        Toolset::new("code_exec", "Code Execution", "Execute code in sandboxed environment")
            .with_tools(&["execute_code", "run_script", "code_interpreter"])
            .with_tags(&["code", "execution"])
    }

    /// Search tools
    pub fn search() -> Toolset {
        Toolset::new("search", "Search", "Search code, files, and knowledge base")
            .with_tools(&["search_code", "search_files", "kb_search", "grep"])
            .webhook_safe()
            .with_tags(&["search", "query"])
    }

    /// Safe tools (webhook-safe by default)
    pub fn safe_tools() -> Toolset {
        Toolset::new("safe_tools", "Safe Tools", "Tools safe for external invocation")
            .with_tools(&["read_file", "list_files", "search_code", "search_files", "kb_search", "http_request", "web_search"])
            .webhook_safe()
            .with_tags(&["safe", "webhook"])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toolset_creation() {
        let toolset = Toolset::new("test", "Test", "Test toolset")
            .with_tool("tool_a")
            .with_tool("tool_b");

        assert_eq!(toolset.id, "test");
        assert_eq!(toolset.len(), 2);
        assert!(toolset.contains("tool_a"));
        assert!(!toolset.contains("tool_c"));
    }

    #[test]
    fn test_toolset_composition() {
        let mut registry = ToolsetRegistry::new();

        registry.register(
            Toolset::new("set1", "Set 1", "First set")
                .with_tools(&["a", "b", "c"]),
        );
        registry.register(
            Toolset::new("set2", "Set 2", "Second set")
                .with_tools(&["b", "c", "d"]),
        );

        // Union
        let union = registry.compose(&["set1", "set2"]);
        assert_eq!(union.len(), 4);
        assert!(union.contains("a"));
        assert!(union.contains("d"));

        // Intersection
        let intersection = registry.compose_intersection(&["set1", "set2"]);
        assert_eq!(intersection.len(), 2);
        assert!(intersection.contains("b"));
        assert!(intersection.contains("c"));
    }

    #[test]
    fn test_webhook_safe() {
        let mut registry = ToolsetRegistry::new();

        registry.register(
            Toolset::new("unsafe", "Unsafe", "Not webhook safe")
                .with_tool("delete_file"),
        );
        registry.register(
            Toolset::new("safe", "Safe", "Webhook safe")
                .with_tool("read_file")
                .webhook_safe(),
        );

        let safe = registry.webhook_safe();
        assert_eq!(safe.len(), 1);
        assert_eq!(safe[0].id, "safe");
    }

    #[test]
    fn test_find_toolsets_with() {
        let mut registry = ToolsetRegistry::new();

        registry.register(presets::file_ops());
        registry.register(presets::search());

        let toolsets = registry.find_toolsets_with("read_file");
        assert_eq!(toolsets.len(), 1);
        assert_eq!(toolsets[0].id, "file_ops");

        // "search_code" is in search toolset
        let toolsets = registry.find_toolsets_with("search_code");
        assert_eq!(toolsets.len(), 1);
        assert_eq!(toolsets[0].id, "search");
    }

    #[test]
    fn test_presets() {
        let file_ops = presets::file_ops();
        assert_eq!(file_ops.id, "file_ops");
        assert!(file_ops.contains("read_file"));
        assert!(!file_ops.webhook_safe);

        let web_tools = presets::web_tools();
        assert!(web_tools.webhook_safe);

        let safe = presets::safe_tools();
        assert!(safe.webhook_safe);
        assert!(safe.contains("read_file"));
    }

    #[test]
    fn test_all_tools() {
        let mut registry = ToolsetRegistry::new();
        registry.register(presets::file_ops());
        registry.register(presets::search());

        let all = registry.all_tools();
        assert!(all.contains("read_file"));
        assert!(all.contains("search_code"));
    }
}
