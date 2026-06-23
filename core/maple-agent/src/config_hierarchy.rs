use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Config Hierarchy — user/project/local 三级配置合并
///
/// Inspired by claw-code's config hierarchy system.
/// Configuration is loaded from three levels with increasing priority:
///
/// 1. User config: ~/.mapleos/config.yaml (global defaults)
/// 2. Project config: .mapleos/config.yaml (project-specific)
/// 3. Local config: .mapleos/local.yaml (local overrides, gitignored)
///
/// Each level can override values from the previous level.
/// Deep merge is used for nested objects.
///
///   Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MapleConfig {
    /// LLM configuration
    #[serde(default)]
    pub llm: LlmConfig,

    /// Agent configuration
    #[serde(default)]
    pub agent: AgentConfig,

    /// Tool configuration
    #[serde(default)]
    pub tools: ToolConfig,

    /// Workflow configuration
    #[serde(default)]
    pub workflow: WorkflowConfig,

    /// Security configuration
    #[serde(default)]
    pub security: SecurityConfig,

    /// Custom key-value pairs
    #[serde(default)]
    pub custom: HashMap<String, serde_json::Value>,
}

/// LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Default provider
    #[serde(default = "default_provider")]
    pub default_provider: String,

    /// Default model
    #[serde(default = "default_model")]
    pub default_model: String,

    /// Temperature
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Max tokens
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// API keys (env var references)
    #[serde(default)]
    pub api_keys: HashMap<String, String>,

    /// Provider-specific config
    #[serde(default)]
    pub providers: HashMap<String, serde_json::Value>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            default_provider: default_provider(),
            default_model: default_model(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            api_keys: HashMap::new(),
            providers: HashMap::new(),
        }
    }
}

fn default_provider() -> String {
    "openai".to_string()
}

fn default_model() -> String {
    "gpt-4o".to_string()
}

fn default_temperature() -> f32 {
    0.7
}

fn default_max_tokens() -> u32 {
    4096
}

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent name
    #[serde(default = "default_agent_name")]
    pub name: String,

    /// System prompt
    #[serde(default)]
    pub system_prompt: Option<String>,

    /// Max iterations
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,

    /// Enable memory
    #[serde(default = "default_true")]
    pub enable_memory: bool,

    /// Enable tools
    #[serde(default = "default_true")]
    pub enable_tools: bool,

    /// Custom agent settings
    #[serde(default)]
    pub settings: HashMap<String, serde_json::Value>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: default_agent_name(),
            system_prompt: None,
            max_iterations: default_max_iterations(),
            enable_memory: default_true(),
            enable_tools: default_true(),
            settings: HashMap::new(),
        }
    }
}

fn default_agent_name() -> String {
    "MapleOS Agent".to_string()
}

fn default_max_iterations() -> u32 {
    10
}

fn default_true() -> bool {
    true
}

/// Tool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    /// Enabled tools
    #[serde(default)]
    pub enabled: Vec<String>,

    /// Disabled tools
    #[serde(default)]
    pub disabled: Vec<String>,

    /// Tool-specific config
    #[serde(default)]
    pub settings: HashMap<String, serde_json::Value>,

    /// Default timeout in seconds
    #[serde(default = "default_tool_timeout")]
    pub timeout_secs: u64,
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self {
            enabled: Vec::new(),
            disabled: Vec::new(),
            settings: HashMap::new(),
            timeout_secs: default_tool_timeout(),
        }
    }
}

fn default_tool_timeout() -> u64 {
    30
}

/// Workflow configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    /// Max concurrent workflows
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,

    /// Default retry count
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,

    /// Workflow-specific config
    #[serde(default)]
    pub settings: HashMap<String, serde_json::Value>,
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
            retry_count: default_retry_count(),
            settings: HashMap::new(),
        }
    }
}

fn default_max_concurrent() -> usize {
    5
}

