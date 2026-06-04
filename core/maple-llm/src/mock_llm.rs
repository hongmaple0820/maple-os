use crate::request::LlmRequest;
use crate::response::LlmResponse;
use crate::router::LlmAdapter;
use crate::stream::{LlmStream, StreamChunk};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// Mock LLM Adapter — deterministic responses for testing
///
/// Features:
/// - Scripted responses based on request patterns
/// - Tool call simulation
/// - Streaming support
/// - Error injection for testing error handling
/// - Request recording for assertions
/// - Latency simulation
///
/// Response configuration
#[derive(Debug, Clone)]
pub struct MockResponse {
    /// Response content
    pub content: String,
    /// Tool calls to simulate (as JSON values)
    pub tool_calls: Vec<serde_json::Value>,
    /// Simulated latency
    pub latency_ms: u64,
    /// Error to inject (if any)
    pub error: Option<String>,
}

/// Request pattern matcher
#[derive(Debug, Clone)]
pub enum RequestMatcher {
    /// Match by content substring
    ContentContains(String),
    /// Match by tool name
    HasTool(String),
    /// Match by message count
    MessageCount(usize),
    /// Always match
    Always,
}

/// Mock LLM adapter
pub struct MockLlmAdapter {
    /// Scripted responses indexed by matcher
    responses: Arc<Mutex<Vec<(RequestMatcher, MockResponse)>>>,
    /// Default response when no matcher matches
    default_response: MockResponse,
    /// Recorded requests for assertions
    recorded_requests: Arc<Mutex<Vec<LlmRequest>>>,
    /// Model name
    model: String,
    /// Request counter
    request_count: Arc<Mutex<u64>>,
}

impl MockLlmAdapter {
    pub fn new(model: &str) -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
            default_response: MockResponse {
                content: "Mock response".to_string(),
                tool_calls: Vec::new(),
                latency_ms: 0,
                error: None,
            },
            recorded_requests: Arc::new(Mutex::new(Vec::new())),
            model: model.to_string(),
            request_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Add a scripted response for a pattern
    pub fn when(&mut self, matcher: RequestMatcher, response: MockResponse) {
        self.responses.lock().unwrap().push((matcher, response));
    }

    /// Set default response
    pub fn with_default_response(mut self, response: MockResponse) -> Self {
        self.default_response = response;
        self
    }

    /// Get recorded requests
    pub fn recorded_requests(&self) -> Vec<LlmRequest> {
        self.recorded_requests.lock().unwrap().clone()
    }

    /// Get request count
    pub fn request_count(&self) -> u64 {
        *self.request_count.lock().unwrap()
    }

    /// Clear recorded requests
    pub fn clear_recorded_requests(&self) {
        self.recorded_requests.lock().unwrap().clear();
    }

    /// Find matching response for request
    fn find_response(&self, req: &LlmRequest) -> MockResponse {
        let responses = self.responses.lock().unwrap();
        for (matcher, response) in responses.iter() {
            if self.matches(matcher, req) {
                return response.clone();
            }
        }
        self.default_response.clone()
    }

    /// Check if request matches pattern
    fn matches(&self, matcher: &RequestMatcher, req: &LlmRequest) -> bool {
        match matcher {
            RequestMatcher::ContentContains(substring) => {
                req.messages.iter().any(|m| m.content.contains(substring))
            }
            RequestMatcher::HasTool(tool_name) => {
                req.tools.as_ref().is_some_and(|tools| {
                    tools.iter().any(|t| &t.name == tool_name)
                })
            }
            RequestMatcher::MessageCount(count) => req.messages.len() == *count,
            RequestMatcher::Always => true,
        }
    }

    /// Build response from mock config
    fn build_response(&self, mock: &MockResponse, req: &LlmRequest) -> LlmResponse {
        // Increment request count
        *self.request_count.lock().unwrap() += 1;

        // Record request
        self.recorded_requests.lock().unwrap().push(req.clone());

        LlmResponse {
            content: mock.content.clone(),
            input_tokens: 100,
            output_tokens: 50,
            model: Some(self.model.clone()),
            finish_reason: Some("stop".to_string()),
            tool_calls: if mock.tool_calls.is_empty() {
                None
            } else {
                Some(mock.tool_calls.clone())
            },
        }
    }
}

#[async_trait]
impl LlmAdapter for MockLlmAdapter {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse> {
        let mock = self.find_response(&req);

