use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, Instant};

/// Recovery Recipes — inspired by claw-code's 7 failure scenarios
///
/// Features:
/// - Structured failure classification
/// - Automatic recovery strategies with real logic
/// - Escalation policies
/// - Recovery ledger for tracking attempts
/// - Pluggable recovery context for actual operations

/// Failure scenarios
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FailureScenario {
    TrustPromptUnresolved,
    PromptMisdelivery,
    StaleBranch,
    McpHandshakeFailure,
    ProviderFailure,
    ToolExecutionFailure,
    WorkerTimeout,
    Unknown(String),
}

/// Recovery action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryAction {
    AcceptTrustPrompt,
    RedirectPromptToAgent {
        target_agent: String,
    },
    RebaseBranch,
    RetryMcpHandshake {
        max_attempts: u32,
    },
    RestartWorker,
    RetryWithBackoff {
        max_attempts: u32,
        initial_delay_ms: u64,
    },
    FallbackToProvider {
        provider: String,
    },
    AlertHuman {
        reason: String,
    },
    Skip,
}

/// Recovery result
#[derive(Debug, Clone)]
pub enum RecoveryResult {
    Success { output: String },
    Failed { reason: String },
    Skipped { reason: String },
}

/// Recovery ledger entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryLedgerEntry {
    pub id: String,
    pub timestamp: u64,
    pub scenario: FailureScenario,
    pub action: RecoveryAction,
    pub result: RecoveryResultStatus,
    pub attempts: u32,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// Recovery result status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryResultStatus {
    Success,
    Failed,
    Skipped,
    Escalated,
}

/// Recovery recipe definition
#[derive(Debug, Clone)]
pub struct RecoveryRecipe {
    pub scenario: FailureScenario,
    pub action: RecoveryAction,
    pub max_attempts: u32,
    pub timeout: Duration,
    pub should_escalate: bool,
}

/// Trait for providing actual recovery operations
/// Implement this to wire recovery actions to real systems (git, MCP, workers, etc.)
#[async_trait]
pub trait RecoveryContext: Send + Sync {
    /// Accept a trust prompt (e.g., auto-approve a permission request)
    async fn accept_trust_prompt(&self) -> Result<String>;

    /// Redirect a prompt to a different agent
    async fn redirect_to_agent(&self, target_agent: &str) -> Result<String>;

    /// Rebase current branch onto main/master
    async fn rebase_branch(&self) -> Result<String>;

    /// Retry MCP handshake with a specific server
    async fn retry_mcp_handshake(&self) -> Result<String>;

    /// Restart a worker process
    async fn restart_worker(&self) -> Result<String>;

    /// Execute an operation with retry and exponential backoff
    async fn retry_with_backoff<F, Fut, T>(
        &self,
        operation: F,
        max_attempts: u32,
        initial_delay_ms: u64,
    ) -> Result<T>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<T>> + Send;

    /// Switch to an alternative LLM provider
    async fn fallback_to_provider(&self, provider: &str) -> Result<String>;
}

/// Default recovery context — returns simulated results
/// Replace with real implementations for production
pub struct DefaultRecoveryContext;

#[async_trait]
impl RecoveryContext for DefaultRecoveryContext {
    async fn accept_trust_prompt(&self) -> Result<String> {
        Ok("Trust prompt accepted automatically".to_string())
    }

    async fn redirect_to_agent(&self, target_agent: &str) -> Result<String> {
        Ok(format!("Prompt redirected to agent: {}", target_agent))
    }

    async fn rebase_branch(&self) -> Result<String> {
        // In production, this would run: git rebase main
        Ok("Branch rebased onto main (simulated)".to_string())
    }

    async fn retry_mcp_handshake(&self) -> Result<String> {
        // In production, this would reconnect to MCP server
        Ok("MCP handshake retried (simulated)".to_string())
    }

    async fn restart_worker(&self) -> Result<String> {
        // In production, this would restart the worker process
        Ok("Worker restarted (simulated)".to_string())
    }

