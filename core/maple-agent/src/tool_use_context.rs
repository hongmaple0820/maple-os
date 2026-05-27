use std::path::PathBuf;
use std::collections::HashMap;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

/// ToolUseContext — unified dependency injection for all tool executions
/// Inspired by cc-haha's ~30-field DI container
///
/// Provides:
/// - Session/workspace context
/// - Permission level
/// - Cancellation support
/// - Progress reporting
/// - Feature flags
/// - Environment variables

#[derive(Debug, Clone)]
pub struct ToolUseContext {
    // Session
    pub session_id: String,
    pub conversation_id: Option<String>,
    pub message_id: Option<String>,

    // Workspace
    pub workspace_root: PathBuf,
    pub cwd: PathBuf,
    pub allowed_paths: Vec<PathBuf>,

    // Permissions
    pub permission_level: PermissionLevel,

    // Execution
    pub max_execution_time: std::time::Duration,
    pub max_output_size: usize,

    // Feature flags
    pub feature_flags: FeatureFlags,

    // Environment
    pub environment: HashMap<String, String>,

    // UI feedback
    pub progress_enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PermissionLevel {
    /// Read-only tools (search, list, get)
    ReadOnly,
    /// Write tools (create, update, write)
    Write,
    /// Full access including dangerous operations
    FullAccess,
    /// Custom permission set
    Custom(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct FeatureFlags {
    pub enable_network: bool,
    pub enable_file_system: bool,
    pub enable_shell: bool,
    pub enable_browser: bool,
    pub enable_computer_use: bool,
    pub enable_delegation: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            enable_network: true,
            enable_file_system: true,
            enable_shell: false,        // Disabled by default for safety
            enable_browser: false,
            enable_computer_use: false,
            enable_delegation: true,
        }
    }
}

impl ToolUseContext {
    /// Create a new context with minimal required fields
    pub fn new(session_id: &str, workspace_root: PathBuf) -> Self {
        Self {
            session_id: session_id.to_string(),
            conversation_id: None,
            message_id: None,
            workspace_root: workspace_root.clone(),
            cwd: workspace_root,
            allowed_paths: Vec::new(),
            permission_level: PermissionLevel::Write,
            max_execution_time: std::time::Duration::from_secs(30),
            max_output_size: 1024 * 1024, // 1MB
            feature_flags: FeatureFlags::default(),
            environment: HashMap::new(),
            progress_enabled: true,
        }
    }

    /// Create a builder for fluent configuration
    pub fn builder(session_id: &str, workspace_root: PathBuf) -> ToolUseContextBuilder {
        ToolUseContextBuilder::new(session_id, workspace_root)
    }

    /// Check if a tool is allowed by current permission level
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        match &self.permission_level {
            PermissionLevel::ReadOnly => {
                let read_only_tools = ["read", "search", "list", "get", "query", "find", "grep"];
                read_only_tools.iter().any(|t| tool_name.contains(t))
            }
            PermissionLevel::Write => {
                let denied = ["delete", "rm", "drop", "destroy", "format", "mkfs"];
                !denied.iter().any(|t| tool_name.contains(t))
            }
            PermissionLevel::FullAccess => true,
            PermissionLevel::Custom(allowed) => allowed.contains(&tool_name.to_string()),
        }
    }

    /// Check if a path is within allowed boundaries
    pub fn is_path_allowed(&self, path: &PathBuf) -> bool {
        // Check if path is within workspace root
        if path.starts_with(&self.workspace_root) {
            return true;
        }

        // Check if path is in allowed paths list
        if self.allowed_paths.iter().any(|p| path.starts_with(p)) {
            return true;
        }

        false
    }

    /// Check if a feature is enabled
    pub fn is_feature_enabled(&self, feature: &str) -> bool {
        match feature {
            "network" => self.feature_flags.enable_network,
            "file_system" => self.feature_flags.enable_file_system,
            "shell" => self.feature_flags.enable_shell,
            "browser" => self.feature_flags.enable_browser,
            "computer_use" => self.feature_flags.enable_computer_use,
            "delegation" => self.feature_flags.enable_delegation,
            _ => false,
        }
    }

    /// Get an environment variable
    pub fn get_env(&self, key: &str) -> Option<&String> {
        self.environment.get(key)
    }
}

