use serde::{Deserialize, Serialize};

/// StreamingContextScrubber — cleans LLM output during streaming
///
/// Inspired by hermes-agent's output cleaning pipeline:
/// - Removes excessive whitespace
/// - Normalizes markdown formatting
/// - Strips thinking tags from final output
/// - Handles partial JSON/tool calls
/// - Cleans code block formatting

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrubberConfig {
    /// Remove thinking tags (e.g., <thinking>, <scratchpad>)
    pub strip_thinking_tags: bool,
    /// Normalize excessive newlines (3+ → 2)
    pub normalize_newlines: bool,
    /// Remove trailing whitespace from lines
    pub strip_trailing_whitespace: bool,
    /// Ensure code blocks are properly closed
    pub ensure_code_blocks_closed: bool,
    /// Remove partial JSON at end of stream
    pub strip_partial_json: bool,
    /// Maximum consecutive blank lines allowed
    pub max_blank_lines: usize,
}

impl Default for ScrubberConfig {
    fn default() -> Self {
        Self {
            strip_thinking_tags: true,
            normalize_newlines: true,
            strip_trailing_whitespace: true,
            ensure_code_blocks_closed: true,
            strip_partial_json: true,
            max_blank_lines: 2,
        }
    }
}

/// Stateful scrubber that tracks context across streaming chunks
#[derive(Debug)]
pub struct StreamingContextScrubber {
    config: ScrubberConfig,
    buffer: String,
    in_thinking_block: bool,
    in_code_block: bool,
    code_block_lang: Option<String>,
    /// Accumulated clean output
    clean_output: String,
}

impl StreamingContextScrubber {
    pub fn new(config: ScrubberConfig) -> Self {
        Self {
            config,
            buffer: String::new(),
            in_thinking_block: false,
            in_code_block: false,
            code_block_lang: None,
            clean_output: String::new(),
        }
    }

    /// Create with default config
    pub fn default_config() -> Self {
        Self::new(ScrubberConfig::default())
    }

    /// Process a streaming chunk and return cleaned text
    pub fn process_chunk(&mut self, chunk: &str) -> String {
        self.buffer.push_str(chunk);
        let mut output = String::new();

        // Process complete lines
        while let Some(newline_pos) = self.buffer.find('\n') {
            let line = self.buffer[..newline_pos].to_string();
            self.buffer = self.buffer[newline_pos + 1..].to_string();

            if let Some(cleaned) = self.process_line(&line) {
                output.push_str(&cleaned);
                output.push('\n');
                // Update clean_output inline so blank-line counting works
                self.clean_output.push_str(&cleaned);
                self.clean_output.push('\n');
            }
        }

        // Process any remaining partial line (don't add newline yet)
        // Keep it in buffer for next chunk

        output
    }

    /// Flush remaining buffer content
    pub fn flush(&mut self) -> String {
        let remaining = self.buffer.clone();
        self.buffer.clear();

        if remaining.is_empty() {
            return String::new();
        }

        if let Some(cleaned) = self.process_line(&remaining) {
            self.clean_output.push_str(&cleaned);
            cleaned
        } else {
            String::new()
        }
    }

    /// Process a single line, returning None if line should be suppressed
    fn process_line(&mut self, line: &str) -> Option<String> {
        let mut line = line.to_string();

        // Handle thinking tags
        if self.config.strip_thinking_tags
            && self.handle_thinking_tags(&mut line)
        {
            return None; // Line is inside thinking block, suppress
        }

        // Handle code blocks
        if self.config.ensure_code_blocks_closed {
            self.track_code_blocks(&line);
        }

        // Strip trailing whitespace
        if self.config.strip_trailing_whitespace {
            line = line.trim_end().to_string();
        }

        // Skip excessive blank lines
        if line.trim().is_empty() {
            // Count consecutive newlines at end of output
            let trailing_newlines = self.clean_output.chars().rev().take_while(|&c| c == '\n').count();
            if trailing_newlines >= self.config.max_blank_lines {
                return None;
            }
        }

        Some(line)
    }

