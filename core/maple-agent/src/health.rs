use std::collections::HashMap;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use std::sync::Arc;

/// Health Monitoring — provider circuit breaker + agent heartbeat + tool statistics
///
/// Features:
/// - Per-provider health tracking
/// - Circuit breaker pattern
/// - Agent heartbeat monitoring
/// - Tool execution statistics
/// - Health check API

/// Provider health state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub provider_id: String,
    pub healthy: bool,
    pub consecutive_failures: u32,
    pub last_success: Option<u64>,
    pub last_failure: Option<u64>,
    pub last_error: Option<String>,
    pub circuit_open: bool,
    pub circuit_open_until: Option<u64>,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub avg_latency_ms: f64,
}

/// Agent health state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHealth {
    pub agent_id: String,
    pub healthy: bool,
    pub last_heartbeat: u64,
    pub state: AgentState,
    pub active_tasks: u32,
    pub completed_tasks: u64,
    pub failed_tasks: u64,
}

/// Agent state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentState {
    Idle,
    Running,
    Paused,
    Error,
    Offline,
}

/// Tool execution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStats {
    pub tool_name: String,
    pub total_calls: u64,
    pub successful_calls: u64,
    pub failed_calls: u64,
    pub avg_duration_ms: f64,
    pub last_called: Option<u64>,
    pub last_error: Option<String>,
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub healthy: bool,
    pub providers: HashMap<String, ProviderHealth>,
    pub agents: HashMap<String, AgentHealth>,
    pub tools: HashMap<String, ToolStats>,
    pub uptime_seconds: u64,
    pub last_check: u64,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub recovery_timeout: Duration,
    pub half_open_max_requests: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(60),
            half_open_max_requests: 3,
        }
    }
}

/// Health monitor
pub struct HealthMonitor {
    providers: Arc<RwLock<HashMap<String, ProviderHealth>>>,
    agents: Arc<RwLock<HashMap<String, AgentHealth>>>,
    tools: Arc<RwLock<HashMap<String, ToolStats>>>,
    circuit_breaker_config: CircuitBreakerConfig,
    start_time: Instant,
}

