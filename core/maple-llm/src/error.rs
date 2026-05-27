use std::time::Duration;

/// LLM error classification — inspired by hermes-agent's 22 FailoverReason categories
/// Each error variant carries enough context for retry/fallback decisions
#[derive(Debug, Clone)]
pub enum LlmError {
    /// 429: Rate limited — retryable with backoff
    RateLimited {
        retry_after: Option<Duration>,
        provider: String,
    },

    /// 401/403: Authentication failed — rotate credential or fail
    AuthFailed {
        provider: String,
        message: String,
    },

    /// Context too long — compress and retry
    ContextTooLong {
        current_tokens: usize,
        max_tokens: usize,
    },

    /// 503/529: Model overloaded — fallback to another provider
    ModelOverloaded {
        provider: String,
        model: String,
    },

    /// Network/timeout errors — retryable
    NetworkError {
        message: String,
        source: Option<String>,
    },

    /// 400: Invalid request — fix and retry
    InvalidRequest {
        message: String,
    },

    /// 402: Quota exceeded — fallback or alert
    QuotaExceeded {
        provider: String,
    },

    /// 5xx: Server error — retryable
    ServerError {
        status: u16,
        provider: String,
        body: String,
    },

    /// 4xx: Client error — not retryable
    ClientError {
        status: u16,
        provider: String,
        body: String,
    },

    /// Unknown/unclassified errors
    Unknown {
        status: u16,
        provider: String,
        body: String,
    },
}

/// Classification result — drives retry/fallback decisions
#[derive(Debug, Clone)]
pub struct ClassifiedError {
    pub error: LlmError,
    pub retryable: bool,
    pub should_compress: bool,
    pub should_rotate_credential: bool,
    pub should_fallback: bool,
    pub retry_after: Option<Duration>,
}

impl LlmError {
    /// Classify an HTTP error response into a structured error
    pub fn classify(status: u16, body: &str, provider: &str) -> Self {
        match status {
            400 => {
                if body.contains("context_length_exceeded") || body.contains("too long") {
                    LlmError::ContextTooLong {
                        current_tokens: 0, // Will be filled by caller
                        max_tokens: 0,
                    }
                } else {
                    LlmError::InvalidRequest {
                        message: body.to_string(),
                    }
                }
            }
            401 | 403 => LlmError::AuthFailed {
                provider: provider.to_string(),
                message: body.to_string(),
            },
            402 => LlmError::QuotaExceeded {
                provider: provider.to_string(),
            },
            429 => {
                let retry_after = Self::parse_retry_after(body);
                LlmError::RateLimited {
                    retry_after,
                    provider: provider.to_string(),
                }
            }
            503 | 529 => LlmError::ModelOverloaded {
                provider: provider.to_string(),
                model: String::new(), // Will be filled by caller
            },
            500..=599 => LlmError::ServerError {
                status,
                provider: provider.to_string(),
                body: body.to_string(),
            },
            400..=499 => LlmError::ClientError {
                status,
                provider: provider.to_string(),
                body: body.to_string(),
            },
            _ => LlmError::Unknown {
                status,
                provider: provider.to_string(),
                body: body.to_string(),
            },
        }
    }

    /// Get retry/fallback decision for this error
    pub fn classify_decision(&self) -> ClassifiedError {
        match self {
            LlmError::RateLimited { retry_after, .. } => ClassifiedError {
                error: self.clone(),
                retryable: true,
                should_compress: false,
                should_rotate_credential: false,
                should_fallback: true,
                retry_after: *retry_after,
            },
            LlmError::AuthFailed { .. } => ClassifiedError {
                error: self.clone(),
                retryable: false,
                should_compress: false,
                should_rotate_credential: true,
                should_fallback: true,
                retry_after: None,
            },
            LlmError::ContextTooLong { .. } => ClassifiedError {
                error: self.clone(),
                retryable: true,
                should_compress: true,
                should_rotate_credential: false,
                should_fallback: false,
                retry_after: None,
            },
            LlmError::ModelOverloaded { .. } => ClassifiedError {
                error: self.clone(),
                retryable: true,
                should_compress: false,
                should_rotate_credential: false,
                should_fallback: true,
                retry_after: Some(Duration::from_secs(5)),
            },
            LlmError::NetworkError { .. } => ClassifiedError {
                error: self.clone(),
                retryable: true,
                should_compress: false,
                should_rotate_credential: false,
                should_fallback: true,
                retry_after: Some(Duration::from_secs(1)),
            },
            LlmError::InvalidRequest { .. } => ClassifiedError {
                error: self.clone(),
                retryable: false,
                should_compress: false,
                should_rotate_credential: false,
                should_fallback: false,
                retry_after: None,
            },
            LlmError::QuotaExceeded { .. } => ClassifiedError {
                error: self.clone(),
                retryable: false,
                should_compress: false,
                should_rotate_credential: false,
                should_fallback: true,
                retry_after: None,
            },
            LlmError::ServerError { .. } => ClassifiedError {
                error: self.clone(),
                retryable: true,
                should_compress: false,
                should_rotate_credential: false,
                should_fallback: true,
                retry_after: Some(Duration::from_secs(2)),
            },
            LlmError::ClientError { .. } => ClassifiedError {
                error: self.clone(),
                retryable: false,
                should_compress: false,
                should_rotate_credential: false,
                should_fallback: false,
                retry_after: None,
            },
            LlmError::Unknown { .. } => ClassifiedError {
                error: self.clone(),
                retryable: false,
                should_compress: false,
                should_rotate_credential: false,
                should_fallback: false,
                retry_after: None,
            },
        }
    }