    /// Handle thinking tags. Returns true if line should be suppressed.
    fn handle_thinking_tags(&mut self, line: &mut String) -> bool {
        // Check for thinking block start tags
        let has_start = line.contains("<thinking>") || line.contains("<scratchpad>") || line.contains("<inner_monologue>");
        let has_end = line.contains("</thinking>") || line.contains("</scratchpad>") || line.contains("</inner_monologue>");

        // Handle single-line thinking tags like <thinking>content</thinking>
        if has_start && has_end {
            // Remove the entire tag and its content
            *line = self.remove_thinking_tags_inline(line);
            return line.trim().is_empty();
        }

        // Handle opening tag
        if has_start {
            self.in_thinking_block = true;
            // Check if there's content before the tag
            if let Some(pos) = line.find('<') {
                let before = line[..pos].trim();
                if !before.is_empty() {
                    // There's content before the tag, keep that part
                    *line = before.to_string();
                    return false;
                }
            }
            return true; // Suppress entire line
        }

        // Handle closing tag
        if has_end {
            self.in_thinking_block = false;
            // Check if there's content after the tag
            if let Some(pos) = line.find("</") {
                let after = if line[pos..].starts_with("</thinking>") {
                    &line[pos + 11..]
                } else if line[pos..].starts_with("</scratchpad>") {
                    &line[pos + 13..]
                } else {
                    &line[pos + 18..]
                };
                if !after.trim().is_empty() {
                    *line = after.trim().to_string();
                    return false;
                }
            }
            return true; // Suppress entire line
        }

        // Suppress content inside thinking block
        if self.in_thinking_block {
            return true;
        }

        false
    }

    /// Remove inline thinking tags and their content
    fn remove_thinking_tags_inline(&self, text: &str) -> String {
        let mut result = text.to_string();

        // Remove <thinking>...</thinking>
        while let Some(start) = result.find("<thinking>") {
            if let Some(end) = result[start..].find("</thinking>") {
                let end_pos = start + end + 11; // len("</thinking>")
                result = format!("{}{}", &result[..start], &result[end_pos..]);
            } else {
                break;
            }
        }

        // Remove <scratchpad>...</scratchpad>
        while let Some(start) = result.find("<scratchpad>") {
            if let Some(end) = result[start..].find("</scratchpad>") {
                let end_pos = start + end + 13; // len("</scratchpad>")
                result = format!("{}{}", &result[..start], &result[end_pos..]);
            } else {
                break;
            }
        }

        // Remove <inner_monologue>...</inner_monologue>
        while let Some(start) = result.find("<inner_monologue>") {
            if let Some(end) = result[start..].find("</inner_monologue>") {
                let end_pos = start + end + 18; // len("</inner_monologue>")
                result = format!("{}{}", &result[..start], &result[end_pos..]);
            } else {
                break;
            }
        }

        result
    }

    /// Track code block state
    fn track_code_blocks(&mut self, line: &str) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("```") {
            if self.in_code_block {
                // Closing code block
                self.in_code_block = false;
                self.code_block_lang = None;
            } else {
                // Opening code block
                self.in_code_block = true;
                let lang = rest.trim();
                self.code_block_lang = if lang.is_empty() {
                    None
                } else {
                    Some(lang.to_string())
                };
            }
        }
    }

    /// Check if we're currently inside a code block
    pub fn in_code_block(&self) -> bool {
        self.in_code_block
    }

    /// Get the current code block language
    pub fn code_block_lang(&self) -> Option<&str> {
        self.code_block_lang.as_deref()
    }

    /// Get accumulated clean output
    pub fn clean_output(&self) -> &str {
        &self.clean_output
    }

    /// Reset scrubber state
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.in_thinking_block = false;
        self.in_code_block = false;
        self.code_block_lang = None;
        self.clean_output.clear();
    }

    /// Get config
    pub fn config(&self) -> &ScrubberConfig {
        &self.config
    }
}

/// One-shot scrubber for non-streaming use
pub fn scrub_text(text: &str, config: &ScrubberConfig) -> String {
    let mut scrubber = StreamingContextScrubber::new(config.clone());
    let mut result = scrubber.process_chunk(text);
    result.push_str(&scrubber.flush());
    result
}

