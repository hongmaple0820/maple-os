use serde::Deserialize;
use std::sync::Arc;
use maple_llm::router::LlmRouter;
use maple_llm::usage::UsageTracker;
use maple_llm::adapters::ollama::OllamaAdapter;
use crate::state::ServerConfig;

#[derive(Debug, Deserialize)]
pub struct LlmConfig {
    pub default: Option<DefaultConfig>,
    pub ollama: Option<OllamaConfig>,
    pub deepseek: Option<ProviderConfig>,
    pub anthropic: Option<ProviderConfig>,
    pub qwen: Option<ProviderConfig>,
    pub glm: Option<ProviderConfig>,
    pub openai: Option<ProviderConfig>,
    pub google: Option<ProviderConfig>,
    pub routing_rules: Option<Vec<RoutingRuleConfig>>,
    pub fallback_chain: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct DefaultConfig {
    pub model: Option<String>,
    pub daily_budget: Option<f64>,
    pub enable_routing: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct OllamaConfig {
    pub enabled: Option<bool>,
    pub base_url: Option<String>,
    pub models: Option<Vec<String>>,
    pub default_model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ProviderConfig {
    pub enabled: Option<bool>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub models: Option<Vec<String>>,
    pub default_model: Option<String>,
    pub pricing: Option<PricingConfig>,
    pub context_length: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct PricingConfig {
    pub input: f64,
    pub output: f64,
}

#[derive(Debug, Deserialize)]
pub struct RoutingRuleConfig {
    pub name: String,
    pub condition: String,
    pub preferred: Vec<String>,
    pub fallback_to_cloud: Option<bool>,
}

pub fn load_llm_config() -> Option<LlmConfig> {
    let config_path = std::path::Path::new("config/llm.toml");
    if !config_path.exists() {
        tracing::info!("LLM config file not found at config/llm.toml, using environment variables");
        return None;
    }

    match std::fs::read_to_string(config_path) {
        Ok(content) => {
            match toml::from_str::<LlmConfig>(&content) {
                Ok(config) => {
                    tracing::info!("Loaded LLM config from config/llm.toml");
                    Some(config)
                }
                Err(e) => {
                    tracing::warn!("Failed to parse LLM config: {}", e);
                    None
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to read LLM config file: {}", e);
            None
        }
    }
}

pub fn build_llm_router(config: &ServerConfig) -> Arc<LlmRouter> {
    let usage_tracker = Arc::new(UsageTracker::new(config.usage_limit_usd));
    let mut router = LlmRouter::new(usage_tracker);

    // 加载配置文件
    let llm_config = load_llm_config();

    // 注册Ollama适配器
    let ollama_config = llm_config.as_ref().and_then(|c| c.ollama.as_ref());
    let ollama_enabled = ollama_config.and_then(|c| c.enabled).unwrap_or(true);

    if ollama_enabled {
        if let Ok(base_url) = std::env::var("OLLAMA_BASE_URL") {
            let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "qwen2.5:7b".to_string());
            let adapter = OllamaAdapter::new(model).with_base_url(base_url);
            router.register_adapter(Box::new(adapter));
        } else if let Some(ollama) = ollama_config {
            let base_url = ollama.base_url.clone().unwrap_or_else(|| "http://127.0.0.1:11434".to_string());
            let models = ollama.models.clone().unwrap_or_else(|| vec!["qwen2.5:7b".to_string()]);
            for model in models {
                let adapter = OllamaAdapter::new(model.clone()).with_base_url(base_url.clone());
                router.register_adapter(Box::new(adapter));
            }
        } else {
            let adapter = OllamaAdapter::qwen_7b();
            router.register_adapter(Box::new(adapter));
        }
    }

    // 注册DeepSeek适配器
    let deepseek_config = llm_config.as_ref().and_then(|c| c.deepseek.as_ref());
    let deepseek_enabled = deepseek_config.and_then(|c| c.enabled).unwrap_or(false);

    if deepseek_enabled || std::env::var("DEEPSEEK_API_KEY").is_ok() {
        let api_key = std::env::var("DEEPSEEK_API_KEY")
            .or_else(|_| {
                deepseek_config
                    .and_then(|c| c.api_key.clone())
                    .ok_or_else(|| std::env::VarError::NotPresent)
            })
            .unwrap_or_default();

        if !api_key.is_empty() {
            let mut adapter = maple_llm::adapters::openai_compat::OpenAiCompatAdapter::deepseek(api_key);
            if let Some(deepseek) = deepseek_config {
                if let Some(base_url) = &deepseek.base_url {
                    adapter = adapter.with_base_url(base_url.clone());
                }
                if let Some(context_length) = deepseek.context_length {
                    adapter = adapter.with_context_length(context_length);
                }
                if let Some(pricing) = &deepseek.pricing {
                    adapter = adapter.with_pricing(pricing.input, pricing.output);
                }
            }
            router.register_adapter(Box::new(adapter));
        }
    }

    // 注册Anthropic适配器
    let anthropic_config = llm_config.as_ref().and_then(|c| c.anthropic.as_ref());
    let anthropic_enabled = anthropic_config.and_then(|c| c.enabled).unwrap_or(false);

    if anthropic_enabled || std::env::var("ANTHROPIC_API_KEY").is_ok() {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .or_else(|_| {
                anthropic_config
                    .and_then(|c| c.api_key.clone())
                    .ok_or_else(|| std::env::VarError::NotPresent)
            })
            .unwrap_or_default();

        if !api_key.is_empty() {
            let model = anthropic_config
                .and_then(|c| c.default_model.clone())
                .unwrap_or_else(|| "claude-3-5-sonnet-20241022".to_string());
            let mut adapter = maple_llm::adapters::anthropic::AnthropicAdapter::new(api_key, model);
            if let Some(anthropic) = anthropic_config {
                if let Some(base_url) = &anthropic.base_url {
                    adapter = adapter.with_base_url(base_url.clone());
                }
            }
            router.register_adapter(Box::new(adapter));
        }
    }

    // 注册通义千问适配器
    let qwen_config = llm_config.as_ref().and_then(|c| c.qwen.as_ref());
    let qwen_enabled = qwen_config.and_then(|c| c.enabled).unwrap_or(false);

    if qwen_enabled || std::env::var("QWEN_API_KEY").is_ok() {
        let api_key = std::env::var("QWEN_API_KEY")
            .or_else(|_| {
                qwen_config
                    .and_then(|c| c.api_key.clone())
                    .ok_or_else(|| std::env::VarError::NotPresent)
            })
            .unwrap_or_default();

        if !api_key.is_empty() {
            let mut adapter = maple_llm::adapters::openai_compat::OpenAiCompatAdapter::qwen(api_key);
            if let Some(qwen) = qwen_config {
                if let Some(base_url) = &qwen.base_url {
                    adapter = adapter.with_base_url(base_url.clone());
                }
                if let Some(context_length) = qwen.context_length {
                    adapter = adapter.with_context_length(context_length);
                }
                if let Some(pricing) = &qwen.pricing {
                    adapter = adapter.with_pricing(pricing.input, pricing.output);
                }
            }
            router.register_adapter(Box::new(adapter));
        }
    }

    // 注册智谱GLM适配器
    let glm_config = llm_config.as_ref().and_then(|c| c.glm.as_ref());
    let glm_enabled = glm_config.and_then(|c| c.enabled).unwrap_or(false);

    if glm_enabled || std::env::var("GLM_API_KEY").is_ok() {
        let api_key = std::env::var("GLM_API_KEY")
            .or_else(|_| {
                glm_config
                    .and_then(|c| c.api_key.clone())
                    .ok_or_else(|| std::env::VarError::NotPresent)
            })
            .unwrap_or_default();

        if !api_key.is_empty() {
            let mut adapter = maple_llm::adapters::openai_compat::OpenAiCompatAdapter::glm(api_key);
            if let Some(glm) = glm_config {
                if let Some(base_url) = &glm.base_url {
                    adapter = adapter.with_base_url(base_url.clone());
                }
                if let Some(context_length) = glm.context_length {
                    adapter = adapter.with_context_length(context_length);
                }
                if let Some(pricing) = &glm.pricing {
                    adapter = adapter.with_pricing(pricing.input, pricing.output);
                }
            }
            router.register_adapter(Box::new(adapter));
        }
    }

    // 注册OpenAI适配器
    let openai_config = llm_config.as_ref().and_then(|c| c.openai.as_ref());
    let openai_enabled = openai_config.and_then(|c| c.enabled).unwrap_or(false);

    if openai_enabled || std::env::var("OPENAI_API_KEY").is_ok() {
        let api_key = std::env::var("OPENAI_API_KEY")
            .or_else(|_| {
                openai_config
                    .and_then(|c| c.api_key.clone())
                    .ok_or_else(|| std::env::VarError::NotPresent)
            })
            .unwrap_or_default();

        if !api_key.is_empty() {
            let model = openai_config
                .and_then(|c| c.default_model.clone())
                .unwrap_or_else(|| "gpt-4o-mini".to_string());
            let mut adapter = maple_llm::adapters::openai_compat::OpenAiCompatAdapter::openai(api_key, model);
            if let Some(openai) = openai_config {
                if let Some(base_url) = &openai.base_url {
                    adapter = adapter.with_base_url(base_url.clone());
                }
                if let Some(context_length) = openai.context_length {
                    adapter = adapter.with_context_length(context_length);
                }
                if let Some(pricing) = &openai.pricing {
                    adapter = adapter.with_pricing(pricing.input, pricing.output);
                }
            }
            router.register_adapter(Box::new(adapter));
        }
    }

    // 注册Google Gemini适配器
    let google_config = llm_config.as_ref().and_then(|c| c.google.as_ref());
    let google_enabled = google_config.and_then(|c| c.enabled).unwrap_or(false);

    if google_enabled || std::env::var("GOOGLE_API_KEY").is_ok() {
        let api_key = std::env::var("GOOGLE_API_KEY")
            .or_else(|_| {
                google_config
                    .and_then(|c| c.api_key.clone())
                    .ok_or_else(|| std::env::VarError::NotPresent)
            })
            .unwrap_or_default();

        if !api_key.is_empty() {
            let model = google_config
                .and_then(|c| c.default_model.clone())
                .unwrap_or_else(|| "gemini-1.5-flash".to_string());
            let mut adapter = maple_llm::adapters::openai_compat::OpenAiCompatAdapter::google(api_key, model);
            if let Some(google) = google_config {
                if let Some(base_url) = &google.base_url {
                    adapter = adapter.with_base_url(base_url.clone());
                }
                if let Some(context_length) = google.context_length {
                    adapter = adapter.with_context_length(context_length);
                }
                if let Some(pricing) = &google.pricing {
                    adapter = adapter.with_pricing(pricing.input, pricing.output);
                }
            }
            router.register_adapter(Box::new(adapter));
        }
    }

    // 设置回退链
    let mut fallback = vec!["ollama/qwen2.5:7b".to_string()];

    // 从配置文件加载回退链
    if let Some(llm_config) = &llm_config {
        if let Some(chain) = &llm_config.fallback_chain {
            fallback = chain.clone();
        }
    }

    // 添加已启用的云端模型到回退链
    if std::env::var("DEEPSEEK_API_KEY").is_ok() || deepseek_enabled {
        if !fallback.contains(&"deepseek-chat".to_string()) {
            fallback.push("deepseek-chat".to_string());
        }
    }
    if std::env::var("ANTHROPIC_API_KEY").is_ok() || anthropic_enabled {
        if !fallback.contains(&"claude-3-5-sonnet-20241022".to_string()) {
            fallback.push("claude-3-5-sonnet-20241022".to_string());
        }
    }
    if std::env::var("QWEN_API_KEY").is_ok() || qwen_enabled {
        if !fallback.contains(&"qwen-plus".to_string()) {
            fallback.push("qwen-plus".to_string());
        }
    }
    if std::env::var("GLM_API_KEY").is_ok() || glm_enabled {
        if !fallback.contains(&"glm-4".to_string()) {
            fallback.push("glm-4".to_string());
        }
    }
    if std::env::var("GOOGLE_API_KEY").is_ok() || google_enabled {
        if !fallback.contains(&"gemini-1.5-flash".to_string()) {
            fallback.push("gemini-1.5-flash".to_string());
        }
    }
    router.set_fallback_chain(fallback);

    // 加载路由规则
    let rules_path = std::env::var("ROUTING_RULES_PATH")
        .unwrap_or_else(|_| "infra/routing_rules.yaml".to_string());
    if let Ok(content) = std::fs::read_to_string(&rules_path) {
        if let Ok(yaml) = serde_yaml::from_str::<serde_json::Value>(&content) {
            if let Some(rules_arr) = yaml.get("rules").and_then(|r| r.as_array()) {
                let rules: Vec<maple_llm::router::RoutingRule> = rules_arr.iter()
                    .filter_map(|r| serde_json::from_value(r.clone()).ok())
                    .collect();
                let rules_count = rules.len();
                router.set_routing_rules(rules);
                tracing::info!("Loaded {} routing rules from {}", rules_count, rules_path);
            }
            if let Some(chain) = yaml.get("fallback_chain").and_then(|c| c.as_array()) {
                let chain: Vec<String> = chain.iter()
                    .filter_map(|c| c.as_str().map(|s| s.to_string()))
                    .collect();
                router.set_fallback_chain(chain);
                tracing::info!("Loaded fallback chain from {}", rules_path);
            }
        }
    } else {
        tracing::info!("No routing_rules.yaml found, using default rules");
    }

    // 从配置文件加载路由规则
    if let Some(llm_config) = &llm_config {
        if let Some(rules_config) = &llm_config.routing_rules {
            let rules: Vec<maple_llm::router::RoutingRule> = rules_config.iter()
                .map(|r| maple_llm::router::RoutingRule {
                    name: r.name.clone(),
                    condition: r.condition.clone(),
                    preferred: r.preferred.clone(),
                    fallback_to_cloud: r.fallback_to_cloud.unwrap_or(true),
                })
                .collect();
            let rules_count = rules.len();
            router.set_routing_rules(rules);
            tracing::info!("Loaded {} routing rules from config file", rules_count);
        }
    }

    Arc::new(router)
}
