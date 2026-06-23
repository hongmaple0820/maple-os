use crate::error::LlmError;
use crate::response::LlmResponse;
use crate::stream::LlmStream;
use crate::usage::UsageTracker;
use crate::{LlmRequest, PrivacyLevel};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Metadata describing a registered LLM model (Track 3 / T3-1).
///
/// The frontend settings-page expects this shape — see #86 where the
/// backend returned `Vec<String>` but the frontend expected
/// `{ id, name, provider }[]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDescriptor {
    /// Unique id, same as the adapter name (e.g. "ollama/qwen2.5:7b")
    pub id: String,
    /// Human-readable name (last segment of id, e.g. "qwen2.5:7b")
    pub name: String,
    /// Provider inferred from id prefix (e.g. "ollama", "openai", "anthropic")
    pub provider: String,
    /// Adapter type — for now always "llm"; image adapters will use "image"
    pub adapter_type: String,
    /// Whether this is a local model (Ollama) vs cloud (OpenAI/Anthropic/etc.)
    pub is_local: bool,
    /// Max context length if known (0 if unknown)
    pub context_length: usize,
    /// Whether this model is registered with the LlmRouter (T3-2).
    /// `None` (omitted from JSON) means "registered" (backward compat).
    /// `Some(false)` means "discovered but not yet registered" — the
    /// frontend should show it but mark it as needing registration.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub registered: Option<bool>,
}

impl ModelDescriptor {
    /// Build a descriptor from a model id by splitting on `/`.
    /// Examples:
    ///   "ollama/qwen2.5:7b"    → provider="ollama", name="qwen2.5:7b", is_local=true
    ///   "openai/gpt-4o"        → provider="openai", name="gpt-4o", is_local=false
    ///   "deepseek-chat"        → provider="unknown", name="deepseek-chat", is_local=false
    pub fn from_model_id(id: &str) -> Self {
        let (provider, name, is_local) = if let Some((p, n)) = id.split_once('/') {
            let is_local = p.eq_ignore_ascii_case("ollama")
                || p.eq_ignore_ascii_case("llama_cpp")
                || p.eq_ignore_ascii_case("local");
            (p.to_string(), n.to_string(), is_local)
        } else {
            // No prefix — guess based on common model name patterns
            let lower = id.to_ascii_lowercase();
            let is_local = lower.starts_with("llama")
                || lower.starts_with("qwen")
                || lower.starts_with("mistral")
                || lower.contains(":");
            let provider = if lower.starts_with("gpt-")
                || lower.starts_with("o1-")
                || lower.starts_with("o3-")
            {
                "openai".to_string()
            } else if lower.starts_with("claude-") {
                "anthropic".to_string()
            } else if lower.starts_with("deepseek") {
                "deepseek".to_string()
            } else if lower.starts_with("gemini") {
                "google".to_string()
            } else if is_local {
                "local".to_string()
            } else {
                "unknown".to_string()
            };
            (provider, id.to_string(), is_local)
        };

        Self {
            id: id.to_string(),
            name,
            provider,
            adapter_type: "llm".to_string(),
            is_local,
            context_length: 0,
            registered: None, // from_model_id is called for registered adapters
        }
    }
}

#[async_trait]
pub trait LlmAdapter: Send + Sync {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse>;
    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmStream>>;
    fn count_tokens(&self, text: &str) -> usize;
    fn max_context_length(&self) -> usize;
    fn cost_per_1k_tokens(&self) -> (f64, f64);
    fn name(&self) -> &str;
    fn supports_vision(&self) -> bool {
        false
    }
    fn supports_function_calling(&self) -> bool {
        true
    }
}

#[async_trait]
impl LlmAdapter for Arc<dyn LlmAdapter> {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse> {
        (**self).complete(req).await
    }
    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmStream>> {
        (**self).stream(req).await
    }
    fn count_tokens(&self, text: &str) -> usize {
        (**self).count_tokens(text)
    }
    fn max_context_length(&self) -> usize {
        (**self).max_context_length()
    }
    fn cost_per_1k_tokens(&self) -> (f64, f64) {
        (**self).cost_per_1k_tokens()
    }
    fn name(&self) -> &str {
        (**self).name()
    }
    fn supports_vision(&self) -> bool {
        (**self).supports_vision()
    }
    fn supports_function_calling(&self) -> bool {
        (**self).supports_function_calling()
    }
}

/// Trait for external provider health checking — allows HealthMonitor (in maple-agent)
/// to feed health data into LlmRouter without creating a circular dependency.
#[async_trait]
pub trait ProviderHealthChecker: Send + Sync {
    async fn is_provider_available(&self, provider_id: &str) -> bool;
    async fn record_provider_result(&self, provider_id: &str, success: bool, latency_ms: u64);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub name: String,
    pub condition: String,
    pub preferred: Vec<String>,
    #[serde(default)]
    pub fallback_to_cloud: bool,
}