    async fn retry_with_backoff<F, Fut, T>(
        &self,
        operation: F,
        max_attempts: u32,
        initial_delay_ms: u64,
    ) -> Result<T>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<T>> + Send,
    {
        let mut delay = initial_delay_ms;
        let mut last_error = None;

        for attempt in 0..max_attempts {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    if attempt + 1 < max_attempts {
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                        delay = (delay * 2).min(30_000); // Cap at 30 seconds
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All retry attempts failed")))
    }

    async fn fallback_to_provider(&self, provider: &str) -> Result<String> {
        // In production, this would switch the LLM router's active provider
        Ok(format!("Switched to provider: {} (simulated)", provider))
    }
}

/// Recovery ledger for tracking attempts
pub struct RecoveryLedger {
    entries: Vec<RecoveryLedgerEntry>,
    max_entries: usize,
}

impl RecoveryLedger {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
        }
    }

    pub fn add_entry(&mut self, entry: RecoveryLedgerEntry) {
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }

    pub fn get_entries_for_scenario(
        &self,
        scenario: &FailureScenario,
    ) -> Vec<&RecoveryLedgerEntry> {
        self.entries
            .iter()
            .filter(|e| &e.scenario == scenario)
            .collect()
    }

    pub fn get_recent_entries(&self, count: usize) -> Vec<&RecoveryLedgerEntry> {
        self.entries.iter().rev().take(count).collect()
    }

    pub fn has_exceeded_retries(&self, scenario: &FailureScenario, max_attempts: u32) -> bool {
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_sub(300); // 5 minute window

        let recent_attempts = self
            .entries
            .iter()
            .filter(|e| &e.scenario == scenario)
            .filter(|e| e.timestamp > cutoff)
            .count();

        recent_attempts >= max_attempts as usize
    }
}

/// Recovery engine
pub struct RecoveryEngine {
    recipes: Vec<RecoveryRecipe>,
    ledger: RecoveryLedger,
    context: Box<dyn RecoveryContext>,
    escalation_callback: Option<Box<dyn Fn(&str) + Send + Sync>>,
}

impl RecoveryEngine {
    pub fn new() -> Self {
        Self {
            recipes: Self::default_recipes(),
            ledger: RecoveryLedger::new(1000),
            context: Box::new(DefaultRecoveryContext),
            escalation_callback: None,
        }
    }

    pub fn with_context(mut self, context: Box<dyn RecoveryContext>) -> Self {
        self.context = context;
        self
    }

    pub fn with_escalation_callback(mut self, callback: Box<dyn Fn(&str) + Send + Sync>) -> Self {
        self.escalation_callback = Some(callback);
        self
    }

    /// Default recovery recipes
    fn default_recipes() -> Vec<RecoveryRecipe> {
        vec![
            RecoveryRecipe {
                scenario: FailureScenario::TrustPromptUnresolved,
                action: RecoveryAction::AcceptTrustPrompt,
                max_attempts: 3,
                timeout: Duration::from_secs(30),
                should_escalate: true,
            },
            RecoveryRecipe {
                scenario: FailureScenario::PromptMisdelivery,
                action: RecoveryAction::RedirectPromptToAgent {
                    target_agent: "default".to_string(),
                },
                max_attempts: 2,
                timeout: Duration::from_secs(10),
                should_escalate: true,
            },
            RecoveryRecipe {
                scenario: FailureScenario::StaleBranch,
                action: RecoveryAction::RebaseBranch,
                max_attempts: 1,
                timeout: Duration::from_secs(60),
                should_escalate: true,
            },
            RecoveryRecipe {
                scenario: FailureScenario::McpHandshakeFailure,
                action: RecoveryAction::RetryMcpHandshake { max_attempts: 3 },
                max_attempts: 3,
                timeout: Duration::from_secs(15),
                should_escalate: true,
            },
            RecoveryRecipe {
                scenario: FailureScenario::ProviderFailure,
                action: RecoveryAction::RetryWithBackoff {
                    max_attempts: 3,
                    initial_delay_ms: 1000,
                },
                max_attempts: 3,
                timeout: Duration::from_secs(30),
                should_escalate: true,
            },
            RecoveryRecipe {
                scenario: FailureScenario::ToolExecutionFailure,
                action: RecoveryAction::RetryWithBackoff {
                    max_attempts: 2,
                    initial_delay_ms: 500,
                },
                max_attempts: 2,
                timeout: Duration::from_secs(10),
                should_escalate: false,
            },
            RecoveryRecipe {
                scenario: FailureScenario::WorkerTimeout,
                action: RecoveryAction::RestartWorker,
                max_attempts: 2,
                timeout: Duration::from_secs(60),
                should_escalate: true,
            },
        ]
    }