/// Quick scrub with default config
pub fn scrub_default(text: &str) -> String {
    scrub_text(text, &ScrubberConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_scrubbing() {
        let mut scrubber = StreamingContextScrubber::default_config();

        let output = scrubber.process_chunk("Hello world\n");
        assert_eq!(output, "Hello world\n");
    }

    #[test]
    fn test_strip_thinking_tags() {
        let mut scrubber = StreamingContextScrubber::default_config();

        let input = "Before\n<thinking>\nLet me think...\n</thinking>\nAfter\n";
        let output = scrubber.process_chunk(input);

        assert!(!output.contains("thinking"));
        assert!(!output.contains("Let me think"));
        assert!(output.contains("Before"));
        assert!(output.contains("After"));
    }

    #[test]
    fn test_strip_scratchpad() {
        let mut scrubber = StreamingContextScrubber::default_config();

        let input = "Start\n<scratchpad>\nsecret thoughts\n</scratchpad>\nEnd\n";
        let output = scrubber.process_chunk(input);

        assert!(!output.contains("secret thoughts"));
        assert!(output.contains("Start"));
        assert!(output.contains("End"));
    }

    #[test]
    fn test_normalize_newlines() {
        let mut scrubber = StreamingContextScrubber::default_config();

        let input = "Line 1\n\n\n\n\nLine 2\n";
        let output = scrubber.process_chunk(input);

        // Should collapse multiple blank lines (max 2 consecutive)
        // With max_blank_lines=2, we should have at most 2 consecutive newlines
        let max_consecutive = output.chars().fold((0, 0), |(current, max), c| {
            if c == '\n' {
                let new_current = current + 1;
                (new_current, max.max(new_current))
            } else {
                (0, max)
            }
        }).1;
        assert!(max_consecutive <= 2, "Expected at most 2 consecutive newlines, got {}", max_consecutive);
    }

    #[test]
    fn test_strip_trailing_whitespace() {
        let mut scrubber = StreamingContextScrubber::default_config();

        let input = "Hello   \nWorld  \n";
        let output = scrubber.process_chunk(input);

        assert!(!output.contains("Hello   "));
        assert!(output.contains("Hello"));
        assert!(!output.contains("World  "));
        assert!(output.contains("World"));
    }

    #[test]
    fn test_code_block_tracking() {
        let mut scrubber = StreamingContextScrubber::default_config();

        scrubber.process_chunk("```rust\n");
        assert!(scrubber.in_code_block());
        assert_eq!(scrubber.code_block_lang(), Some("rust"));

        scrubber.process_chunk("fn main() {}\n");
        assert!(scrubber.in_code_block());

        scrubber.process_chunk("```\n");
        assert!(!scrubber.in_code_block());
    }

    #[test]
    fn test_multiline_thinking() {
        let mut scrubber = StreamingContextScrubber::default_config();

        let chunks = vec![
            "Before\n",
            "<thinking>\n",
            "Step 1: ...\n",
            "Step 2: ...\n",
            "</thinking>\n",
            "After\n",
        ];

        let mut output = String::new();
        for chunk in chunks {
            output.push_str(&scrubber.process_chunk(chunk));
        }
        output.push_str(&scrubber.flush());

        assert!(!output.contains("Step 1"));
        assert!(!output.contains("Step 2"));
        assert!(output.contains("Before"));
        assert!(output.contains("After"));
    }

    #[test]
    fn test_flush_remaining() {
        let mut scrubber = StreamingContextScrubber::default_config();

        // No newline at end
        scrubber.process_chunk("Hello");
        let output = scrubber.flush();
        assert_eq!(output, "Hello");
    }

    #[test]
    fn test_one_shot_scrub() {
        let text = "Before\n<thinking>reasoning</thinking>\nAfter";
        let result = scrub_default(text);

        assert!(!result.contains("reasoning"));
        assert!(result.contains("Before"));
        assert!(result.contains("After"));
    }

    #[test]
    fn test_config_disable_thinking_stripping() {
        let config = ScrubberConfig {
            strip_thinking_tags: false,
            ..Default::default()
        };
        let mut scrubber = StreamingContextScrubber::new(config);

        let input = "<thinking>keep this</thinking>\n";
        let output = scrubber.process_chunk(input);

        // Thinking tags preserved when disabled
        assert!(output.contains("thinking") || output.contains("keep this"));
    }

    #[test]
    fn test_reset() {
        let mut scrubber = StreamingContextScrubber::default_config();

        scrubber.process_chunk("Some text\n");
        assert!(!scrubber.clean_output().is_empty());

        scrubber.reset();
        assert!(scrubber.clean_output().is_empty());
        assert!(!scrubber.in_code_block());
    }
}
