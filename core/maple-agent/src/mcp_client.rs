use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// MCP Client Enhancements — auto-reconnect, tool sync, parallel calls, credential stripping
///
/// Builds on top of the gateway's McpHostManager to provide:
/// - Auto-reconnection with exponential backoff
/// - Tool list change notifications
/// - Parallel tool invocation across servers
/// - Credential/secret stripping from tool results
/// - Per-server health tracking and degraded-startup reports
///
///   MCP server health state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerHealth {
    Healthy,
    Degraded,
    Unhealthy,
    Disconnected,
}

/// Failure phase classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailurePhase {
    /// Failed during initial handshake
    Handshake,
    /// Failed during tool discovery
    ToolDiscovery,
    /// Failed during tool invocation
    ToolInvocation,
    /// Server became unresponsive
    Unresponsive,
    /// Connection dropped
    ConnectionLost,
}

/// Per-server health tracker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerHealthRecord {
    pub server_id: String,
    pub health: ServerHealth,
    pub failure_phase: Option<FailurePhase>,
    pub consecutive_failures: u32,
    pub total_failures: u64,
    pub total_calls: u64,
    pub last_success: Option<i64>,
    pub last_failure: Option<i64>,
    pub last_error: Option<String>,
    pub avg_latency_ms: f64,
}

impl ServerHealthRecord {
    pub fn new(server_id: &str) -> Self {
        Self {
            server_id: server_id.into(),
            health: ServerHealth::Healthy,
            failure_phase: None,
            consecutive_failures: 0,
            total_failures: 0,
            total_calls: 0,
            last_success: None,
            last_failure: None,
            last_error: None,
            avg_latency_ms: 0.0,
        }
    }

    /// Record a successful call
    pub fn record_success(&mut self, latency_ms: f64) {
        self.total_calls += 1;
        self.consecutive_failures = 0;
        self.failure_phase = None;
        self.last_success = Some(chrono::Utc::now().timestamp());
        // Exponential moving average
        self.avg_latency_ms = if self.avg_latency_ms == 0.0 {
            latency_ms
        } else {
            0.8 * self.avg_latency_ms + 0.2 * latency_ms
        };
        self.health = ServerHealth::Healthy;
    }

    /// Record a failure
    pub fn record_failure(&mut self, phase: FailurePhase, error: String) {
        self.total_calls += 1;
        self.total_failures += 1;
        self.consecutive_failures += 1;
        self.failure_phase = Some(phase);
        self.last_failure = Some(chrono::Utc::now().timestamp());
        self.last_error = Some(error);
        self.health = match self.consecutive_failures {
            0..=2 => ServerHealth::Degraded,
            3..=5 => ServerHealth::Unhealthy,
            _ => ServerHealth::Disconnected,
        };
    }
}

/// Reconnection policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectPolicy {
    /// Base delay between reconnection attempts
    pub base_delay: Duration,
    /// Maximum delay (exponential backoff cap)
    pub max_delay: Duration,
    /// Maximum number of reconnection attempts (0 = unlimited)
    pub max_attempts: u32,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            max_attempts: 10,
            backoff_multiplier: 2.0,
        }
    }
}

/// Auto-reconnection manager
pub struct ReconnectManager {
    policy: ReconnectPolicy,
    /// server_id -> (attempt_count, last_attempt)
    state: HashMap<String, (u32, Instant)>,
}

impl ReconnectManager {
    pub fn new(policy: ReconnectPolicy) -> Self {
        Self {
            policy,
            state: HashMap::new(),
        }
    }

    /// Check if we should attempt reconnection for a server
    pub fn should_reconnect(&self, server_id: &str) -> bool {
        let attempts = self.state.get(server_id).map(|(a, _)| *a).unwrap_or(0);
        self.policy.max_attempts == 0 || attempts < self.policy.max_attempts
    }