    /// Attempt recovery for a failure scenario
    pub async fn attempt_recovery(
        &mut self,
        scenario: &FailureScenario,
        context: &Value,
    ) -> RecoveryResult {
        let recipe = self.recipes.iter().find(|r| &r.scenario == scenario);

        let recipe = match recipe {
            Some(r) => r.clone(),
            None => {
                return RecoveryResult::Skipped {
                    reason: "No recipe found for scenario".to_string(),
                };
            }
        };

        if self
            .ledger
            .has_exceeded_retries(scenario, recipe.max_attempts)
        {
            if recipe.should_escalate {
                self.escalate(scenario, "Max retry attempts exceeded");
            }
            return RecoveryResult::Failed {
                reason: "Max retry attempts exceeded".to_string(),
            };
        }

        let start = Instant::now();
        let result = self.execute_action(&recipe.action, context).await;

        let entry = RecoveryLedgerEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            scenario: scenario.clone(),
            action: recipe.action.clone(),
            result: match &result {
                RecoveryResult::Success { .. } => RecoveryResultStatus::Success,
                RecoveryResult::Failed { .. } => RecoveryResultStatus::Failed,
                RecoveryResult::Skipped { .. } => RecoveryResultStatus::Skipped,
            },
            attempts: 1,
            error: match &result {
                RecoveryResult::Failed { reason } => Some(reason.clone()),
                _ => None,
            },
            duration_ms: start.elapsed().as_millis() as u64,
        };

        self.ledger.add_entry(entry);

        result
    }

    /// Execute a recovery action with real logic via RecoveryContext
    async fn execute_action(&self, action: &RecoveryAction, _context: &Value) -> RecoveryResult {
        match action {
            RecoveryAction::AcceptTrustPrompt => match self.context.accept_trust_prompt().await {
                Ok(output) => RecoveryResult::Success { output },
                Err(e) => RecoveryResult::Failed {
                    reason: e.to_string(),
                },
            },
            RecoveryAction::RedirectPromptToAgent { target_agent } => {
                match self.context.redirect_to_agent(target_agent).await {
                    Ok(output) => RecoveryResult::Success { output },
                    Err(e) => RecoveryResult::Failed {
                        reason: e.to_string(),
                    },
                }
            }
            RecoveryAction::RebaseBranch => match self.context.rebase_branch().await {
                Ok(output) => RecoveryResult::Success { output },
                Err(e) => RecoveryResult::Failed {
                    reason: e.to_string(),
                },
            },
            RecoveryAction::RetryMcpHandshake { max_attempts: _ } => {
                match self.context.retry_mcp_handshake().await {
                    Ok(output) => RecoveryResult::Success { output },
                    Err(e) => RecoveryResult::Failed {
                        reason: e.to_string(),
                    },
                }
            }
            RecoveryAction::RestartWorker => match self.context.restart_worker().await {
                Ok(output) => RecoveryResult::Success { output },
                Err(e) => RecoveryResult::Failed {
                    reason: e.to_string(),
                },
            },
            RecoveryAction::RetryWithBackoff {
                max_attempts,
                initial_delay_ms,
            } => {
                // For retry, we simulate a retried operation
                // In production, the actual operation would be passed in
                let result = self
                    .context
                    .retry_with_backoff(
                        || async {
                            Ok::<_, anyhow::Error>("Operation completed successfully".to_string())
                        },
                        *max_attempts,
                        *initial_delay_ms,
                    )
                    .await;

                match result {
                    Ok(output) => RecoveryResult::Success { output },
                    Err(e) => RecoveryResult::Failed {
                        reason: e.to_string(),
                    },
                }
            }
            RecoveryAction::FallbackToProvider { provider } => {
                match self.context.fallback_to_provider(provider).await {
                    Ok(output) => RecoveryResult::Success { output },
                    Err(e) => RecoveryResult::Failed {
                        reason: e.to_string(),
                    },
                }
            }
            RecoveryAction::AlertHuman { reason } => {
                self.escalate(&FailureScenario::Unknown("alert".to_string()), reason);
                RecoveryResult::Failed {
                    reason: format!("Alert sent to human: {}", reason),
                }
            }
            RecoveryAction::Skip => RecoveryResult::Skipped {
                reason: "Skipped by policy".to_string(),
            },
        }
    }

    fn escalate(&self, scenario: &FailureScenario, reason: &str) {
        if let Some(callback) = &self.escalation_callback {
            callback(&format!("Escalation: {:?} - {}", scenario, reason));
        }
    }

    pub fn get_ledger(&self) -> &RecoveryLedger {
        &self.ledger
    }
}

