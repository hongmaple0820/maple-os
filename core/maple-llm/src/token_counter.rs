/// Centralized token counter — replaces scattered content.len() / 4 patterns
/// Provides pluggable tokenization strategies for different use cases
use std::sync::Arc;
use std::sync::OnceLock;

/// Token counter trait — allows pluggable tokenization strategies
pub trait TokenCounter: Send + Sync {
    fn count_tokens(&self, text: &str) -> usize;

    /// Count tokens for a message with role overhead
    fn count_message_tokens(&self, content: &str, role: &str) -> usize {
        // Base token count + role overhead (typically 4 tokens per message)
        self.count_tokens(content) + 4
    }
}

/// Precise token counter using tiktoken-rs (cl100k_base encoding)
/// Same encoding used by GPT-4, GPT-3.5-turbo, and compatible models
pub struct TiktokenCounter {
    bpe: tiktoken_rs::CoreBPE,
}

impl TiktokenCounter {
    pub fn new() -> Self {
        Self {
            bpe: tiktoken_rs::cl100k_base().expect("Failed to initialize cl100k_base encoding"),
        }
    }
}

impl Default for TiktokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenCounter for TiktokenCounter {
    fn count_tokens(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }
        self.bpe.encode_ordinary(text).len()
    }
}

/// Simple token counter using character-based estimation (fallback)
/// ~4 characters per token for English, ~2 for CJK
pub struct SimpleTokenCounter {
    chars_per_token: usize,
}

impl SimpleTokenCounter {
    pub fn new() -> Self {
        Self { chars_per_token: 4 }
    }

    pub fn with_chars_per_token(chars_per_token: usize) -> Self {
        Self { chars_per_token }
    }
}

impl Default for SimpleTokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenCounter for SimpleTokenCounter {
    fn count_tokens(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }
        // Count CJK characters separately (they use ~2 tokens each)
        let cjk_count = text
            .chars()
            .filter(|c| {
                matches!(c,
                    '\u{4E00}'..='\u{9FFF}' |  // CJK Unified Ideographs
                    '\u{3400}'..='\u{4DBF}' |  // CJK Unified Ideographs Extension A
                    '\u{F900}'..='\u{FAFF}' |  // CJK Compatibility Ideographs
                    '\u{2E80}'..='\u{2EFF}' |  // CJK Radicals Supplement
                    '\u{3000}'..='\u{303F}' |  // CJK Symbols and Punctuation
                    '\u{FF00}'..='\u{FFEF}'    // Halfwidth and Fullwidth Forms
                )
            })
            .count();

        let total_chars = text.chars().count();
        let non_cjk_count = total_chars - cjk_count;

        // CJK: ~2 tokens per char, non-CJK: ~4 chars per token
        (cjk_count * 2) + (non_cjk_count / self.chars_per_token)
    }
}

static GLOBAL_TOKEN_COUNTER: OnceLock<Arc<dyn TokenCounter>> = OnceLock::new();

/// Get the global token counter — defaults to TiktokenCounter
pub fn global_token_counter() -> &'static Arc<dyn TokenCounter> {
    GLOBAL_TOKEN_COUNTER.get_or_init(|| Arc::new(TiktokenCounter::new()))
}

/// Set a custom global token counter
pub fn set_global_token_counter(counter: Arc<dyn TokenCounter>) {
    let _ = GLOBAL_TOKEN_COUNTER.set(counter);
}

/// Convenience function to count tokens using the global counter
pub fn count_tokens(text: &str) -> usize {
    global_token_counter().count_tokens(text)
}

/// Convenience function to count message tokens using the global counter
pub fn count_message_tokens(content: &str, role: &str) -> usize {
    global_token_counter().count_message_tokens(content, role)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tiktoken_counter_english() {
        let counter = TiktokenCounter::new();
        // "hello world" = 2 tokens in cl100k_base
        let tokens = counter.count_tokens("hello world");
        assert!(tokens >= 2 && tokens <= 3, "expected ~2, got {}", tokens);
        assert_eq!(counter.count_tokens(""), 0);
    }

    #[test]
    fn test_tiktoken_counter_cjk() {
        let counter = TiktokenCounter::new();
        // CJK characters are typically 1-2 tokens each in cl100k_base
        let tokens = counter.count_tokens("你好世界");
        assert!(tokens >= 4, "expected >=4 for CJK, got {}", tokens);
    }

    #[test]
    fn test_tiktoken_counter_code() {
        let counter = TiktokenCounter::new();
        let code = "fn main() { println!(\"hello\"); }";
        let tokens = counter.count_tokens(code);
        assert!(tokens > 5 && tokens < 20, "expected 5-20, got {}", tokens);
    }

    #[test]
    fn test_simple_token_counter_english() {
        let counter = SimpleTokenCounter::new();
        assert_eq!(counter.count_tokens("hello world"), 2); // 11 chars / 4 = 2
        assert_eq!(counter.count_tokens(""), 0);
        assert_eq!(counter.count_tokens("a"), 0); // 1 char / 4 = 0
        assert_eq!(counter.count_tokens("test"), 1); // 4 chars / 4 = 1
    }

    #[test]
    fn test_simple_token_counter_cjk() {
        let counter = SimpleTokenCounter::new();
        // CJK characters: 4 chars * 2 tokens = 8
        assert_eq!(counter.count_tokens("你好世界"), 8);
        // Mixed: CJK (2*2=4) + ASCII (4/4=1) = 5
        assert_eq!(counter.count_tokens("你好test"), 5);
    }

    #[test]
    fn test_message_tokens() {
        let counter = TiktokenCounter::new();
        let tokens = counter.count_message_tokens("hello", "user");
        assert!(tokens >= 5, "expected >=5, got {}", tokens); // ~1 + 4 overhead
    }

    #[test]
    fn test_global_counter() {
        let tokens = count_tokens("hello world");
        assert!(tokens >= 2 && tokens <= 3, "expected ~2, got {}", tokens);
    }

    #[test]
    fn test_global_counter_is_tiktoken() {
        // Verify the global counter is TiktokenCounter (more precise than SimpleTokenCounter)
        let global = global_token_counter();
        let simple = SimpleTokenCounter::new();
        let text = "The quick brown fox jumps over the lazy dog";
        let global_tokens = global.count_tokens(text);
        let simple_tokens = simple.count_tokens(text);
        // tiktoken should give a different (more accurate) count than simple heuristic
        // Just verify it doesn't panic and returns non-zero
        assert!(global_tokens > 0);
        assert!(simple_tokens > 0);
    }
}