    /// Get the delay before next reconnection attempt
    pub fn next_delay(&self, server_id: &str) -> Duration {
        let attempts = self.state.get(server_id).map(|(a, _)| *a).unwrap_or(0);
        let delay_ms = self.policy.base_delay.as_millis() as f64
            * self.policy.backoff_multiplier.powi(attempts as i32);
        let delay = Duration::from_millis(delay_ms as u64);
        delay.min(self.policy.max_delay)
    }

    /// Record a reconnection attempt
    pub fn record_attempt(&mut self, server_id: &str) {
        let entry = self.state.entry(server_id.to_string()).or_insert((0, Instant::now()));
        entry.0 += 1;
        entry.1 = Instant::now();
    }

    /// Reset reconnection state after successful connection
    pub fn reset(&mut self, server_id: &str) {
        self.state.remove(server_id);
    }

    /// Get attempt count for a server
    pub fn attempt_count(&self, server_id: &str) -> u32 {
        self.state.get(server_id).map(|(a, _)| *a).unwrap_or(0)
    }
}

impl Default for ReconnectManager {
    fn default() -> Self {
        Self::new(ReconnectPolicy::default())
    }
}

/// Tool refresh event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRefreshEvent {
    pub server_id: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub timestamp: i64,
}

/// Tool sync manager — tracks tool lists and detects changes
pub struct ToolSyncManager {
    /// server_id -> list of tool names
    tool_lists: HashMap<String, Vec<String>>,
    /// Change event log
    events: Vec<ToolRefreshEvent>,
}

impl ToolSyncManager {
    pub fn new() -> Self {
        Self {
            tool_lists: HashMap::new(),
            events: Vec::new(),
        }
    }

    /// Update tool list for a server, returns change event if tools changed
    pub fn update_tools(
        &mut self,
        server_id: &str,
        new_tools: Vec<String>,
    ) -> Option<ToolRefreshEvent> {
        let old_tools = self.tool_lists.get(server_id).cloned().unwrap_or_default();

        let old_set: std::collections::HashSet<&str> =
            old_tools.iter().map(|s| s.as_str()).collect();
        let new_set: std::collections::HashSet<&str> =
            new_tools.iter().map(|s| s.as_str()).collect();

        let added: Vec<String> = new_set
            .difference(&old_set)
            .map(|s| s.to_string())
            .collect();
        let removed: Vec<String> = old_set
            .difference(&new_set)
            .map(|s| s.to_string())
            .collect();

        self.tool_lists.insert(server_id.into(), new_tools);

        if added.is_empty() && removed.is_empty() {
            return None;
        }

        let event = ToolRefreshEvent {
            server_id: server_id.into(),
            added,
            removed,
            timestamp: chrono::Utc::now().timestamp(),
        };
        self.events.push(event.clone());
        Some(event)
    }

    /// Get tools for a server
    pub fn get_tools(&self, server_id: &str) -> Option<&Vec<String>> {
        self.tool_lists.get(server_id)
    }

    /// Get all tools across all servers (namespaced)
    pub fn all_tools(&self) -> Vec<String> {
        self.tool_lists
            .iter()
            .flat_map(|(server, tools)| {
                tools
                    .iter()
                    .map(move |t| format!("mcp__{}__{}", server, t))
            })
            .collect()
    }

    /// Get change events
    pub fn events(&self) -> &[ToolRefreshEvent] {
        &self.events
    }
}

impl Default for ToolSyncManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Credential patterns to strip from tool results
const CREDENTIAL_PATTERNS: &[&str] = &[
    "api_key",
    "apikey",
    "api-key",
    "secret",
    "token",
    "password",
    "passwd",
    "credential",
    "authorization",
    "auth",
    "bearer",
    "private_key",
    "access_key",
    "access_token",
    "refresh_token",
    "client_secret",
    "client_id",
];

