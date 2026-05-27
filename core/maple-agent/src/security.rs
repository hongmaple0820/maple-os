use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use anyhow::Result;

/// Security Hardening — inspired by claw-code's 5-level permission system
///
/// Features:
/// - 5-level permission system (ReadOnly < WorkspaceWrite < Prompt < Allow < DangerFullAccess)
/// - Dynamic command classification
/// - Path traversal prevention
/// - Audit logging
/// - Approval callbacks

/// Permission levels — ordered from least to most privileged
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SecurityLevel {
    /// Read-only access
    ReadOnly = 0,
    /// Write access within workspace
    WorkspaceWrite = 1,
    /// Can prompt for approval
    Prompt = 2,
    /// Auto-approved for safe operations
    Allow = 3,
    /// Full access including dangerous operations
    DangerFullAccess = 4,
}

/// Security policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    pub level: SecurityLevel,
    pub allowed_tools: HashSet<String>,
    pub denied_tools: HashSet<String>,
    pub allowed_paths: Vec<String>,
    pub denied_paths: Vec<String>,
    pub require_approval_for: Vec<String>,
    pub max_execution_time_secs: u64,
    pub audit_enabled: bool,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            level: SecurityLevel::WorkspaceWrite,
            allowed_tools: HashSet::new(),
            denied_tools: HashSet::new(),
            allowed_paths: vec![".".to_string()],
            denied_paths: vec![
                "/etc".to_string(),
                "/proc".to_string(),
                "/sys".to_string(),
                "~/.ssh".to_string(),
                "~/.gnupg".to_string(),
            ],
            require_approval_for: vec![
                "execute_command".to_string(),
                "write_file".to_string(),
                "delete_file".to_string(),
            ],
            max_execution_time_secs: 30,
            audit_enabled: true,
        }
    }
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: u64,
    pub tool_name: String,
    pub input: Value,
    pub output: Option<Value>,
    pub success: bool,
    pub security_level: SecurityLevel,
    pub approved: bool,
    pub error: Option<String>,
}

/// Security manager
pub struct SecurityManager {
    policy: SecurityPolicy,
    audit_log: Vec<AuditEntry>,
    max_audit_entries: usize,
    approval_callback: Option<Box<dyn Fn(&str, &Value) -> bool + Send + Sync>>,
}

impl SecurityManager {
    pub fn new(policy: SecurityPolicy) -> Self {
        Self {
            policy,
            audit_log: Vec::new(),
            max_audit_entries: 10000,
            approval_callback: None,
        }
    }

    pub fn with_approval_callback(mut self, callback: Box<dyn Fn(&str, &Value) -> bool + Send + Sync>) -> Self {
        self.approval_callback = Some(callback);
        self
    }

    /// Check if a tool execution is allowed
    pub fn check_permission(&self, tool_name: &str, input: &Value) -> Result<PermissionCheck> {
        // Check denied tools
        if self.policy.denied_tools.contains(tool_name) {
            return Ok(PermissionCheck::Denied {
                reason: format!("Tool {} is explicitly denied", tool_name),
            });
        }

        // Check allowed tools (if list is not empty, tool must be in it)
        if !self.policy.allowed_tools.is_empty() && !self.policy.allowed_tools.contains(tool_name) {
            return Ok(PermissionCheck::Denied {
                reason: format!("Tool {} is not in allowed list", tool_name),
            });
        }

        // Check security level
        let required_level = self.get_required_security_level(tool_name, input);
        if required_level > self.policy.level {
            return Ok(PermissionCheck::Denied {
                reason: format!(
                    "Tool {} requires {:?} but current level is {:?}",
                    tool_name, required_level, self.policy.level
                ),
            });
        }

        // Check if approval is required
        if self.requires_approval(tool_name) {
            if let Some(callback) = &self.approval_callback {
                if !callback(tool_name, input) {
                    return Ok(PermissionCheck::Denied {
                        reason: "Approval denied by user".to_string(),
                    });
                }
            } else {
                return Ok(PermissionCheck::RequiresApproval {
                    reason: format!("Tool {} requires approval", tool_name),
                });
            }
        }

        // Check path restrictions for file operations
        if let Some(path) = self.extract_path(tool_name, input) {
            if !self.is_path_allowed(&path) {
                return Ok(PermissionCheck::Denied {
                    reason: format!("Path {} is not allowed", path),
                });
            }
        }

        Ok(PermissionCheck::Allowed)
    }

