use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ProviderProfile — declarative LLM provider configuration
///
/// Inspired by hermes-agent's provider config system.
/// Each profile describes a provider's capabilities, models, and connection details.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderProfile {
    /// Unique provider identifier (e.g., "openai", "anthropic", "ollama")
    pub id: String,
    /// Human-readable display name
    pub name: String,
    /// Provider type
    pub provider_type: ProviderType,
    /// API endpoint configuration
    pub endpoint: EndpointConfig,
    /// Available models from this provider
    pub models: Vec<ModelConfig>,
    /// Default model to use when none specified
    pub default_model: Option<String>,
    /// Provider-level capabilities
    pub capabilities: ProviderCapabilities,
    /// Rate limiting configuration
    pub rate_limit: Option<RateLimitConfig>,
    /// Cost configuration (per 1k tokens)
    pub cost: CostConfig,
    /// Whether this provider handles sensitive data locally
    pub privacy_level: PrivacySupport,
    /// Custom headers or metadata
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    /// OpenAI-compatible API (OpenAI, DeepSeek, GLM, etc.)
    OpenAiCompatible,
    /// Anthropic Messages API
    Anthropic,
    /// Ollama local API
    Ollama,
    /// Custom API with adapter
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointConfig {
    /// Base URL for API calls
    pub base_url: String,
    /// API key (can be env var reference: "${OPENAI_API_KEY}")
    pub api_key: Option<String>,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Maximum retries on failure
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Model identifier (e.g., "gpt-4o", "claude-3-5-sonnet")
    pub id: String,
    /// Display name
    pub name: String,
    /// Maximum context window
    pub context_length: usize,
    /// Supports vision/images
    pub supports_vision: bool,
    /// Supports function/tool calling
    pub supports_function_calling: bool,
    /// Supports streaming
    pub supports_streaming: bool,
    /// Cost per 1k input tokens
    pub cost_input_per_1k: f64,
    /// Cost per 1k output tokens
    pub cost_output_per_1k: f64,
    /// Maximum output tokens
    pub max_output_tokens: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// Supports streaming responses
    pub streaming: bool,
    /// Supports function/tool calling
    pub function_calling: bool,
    /// Supports image/vision inputs
    pub vision: bool,
    /// Supports batch processing
    pub batch: bool,
    /// Supports embeddings
    pub embeddings: bool,
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            streaming: true,
            function_calling: true,
            vision: false,
            batch: false,
            embeddings: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Requests per minute
    pub requests_per_minute: Option<u32>,
    /// Tokens per minute
    pub tokens_per_minute: Option<u32>,
    /// Concurrent requests limit
    pub max_concurrent: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostConfig {
    /// Default cost per 1k input tokens
    pub input_per_1k: f64,
    /// Default cost per 1k output tokens
    pub output_per_1k: f64,
    /// Daily budget limit (0 = unlimited)
    pub daily_budget: f64,
}

impl Default for CostConfig {
    fn default() -> Self {
        Self {
            input_per_1k: 0.0,
            output_per_1k: 0.0,
            daily_budget: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PrivacySupport {
    /// All data stays local (Ollama, local models)
    LocalOnly,
    /// Data may be sent to cloud APIs
    Cloud,
    /// Supports both local and cloud routing
    Hybrid,
}

impl ProviderProfile {
    /// Create a profile for OpenAI-compatible providers
    pub fn openai_compatible(id: &str, name: &str, base_url: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            provider_type: ProviderType::OpenAiCompatible,
            endpoint: EndpointConfig {
                base_url: base_url.to_string(),
                api_key: None,
                timeout_secs: 120,
                max_retries: 3,
            },
            models: Vec::new(),
            default_model: None,
            capabilities: ProviderCapabilities::default(),
            rate_limit: None,
            cost: CostConfig::default(),
            privacy_level: PrivacySupport::Cloud,
            metadata: HashMap::new(),
        }
    }

    /// Create a profile for Ollama
    pub fn ollama(base_url: &str) -> Self {
        Self {
            id: "ollama".to_string(),
            name: "Ollama (Local)".to_string(),
            provider_type: ProviderType::Ollama,
            endpoint: EndpointConfig {
                base_url: base_url.to_string(),
                api_key: None,
                timeout_secs: 300,
                max_retries: 1,
            },
            models: Vec::new(),
            default_model: None,
            capabilities: ProviderCapabilities {
                streaming: true,
                function_calling: true,
                vision: false,
                batch: false,
                embeddings: true,
            },
            rate_limit: None,
            cost: CostConfig {
                input_per_1k: 0.0,
                output_per_1k: 0.0,
                daily_budget: 0.0,
            },
            privacy_level: PrivacySupport::LocalOnly,
            metadata: HashMap::new(),
        }
    }

    /// Resolve API key from environment if it's a reference like "${VAR}"
    pub fn resolve_api_key(&mut self) {
        if let Some(ref key) = self.endpoint.api_key.clone()
            && key.starts_with("${")
            && key.ends_with('}')
        {
            let env_var = &key[2..key.len() - 1];
            self.endpoint.api_key = std::env::var(env_var).ok();
        }
    }

    /// Find a model by ID
    pub fn find_model(&self, model_id: &str) -> Option<&ModelConfig> {
        self.models.iter().find(|m| m.id == model_id)
    }

    /// Get the effective model to use
    pub fn effective_model(&self, requested: &str) -> Option<&ModelConfig> {
        if requested.is_empty() || requested == "default" || requested == "auto" {
            self.default_model
                .as_ref()
                .and_then(|id| self.find_model(id))
        } else {
            self.find_model(requested)
        }
    }

    /// Validate the profile configuration
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.id.is_empty() {
            errors.push("Provider ID is required".to_string());
        }

        if self.endpoint.base_url.is_empty() {
            errors.push("Endpoint base_url is required".to_string());
        }

        if self.models.is_empty() {
            errors.push("At least one model must be configured".to_string());
        }

        if let Some(ref default) = self.default_model
            && !self.models.iter().any(|m| &m.id == default)
        {
            errors.push(format!("Default model '{}' not found in models list", default));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Collection of provider profiles with lookup capabilities
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderRegistry {
    pub profiles: HashMap<String, ProviderProfile>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            profiles: HashMap::new(),
        }
    }

    /// Register a provider profile
    pub fn register(&mut self, profile: ProviderProfile) {
        self.profiles.insert(profile.id.clone(), profile);
    }

    /// Get a profile by ID
    pub fn get(&self, id: &str) -> Option<&ProviderProfile> {
        self.profiles.get(id)
    }

    /// Get all profiles that support a specific capability
    pub fn find_with_capability(&self, needs_vision: bool, needs_function_calling: bool) -> Vec<&ProviderProfile> {
        self.profiles
            .values()
            .filter(|p| {
                (!needs_vision || p.capabilities.vision)
                    && (!needs_function_calling || p.capabilities.function_calling)
            })
            .collect()
    }

    /// Get all local-only providers (for sensitive data)
    pub fn local_providers(&self) -> Vec<&ProviderProfile> {
        self.profiles
            .values()
            .filter(|p| p.privacy_level == PrivacySupport::LocalOnly)
            .collect()
    }

    /// Load profiles from YAML string
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    /// Serialize to YAML
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self)
    }

    /// Validate all profiles
    pub fn validate_all(&self) -> HashMap<String, Result<(), Vec<String>>> {
        self.profiles
            .iter()
            .map(|(id, profile)| (id.clone(), profile.validate()))
            .collect()
    }
}

/// Create a registry with all built-in provider profiles
pub fn builtin_providers() -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();

    // OpenAI
    let mut openai = ProviderProfile::openai_compatible(
        "openai",
        "OpenAI",
        "https://api.openai.com/v1",
    );
    openai.endpoint.api_key = Some("${OPENAI_API_KEY}".to_string());
    openai.models.push(ModelConfig {
        id: "gpt-4o".to_string(),
        name: "GPT-4o".to_string(),
        context_length: 128_000,
        supports_vision: true,
        supports_function_calling: true,
        supports_streaming: true,
        cost_input_per_1k: 0.005,
        cost_output_per_1k: 0.015,
        max_output_tokens: Some(4096),
    });
    openai.default_model = Some("gpt-4o".to_string());
    openai.capabilities.vision = true;
    openai.capabilities.embeddings = true;
    registry.register(openai);

    // Anthropic
    let anthropic = ProviderProfile {
        id: "anthropic".to_string(),
        name: "Anthropic".to_string(),
        provider_type: ProviderType::Anthropic,
        endpoint: EndpointConfig {
            base_url: "https://api.anthropic.com".to_string(),
            api_key: Some("${ANTHROPIC_API_KEY}".to_string()),
            timeout_secs: 120,
            max_retries: 3,
        },
        models: vec![ModelConfig {
            id: "claude-3-5-sonnet".to_string(),
            name: "Claude 3.5 Sonnet".to_string(),
            context_length: 200_000,
            supports_vision: true,
            supports_function_calling: true,
            supports_streaming: true,
            cost_input_per_1k: 0.003,
            cost_output_per_1k: 0.015,
            max_output_tokens: Some(4096),
        }],
        default_model: Some("claude-3-5-sonnet".to_string()),
        capabilities: ProviderCapabilities {
            streaming: true,
            function_calling: true,
            vision: true,
            batch: true,
            embeddings: false,
        },
        rate_limit: None,
        cost: CostConfig::default(),
        privacy_level: PrivacySupport::Cloud,
        metadata: HashMap::new(),
    };
    registry.register(anthropic);

    // Ollama (local)
    let mut ollama = ProviderProfile::ollama("http://localhost:11434");
    ollama.models.push(ModelConfig {
        id: "qwen2.5:7b".to_string(),
        name: "Qwen 2.5 7B".to_string(),
        context_length: 32_000,
        supports_vision: false,
        supports_function_calling: true,
        supports_streaming: true,
        cost_input_per_1k: 0.0,
        cost_output_per_1k: 0.0,
        max_output_tokens: Some(4096),
    });
    ollama.default_model = Some("qwen2.5:7b".to_string());
    registry.register(ollama);

    // DeepSeek
    let mut deepseek = ProviderProfile::openai_compatible(
        "deepseek",
        "DeepSeek",
        "https://api.deepseek.com",
    );
    deepseek.endpoint.api_key = Some("${DEEPSEEK_API_KEY}".to_string());
    deepseek.models.push(ModelConfig {
        id: "deepseek-chat".to_string(),
        name: "DeepSeek Chat".to_string(),
        context_length: 128_000,
        supports_vision: false,
        supports_function_calling: true,
        supports_streaming: true,
        cost_input_per_1k: 0.0014,
        cost_output_per_1k: 0.0028,
        max_output_tokens: Some(4096),
    });
    deepseek.default_model = Some("deepseek-chat".to_string());
    registry.register(deepseek);

    // Qwen (通义千问)
    let mut qwen = ProviderProfile::openai_compatible(
        "qwen",
        "Qwen (通义千问)",
        "https://dashscope.aliyuncs.com/compatible-mode",
    );
    qwen.endpoint.api_key = Some("${QWEN_API_KEY}".to_string());
    qwen.models.push(ModelConfig {
        id: "qwen-plus".to_string(),
        name: "Qwen Plus".to_string(),
        context_length: 128_000,
        supports_vision: false,
        supports_function_calling: true,
        supports_streaming: true,
        cost_input_per_1k: 0.0008,
        cost_output_per_1k: 0.002,
        max_output_tokens: Some(4096),
    });
    qwen.default_model = Some("qwen-plus".to_string());
    registry.register(qwen);

    // GLM (智谱)
    let mut glm = ProviderProfile::openai_compatible(
        "glm",
        "GLM (智谱)",
        "https://open.bigmodel.cn/api/paas",
    );
    glm.endpoint.api_key = Some("${GLM_API_KEY}".to_string());
    glm.models.push(ModelConfig {
        id: "glm-4".to_string(),
        name: "GLM-4".to_string(),
        context_length: 128_000,
        supports_vision: false,
        supports_function_calling: true,
        supports_streaming: true,
        cost_input_per_1k: 0.001,
        cost_output_per_1k: 0.001,
        max_output_tokens: Some(4096),
    });
    glm.default_model = Some("glm-4".to_string());
    registry.register(glm);

    // Google Gemini
    let mut google = ProviderProfile::openai_compatible(
        "google",
        "Google Gemini",
        "https://generativelanguage.googleapis.com/v1beta/openai",
    );
    google.endpoint.api_key = Some("${GOOGLE_API_KEY}".to_string());
    google.models.push(ModelConfig {
        id: "gemini-2.0-flash".to_string(),
        name: "Gemini 2.0 Flash".to_string(),
        context_length: 1_000_000,
        supports_vision: true,
        supports_function_calling: true,
        supports_streaming: true,
        cost_input_per_1k: 0.00125,
        cost_output_per_1k: 0.005,
        max_output_tokens: Some(4096),
    });
    google.default_model = Some("gemini-2.0-flash".to_string());
    google.capabilities.vision = true;
    registry.register(google);

    // Mistral
    let mut mistral = ProviderProfile::openai_compatible(
        "mistral",
        "Mistral AI",
        "https://api.mistral.ai",
    );
    mistral.endpoint.api_key = Some("${MISTRAL_API_KEY}".to_string());
    mistral.models.push(ModelConfig {
        id: "mistral-large".to_string(),
        name: "Mistral Large".to_string(),
        context_length: 128_000,
        supports_vision: false,
        supports_function_calling: true,
        supports_streaming: true,
        cost_input_per_1k: 0.002,
        cost_output_per_1k: 0.006,
        max_output_tokens: Some(4096),
    });
    mistral.default_model = Some("mistral-large".to_string());
    registry.register(mistral);

    // Groq
    let mut groq = ProviderProfile::openai_compatible(
        "groq",
        "Groq",
        "https://api.groq.com/openai",
    );
    groq.endpoint.api_key = Some("${GROQ_API_KEY}".to_string());
    groq.models.push(ModelConfig {
        id: "llama-3.3-70b".to_string(),
        name: "Llama 3.3 70B".to_string(),
        context_length: 32_000,
        supports_vision: false,
        supports_function_calling: true,
        supports_streaming: true,
        cost_input_per_1k: 0.0005,
        cost_output_per_1k: 0.0008,
        max_output_tokens: Some(4096),
    });
    groq.default_model = Some("llama-3.3-70b".to_string());
    registry.register(groq);

    // Moonshot (月之暗面)
    let mut moonshot = ProviderProfile::openai_compatible(
        "moonshot",
        "Moonshot (月之暗面)",
        "https://api.moonshot.cn",
    );
    moonshot.endpoint.api_key = Some("${MOONSHOT_API_KEY}".to_string());
    moonshot.models.push(ModelConfig {
        id: "moonshot-v1-128k".to_string(),
        name: "Moonshot V1 128K".to_string(),
        context_length: 128_000,
        supports_vision: false,
        supports_function_calling: true,
        supports_streaming: true,
        cost_input_per_1k: 0.006,
        cost_output_per_1k: 0.012,
        max_output_tokens: Some(4096),
    });
    moonshot.default_model = Some("moonshot-v1-128k".to_string());
    registry.register(moonshot);

    // Yi (零一万物)
    let mut yi = ProviderProfile::openai_compatible(
        "yi",
        "Yi (零一万物)",
        "https://api.lingyiwanwu.com",
    );
    yi.endpoint.api_key = Some("${YI_API_KEY}".to_string());
    yi.models.push(ModelConfig {
        id: "yi-large".to_string(),
        name: "Yi Large".to_string(),
        context_length: 200_000,
        supports_vision: false,
        supports_function_calling: true,
        supports_streaming: true,
        cost_input_per_1k: 0.006,
        cost_output_per_1k: 0.006,
        max_output_tokens: Some(4096),
    });
    yi.default_model = Some("yi-large".to_string());
    registry.register(yi);

    // Baichuan (百川)
    let mut baichuan = ProviderProfile::openai_compatible(
        "baichuan",
        "Baichuan (百川)",
        "https://api.baichuan-ai.com",
    );
    baichuan.endpoint.api_key = Some("${BAICHUAN_API_KEY}".to_string());
    baichuan.models.push(ModelConfig {
        id: "Baichuan4".to_string(),
        name: "Baichuan 4".to_string(),
        context_length: 32_000,
        supports_vision: false,
        supports_function_calling: true,
        supports_streaming: true,
        cost_input_per_1k: 0.001,
        cost_output_per_1k: 0.002,
        max_output_tokens: Some(4096),
    });
    baichuan.default_model = Some("Baichuan4".to_string());
    registry.register(baichuan);

    // Minimax
    let mut minimax = ProviderProfile::openai_compatible(
        "minimax",
        "Minimax",
        "https://api.minimax.chat",
    );
    minimax.endpoint.api_key = Some("${MINIMAX_API_KEY}".to_string());
    minimax.models.push(ModelConfig {
        id: "abab6.5s-chat".to_string(),
        name: "Abab 6.5s".to_string(),
        context_length: 32_000,
        supports_vision: false,
        supports_function_calling: true,
        supports_streaming: true,
        cost_input_per_1k: 0.001,
        cost_output_per_1k: 0.001,
        max_output_tokens: Some(4096),
    });
    minimax.default_model = Some("abab6.5s-chat".to_string());
    registry.register(minimax);

    // Stepfun (阶跃星辰)
    let mut stepfun = ProviderProfile::openai_compatible(
        "stepfun",
        "Stepfun (阶跃星辰)",
        "https://api.stepfun.com",
    );
    stepfun.endpoint.api_key = Some("${STEPFUN_API_KEY}".to_string());
    stepfun.models.push(ModelConfig {
        id: "step-2-16k".to_string(),
        name: "Step 2 16K".to_string(),
        context_length: 256_000,
        supports_vision: false,
        supports_function_calling: true,
        supports_streaming: true,
        cost_input_per_1k: 0.004,
        cost_output_per_1k: 0.008,
        max_output_tokens: Some(4096),
    });
    stepfun.default_model = Some("step-2-16k".to_string());
    registry.register(stepfun);

    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_model() -> ModelConfig {
        ModelConfig {
            id: "gpt-4o".to_string(),
            name: "GPT-4o".to_string(),
            context_length: 128_000,
            supports_vision: true,
            supports_function_calling: true,
            supports_streaming: true,
            cost_input_per_1k: 0.005,
            cost_output_per_1k: 0.015,
            max_output_tokens: Some(4096),
        }
    }

    fn sample_profile() -> ProviderProfile {
        let mut profile = ProviderProfile::openai_compatible(
            "openai",
            "OpenAI",
            "https://api.openai.com/v1",
        );
        profile.endpoint.api_key = Some("${OPENAI_API_KEY}".to_string());
        profile.models.push(sample_model());
        profile.default_model = Some("gpt-4o".to_string());
        profile.capabilities.vision = true;
        profile
    }

    #[test]
    fn test_provider_profile_creation() {
        let profile = sample_profile();
        assert_eq!(profile.id, "openai");
        assert_eq!(profile.provider_type, ProviderType::OpenAiCompatible);
        assert_eq!(profile.models.len(), 1);
    }

    #[test]
    fn test_ollama_profile() {
        let profile = ProviderProfile::ollama("http://localhost:11434");
        assert_eq!(profile.id, "ollama");
        assert_eq!(profile.provider_type, ProviderType::Ollama);
        assert_eq!(profile.privacy_level, PrivacySupport::LocalOnly);
        assert_eq!(profile.cost.input_per_1k, 0.0);
    }

    #[test]
    fn test_find_model() {
        let profile = sample_profile();
        assert!(profile.find_model("gpt-4o").is_some());
        assert!(profile.find_model("gpt-3.5").is_none());
    }

    #[test]
    fn test_effective_model() {
        let profile = sample_profile();

        // Explicit model
        assert!(profile.effective_model("gpt-4o").is_some());

        // Default model
        assert!(profile.effective_model("").is_some());
        assert!(profile.effective_model("default").is_some());
        assert!(profile.effective_model("auto").is_some());

        // Missing model
        assert!(profile.effective_model("nonexistent").is_none());
    }

    #[test]
    fn test_profile_validation() {
        let profile = sample_profile();
        assert!(profile.validate().is_ok());

        // Missing ID
        let mut invalid = sample_profile();
        invalid.id = String::new();
        assert!(invalid.validate().is_err());

        // Missing models
        let mut invalid = sample_profile();
        invalid.models.clear();
        assert!(invalid.validate().is_err());

        // Invalid default model
        let mut invalid = sample_profile();
        invalid.default_model = Some("nonexistent".to_string());
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_provider_registry() {
        let mut registry = ProviderRegistry::new();
        registry.register(sample_profile());
        registry.register(ProviderProfile::ollama("http://localhost:11434"));

        assert_eq!(registry.profiles.len(), 2);
        assert!(registry.get("openai").is_some());
        assert!(registry.get("ollama").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_find_with_capability() {
        let mut registry = ProviderRegistry::new();
        registry.register(sample_profile());
        registry.register(ProviderProfile::ollama("http://localhost:11434"));

        // Vision-capable providers
        let vision_providers = registry.find_with_capability(true, false);
        assert_eq!(vision_providers.len(), 1);
        assert_eq!(vision_providers[0].id, "openai");

        // All providers
        let all = registry.find_with_capability(false, false);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_local_providers() {
        let mut registry = ProviderRegistry::new();
        registry.register(sample_profile());
        registry.register(ProviderProfile::ollama("http://localhost:11434"));

        let local = registry.local_providers();
        assert_eq!(local.len(), 1);
        assert_eq!(local[0].id, "ollama");
    }

    #[test]
    fn test_yaml_serialization() {
        let mut registry = ProviderRegistry::new();
        registry.register(sample_profile());

        let yaml = registry.to_yaml().unwrap();
        assert!(yaml.contains("openai"));
        assert!(yaml.contains("gpt-4o"));

        let deserialized = ProviderRegistry::from_yaml(&yaml).unwrap();
        assert_eq!(deserialized.profiles.len(), 1);
        assert!(deserialized.get("openai").is_some());
    }

    #[test]
    fn test_resolve_api_key() {
        let mut profile = sample_profile();
        // Note: set_var is unsafe in edition 2024, so we test with a non-env pattern
        profile.endpoint.api_key = Some("sk-direct-key".to_string());

        profile.resolve_api_key();
        assert_eq!(profile.endpoint.api_key, Some("sk-direct-key".to_string()));
    }

    #[test]
    fn test_validate_all() {
        let mut registry = ProviderRegistry::new();
        registry.register(sample_profile());

        let mut ollama = ProviderProfile::ollama("http://localhost:11434");
        ollama.models.push(ModelConfig {
            id: "qwen2.5:7b".to_string(),
            name: "Qwen 2.5 7B".to_string(),
            context_length: 32_000,
            supports_vision: false,
            supports_function_calling: true,
            supports_streaming: true,
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            max_output_tokens: Some(4096),
        });
        ollama.default_model = Some("qwen2.5:7b".to_string());
        registry.register(ollama);

        let results = registry.validate_all();
        assert!(results["openai"].is_ok());
        assert!(results["ollama"].is_ok());
    }

    #[test]
    fn test_builtin_providers() {
        let registry = builtin_providers();

        // Should have 14 providers
        assert_eq!(registry.profiles.len(), 14);

        // Verify all providers are present
        let expected_ids = vec![
            "openai", "anthropic", "ollama", "deepseek", "qwen", "glm",
            "google", "mistral", "groq", "moonshot", "yi", "baichuan",
            "minimax", "stepfun",
        ];
        for id in expected_ids {
            assert!(registry.get(id).is_some(), "Missing provider: {}", id);
        }

        // Verify local providers
        let local = registry.local_providers();
        assert_eq!(local.len(), 1);
        assert_eq!(local[0].id, "ollama");

        // Verify vision-capable providers
        let vision = registry.find_with_capability(true, false);
        assert!(vision.len() >= 3); // openai, anthropic, google

        // Validate all profiles
        let results = registry.validate_all();
        for (id, result) in &results {
            assert!(result.is_err() || result.is_ok(), "Validation failed for {}", id);
            // All builtin providers should have valid configs
            if let Err(errors) = result {
                panic!("Provider {} has validation errors: {:?}", id, errors);
            }
        }
    }
}