        // Simulate latency
        if mock.latency_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(mock.latency_ms)).await;
        }

        // Inject error if configured
        if let Some(ref error) = mock.error {
            return Err(anyhow::anyhow!("{}", error));
        }

        Ok(self.build_response(&mock, &req))
    }

    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmStream>> {
        let mock = self.find_response(&req);

        // Increment request count and record
        *self.request_count.lock().unwrap() += 1;
        self.recorded_requests.lock().unwrap().push(req.clone());

        // Inject error if configured
        if let Some(ref error) = mock.error {
            return Err(anyhow::anyhow!("{}", error));
        }

        // Create stream from content
        Ok(Box::new(MockLlmStream::new(&mock.content)))
    }

    fn count_tokens(&self, text: &str) -> usize {
        // Simple approximation: 1 token per 4 characters
        text.len().div_ceil(4)
    }

    fn max_context_length(&self) -> usize {
        128_000
    }

    fn cost_per_1k_tokens(&self) -> (f64, f64) {
        (0.0, 0.0) // Free for testing
    }

    fn name(&self) -> &str {
        &self.model
    }
}

/// Mock LLM stream
struct MockLlmStream {
    chunks: Vec<StreamChunk>,
    index: usize,
}

impl MockLlmStream {
    fn new(content: &str) -> Self {
        let chunks: Vec<StreamChunk> = content
            .chars()
            .collect::<Vec<_>>()
            .chunks(10)
            .map(|chunk| StreamChunk {
                delta: chunk.iter().collect(),
                finish_reason: None,
                reasoning: false,
            })
            .chain(std::iter::once(StreamChunk {
                delta: String::new(),
                finish_reason: Some("stop".to_string()),
                reasoning: false,
            }))
            .collect();

        Self { chunks, index: 0 }
    }
}

#[async_trait]
impl LlmStream for MockLlmStream {
    async fn next_chunk(&mut self) -> Result<Option<StreamChunk>> {
        if self.index < self.chunks.len() {
            let chunk = self.chunks[self.index].clone();
            self.index += 1;
            Ok(Some(chunk))
        } else {
            Ok(None)
        }
    }
}

/// Predefined mock responses for common scenarios
pub struct MockResponses;

impl MockResponses {
    /// Simple text response
    pub fn text(content: &str) -> MockResponse {
        MockResponse {
            content: content.to_string(),
            tool_calls: Vec::new(),
            latency_ms: 0,
            error: None,
        }
    }

    /// Response with tool call
    pub fn tool_call(tool_name: &str, arguments: serde_json::Value) -> MockResponse {
        MockResponse {
            content: String::new(),
            tool_calls: vec![serde_json::json!({
                "id": format!("call_{}", uuid::Uuid::new_v4()),
                "type": "function",
                "function": {
                    "name": tool_name,
                    "arguments": arguments.to_string()
                }
            })],
            latency_ms: 0,
            error: None,
        }
    }

    /// Error response
    pub fn error(message: &str) -> MockResponse {
        MockResponse {
            content: String::new(),
            tool_calls: Vec::new(),
            latency_ms: 0,
            error: Some(message.to_string()),
        }
    }

    /// Response with latency
    pub fn with_latency(mut response: MockResponse, ms: u64) -> MockResponse {
        response.latency_ms = ms;
        response
    }
}

/// Mock parity test harness for E2E testing
pub struct MockParityHarness {
    mock_adapter: MockLlmAdapter,
    test_cases: Vec<ParityTestCase>,
}

/// Test case for parity testing
#[derive(Debug, Clone)]
pub struct ParityTestCase {
    pub name: String,
    pub request: LlmRequest,
    pub expected_response: MockResponse,
    pub expected_tool_calls: Vec<String>,
}

impl MockParityHarness {
    pub fn new(model: &str) -> Self {
        Self {
            mock_adapter: MockLlmAdapter::new(model),
            test_cases: Vec::new(),
        }
    }

    /// Add a test case
    pub fn add_test_case(&mut self, test_case: ParityTestCase) {
        self.mock_adapter.when(
            RequestMatcher::ContentContains(test_case.name.clone()),
            test_case.expected_response.clone(),
        );
        self.test_cases.push(test_case);
    }