/// Per-adapter health state — tracks errors for circuit-breaker behavior
#[derive(Debug, Clone, Default)]
pub struct AdapterHealth {
    pub consecutive_errors: u32,
    pub last_error: Option<String>,
    pub is_circuit_open: bool,
    pub circuit_open_until: Option<std::time::Instant>,
}

pub struct LlmRouter {
    adapters: HashMap<String, Box<dyn LlmAdapter>>,
    routing_rules: Vec<RoutingRule>,
    fallback_chain: Vec<String>,
    usage_tracker: Arc<UsageTracker>,
    health: RwLock<HashMap<String, AdapterHealth>>,
    max_consecutive_errors: u32,
    circuit_breaker_duration: std::time::Duration,
    external_health_checker: Option<Arc<dyn ProviderHealthChecker>>,
}

impl LlmRouter {
    pub fn new(usage_tracker: Arc<UsageTracker>) -> Self {
        Self {
            adapters: HashMap::new(),
            routing_rules: Vec::new(),
            fallback_chain: Vec::new(),
            usage_tracker,
            health: RwLock::new(HashMap::new()),
            max_consecutive_errors: 5,
            circuit_breaker_duration: std::time::Duration::from_secs(60),
            external_health_checker: None,
        }
    }

    /// Set an external provider health checker (e.g., HealthMonitor from maple-agent)
    pub fn with_health_checker(mut self, checker: Arc<dyn ProviderHealthChecker>) -> Self {
        self.external_health_checker = Some(checker);
        self
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
        if let Some(adapter) = self.adapters.get(&req.requested_model)
            && !req.requested_model.is_empty()
            && req.requested_model != "default"
            && req.requested_model != "auto"
        {
            return Ok(adapter.as_ref());
        }

        if req.privacy_level == PrivacyLevel::Sensitive {
            return self
                .get_local_adapter()
                .ok_or_else(|| anyhow::anyhow!("No local model available for sensitive data"));
        }

        if self.usage_tracker.daily_budget_exceeded().await {
            return self
                .get_local_adapter()
                .ok_or_else(|| anyhow::anyhow!("Daily budget exceeded"));
        }

        for rule in &self.routing_rules {
            if rule.matches(req) {
                for model_id in &rule.preferred {
                    if let Some(adapter) = self.adapters.get(model_id)
                        && self.is_available(model_id).await
                    {
                        return Ok(adapter.as_ref());
                    }
                }
            }
        }

        for model_id in &self.fallback_chain {
            if let Some(adapter) = self.adapters.get(model_id)
                && self.is_available(model_id).await
            {
                return Ok(adapter.as_ref());
            }
        }

        Err(anyhow::anyhow!("No available LLM model"))
    }

    fn get_local_adapter(&self) -> Option<&dyn LlmAdapter> {
        self.adapters
            .iter()
            .find(|(k, _)| k.starts_with("ollama/"))
            .map(|(_, v)| v.as_ref())
    }

    async fn is_available(&self, model_id: &str) -> bool {
        if !self.adapters.contains_key(model_id) {
            return false;
        }

        // Check internal circuit breaker
        let health = self.health.read().await;
        if let Some(adapter_health) = health.get(model_id)
            && adapter_health.is_circuit_open
            && let Some(until) = adapter_health.circuit_open_until
            && std::time::Instant::now() < until
        {
            return false;
        }
        drop(health);

        // Check external health checker (HealthMonitor)
        if let Some(ref checker) = self.external_health_checker
            && !checker.is_provider_available(model_id).await
        {
            return false;
        }

        true
    }

    pub async fn list_models(&self) -> Vec<String> {
        self.adapters.keys().cloned().collect()
    }

    /// List models with full descriptor metadata (Track 3 / T3-1).
    ///
    /// Returns one `ModelDescriptor` per registered adapter, with `provider`
    /// inferred from the adapter name's prefix (`ollama/...`, `openai/...`,
    /// `anthropic/...`) or falling back to "unknown". The frontend
    /// settings-page expects this shape (see #86).
    pub async fn list_models_detailed(&self) -> Vec<ModelDescriptor> {
        self.adapters
            .keys()
            .map(|name| ModelDescriptor::from_model_id(name))
            .collect()
    }

    /// Discover models available on a local Ollama instance (Track 3 / T3-2).
    ///
    /// Fetches `GET {ollama_url}/v1/models` and returns a list of
    /// `ModelDescriptor` with `provider="ollama"` and `is_local=true`.
    /// Failed requests (network error, non-200, parse error) return an
    /// empty Vec — the caller should treat this as "Ollama not running
    /// or unreachable", not a hard error.
    ///
    /// The returned descriptors are NOT registered with the router; the
    /// caller decides whether to call `register_adapter` for each.
    pub async fn discover_ollama_models(ollama_url: &str) -> Vec<ModelDescriptor> {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
        {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let url = format!("{}/v1/models", ollama_url.trim_end_matches('/'));
        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(error = %e, url = %url, "Ollama /v1/models unreachable");
                return Vec::new();
            }
        };