/// Builder for ToolUseContext
pub struct ToolUseContextBuilder {
    context: ToolUseContext,
}

impl ToolUseContextBuilder {
    pub fn new(session_id: &str, workspace_root: PathBuf) -> Self {
        Self {
            context: ToolUseContext::new(session_id, workspace_root),
        }
    }

    pub fn conversation_id(mut self, id: &str) -> Self {
        self.context.conversation_id = Some(id.to_string());
        self
    }

    pub fn message_id(mut self, id: &str) -> Self {
        self.context.message_id = Some(id.to_string());
        self
    }

    pub fn cwd(mut self, path: PathBuf) -> Self {
        self.context.cwd = path;
        self
    }

    pub fn permission_level(mut self, level: PermissionLevel) -> Self {
        self.context.permission_level = level;
        self
    }

    pub fn max_execution_time(mut self, duration: std::time::Duration) -> Self {
        self.context.max_execution_time = duration;
        self
    }

    pub fn max_output_size(mut self, size: usize) -> Self {
        self.context.max_output_size = size;
        self
    }

    pub fn feature_flags(mut self, flags: FeatureFlags) -> Self {
        self.context.feature_flags = flags;
        self
    }

    pub fn environment(mut self, env: HashMap<String, String>) -> Self {
        self.context.environment = env;
        self
    }

    pub fn progress_enabled(mut self, enabled: bool) -> Self {
        self.context.progress_enabled = enabled;
        self
    }

    pub fn build(self) -> ToolUseContext {
        self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_levels() {
        let ctx = ToolUseContext::new("test", PathBuf::from("/workspace"));

        // ReadOnly
        let ctx_readonly = ToolUseContext::builder("test", PathBuf::from("/workspace"))
            .permission_level(PermissionLevel::ReadOnly)
            .build();
        assert!(ctx_readonly.is_tool_allowed("read_file"));
        assert!(ctx_readonly.is_tool_allowed("search_code"));
        assert!(!ctx_readonly.is_tool_allowed("write_file"));

        // Write
        assert!(ctx.is_tool_allowed("read_file"));
        assert!(ctx.is_tool_allowed("write_file"));
        assert!(!ctx.is_tool_allowed("delete_file"));
        assert!(!ctx.is_tool_allowed("rm_rf"));

        // FullAccess
        let ctx_full = ToolUseContext::builder("test", PathBuf::from("/workspace"))
            .permission_level(PermissionLevel::FullAccess)
            .build();
        assert!(ctx_full.is_tool_allowed("delete_file"));
        assert!(ctx_full.is_tool_allowed("rm_rf"));
    }

    #[test]
    fn test_path_validation() {
        let ctx = ToolUseContext::new("test", PathBuf::from("/workspace"));

        assert!(ctx.is_path_allowed(&PathBuf::from("/workspace/src/main.rs")));
        assert!(ctx.is_path_allowed(&PathBuf::from("/workspace")));
        assert!(!ctx.is_path_allowed(&PathBuf::from("/etc/passwd")));
        assert!(!ctx.is_path_allowed(&PathBuf::from("/home/user")));
    }

    #[test]
    fn test_feature_flags() {
        let ctx = ToolUseContext::new("test", PathBuf::from("/workspace"));

        assert!(ctx.is_feature_enabled("network"));
        assert!(ctx.is_feature_enabled("file_system"));
        assert!(!ctx.is_feature_enabled("shell"));
        assert!(!ctx.is_feature_enabled("browser"));
        assert!(!ctx.is_feature_enabled("unknown"));
    }

    #[test]
    fn test_builder() {
        let ctx = ToolUseContext::builder("session1", PathBuf::from("/workspace"))
            .conversation_id("conv1")
            .message_id("msg1")
            .permission_level(PermissionLevel::FullAccess)
            .max_execution_time(std::time::Duration::from_secs(60))
            .build();

        assert_eq!(ctx.session_id, "session1");
        assert_eq!(ctx.conversation_id, Some("conv1".to_string()));
        assert_eq!(ctx.message_id, Some("msg1".to_string()));
        assert_eq!(ctx.permission_level, PermissionLevel::FullAccess);
        assert_eq!(ctx.max_execution_time, std::time::Duration::from_secs(60));
    }
}
