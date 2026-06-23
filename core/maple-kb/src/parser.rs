use anyhow::Result;

/// Document parser trait — extracts plain text from binary document formats
pub trait DocumentParser: Send + Sync {
    fn supported_extensions(&self) -> &[&str];
    fn parse(&self, bytes: &[u8]) -> Result<String>;
}

/// Plain text / Markdown parser (passthrough)
pub struct TextParser;

impl DocumentParser for TextParser {
    fn supported_extensions(&self) -> &[&str] {
        &["txt", "md", "markdown", "json", "yaml", "yml", "toml", "csv", "log"]
    }

    fn parse(&self, bytes: &[u8]) -> Result<String> {
        Ok(String::from_utf8_lossy(bytes).to_string())
    }
}

/// PDF text extraction parser
pub struct PdfParser;

impl DocumentParser for PdfParser {
    fn supported_extensions(&self) -> &[&str] {
        &["pdf"]
    }

    fn parse(&self, bytes: &[u8]) -> Result<String> {
        let text = pdf_extract::extract_text_from_mem(bytes)?;
        Ok(text)
    }
}

/// Composite parser that dispatches by file extension
pub struct DocumentParserRegistry {
    parsers: Vec<Box<dyn DocumentParser>>,
}

impl Default for DocumentParserRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentParserRegistry {
    pub fn new() -> Self {
        Self {
            parsers: vec![
                Box::new(PdfParser),
                Box::new(TextParser),
            ],
        }
    }

    pub fn parse_by_extension(&self, filename: &str, bytes: &[u8]) -> Result<String> {
        let ext = filename
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase();

        for parser in &self.parsers {
            if parser.supported_extensions().contains(&ext.as_str()) {
                return parser.parse(bytes);
            }
        }

        // Fallback: treat as plain text
        Ok(String::from_utf8_lossy(bytes).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_parser() {
        let parser = TextParser;
        let result = parser.parse(b"hello world").unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_text_parser_utf8_lossy() {
        let parser = TextParser;
        let bytes = vec![0xFF, 0xFE, b'h', b'i'];
        let result = parser.parse(&bytes).unwrap();
        assert!(result.contains("hi"));
    }

    #[test]
    fn test_registry_by_extension() {
        let registry = DocumentParserRegistry::new();
        let result = registry.parse_by_extension("readme.md", b"# Hello").unwrap();
        assert_eq!(result, "# Hello");
    }

    #[test]
    fn test_registry_unknown_extension_fallback() {
        let registry = DocumentParserRegistry::new();
        let result = registry.parse_by_extension("data.xyz", b"some content").unwrap();
        assert_eq!(result, "some content");
    }

    #[test]
    fn test_pdf_parser_extensions() {
        let parser = PdfParser;
        assert!(parser.supported_extensions().contains(&"pdf"));
    }
}