/// Strip credentials from a JSON value (redacts matching keys)
pub fn strip_credentials(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut cleaned = serde_json::Map::new();
            for (k, v) in map {
                let lower = k.to_lowercase();
                let is_credential = CREDENTIAL_PATTERNS.iter().any(|p| lower.contains(p));
                if is_credential {
                    cleaned.insert(k.clone(), serde_json::Value::String("[REDACTED]".into()));
                } else {
                    cleaned.insert(k.clone(), strip_credentials(v));
                }
            }
            serde_json::Value::Object(cleaned)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(strip_credentials).collect())
        }
        other => other.clone(),
    }
}

/// Strip credentials from a text string (masks common patterns)
pub fn strip_credentials_text(text: &str) -> String {
    let mut result = text.to_string();

    // AWS-style keys: AKIA followed by 16 uppercase alphanumeric
    let aws_pattern = "AKIA";
    let mut offset = 0;
    while let Some(pos) = result[offset..].find(aws_pattern) {
        let abs_pos = offset + pos;
        let after = &result[abs_pos + 4..];
        let key_len = after
            .chars()
            .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            .count();
        if key_len >= 16 {
            let end = abs_pos + 4 + key_len;
            result = format!("{}AKIA[REDACTED]{}", &result[..abs_pos], &result[end..]);
            offset = abs_pos + 14; // length of "AKIA[REDACTED]"
        } else {
            offset = abs_pos + 4;
        }
    }

    result
}

/// Parallel tool call result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelToolResult {
    pub server_id: String,
    pub tool_name: String,
    pub result: serde_json::Value,
    pub success: bool,
    pub latency_ms: f64,
}

/// Parallel tool executor
pub struct ParallelToolExecutor {
    max_concurrency: usize,
}

impl ParallelToolExecutor {
    pub fn new(max_concurrency: usize) -> Self {
        Self { max_concurrency }
    }

    /// Execute multiple tool calls in parallel, respecting concurrency limits
    pub async fn execute_parallel<F, Fut>(
        &self,
        calls: Vec<(String, String)>, // (server_id, tool_name)
        executor: F,
    ) -> Vec<ParallelToolResult>
    where
        F: Fn(String, String) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<serde_json::Value, String>> + Send,
    {
        let executor = Arc::new(executor);
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.max_concurrency));
        let mut handles = Vec::new();

        for (server_id, tool_name) in calls {
            let sem = semaphore.clone();
            let exec = executor.clone();
            let server = server_id.clone();
            let tool = tool_name.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let start = Instant::now();
                let result = exec(server.clone(), tool.clone()).await;
                let latency = start.elapsed().as_millis() as f64;

                match result {
                    Ok(value) => ParallelToolResult {
                        server_id: server,
                        tool_name: tool,
                        result: value,
                        success: true,
                        latency_ms: latency,
                    },
                    Err(e) => ParallelToolResult {
                        server_id: server,
                        tool_name: tool,
                        result: serde_json::json!({ "error": e }),
                        success: false,
                        latency_ms: latency,
                    },
                }
            });
            handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in handles {
            if let Ok(r) = handle.await {
                results.push(r);
            }
        }
        results
    }

    pub fn max_concurrency(&self) -> usize {
        self.max_concurrency
    }
}

impl Default for ParallelToolExecutor {
    fn default() -> Self {
        Self::new(10)
    }
}