impl HealthMonitor {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
            agents: Arc::new(RwLock::new(HashMap::new())),
            tools: Arc::new(RwLock::new(HashMap::new())),
            circuit_breaker_config: CircuitBreakerConfig::default(),
            start_time: Instant::now(),
        }
    }

    pub fn with_circuit_breaker_config(mut self, config: CircuitBreakerConfig) -> Self {
        self.circuit_breaker_config = config;
        self
    }

    /// Record a provider request result
    pub async fn record_provider_request(
        &self,
        provider_id: &str,
        success: bool,
        latency: Duration,
        error: Option<String>,
    ) {
        let mut providers = self.providers.write().await;
        let health = providers.entry(provider_id.to_string()).or_insert_with(|| {
            ProviderHealth {
                provider_id: provider_id.to_string(),
                healthy: true,
                consecutive_failures: 0,
                last_success: None,
                last_failure: None,
                last_error: None,
                circuit_open: false,
                circuit_open_until: None,
                total_requests: 0,
                successful_requests: 0,
                failed_requests: 0,
                avg_latency_ms: 0.0,
            }
        });

        health.total_requests += 1;

        // Update average latency
        let total_latency = health.avg_latency_ms * (health.total_requests - 1) as f64;
        health.avg_latency_ms = (total_latency + latency.as_millis() as f64) / health.total_requests as f64;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if success {
            health.successful_requests += 1;
            health.consecutive_failures = 0;
            health.last_success = Some(now);
            health.last_error = None;

            // Close circuit if it was open
            if health.circuit_open {
                health.circuit_open = false;
                health.circuit_open_until = None;
            }
        } else {
            health.failed_requests += 1;
            health.consecutive_failures += 1;
            health.last_failure = Some(now);
            health.last_error = error;

            // Open circuit if threshold exceeded
            if health.consecutive_failures >= self.circuit_breaker_config.failure_threshold {
                health.circuit_open = true;
                health.circuit_open_until = Some(
                    now + self.circuit_breaker_config.recovery_timeout.as_secs()
                );
            }
        }

        health.healthy = !health.circuit_open;
    }

    /// Check if a provider is available
    pub async fn is_provider_available(&self, provider_id: &str) -> bool {
        let providers = self.providers.read().await;
        if let Some(health) = providers.get(provider_id) {
            if health.circuit_open {
                // Check if recovery timeout has passed
                if let Some(until) = health.circuit_open_until {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    return now >= until;
                }
                return false;
            }
            true
        } else {
            true // Unknown providers are assumed available
        }
    }

    /// Update agent heartbeat
    pub async fn update_agent_heartbeat(
        &self,
        agent_id: &str,
        state: AgentState,
        active_tasks: u32,
    ) {
        let mut agents = self.agents.write().await;
        let health = agents.entry(agent_id.to_string()).or_insert_with(|| {
            AgentHealth {
                agent_id: agent_id.to_string(),
                healthy: true,
                last_heartbeat: 0,
                state: AgentState::Idle,
                active_tasks: 0,
                completed_tasks: 0,
                failed_tasks: 0,
            }
        });

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        health.last_heartbeat = now;
        health.state = state;
        health.active_tasks = active_tasks;
        health.healthy = true;
    }

    /// Record agent task completion
    pub async fn record_agent_task(&self, agent_id: &str, success: bool) {
        let mut agents = self.agents.write().await;
        if let Some(health) = agents.get_mut(agent_id) {
            if success {
                health.completed_tasks += 1;
            } else {
                health.failed_tasks += 1;
            }
        }
    }

    /// Record tool execution
    pub async fn record_tool_execution(
        &self,
        tool_name: &str,
        success: bool,
        duration: Duration,
        error: Option<String>,
    ) {
        let mut tools = self.tools.write().await;
        let stats = tools.entry(tool_name.to_string()).or_insert_with(|| {
            ToolStats {
                tool_name: tool_name.to_string(),
                total_calls: 0,
                successful_calls: 0,
                failed_calls: 0,
                avg_duration_ms: 0.0,
                last_called: None,
                last_error: None,
            }
        });

        stats.total_calls += 1;

        // Update average duration
        let total_duration = stats.avg_duration_ms * (stats.total_calls - 1) as f64;
        stats.avg_duration_ms = (total_duration + duration.as_millis() as f64) / stats.total_calls as f64;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        stats.last_called = Some(now);

        if success {
            stats.successful_calls += 1;
            stats.last_error = None;
        } else {
            stats.failed_calls += 1;
            stats.last_error = error;
        }
    }

    /// Perform health check
    pub async fn check_health(&self) -> HealthCheckResult {
        let providers = self.providers.read().await.clone();
        let agents = self.agents.read().await.clone();
        let tools = self.tools.read().await.clone();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Check agent heartbeats (mark offline if no heartbeat for 5 minutes)
        let mut agents_checked = agents.clone();
        for (_, health) in agents_checked.iter_mut() {
            if now - health.last_heartbeat > 300 {
                health.healthy = false;
                health.state = AgentState::Offline;
            }
        }

        let healthy = providers.values().all(|p| p.healthy) &&
                     agents_checked.values().all(|a| a.healthy);

        HealthCheckResult {
            healthy,
            providers,
            agents: agents_checked,
            tools,
            uptime_seconds: self.start_time.elapsed().as_secs(),
            last_check: now,
        }
    }

    /// Get provider health
    pub async fn get_provider_health(&self, provider_id: &str) -> Option<ProviderHealth> {
        let providers = self.providers.read().await;
        providers.get(provider_id).cloned()
    }

    /// Get agent health
    pub async fn get_agent_health(&self, agent_id: &str) -> Option<AgentHealth> {
        let agents = self.agents.read().await;
        agents.get(agent_id).cloned()
    }

    /// Get tool stats
    pub async fn get_tool_stats(&self, tool_name: &str) -> Option<ToolStats> {
        let tools = self.tools.read().await;
        tools.get(tool_name).cloned()
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_provider_health() {
        let monitor = HealthMonitor::new();

        // Record successful request
        monitor.record_provider_request(
            "openai",
            true,
            Duration::from_millis(100),
            None,
        ).await;

        let health = monitor.get_provider_health("openai").await.unwrap();
        assert!(health.healthy);
        assert_eq!(health.successful_requests, 1);
        assert_eq!(health.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker() {
        let monitor = HealthMonitor::new()
            .with_circuit_breaker_config(CircuitBreakerConfig {
                failure_threshold: 3,
                recovery_timeout: Duration::from_secs(60),
                half_open_max_requests: 1,
            });

        // Record failures to trigger circuit breaker
        for _ in 0..3 {
            monitor.record_provider_request(
                "openai",
                false,
                Duration::from_millis(100),
                Some("Error".to_string()),
            ).await;
        }

        let health = monitor.get_provider_health("openai").await.unwrap();
        assert!(health.circuit_open);
        assert!(!health.healthy);
    }

    #[tokio::test]
    async fn test_tool_stats() {
        let monitor = HealthMonitor::new();

        monitor.record_tool_execution(
            "read_file",
            true,
            Duration::from_millis(50),
            None,
        ).await;

        let stats = monitor.get_tool_stats("read_file").await.unwrap();
        assert_eq!(stats.total_calls, 1);
        assert_eq!(stats.successful_calls, 1);
    }

    #[tokio::test]
    async fn test_health_check() {
        let monitor = HealthMonitor::new();

        let result = monitor.check_health().await;
        assert!(result.healthy);
        assert!(result.uptime_seconds < 5);
    }
}