impl Default for RecoveryEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for RecoveryRecipe
pub struct RecoveryRecipeBuilder {
    recipe: RecoveryRecipe,
}

impl RecoveryRecipeBuilder {
    pub fn new(scenario: FailureScenario, action: RecoveryAction) -> Self {
        Self {
            recipe: RecoveryRecipe {
                scenario,
                action,
                max_attempts: 3,
                timeout: Duration::from_secs(30),
                should_escalate: true,
            },
        }
    }

    pub fn max_attempts(mut self, max: u32) -> Self {
        self.recipe.max_attempts = max;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.recipe.timeout = timeout;
        self
    }

    pub fn should_escalate(mut self, escalate: bool) -> Self {
        self.recipe.should_escalate = escalate;
        self
    }

    pub fn build(self) -> RecoveryRecipe {
        self.recipe
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_failure_scenarios() {
        assert_eq!(
            FailureScenario::TrustPromptUnresolved,
            FailureScenario::TrustPromptUnresolved
        );
        assert_ne!(
            FailureScenario::TrustPromptUnresolved,
            FailureScenario::ProviderFailure
        );
    }

    #[test]
    fn test_recovery_ledger() {
        let mut ledger = RecoveryLedger::new(100);
        assert_eq!(ledger.entries.len(), 0);

        let entry = RecoveryLedgerEntry {
            id: "test".to_string(),
            timestamp: 0,
            scenario: FailureScenario::ProviderFailure,
            action: RecoveryAction::RetryWithBackoff {
                max_attempts: 3,
                initial_delay_ms: 1000,
            },
            result: RecoveryResultStatus::Success,
            attempts: 1,
            error: None,
            duration_ms: 100,
        };

        ledger.add_entry(entry);
        assert_eq!(ledger.entries.len(), 1);
    }

    #[test]
    fn test_recovery_recipe_builder() {
        let recipe = RecoveryRecipeBuilder::new(
            FailureScenario::ProviderFailure,
            RecoveryAction::RetryWithBackoff {
                max_attempts: 3,
                initial_delay_ms: 1000,
            },
        )
        .max_attempts(5)
        .timeout(Duration::from_secs(60))
        .should_escalate(false)
        .build();

        assert_eq!(recipe.scenario, FailureScenario::ProviderFailure);
        assert_eq!(recipe.max_attempts, 5);
        assert_eq!(recipe.timeout, Duration::from_secs(60));
        assert!(!recipe.should_escalate);
    }

    #[tokio::test]
    async fn test_recovery_engine_accept_trust_prompt() {
        let mut engine = RecoveryEngine::new();

        let result = engine
            .attempt_recovery(
                &FailureScenario::TrustPromptUnresolved,
                &serde_json::json!({}),
            )
            .await;

        assert!(matches!(result, RecoveryResult::Success { .. }));
    }

    #[tokio::test]
    async fn test_recovery_engine_rebase_branch() {
        let mut engine = RecoveryEngine::new();

        let result = engine
            .attempt_recovery(&FailureScenario::StaleBranch, &serde_json::json!({}))
            .await;

        assert!(matches!(result, RecoveryResult::Success { .. }));
    }

    #[tokio::test]
    async fn test_recovery_engine_no_recipe() {
        let mut engine = RecoveryEngine::new();

        let result = engine
            .attempt_recovery(
                &FailureScenario::Unknown("test".to_string()),
                &serde_json::json!({}),
            )
            .await;

        assert!(matches!(result, RecoveryResult::Skipped { .. }));
    }
}
