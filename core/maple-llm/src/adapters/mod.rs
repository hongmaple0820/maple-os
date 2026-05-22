pub mod anthropic;
pub mod openai_compat;
pub mod ollama;

pub use anthropic::AnthropicAdapter;
pub use openai_compat::OpenAiCompatAdapter;
pub use ollama::OllamaAdapter;