        if !resp.status().is_success() {
            tracing::debug!(status = %resp.status(), url = %url, "Ollama /v1/models non-200");
            return Vec::new();
        }

        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!(error = %e, "Ollama /v1/models body parse failed");
                return Vec::new();
            }
        };

        let data = body["data"].as_array();
        match data {
            Some(arr) => arr
                .iter()
                .filter_map(|m| {
                    let id = m["id"].as_str()?.to_string();
                    let full_id = format!("ollama/{}", id);
                    Some(ModelDescriptor {
                        id: full_id.clone(),
                        name: id,
                        provider: "ollama".to_string(),
                        adapter_type: "llm".to_string(),
                        is_local: true,
                        context_length: m["context_length"].as_u64().unwrap_or(0) as usize,
                        registered: Some(false), // discovered, not yet registered
                    })
                })
                .collect(),
            None => Vec::new(),
        }
    }

    /// Record an error for an adapter — used by error classifier
    pub async fn record_error(&self, model_id: &str, error: &LlmError) {
        let _decision = error.classify_decision();

        let mut health = self.health.write().await;
        let adapter_health = health.entry(model_id.to_string()).or_default();

        adapter_health.consecutive_errors += 1;
        adapter_health.last_error = Some(error.to_string());

        if adapter_health.consecutive_errors >= self.max_consecutive_errors {
            adapter_health.is_circuit_open = true;
            adapter_health.circuit_open_until =
                Some(std::time::Instant::now() + self.circuit_breaker_duration);
        }
        drop(health);

        // Forward to external health checker
        if let Some(ref checker) = self.external_health_checker {
            checker.record_provider_result(model_id, false, 0).await;
        }
    }

    /// Record a successful call — resets error counter
    pub async fn record_success(&self, model_id: &str) {
        let mut health = self.health.write().await;
        if let Some(adapter_health) = health.get_mut(model_id) {
            adapter_health.consecutive_errors = 0;
            adapter_health.last_error = None;
            adapter_health.is_circuit_open = false;
            adapter_health.circuit_open_until = None;
        }
        drop(health);

        // Forward to external health checker
        if let Some(ref checker) = self.external_health_checker {
            checker.record_provider_result(model_id, true, 0).await;
        }
    }

    /// Get adapter with error-classified retry logic
    pub async fn route_with_retry(&self, req: &LlmRequest) -> Result<(&dyn LlmAdapter, String)> {
        let adapter = self.route(req).await?;
        let model_id = adapter.name().to_string();
        Ok((adapter, model_id))
    }

    /// Get health status for all adapters
    pub async fn get_health_status(&self) -> HashMap<String, AdapterHealth> {
        self.health.read().await.clone()
    }
}

