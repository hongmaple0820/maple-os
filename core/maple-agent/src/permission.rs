use serde::{Deserialize, Serialize};
use serde_json::Value;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionLevel {
    ReadOnly,
    Write,
    FullAccess,
}

pub struct PermissionPolicy {
    level: PermissionLevel,
}

impl PermissionPolicy {
    pub fn new(level: PermissionLevel) -> Self {
        Self { level }
    }

    pub fn authorize(&self, tool_name: &str, _input: &Value) -> Result<()> {
        match self.level {
            PermissionLevel::ReadOnly => {
                let read_only_tools = ["read", "search", "list", "get", "query"];
                let is_read = read_only_tools.iter().any(|t| tool_name.contains(t));
                if !is_read {
                    anyhow::bail!("Permission denied: {} not allowed in read-only mode", tool_name);
                }
            }
            PermissionLevel::Write => {
                let denied = ["delete", "rm", "drop", "destroy"];
                if denied.iter().any(|t| tool_name.contains(t)) {
                    anyhow::bail!("Permission denied: {} not allowed in write mode", tool_name);
                }
            }
            PermissionLevel::FullAccess => {}
        }
        Ok(())
    }

    pub fn authorize_with_context(
        &self,
        tool_use: &crate::react_loop::ToolUse,
        _session: &crate::react_loop::Session,
    ) -> Result<()> {
        self.authorize(&tool_use.name, &tool_use.input)
    }
}