fn default_retry_count() -> u32 {
    3
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable audit logging
    #[serde(default = "default_true")]
    pub audit_logging: bool,

    /// Allowed tools
    #[serde(default)]
    pub allowed_tools: Vec<String>,

    /// Blocked tools
    #[serde(default)]
    pub blocked_tools: Vec<String>,

    /// Max file size in bytes
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,

    /// Security-specific config
    #[serde(default)]
    pub settings: HashMap<String, serde_json::Value>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            audit_logging: default_true(),
            allowed_tools: Vec::new(),
            blocked_tools: Vec::new(),
            max_file_size: default_max_file_size(),
            settings: HashMap::new(),
        }
    }
}

fn default_max_file_size() -> u64 {
    10 * 1024 * 1024 // 10MB
}

/// Config source information
#[derive(Debug, Clone)]
pub struct ConfigSource {
    pub path: PathBuf,
    pub level: ConfigLevel,
    pub exists: bool,
}

/// Configuration level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigLevel {
    /// User-level config (~/.mapleos/config.yaml)
    User,
    /// Project-level config (.mapleos/config.yaml)
    Project,
    /// Local config (.mapleos/local.yaml)
    Local,
}

impl ConfigLevel {
    pub fn name(&self) -> &str {
        match self {
            ConfigLevel::User => "user",
            ConfigLevel::Project => "project",
            ConfigLevel::Local => "local",
        }
    }

    pub fn priority(&self) -> u8 {
        match self {
            ConfigLevel::User => 1,
            ConfigLevel::Project => 2,
            ConfigLevel::Local => 3,
        }
    }
}

/// Config hierarchy manager
pub struct ConfigHierarchy {
    /// User config directory
    user_dir: PathBuf,
    /// Project config directory
    project_dir: PathBuf,
    /// Merged configuration
    config: MapleConfig,
    /// Sources that were loaded
    sources: Vec<ConfigSource>,
}

impl ConfigHierarchy {
    /// Create a new config hierarchy
    pub fn new(project_dir: &Path) -> Self {
        let user_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".mapleos");