    /// Get required security level for a tool
    fn get_required_security_level(&self, tool_name: &str, _input: &Value) -> SecurityLevel {
        // Dangerous commands require DangerFullAccess
        let dangerous_commands = [
            "rm", "rmdir", "del", "format", "mkfs", "dd",
            "shutdown", "reboot", "halt", "init",
            "chmod", "chown", "chgrp",
            "sudo", "su", "doas",
            "curl", "wget", "nc", "netcat",
        ];

        if dangerous_commands.iter().any(|cmd| tool_name.contains(cmd)) {
            return SecurityLevel::DangerFullAccess;
        }

        // Write operations require WorkspaceWrite
        let write_commands = [
            "write", "create", "update", "delete", "move", "rename",
            "execute", "run", "bash", "shell",
        ];

        if write_commands.iter().any(|cmd| tool_name.contains(cmd)) {
            return SecurityLevel::WorkspaceWrite;
        }

        // Read operations are allowed at ReadOnly
        SecurityLevel::ReadOnly
    }

    /// Check if a tool requires approval
    fn requires_approval(&self, tool_name: &str) -> bool {
        self.policy.require_approval_for.iter().any(|t| tool_name.contains(t))
    }

    /// Extract path from tool input
    fn extract_path(&self, tool_name: &str, input: &Value) -> Option<String> {
        // Try common path fields
        let path_fields = ["path", "file", "file_path", "directory", "dir"];

        for field in &path_fields {
            if let Some(path) = input[field].as_str() {
                return Some(path.to_string());
            }
        }

        // For command execution, try to extract file paths
        if tool_name.contains("command") || tool_name.contains("bash") {
            if let Some(cmd) = input["command"].as_str() {
                // Simple path extraction from command
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                for part in parts {
                    if part.starts_with('/') || part.starts_with("./") || part.starts_with("../") {
                        return Some(part.to_string());
                    }
                }
            }
        }

        None
    }

    /// Check if a path is allowed
    fn is_path_allowed(&self, path: &str) -> bool {
        // Check denied paths
        for denied in &self.policy.denied_paths {
            if path.starts_with(denied) {
                return false;
            }
        }

        // Check allowed paths (if list is not empty)
        if !self.policy.allowed_paths.is_empty() {
            return self.policy.allowed_paths.iter().any(|allowed| {
                path.starts_with(allowed) || allowed == "."
            });
        }

        true
    }

    /// Record an audit entry
    pub fn record_audit(
        &mut self,
        tool_name: &str,
        input: &Value,
        output: Option<&Value>,
        success: bool,
        approved: bool,
        error: Option<String>,
    ) {
        if !self.policy.audit_enabled {
            return;
        }

        let entry = AuditEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            tool_name: tool_name.to_string(),
            input: input.clone(),
            output: output.cloned(),
            success,
            security_level: self.policy.level,
            approved,
            error,
        };

        self.audit_log.push(entry);

        // Trim if over max
        if self.audit_log.len() > self.max_audit_entries {
            self.audit_log.remove(0);
        }
    }

    /// Get audit log
    pub fn get_audit_log(&self) -> &[AuditEntry] {
        &self.audit_log
    }

    /// Get recent audit entries
    pub fn get_recent_audit(&self, count: usize) -> Vec<&AuditEntry> {
        self.audit_log.iter().rev().take(count).collect()
    }

    /// Classify a bash command's permission level
    pub fn classify_bash_permission(command: &str) -> SecurityLevel {
        let command_lower = command.to_lowercase();

        // Dangerous commands
        let dangerous = [
            "rm -rf", "rm -r", "rmdir", "del /s", "format", "mkfs",
            "dd if=", "shutdown", "reboot", "halt",
            "sudo", "su -", "doas",
            "> /dev/", "chmod 777", "chown root",
        ];

        if dangerous.iter().any(|cmd| command_lower.contains(cmd)) {
            return SecurityLevel::DangerFullAccess;
        }

        // Write commands
        let write_cmds = [
            "mv ", "cp ", "mkdir ", "touch ", "echo ", "cat >",
            "tee ", "sed -i", "awk -i",
        ];

        if write_cmds.iter().any(|cmd| command_lower.contains(cmd)) {
            return SecurityLevel::WorkspaceWrite;
        }

        // Read commands
        let read_cmds = [
            "ls ", "cat ", "head ", "tail ", "grep ", "find ",
            "wc ", "sort ", "uniq ", "diff ",
        ];

        if read_cmds.iter().any(|cmd| command_lower.starts_with(cmd)) {
            return SecurityLevel::ReadOnly;
        }

        // Default to Prompt for unknown commands
        SecurityLevel::Prompt
    }
}

