pub mod anthropic;
pub mod ollama;
pub mod openai_compat;

pub use anthropic::AnthropicAdapter;
pub use ollama::OllamaAdapter;
pub use openai_compat::OpenAiCompatAdapter;
