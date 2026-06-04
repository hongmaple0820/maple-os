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
    Skip(String),
}

pub struct HookRunner {
    hooks: Vec<Box<dyn Hook + Send + Sync>>,
}

impl Default for HookRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Lifecycle hooks — inspired by rig's 7 hook points
/// Each hook can inspect, modify, or block the action
pub trait Hook: Send + Sync {
    fn name(&self) -> &str;

    /// Called when a new session starts
    fn on_session_start(&self, _session_id: &str) {}

    /// Called before LLM API call — can modify or block
    fn on_pre_llm_call(&self, _model: &str, _message_count: usize) -> HookDecision {
        HookDecision::Allow
    }

    /// Called after LLM response received
    fn on_post_llm_call(&self, _model: &str, _input_tokens: usize, _output_tokens: usize) {}

    /// Called before tool execution — can block or skip
    fn on_pre_tool_use(&self, _tool_name: &str, _input: &Value) -> HookDecision {
        HookDecision::Allow
    }

    /// Called after tool execution
    fn on_post_tool_use(&self, _tool_name: &str, _result: &Value) {}

    /// Called on any error during execution
    fn on_error(&self, _error: &str) {}

    /// Called when session ends
    fn on_session_end(&self, _session_id: &str) {}
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

    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty()
    }

    pub async fn run_session_start(&self, session_id: &str) {
        for hook in &self.hooks {
            hook.on_session_start(session_id);
        }
    }

    pub async fn run_pre_llm_call(&self, model: &str, message_count: usize) -> Result<HookDecision> {
        for hook in &self.hooks {
            match hook.on_pre_llm_call(model, message_count) {
                HookDecision::Allow => continue,
                other => return Ok(other),
            }
        }
        Ok(HookDecision::Allow)
    }

    pub async fn run_post_llm_call(&self, model: &str, input_tokens: usize, output_tokens: usize) {
        for hook in &self.hooks {
            hook.on_post_llm_call(model, input_tokens, output_tokens);
        }
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

    pub async fn run_error(&self, error: &str) {
        for hook in &self.hooks {
            hook.on_error(error);
        }
    }

    pub async fn run_session_end(&self, session_id: &str) {
        for hook in &self.hooks {
            hook.on_session_end(session_id);
        }
    }
}