        Self {
            user_dir,
            project_dir: project_dir.join(".mapleos"),
            config: MapleConfig::default(),
            sources: Vec::new(),
        }
    }

    /// Create with custom directories (for testing)
    pub fn with_dirs(user_dir: &Path, project_dir: &Path) -> Self {
        Self {
            user_dir: user_dir.join(".mapleos"),
            project_dir: project_dir.join(".mapleos"),
            config: MapleConfig::default(),
            sources: Vec::new(),
        }
    }

    /// Load and merge all config levels
    pub fn load(&mut self) -> Result<&MapleConfig> {
        self.sources.clear();

        let mut merged = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());

        // Load user config
        let user_path = self.user_dir.join("config.yaml");
        let user_exists = user_path.exists();
        self.sources.push(ConfigSource {
            path: user_path.clone(),
            level: ConfigLevel::User,
            exists: user_exists,
        });
        if user_exists {
            let content = std::fs::read_to_string(&user_path)?;
            let value: serde_yaml::Value = serde_yaml::from_str(&content)?;
            deep_merge(&mut merged, &value);
        }

        // Load project config
        let project_path = self.project_dir.join("config.yaml");
        let project_exists = project_path.exists();
        self.sources.push(ConfigSource {
            path: project_path.clone(),
            level: ConfigLevel::Project,
            exists: project_exists,
        });
        if project_exists {
            let content = std::fs::read_to_string(&project_path)?;
            let value: serde_yaml::Value = serde_yaml::from_str(&content)?;
            deep_merge(&mut merged, &value);
        }

        // Load local config
        let local_path = self.project_dir.join("local.yaml");
        let local_exists = local_path.exists();
        self.sources.push(ConfigSource {
            path: local_path.clone(),
            level: ConfigLevel::Local,
            exists: local_exists,
        });
        if local_exists {
            let content = std::fs::read_to_string(&local_path)?;
            let value: serde_yaml::Value = serde_yaml::from_str(&content)?;
            deep_merge(&mut merged, &value);
        }

        self.config = serde_yaml::from_value(merged)?;
        Ok(&self.config)
    }

    /// Get the merged configuration
    pub fn config(&self) -> &MapleConfig {
        &self.config
    }

    /// Get the sources that were loaded
    pub fn sources(&self) -> &[ConfigSource] {
        &self.sources
    }

    /// Get a specific value by path (e.g., "llm.default_model")
    pub fn get(&self, path: &str) -> Option<serde_json::Value> {
        let parts: Vec<&str> = path.split('.').collect();
        match parts.as_slice() {
            ["llm", key] => match *key {
                "default_provider" => Some(serde_json::Value::String(self.config.llm.default_provider.clone())),
                "default_model" => Some(serde_json::Value::String(self.config.llm.default_model.clone())),
                "temperature" => Some(serde_json::json!(self.config.llm.temperature)),
                "max_tokens" => Some(serde_json::json!(self.config.llm.max_tokens)),
                _ => self.config.llm.providers.get(*key).cloned(),
            },
            ["agent", key] => match *key {
                "name" => Some(serde_json::Value::String(self.config.agent.name.clone())),
                "max_iterations" => Some(serde_json::json!(self.config.agent.max_iterations)),
                "enable_memory" => Some(serde_json::json!(self.config.agent.enable_memory)),
                "enable_tools" => Some(serde_json::json!(self.config.agent.enable_tools)),
                _ => self.config.agent.settings.get(*key).cloned(),
            },
            ["tools", key] => match *key {
                "timeout_secs" => Some(serde_json::json!(self.config.tools.timeout_secs)),
                _ => self.config.tools.settings.get(*key).cloned(),
            },
            ["workflow", key] => match *key {
                "max_concurrent" => Some(serde_json::json!(self.config.workflow.max_concurrent)),
                "retry_count" => Some(serde_json::json!(self.config.workflow.retry_count)),
                _ => self.config.workflow.settings.get(*key).cloned(),
            },
            ["security", key] => match *key {
                "audit_logging" => Some(serde_json::json!(self.config.security.audit_logging)),
                "max_file_size" => Some(serde_json::json!(self.config.security.max_file_size)),
                _ => self.config.security.settings.get(*key).cloned(),
            },
            ["custom", key] => self.config.custom.get(*key).cloned(),
            _ => None,
        }
    }

    /// Save the current config to a specific level
    pub fn save(&self, level: ConfigLevel) -> Result<()> {
        let path = match level {
            ConfigLevel::User => self.user_dir.join("config.yaml"),
            ConfigLevel::Project => self.project_dir.join("config.yaml"),
            ConfigLevel::Local => self.project_dir.join("local.yaml"),
        };

        // Create directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_yaml::to_string(&self.config)?;
        std::fs::write(&path, content)?;

        Ok(())
    }

    /// Get the path for a specific config level
    pub fn path_for_level(&self, level: ConfigLevel) -> PathBuf {
        match level {
            ConfigLevel::User => self.user_dir.join("config.yaml"),
            ConfigLevel::Project => self.project_dir.join("config.yaml"),
            ConfigLevel::Local => self.project_dir.join("local.yaml"),
        }
    }

    /// Check if a config file exists for a specific level
    pub fn exists(&self, level: ConfigLevel) -> bool {
        self.path_for_level(level).exists()
    }

    /// Get a summary of the config hierarchy
    pub fn summary(&self) -> ConfigSummary {
        ConfigSummary {
            sources: self.sources.clone(),
            merged_fields: self.count_fields(),
        }
    }

    fn count_fields(&self) -> usize {
        let mut count = 0;
        count += if self.config.llm.default_provider != default_provider() { 1 } else { 0 };
        count += if self.config.llm.default_model != default_model() { 1 } else { 0 };
        count += if (self.config.llm.temperature - default_temperature()).abs() > f32::EPSILON { 1 } else { 0 };
        count += if self.config.llm.max_tokens != default_max_tokens() { 1 } else { 0 };
        count += self.config.llm.api_keys.len();
        count += self.config.llm.providers.len();
        count += if self.config.agent.name != default_agent_name() { 1 } else { 0 };
        count += if self.config.agent.system_prompt.is_some() { 1 } else { 0 };
        count += self.config.agent.settings.len();
        count += self.config.tools.settings.len();
        count += self.config.workflow.settings.len();
        count += self.config.security.settings.len();
        count += self.config.custom.len();
        count
    }
}