impl RoutingRule {
    pub fn matches(&self, req: &LlmRequest) -> bool {
        let cond = &self.condition;
        let task_type = serde_json::to_string(&req.task_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        let priority = serde_json::to_string(&req.priority)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        if cond.contains(&format!("task.type == '{}'", task_type)) {
            return true;
        }
        if cond.contains(&format!("task.priority == '{}'", priority)) {
            return true;
        }
        if cond.contains("task.has_image == true") && req.has_image {
            return true;
        }
        if cond.contains("task.privacy_level == 'sensitive'")
            && req.privacy_level == PrivacyLevel::Sensitive
        {
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::{LlmRequest, Priority, TaskType};

    #[test]
    fn test_routing_rule_matches_task_type() {
        let rule = RoutingRule {
            name: "code-tasks".to_string(),
            condition: "task.type == 'code_generation'".to_string(),
            preferred: vec!["deepseek-coder".to_string()],
            fallback_to_cloud: true,
        };

        let mut req = LlmRequest::new("Write a function".to_string(), "default");
        req.task_type = TaskType::CodeGeneration;

        assert!(rule.matches(&req));

        req.task_type = TaskType::General;
        assert!(!rule.matches(&req));
    }

    #[test]
    fn test_routing_rule_matches_priority() {
        let rule = RoutingRule {
            name: "high-priority".to_string(),
            condition: "task.priority == 'speed'".to_string(),
            preferred: vec!["gpt-4o".to_string()],
            fallback_to_cloud: true,
        };

        let mut req = LlmRequest::new("Urgent task".to_string(), "default");
        req.priority = Priority::Speed;

        assert!(rule.matches(&req));

        req.priority = Priority::Cost;
        assert!(!rule.matches(&req));
    }

    #[test]
    fn test_routing_rule_matches_image() {
        let rule = RoutingRule {
            name: "vision-tasks".to_string(),
            condition: "task.has_image == true".to_string(),
            preferred: vec!["claude-3-5-sonnet-20241022".to_string()],
            fallback_to_cloud: true,
        };

        let mut req = LlmRequest::new("Describe this image".to_string(), "default");
        req.has_image = true;

        assert!(rule.matches(&req));

        req.has_image = false;
        assert!(!rule.matches(&req));
    }

    #[test]
    fn test_routing_rule_matches_privacy() {
        let rule = RoutingRule {
            name: "privacy-sensitive".to_string(),
            condition: "task.privacy_level == 'sensitive'".to_string(),
            preferred: vec!["ollama/qwen2.5:7b".to_string()],
            fallback_to_cloud: false,
        };

        let mut req = LlmRequest::new("Private data".to_string(), "default");
        req.privacy_level = PrivacyLevel::Sensitive;

        assert!(rule.matches(&req));

        req.privacy_level = PrivacyLevel::Public;
        assert!(!rule.matches(&req));
    }

    #[test]
    fn test_routing_rule_no_match() {
        let rule = RoutingRule {
            name: "code-tasks".to_string(),
            condition: "task.type == 'code_generation'".to_string(),
            preferred: vec!["deepseek-coder".to_string()],
            fallback_to_cloud: true,
        };

        let req = LlmRequest::new("Chat message".to_string(), "default");
        assert!(!rule.matches(&req));
    }

    #[test]
    fn test_llm_router_creation() {
        let usage_tracker = Arc::new(UsageTracker::new(100.0));
        let router = LlmRouter::new(usage_tracker);

        assert!(router.adapters.is_empty());
        assert!(router.routing_rules.is_empty());
        assert!(router.fallback_chain.is_empty());
    }

    #[test]
    fn test_llm_router_register_adapter() {
        let usage_tracker = Arc::new(UsageTracker::new(100.0));
        let mut router = LlmRouter::new(usage_tracker);

        // 创建一个模拟适配器
        struct MockAdapter;

        #[async_trait]
        impl LlmAdapter for MockAdapter {
            async fn complete(&self, _req: LlmRequest) -> Result<LlmResponse> {
                Ok(LlmResponse::new("test".to_string(), 10, 5))
            }

            async fn stream(&self, _req: LlmRequest) -> Result<Box<dyn LlmStream>> {
                todo!()
            }

            fn count_tokens(&self, text: &str) -> usize {
                crate::token_counter::count_tokens(text)
            }

            fn max_context_length(&self) -> usize {
                4096
            }

            fn cost_per_1k_tokens(&self) -> (f64, f64) {
                (0.001, 0.002)
            }

            fn name(&self) -> &str {
                "mock-model"
            }
        }

        router.register_adapter(Box::new(MockAdapter));
        assert_eq!(router.adapters.len(), 1);
        assert!(router.adapters.contains_key("mock-model"));
    }

    // ── T3-1: ModelDescriptor::from_model_id ──
    #[test]
    fn test_model_descriptor_from_prefixed_id() {
        let d = ModelDescriptor::from_model_id("ollama/qwen2.5:7b");
        assert_eq!(d.id, "ollama/qwen2.5:7b");
        assert_eq!(d.name, "qwen2.5:7b");
        assert_eq!(d.provider, "ollama");
        assert!(d.is_local);
        assert_eq!(d.adapter_type, "llm");
    }

    #[test]
    fn test_model_descriptor_from_openai_prefixed() {
        let d = ModelDescriptor::from_model_id("openai/gpt-4o");
        assert_eq!(d.name, "gpt-4o");
        assert_eq!(d.provider, "openai");
        assert!(!d.is_local);
    }

    #[test]
    fn test_model_descriptor_from_bare_openai_name() {
        let d = ModelDescriptor::from_model_id("gpt-4o-mini");
        assert_eq!(d.name, "gpt-4o-mini");
        assert_eq!(d.provider, "openai");
        assert!(!d.is_local);
    }

    #[test]
    fn test_model_descriptor_from_bare_anthropic_name() {
        let d = ModelDescriptor::from_model_id("claude-3-5-sonnet");
        assert_eq!(d.provider, "anthropic");
        assert!(!d.is_local);
    }

    #[test]
    fn test_model_descriptor_from_bare_local_name() {
        // Bare local model name with `:` (Ollama tag pattern)
        let d = ModelDescriptor::from_model_id("qwen2.5:7b");
        assert_eq!(d.provider, "local");
        assert!(d.is_local);
    }

    #[test]
    fn test_model_descriptor_from_unknown_name() {
        let d = ModelDescriptor::from_model_id("some-custom-model");
        assert_eq!(d.provider, "unknown");
        assert!(!d.is_local);
    }
}