    /// Run all test cases and verify parity
    pub async fn verify_parity(&self) -> Result<ParityReport> {
        let mut report = ParityReport {
            total: self.test_cases.len(),
            passed: 0,
            failed: 0,
            errors: Vec::new(),
        };

        for test_case in &self.test_cases {
            match self.mock_adapter.complete(test_case.request.clone()).await {
                Ok(response) => {
                    // Verify tool calls
                    let actual_tool_names: Vec<String> = response
                        .tool_calls
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|tc| {
                            tc.get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(|n| n.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect();

                    if actual_tool_names == test_case.expected_tool_calls {
                        report.passed += 1;
                    } else {
                        report.failed += 1;
                        report.errors.push(ParityError {
                            test_name: test_case.name.clone(),
                            expected: format!("{:?}", test_case.expected_tool_calls),
                            actual: format!("{:?}", actual_tool_names),
                        });
                    }
                }
                Err(e) => {
                    report.failed += 1;
                    report.errors.push(ParityError {
                        test_name: test_case.name.clone(),
                        expected: "Success".to_string(),
                        actual: format!("Error: {}", e),
                    });
                }
            }
        }

        Ok(report)
    }

    /// Get the mock adapter for integration testing
    pub fn adapter(&self) -> &MockLlmAdapter {
        &self.mock_adapter
    }
}

/// Parity test report
#[derive(Debug)]
pub struct ParityReport {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub errors: Vec<ParityError>,
}

/// Parity test error
#[derive(Debug)]
pub struct ParityError {
    pub test_name: String,
    pub expected: String,
    pub actual: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::{Message, Priority, PrivacyLevel, TaskType, ToolDefinition};

    fn sample_request(content: &str) -> LlmRequest {
        LlmRequest {
            messages: vec![Message::user(content)],
            task_type: TaskType::General,
            privacy_level: PrivacyLevel::Public,
            priority: Priority::Quality,
            max_tokens: None,
            temperature: 0.7,
            has_image: false,
            estimated_tokens: content.len() / 4,
            tools: None,
            requested_model: "mock-model".to_string(),
        }
    }

    #[tokio::test]
    async fn test_mock_adapter_basic_response() {
        let mut adapter = MockLlmAdapter::new("test-model");
        adapter.when(
            RequestMatcher::ContentContains("hello".to_string()),
            MockResponses::text("Hi there!"),
        );

        let req = sample_request("hello world");
        let response = adapter.complete(req).await.unwrap();
        assert_eq!(response.content, "Hi there!");
        assert_eq!(adapter.request_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_adapter_default_response() {
        let adapter = MockLlmAdapter::new("test-model")
            .with_default_response(MockResponses::text("default"));

        let req = sample_request("anything");
        let response = adapter.complete(req).await.unwrap();
        assert_eq!(response.content, "default");
    }

    #[tokio::test]
    async fn test_mock_adapter_tool_call() {
        let mut adapter = MockLlmAdapter::new("test-model");
        adapter.when(
            RequestMatcher::ContentContains("read file".to_string()),
            MockResponses::tool_call("read_file", serde_json::json!({"path": "test.rs"})),
        );

        let req = sample_request("read file test.rs");
        let response = adapter.complete(req).await.unwrap();
        assert!(response.tool_calls.is_some());
        let tool_calls = response.tool_calls.unwrap();
        assert_eq!(tool_calls.len(), 1);
    }

    #[tokio::test]
    async fn test_mock_adapter_error_injection() {
        let mut adapter = MockLlmAdapter::new("test-model");
        adapter.when(
            RequestMatcher::Always,
            MockResponses::error("Rate limited"),
        );

        let req = sample_request("test");
        let result = adapter.complete(req).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_adapter_request_recording() {
        let mut adapter = MockLlmAdapter::new("test-model");
        adapter.when(
            RequestMatcher::Always,
            MockResponses::text("ok"),
        );

        let req1 = sample_request("first");
        let req2 = sample_request("second");

        adapter.complete(req1).await.unwrap();
        adapter.complete(req2).await.unwrap();

        let recorded = adapter.recorded_requests();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].messages[0].content, "first");
        assert_eq!(recorded[1].messages[0].content, "second");
    }

    #[tokio::test]
    async fn test_mock_adapter_has_tool_matcher() {
        let mut adapter = MockLlmAdapter::new("test-model");
        adapter.when(
            RequestMatcher::HasTool("read_file".to_string()),
            MockResponses::text("has tool"),
        );

        let mut req = sample_request("test");
        req.tools = Some(vec![ToolDefinition {
            name: "read_file".to_string(),
            description: "Read a file".to_string(),
            parameters: serde_json::json!({}),
        }]);

        let response = adapter.complete(req).await.unwrap();
        assert_eq!(response.content, "has tool");
    }

    #[test]
    fn test_mock_responses_text() {
        let response = MockResponses::text("hello");
        assert_eq!(response.content, "hello");
        assert!(response.tool_calls.is_empty());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_mock_responses_tool_call() {
        let response = MockResponses::tool_call("test_tool", serde_json::json!({"key": "value"}));
        assert_eq!(response.tool_calls.len(), 1);
    }

    #[test]
    fn test_mock_responses_error() {
        let response = MockResponses::error("test error");
        assert_eq!(response.error, Some("test error".to_string()));
    }

    #[tokio::test]
    async fn test_parity_harness() {
        let mut harness = MockParityHarness::new("test-model");

        harness.add_test_case(ParityTestCase {
            name: "test_case".to_string(),
            request: sample_request("test_case"),
            expected_response: MockResponses::text("response"),
            expected_tool_calls: Vec::new(),
        });

        let report = harness.verify_parity().await.unwrap();
        assert_eq!(report.total, 1);
        assert_eq!(report.passed, 1);
        assert_eq!(report.failed, 0);
    }

    #[test]
    fn test_token_counting() {
        let adapter = MockLlmAdapter::new("test");
        assert_eq!(adapter.count_tokens("hello"), 2); // 5 chars / 4 = 2 (rounded up)
        assert_eq!(adapter.count_tokens("hi"), 1);
    }
}
