use crate::response::LlmResponse;
use crate::stream::LlmStream;
use crate::usage::UsageTracker;
use crate::{LlmRequest, PrivacyLevel};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use anyhow::Result;

#[async_trait]
pub trait LlmAdapter: Send + Sync {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse>;
    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmStream>>;
    fn count_tokens(&self, text: &str) -> usize;
    fn max_context_length(&self) -> usize;
    fn cost_per_1k_tokens(&self) -> (f64, f64);
    fn name(&self) -> &str;
    fn supports_vision(&self) -> bool { false }
    fn supports_function_calling(&self) -> bool { true }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub name: String,
    pub condition: String,
    pub preferred: Vec<String>,
    #[serde(default)]
    pub fallback_to_cloud: bool,
}

pub struct LlmRouter {
    adapters: HashMap<String, Box<dyn LlmAdapter>>,
    routing_rules: Vec<RoutingRule>,
    fallback_chain: Vec<String>,
    usage_tracker: Arc<UsageTracker>,
}

impl LlmRouter {
    pub fn new(usage_tracker: Arc<UsageTracker>) -> Self {
        Self {
            adapters: HashMap::new(),
            routing_rules: Vec::new(),
            fallback_chain: Vec::new(),
            usage_tracker,
        }
    }

    pub fn register_adapter(&mut self, adapter: Box<dyn LlmAdapter>) {
        let name = adapter.name().to_string();
        self.adapters.insert(name, adapter);
    }

    pub fn set_routing_rules(&mut self, rules: Vec<RoutingRule>) {
        self.routing_rules = rules;
    }

    pub fn set_fallback_chain(&mut self, chain: Vec<String>) {
        self.fallback_chain = chain;
    }

    pub async fn route(&self, req: &LlmRequest) -> Result<&dyn LlmAdapter> {
        if req.privacy_level == PrivacyLevel::Sensitive {
            return self.get_local_adapter()
                .ok_or_else(|| anyhow::anyhow!("No local model available for sensitive data"));
        }

        if self.usage_tracker.daily_budget_exceeded().await {
            return self.get_local_adapter()
                .ok_or_else(|| anyhow::anyhow!("Daily budget exceeded"));
        }

        for rule in &self.routing_rules {
            if rule.matches(req) {
                for model_id in &rule.preferred {
                    if let Some(adapter) = self.adapters.get(model_id) {
                        if self.is_available(model_id).await {
                            return Ok(adapter.as_ref());
                        }
                    }
                }
            }
        }

        for model_id in &self.fallback_chain {
            if let Some(adapter) = self.adapters.get(model_id) {
                if self.is_available(model_id).await {
                    return Ok(adapter.as_ref());
                }
            }
        }

        Err(anyhow::anyhow!("No available LLM model"))
    }

    fn get_local_adapter(&self) -> Option<&dyn LlmAdapter> {
        self.adapters.iter()
            .find(|(k, _)| k.starts_with("ollama/"))
            .map(|(_, v)| v.as_ref())
    }

    async fn is_available(&self, model_id: &str) -> bool {
        self.adapters.contains_key(model_id)
    }

    pub async fn list_models(&self) -> Vec<String> {
        self.adapters.keys().cloned().collect()
    }
}

impl RoutingRule {
    pub fn matches(&self, req: &LlmRequest) -> bool {
        let cond = &self.condition;
        let task_type = format!("{:?}", req.task_type).to_lowercase();
        let priority = format!("{:?}", req.priority).to_lowercase();

        if cond.contains(&format!("task.type == '{}'", task_type)) {
            return true;
        }
        if cond.contains(&format!("task.priority == '{}'", priority)) {
            return true;
        }
        if cond.contains("task.has_image == true") && req.has_image {
            return true;
        }
        if cond.contains("task.privacy_level == 'sensitive'") && req.privacy_level == PrivacyLevel::Sensitive {
            return true;
        }
        false
    }
}
