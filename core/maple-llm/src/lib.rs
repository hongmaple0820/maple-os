#![allow(clippy::all)]
pub mod adapters;
pub mod embedding;
pub mod error;
pub mod iteration_budget;
pub mod mock_llm;
pub mod provider_profile;
pub mod request;
pub mod response;
pub mod router;
pub mod stream;
pub mod token_counter;
pub mod usage;

pub use embedding::{Embedder, FallbackEmbedder, OllamaEmbedder, simple_embedding};
pub use error::{ClassifiedError, LlmError};
pub use iteration_budget::{BudgetDecision, BudgetState, BudgetWarning, IterationBudget, IterationBudgetBuilder};
pub use mock_llm::{
    MockLlmAdapter, MockParityHarness, MockResponse, MockResponses,
    ParityError, ParityReport, ParityTestCase, RequestMatcher,
};
pub use maple_macro::tool;
pub use provider_profile::{
    builtin_providers, CostConfig, EndpointConfig, ModelConfig, PrivacySupport,
    ProviderCapabilities, ProviderProfile, ProviderRegistry, ProviderType, RateLimitConfig,
};
pub use request::{LlmRequest, Priority, PrivacyLevel, TaskType, ToolDefinition};
pub use response::{LlmResponse, ParsedToolCall};
pub use router::{LlmRouter, ModelDescriptor};
pub use token_counter::{
    SimpleTokenCounter, TiktokenCounter, TokenCounter, count_message_tokens, count_tokens,
};
pub use usage::{UsageTracker, UsageSnapshot};
