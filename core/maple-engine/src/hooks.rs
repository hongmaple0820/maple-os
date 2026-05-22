use serde::{Deserialize, Serialize};
use serde_json::Value;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    pub on_start: Option<Vec<String>>,
    pub on_error: Option<Vec<String>>,
    pub on_complete: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub enum HookDecision {
    Allow,
    Deny(String),
    Modify(Value),
}

pub struct HookRunner {
    hooks: Vec<Box<dyn Hook + Send + Sync>>,
}

pub trait Hook: Send + Sync {
    fn name(&self) -> &str;
    fn on_pre_tool_use(&self, _tool_name: &str, _input: &Value) -> HookDecision {
        HookDecision::Allow
    }
    fn on_post_tool_use(&self, _tool_name: &str, _result: &Value) {}
}

impl HookRunner {
    pub fn new() -> Self {
        Self {
            hooks: Vec::new(),
        }
    }

    pub fn register(&mut self, hook: Box<dyn Hook + Send + Sync>) {
        self.hooks.push(hook);
    }

    pub async fn run_pre_tool_use(&self, tool_name: &str, input: &Value) -> Result<HookDecision> {
        for hook in &self.hooks {
            match hook.on_pre_tool_use(tool_name, input) {
                HookDecision::Allow => continue,
                other => return Ok(other),
            }
        }
        Ok(HookDecision::Allow)
    }

    pub async fn run_post_tool_use(&self, tool_name: &str, result: &Value) -> Result<()> {
        for hook in &self.hooks {
            hook.on_post_tool_use(tool_name, result);
        }
        Ok(())
    }
}