    /// Parse retry-after from error body (e.g., "retry after 30 seconds")
    fn parse_retry_after(body: &str) -> Option<Duration> {
        // Common patterns: "retry after 30s", "retry-after: 30", "wait 30 seconds"
        let body_lower = body.to_lowercase();

        if let Some(pos) = body_lower.find("retry") {
            let after = &body[pos..];
            if let Some(num_start) = after.find(|c: char| c.is_ascii_digit()) {
                let num_str: String = after[num_start..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(secs) = num_str.parse::<u64>() {
                    return Some(Duration::from_secs(secs));
                }
            }
        }

        if let Some(pos) = body_lower.find("wait") {
            let after = &body[pos..];
            if let Some(num_start) = after.find(|c: char| c.is_ascii_digit()) {
                let num_str: String = after[num_start..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(secs) = num_str.parse::<u64>() {
                    return Some(Duration::from_secs(secs));
                }
            }
        }

        None
    }
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::RateLimited { provider, retry_after } => {
                write!(f, "Rate limited by {}", provider)?;
                if let Some(after) = retry_after {
                    write!(f, " (retry after {}s)", after.as_secs())?;
                }
                Ok(())
            }
            LlmError::AuthFailed { provider, message } => {
                write!(f, "Auth failed for {}: {}", provider, message)
            }
            LlmError::ContextTooLong { current_tokens, max_tokens } => {
                write!(f, "Context too long: {} tokens (max {})", current_tokens, max_tokens)
            }
            LlmError::ModelOverloaded { provider, model } => {
                write!(f, "Model overloaded: {}/{}", provider, model)
            }
            LlmError::NetworkError { message, .. } => {
                write!(f, "Network error: {}", message)
            }
            LlmError::InvalidRequest { message } => {
                write!(f, "Invalid request: {}", message)
            }
            LlmError::QuotaExceeded { provider } => {
                write!(f, "Quota exceeded for {}", provider)
            }
            LlmError::ServerError { status, provider, .. } => {
                write!(f, "Server error {} from {}", status, provider)
            }
            LlmError::ClientError { status, provider, .. } => {
                write!(f, "Client error {} from {}", status, provider)
            }
            LlmError::Unknown { status, provider, .. } => {
                write!(f, "Unknown error {} from {}", status, provider)
            }
        }
    }
}

impl std::error::Error for LlmError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_rate_limited() {
        let err = LlmError::classify(429, "Rate limit exceeded. Retry after 30 seconds", "openai");
        assert!(matches!(err, LlmError::RateLimited { .. }));
        let decision = err.classify_decision();
        assert!(decision.retryable);
        assert!(decision.should_fallback);
        assert_eq!(decision.retry_after, Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_classify_auth_failed() {
        let err = LlmError::classify(401, "Invalid API key", "openai");
        assert!(matches!(err, LlmError::AuthFailed { .. }));
        let decision = err.classify_decision();
        assert!(!decision.retryable);
        assert!(decision.should_rotate_credential);
    }

    #[test]
    fn test_classify_context_too_long() {
        let err = LlmError::classify(400, "context_length_exceeded: maximum context length is 128000", "openai");
        assert!(matches!(err, LlmError::ContextTooLong { .. }));
        let decision = err.classify_decision();
        assert!(decision.retryable);
        assert!(decision.should_compress);
    }

    #[test]
    fn test_classify_server_error() {
        let err = LlmError::classify(500, "Internal server error", "openai");
        assert!(matches!(err, LlmError::ServerError { .. }));
        let decision = err.classify_decision();
        assert!(decision.retryable);
    }

    #[test]
    fn test_classify_quota_exceeded() {
        let err = LlmError::classify(402, "Quota exceeded", "openai");
        assert!(matches!(err, LlmError::QuotaExceeded { .. }));
        let decision = err.classify_decision();
        assert!(!decision.retryable);
        assert!(decision.should_fallback);
    }

    #[test]
    fn test_parse_retry_after() {
        assert_eq!(
            LlmError::parse_retry_after("Rate limit exceeded. Retry after 30 seconds"),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            LlmError::parse_retry_after("Please wait 60 seconds"),
            Some(Duration::from_secs(60))
        );
        assert_eq!(
            LlmError::parse_retry_after("Some other error"),
            None
        );
    }
}