/// Degraded startup report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupReport {
    pub total_servers: usize,
    pub healthy: usize,
    pub degraded: Vec<DegradedServer>,
    pub failed: Vec<FailedServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradedServer {
    pub server_id: String,
    pub reason: String,
    pub available_tools: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedServer {
    pub server_id: String,
    pub failure_phase: FailurePhase,
    pub error: String,
}

/// MCP lifecycle manager — orchestrates health, reconnection, and tool sync
pub struct McpLifecycleManager {
    health_records: HashMap<String, ServerHealthRecord>,
    reconnect_manager: ReconnectManager,
    tool_sync: ToolSyncManager,
}

impl McpLifecycleManager {
    pub fn new(reconnect_policy: ReconnectPolicy) -> Self {
        Self {
            health_records: HashMap::new(),
            reconnect_manager: ReconnectManager::new(reconnect_policy),
            tool_sync: ToolSyncManager::new(),
        }
    }

    /// Register a server
    pub fn register_server(&mut self, server_id: &str) {
        self.health_records
            .insert(server_id.into(), ServerHealthRecord::new(server_id));
    }

    /// Record a successful tool call
    pub fn record_success(&mut self, server_id: &str, latency_ms: f64) {
        if let Some(record) = self.health_records.get_mut(server_id) {
            record.record_success(latency_ms);
            self.reconnect_manager.reset(server_id);
        }
    }

    /// Record a failure
    pub fn record_failure(&mut self, server_id: &str, phase: FailurePhase, error: String) {
        if let Some(record) = self.health_records.get_mut(server_id) {
            record.record_failure(phase, error);
        }
    }

    /// Check if a server should be reconnected
    pub fn should_reconnect(&self, server_id: &str) -> bool {
        self.reconnect_manager.should_reconnect(server_id)
    }

    /// Get reconnect delay
    pub fn reconnect_delay(&self, server_id: &str) -> Duration {
        self.reconnect_manager.next_delay(server_id)
    }

    /// Record a reconnection attempt
    pub fn record_reconnect_attempt(&mut self, server_id: &str) {
        self.reconnect_manager.record_attempt(server_id);
    }

    /// Update tools for a server
    pub fn update_tools(
        &mut self,
        server_id: &str,
        tools: Vec<String>,
    ) -> Option<ToolRefreshEvent> {
        self.tool_sync.update_tools(server_id, tools)
    }

    /// Get health for a server
    pub fn health(&self, server_id: &str) -> Option<&ServerHealthRecord> {
        self.health_records.get(server_id)
    }

    /// Get all health records
    pub fn all_health(&self) -> &HashMap<String, ServerHealthRecord> {
        &self.health_records
    }

    /// Generate startup report
    pub fn startup_report(&self) -> StartupReport {
        let mut healthy = 0;
        let mut degraded = Vec::new();
        let mut failed = Vec::new();

        for record in self.health_records.values() {
            match record.health {
                ServerHealth::Healthy => healthy += 1,
                ServerHealth::Degraded => degraded.push(DegradedServer {
                    server_id: record.server_id.clone(),
                    reason: record
                        .last_error
                        .clone()
                        .unwrap_or_else(|| "unknown".into()),
                    available_tools: self
                        .tool_sync
                        .get_tools(&record.server_id)
                        .map(|t| t.len())
                        .unwrap_or(0),
                }),
                ServerHealth::Unhealthy | ServerHealth::Disconnected => {
                    failed.push(FailedServer {
                        server_id: record.server_id.clone(),
                        failure_phase: record.failure_phase.unwrap_or(FailurePhase::Unresponsive),
                        error: record
                            .last_error
                            .clone()
                            .unwrap_or_else(|| "unknown".into()),
                    })
                }
            }
        }

        StartupReport {
            total_servers: self.health_records.len(),
            healthy,
            degraded,
            failed,
        }
    }

    /// Get tool sync manager
    pub fn tool_sync(&self) -> &ToolSyncManager {
        &self.tool_sync
    }

    /// Get all namespaced tool names
    pub fn all_tools(&self) -> Vec<String> {
        self.tool_sync.all_tools()
    }
}

impl Default for McpLifecycleManager {
    fn default() -> Self {
        Self::new(ReconnectPolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_record_lifecycle() {
        let mut record = ServerHealthRecord::new("s1");
        assert_eq!(record.health, ServerHealth::Healthy);

        record.record_success(100.0);
        assert_eq!(record.consecutive_failures, 0);
        assert!(record.avg_latency_ms > 0.0);

        record.record_failure(FailurePhase::ToolInvocation, "timeout".into());
        assert_eq!(record.consecutive_failures, 1);
        assert_eq!(record.health, ServerHealth::Degraded);
    }

    #[test]
    fn test_health_degrades_with_failures() {
        let mut record = ServerHealthRecord::new("s1");

        record.record_failure(FailurePhase::Handshake, "err1".into());
        assert_eq!(record.health, ServerHealth::Degraded);

        record.record_failure(FailurePhase::Handshake, "err2".into());
        assert_eq!(record.health, ServerHealth::Degraded);

        record.record_failure(FailurePhase::Handshake, "err3".into());
        assert_eq!(record.health, ServerHealth::Unhealthy);

        record.record_failure(FailurePhase::Handshake, "err4".into());
        record.record_failure(FailurePhase::Handshake, "err5".into());
        record.record_failure(FailurePhase::Handshake, "err6".into());
        assert_eq!(record.health, ServerHealth::Disconnected);
    }

    #[test]
    fn test_health_resets_on_success() {
        let mut record = ServerHealthRecord::new("s1");
        record.record_failure(FailurePhase::ToolInvocation, "err".into());
        assert_eq!(record.health, ServerHealth::Degraded);

        record.record_success(50.0);
        assert_eq!(record.health, ServerHealth::Healthy);
        assert_eq!(record.consecutive_failures, 0);
    }

    #[test]
    fn test_reconnect_manager() {
        let mut mgr = ReconnectManager::default();

        assert!(mgr.should_reconnect("s1"));
        assert_eq!(mgr.attempt_count("s1"), 0);

        let delay1 = mgr.next_delay("s1");
        assert_eq!(delay1, Duration::from_secs(1));

        mgr.record_attempt("s1");
        assert_eq!(mgr.attempt_count("s1"), 1);

        let delay2 = mgr.next_delay("s1");
        assert_eq!(delay2, Duration::from_secs(2));

        mgr.reset("s1");
        assert_eq!(mgr.attempt_count("s1"), 0);
    }

    #[test]
    fn test_reconnect_max_attempts() {
        let policy = ReconnectPolicy {
            max_attempts: 2,
            ..Default::default()
        };
        let mut mgr = ReconnectManager::new(policy);

        mgr.record_attempt("s1");
        assert!(mgr.should_reconnect("s1"));

        mgr.record_attempt("s1");
        assert!(!mgr.should_reconnect("s1"));
    }

    #[test]
    fn test_reconnect_max_delay() {
        let policy = ReconnectPolicy {
            max_delay: Duration::from_secs(10),
            ..Default::default()
        };
        let mut mgr = ReconnectManager::new(policy);

        for _ in 0..20 {
            mgr.record_attempt("s1");
        }

        let delay = mgr.next_delay("s1");
        assert!(delay <= Duration::from_secs(10));
    }

    #[test]
    fn test_tool_sync_initial() {
        let mut sync = ToolSyncManager::new();
        let event = sync.update_tools("s1", vec!["tool_a".into(), "tool_b".into()]);

        // First update reports all as added
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.added.len(), 2);
        assert!(event.removed.is_empty());
    }

    #[test]
    fn test_tool_sync_no_change() {
        let mut sync = ToolSyncManager::new();
        sync.update_tools("s1", vec!["tool_a".into()]);
        let event = sync.update_tools("s1", vec!["tool_a".into()]);
        assert!(event.is_none());
    }

    #[test]
    fn test_tool_sync_changes() {
        let mut sync = ToolSyncManager::new();
        sync.update_tools("s1", vec!["a".into(), "b".into()]);

        let event = sync.update_tools("s1", vec!["b".into(), "c".into()]).unwrap();
        assert_eq!(event.added, vec!["c"]);
        assert_eq!(event.removed, vec!["a"]);
    }

    #[test]
    fn test_tool_sync_all_tools_namespaced() {
        let mut sync = ToolSyncManager::new();
        sync.update_tools("server1", vec!["read".into()]);
        sync.update_tools("server2", vec!["write".into()]);

        let all = sync.all_tools();
        assert!(all.contains(&"mcp__server1__read".to_string()));
        assert!(all.contains(&"mcp__server2__write".to_string()));
    }

    #[test]
    fn test_strip_credentials_json() {
        let input = serde_json::json!({
            "name": "test",
            "api_key": "sk-1234567890",
            "nested": {
                "secret": "mysecret",
                "safe": "value"
            }
        });

        let cleaned = strip_credentials(&input);
        assert_eq!(cleaned["name"], "test");
        assert_eq!(cleaned["api_key"], "[REDACTED]");
        assert_eq!(cleaned["nested"]["secret"], "[REDACTED]");
        assert_eq!(cleaned["nested"]["safe"], "value");
    }

    #[test]
    fn test_strip_credentials_preserves_structure() {
        let input = serde_json::json!({
            "items": [
                { "token": "abc123", "name": "item1" },
                { "token": "def456", "name": "item2" }
            ]
        });

        let cleaned = strip_credentials(&input);
        assert_eq!(cleaned["items"][0]["token"], "[REDACTED]");
        assert_eq!(cleaned["items"][0]["name"], "item1");
        assert_eq!(cleaned["items"][1]["token"], "[REDACTED]");
    }

    #[test]
    fn test_strip_credentials_text_aws() {
        let text = "Using key AKIAIOSFODNN7EXAMPLE for access";
        let cleaned = strip_credentials_text(text);
        assert!(cleaned.contains("[REDACTED]"));
        assert!(!cleaned.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[tokio::test]
    async fn test_parallel_executor() {
        let executor = ParallelToolExecutor::new(2);
        let calls = vec![
            ("s1".into(), "tool_a".into()),
            ("s2".into(), "tool_b".into()),
        ];

        let results = executor
            .execute_parallel(calls, |_server, tool| async move {
                Ok(serde_json::json!({ "tool": tool }))
            })
            .await;

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.success));
    }

    #[tokio::test]
    async fn test_parallel_executor_with_failure() {
        let executor = ParallelToolExecutor::new(5);
        let calls = vec![
            ("s1".into(), "good".into()),
            ("s2".into(), "bad".into()),
        ];

        let results = executor
            .execute_parallel(calls, |_, tool| async move {
                if tool == "bad" {
                    Err("tool failed".into())
                } else {
                    Ok(serde_json::json!({ "ok": true }))
                }
            })
            .await;

        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|r| r.success));
        assert!(results.iter().any(|r| !r.success));
    }

    #[test]
    fn test_lifecycle_manager() {
        let mut mgr = McpLifecycleManager::default();
        mgr.register_server("s1");
        mgr.register_server("s2");

        mgr.record_success("s1", 100.0);
        mgr.record_failure("s2", FailurePhase::Handshake, "timeout".into());

        let report = mgr.startup_report();
        assert_eq!(report.total_servers, 2);
        assert_eq!(report.healthy, 1);
        assert_eq!(report.degraded.len(), 1);
    }

    #[test]
    fn test_lifecycle_reconnect() {
        let mut mgr = McpLifecycleManager::default();
        mgr.register_server("s1");

        assert!(mgr.should_reconnect("s1"));
        // Before any attempt, delay = base (1s)
        assert_eq!(mgr.reconnect_delay("s1"), Duration::from_secs(1));

        mgr.record_reconnect_attempt("s1");
        // After 1 attempt, delay = 1s * 2^1 = 2s
        assert_eq!(mgr.reconnect_delay("s1"), Duration::from_secs(2));

        mgr.record_reconnect_attempt("s1");
        // After 2 attempts, delay = 1s * 2^2 = 4s
        assert_eq!(mgr.reconnect_delay("s1"), Duration::from_secs(4));
    }

    #[test]
    fn test_lifecycle_tool_sync() {
        let mut mgr = McpLifecycleManager::default();
        mgr.register_server("s1");

        mgr.update_tools("s1", vec!["read".into(), "write".into()]);
        let tools = mgr.all_tools();
        assert!(tools.contains(&"mcp__s1__read".to_string()));
    }
}