/// Config hierarchy summary
#[derive(Debug)]
pub struct ConfigSummary {
    pub sources: Vec<ConfigSource>,
    pub merged_fields: usize,
}

/// Deep merge two YAML values. `higher` takes precedence over `lower`.
/// For mappings, keys are merged recursively. For all other types, `higher` wins.
fn deep_merge(lower: &mut serde_yaml::Value, higher: &serde_yaml::Value) {
    match (lower, higher) {
        (serde_yaml::Value::Mapping(lower_map), serde_yaml::Value::Mapping(higher_map)) => {
            for (key, higher_val) in higher_map {
                if let Some(lower_val) = lower_map.get_mut(key) {
                    deep_merge(lower_val, higher_val);
                } else {
                    lower_map.insert(key.clone(), higher_val.clone());
                }
            }
        }
        (lower, higher) => {
            *lower = higher.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_dirs() -> (TempDir, TempDir) {
        let user_dir = TempDir::new().unwrap();
        let project_dir = TempDir::new().unwrap();
        (user_dir, project_dir)
    }

    #[test]
    fn test_default_config() {
        let config = MapleConfig::default();
        assert_eq!(config.llm.default_provider, "openai");
        assert_eq!(config.llm.default_model, "gpt-4o");
        assert_eq!(config.llm.temperature, 0.7);
        assert_eq!(config.agent.name, "MapleOS Agent");
        assert_eq!(config.tools.timeout_secs, 30);
    }

    #[test]
    fn test_config_hierarchy_no_files() {
        let (user_dir, project_dir) = setup_dirs();
        let mut hierarchy = ConfigHierarchy::with_dirs(user_dir.path(), project_dir.path());
        let config = hierarchy.load().unwrap();

        // Should have defaults
        assert_eq!(config.llm.default_provider, "openai");
        assert_eq!(config.llm.default_model, "gpt-4o");
    }

    #[test]
    fn test_config_hierarchy_user_config() {
        let (user_dir, project_dir) = setup_dirs();

        // Create user config
        let user_config = r#"
llm:
  default_provider: anthropic
  default_model: claude-3-5-sonnet
  temperature: 0.5
"#;
        fs::create_dir_all(user_dir.path().join(".mapleos")).unwrap();
        fs::write(user_dir.path().join(".mapleos/config.yaml"), user_config).unwrap();

        let mut hierarchy = ConfigHierarchy::with_dirs(user_dir.path(), project_dir.path());
        let config = hierarchy.load().unwrap();

        assert_eq!(config.llm.default_provider, "anthropic");
        assert_eq!(config.llm.default_model, "claude-3-5-sonnet");
        assert_eq!(config.llm.temperature, 0.5);
    }

    #[test]
    fn test_config_hierarchy_project_overrides_user() {
        let (user_dir, project_dir) = setup_dirs();

        // Create user config
        let user_config = r#"
llm:
  default_provider: anthropic
  default_model: claude-3-5-sonnet
  temperature: 0.5
"#;
        fs::create_dir_all(user_dir.path().join(".mapleos")).unwrap();
        fs::write(user_dir.path().join(".mapleos/config.yaml"), user_config).unwrap();

        // Create project config
        let project_config = r#"
llm:
  default_model: deepseek-chat
  temperature: 0.3
"#;
        fs::create_dir_all(project_dir.path().join(".mapleos")).unwrap();
        fs::write(project_dir.path().join(".mapleos/config.yaml"), project_config).unwrap();

        let mut hierarchy = ConfigHierarchy::with_dirs(user_dir.path(), project_dir.path());
        let config = hierarchy.load().unwrap();

        // User provider, project model and temperature
        assert_eq!(config.llm.default_provider, "anthropic");
        assert_eq!(config.llm.default_model, "deepseek-chat");
        assert_eq!(config.llm.temperature, 0.3);
    }

    #[test]
    fn test_config_hierarchy_local_overrides_all() {
        let (user_dir, project_dir) = setup_dirs();

        // Create user config
        let user_config = r#"
llm:
  default_provider: anthropic
  temperature: 0.5
"#;
        fs::create_dir_all(user_dir.path().join(".mapleos")).unwrap();
        fs::write(user_dir.path().join(".mapleos/config.yaml"), user_config).unwrap();

        // Create project config
        let project_config = r#"
llm:
  default_model: deepseek-chat
"#;
        fs::create_dir_all(project_dir.path().join(".mapleos")).unwrap();
        fs::write(project_dir.path().join(".mapleos/config.yaml"), project_config).unwrap();

        // Create local config
        let local_config = r#"
llm:
  default_provider: ollama
  temperature: 0.1
"#;
        fs::write(project_dir.path().join(".mapleos/local.yaml"), local_config).unwrap();

        let mut hierarchy = ConfigHierarchy::with_dirs(user_dir.path(), project_dir.path());
        let config = hierarchy.load().unwrap();

        // Local overrides user and project
        assert_eq!(config.llm.default_provider, "ollama");
        assert_eq!(config.llm.default_model, "deepseek-chat");
        assert_eq!(config.llm.temperature, 0.1);
    }

    #[test]
    fn test_config_hierarchy_custom_fields() {
        let (user_dir, project_dir) = setup_dirs();

        let user_config = r#"
custom:
  my_key: my_value
  another_key: 42
"#;
        fs::create_dir_all(user_dir.path().join(".mapleos")).unwrap();
        fs::write(user_dir.path().join(".mapleos/config.yaml"), user_config).unwrap();

        let mut hierarchy = ConfigHierarchy::with_dirs(user_dir.path(), project_dir.path());
        let config = hierarchy.load().unwrap();

        assert_eq!(config.custom.get("my_key").unwrap(), "my_value");
        assert_eq!(config.custom.get("another_key").unwrap(), 42);
    }

    #[test]
    fn test_config_get_by_path() {
        let (user_dir, project_dir) = setup_dirs();

        let user_config = r#"
llm:
  default_provider: anthropic
  temperature: 0.5
agent:
  name: My Agent
"#;
        fs::create_dir_all(user_dir.path().join(".mapleos")).unwrap();
        fs::write(user_dir.path().join(".mapleos/config.yaml"), user_config).unwrap();

        let mut hierarchy = ConfigHierarchy::with_dirs(user_dir.path(), project_dir.path());
        hierarchy.load().unwrap();

        assert_eq!(
            hierarchy.get("llm.default_provider").unwrap(),
            serde_json::Value::String("anthropic".to_string())
        );
        assert_eq!(
            hierarchy.get("agent.name").unwrap(),
            serde_json::Value::String("My Agent".to_string())
        );
        assert!(hierarchy.get("nonexistent.path").is_none());
    }

    #[test]
    fn test_config_sources() {
        let (user_dir, project_dir) = setup_dirs();

        let user_config = r#"
llm:
  default_provider: anthropic
"#;
        fs::create_dir_all(user_dir.path().join(".mapleos")).unwrap();
        fs::write(user_dir.path().join(".mapleos/config.yaml"), user_config).unwrap();

        let mut hierarchy = ConfigHierarchy::with_dirs(user_dir.path(), project_dir.path());
        hierarchy.load().unwrap();

        let sources = hierarchy.sources();
        assert_eq!(sources.len(), 3);
        assert!(sources[0].exists);  // user
        assert!(!sources[1].exists); // project
        assert!(!sources[2].exists); // local
    }

    #[test]
    fn test_config_level_priority() {
        assert!(ConfigLevel::Local.priority() > ConfigLevel::Project.priority());
        assert!(ConfigLevel::Project.priority() > ConfigLevel::User.priority());
    }

    #[test]
    fn test_config_summary() {
        let (user_dir, project_dir) = setup_dirs();

        let user_config = r#"
llm:
  default_provider: anthropic
  temperature: 0.5
custom:
  key: value
"#;
        fs::create_dir_all(user_dir.path().join(".mapleos")).unwrap();
        fs::write(user_dir.path().join(".mapleos/config.yaml"), user_config).unwrap();

        let mut hierarchy = ConfigHierarchy::with_dirs(user_dir.path(), project_dir.path());
        hierarchy.load().unwrap();

        let summary = hierarchy.summary();
        assert_eq!(summary.sources.len(), 3);
        assert!(summary.merged_fields > 0);
    }
}