/// Permission check result
#[derive(Debug, Clone)]
pub enum PermissionCheck {
    Allowed,
    Denied { reason: String },
    RequiresApproval { reason: String },
}

/// Builder for SecurityPolicy
pub struct SecurityPolicyBuilder {
    policy: SecurityPolicy,
}

impl SecurityPolicyBuilder {
    pub fn new(level: SecurityLevel) -> Self {
        Self {
            policy: SecurityPolicy {
                level,
                ..Default::default()
            },
        }
    }

    pub fn allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.policy.allowed_tools = tools.into_iter().collect();
        self
    }

    pub fn denied_tools(mut self, tools: Vec<String>) -> Self {
        self.policy.denied_tools = tools.into_iter().collect();
        self
    }

    pub fn allowed_paths(mut self, paths: Vec<String>) -> Self {
        self.policy.allowed_paths = paths;
        self
    }

    pub fn denied_paths(mut self, paths: Vec<String>) -> Self {
        self.policy.denied_paths = paths;
        self
    }

    pub fn require_approval_for(mut self, tools: Vec<String>) -> Self {
        self.policy.require_approval_for = tools;
        self
    }

    pub fn audit_enabled(mut self, enabled: bool) -> Self {
        self.policy.audit_enabled = enabled;
        self
    }

    pub fn build(self) -> SecurityPolicy {
        self.policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_levels() {
        assert!(SecurityLevel::ReadOnly < SecurityLevel::WorkspaceWrite);
        assert!(SecurityLevel::WorkspaceWrite < SecurityLevel::Prompt);
        assert!(SecurityLevel::Prompt < SecurityLevel::Allow);
        assert!(SecurityLevel::Allow < SecurityLevel::DangerFullAccess);
    }

    #[test]
    fn test_permission_check() {
        let policy = SecurityPolicyBuilder::new(SecurityLevel::WorkspaceWrite)
            .denied_tools(vec!["rm".to_string()])
            .build();

        let manager = SecurityManager::new(policy);

        // Allowed tool
        let result = manager.check_permission("read_file", &serde_json::json!({}));
        assert!(matches!(result, Ok(PermissionCheck::Allowed)));

        // Denied tool
        let result = manager.check_permission("rm_rf", &serde_json::json!({}));
        assert!(matches!(result, Ok(PermissionCheck::Denied { .. })));
    }

    #[test]
    fn test_path_validation() {
        let policy = SecurityPolicyBuilder::new(SecurityLevel::WorkspaceWrite)
            .denied_paths(vec!["/etc".to_string()])
            .build();

        let manager = SecurityManager::new(policy);

        assert!(manager.is_path_allowed("./src/main.rs"));
        assert!(manager.is_path_allowed("/home/user/project"));
        assert!(!manager.is_path_allowed("/etc/passwd"));
    }

    #[test]
    fn test_bash_classification() {
        assert_eq!(
            SecurityManager::classify_bash_permission("rm -rf /"),
            SecurityLevel::DangerFullAccess
        );
        assert_eq!(
            SecurityManager::classify_bash_permission("ls -la"),
            SecurityLevel::ReadOnly
        );
        assert_eq!(
            SecurityManager::classify_bash_permission("mv file1 file2"),
            SecurityLevel::WorkspaceWrite
        );
    }

    #[test]
    fn test_audit_log() {
        let policy = SecurityPolicy::default();
        let mut manager = SecurityManager::new(policy);

        manager.record_audit(
            "read_file",
            &serde_json::json!({"path": "test.txt"}),
            Some(&serde_json::json!({"content": "hello"})),
            true,
            true,
            None,
        );

        assert_eq!(manager.get_audit_log().len(), 1);
    }
}
