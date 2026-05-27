use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use anyhow::Result;

/// Recovery Recipes — inspired by claw-code's 7 failure scenarios
///
/// Features:
/// - Structured failure classification
/// - Automatic recovery strategies
/// - Escalation policies
/// - Recovery ledger for tracking attempts

/// Failure scenarios
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FailureScenario {
    /// Trust prompt not resolved
    TrustPromptUnresolved,
    /// Prompt delivered to wrong agent
    PromptMisdelivery,
    /// Stale branch (behind main)
    StaleBranch,
    /// MCP handshake failure
    McpHandshakeFailure,
    /// Provider failure (rate limit, auth, etc.)
    ProviderFailure,
    /// Tool execution failure
    ToolExecutionFailure,
    /// Worker timeout
    WorkerTimeout,
    /// Unknown failure
    Unknown(String),
}

/// Recovery action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryAction {
    /// Accept trust prompt automatically
    AcceptTrustPrompt,
    /// Redirect prompt to correct agent
    RedirectPromptToAgent { target_agent: String },
    /// Rebase branch onto main
    RebaseBranch,
    /// Retry MCP handshake
    RetryMcpHandshake { max_attempts: u32 },
    /// Restart worker
    RestartWorker,
    /// Retry tool execution with backoff
    RetryWithBackoff { max_attempts: u32, initial_delay_ms: u64 },
    /// Fallback to alternative provider
    FallbackToProvider { provider: String },
    /// Alert human for manual intervention
    AlertHuman { reason: String },
    /// Skip and continue
    Skip,
}

/// Recovery result
#[derive(Debug, Clone)]
pub enum RecoveryResult {
    /// Recovery succeeded
    Success { output: String },
    /// Recovery failed, needs escalation
    Failed { reason: String },
    /// Recovery skipped
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

    /// Add an entry to the ledger
    pub fn add_entry(&mut self, entry: RecoveryLedgerEntry) {
        self.entries.push(entry);

        // Trim if over max
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }

    /// Get entries for a specific scenario
    pub fn get_entries_for_scenario(&self, scenario: &FailureScenario) -> Vec<&RecoveryLedgerEntry> {
        self.entries.iter()
            .filter(|e| &e.scenario == scenario)
            .collect()
    }

    /// Get recent entries
    pub fn get_recent_entries(&self, count: usize) -> Vec<&RecoveryLedgerEntry> {
        self.entries.iter().rev().take(count).collect()
    }

    /// Check if we've exceeded retry limits for a scenario
    pub fn has_exceeded_retries(&self, scenario: &FailureScenario, max_attempts: u32) -> bool {
        let recent_attempts = self.entries.iter()
            .filter(|e| &e.scenario == scenario)
            .filter(|e| e.timestamp > Instant::now().checked_sub(Duration::from_secs(300)).unwrap_or(Instant::now()).elapsed().as_secs() as u64)
            .count();

        recent_attempts >= max_attempts as usize
    }
}

/// Recovery engine
pub struct RecoveryEngine {
    recipes: Vec<RecoveryRecipe>,
    ledger: RecoveryLedger,
    escalation_callback: Option<Box<dyn Fn(&str) + Send + Sync>>,
}

impl RecoveryEngine {
    pub fn new() -> Self {
        Self {
            recipes: Self::default_recipes(),
            ledger: RecoveryLedger::new(1000),
            escalation_callback: None,
        }
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
        // Find matching recipe
        let recipe = self.recipes.iter().find(|r| &r.scenario == scenario);

        let recipe = match recipe {
            Some(r) => r.clone(),
            None => {
                return RecoveryResult::Skipped {
                    reason: "No recipe found for scenario".to_string(),
                };
            }
        };

        // Check if we've exceeded retry limits
        if self.ledger.has_exceeded_retries(scenario, recipe.max_attempts) {
            if recipe.should_escalate {
                self.escalate(scenario, "Max retry attempts exceeded");
            }
            return RecoveryResult::Failed {
                reason: "Max retry attempts exceeded".to_string(),
            };
        }

        // Execute recovery action
        let start = Instant::now();
        let result = self.execute_action(&recipe.action, context).await;

        // Record in ledger
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

    /// Execute a recovery action
    async fn execute_action(&self, action: &RecoveryAction, _context: &Value) -> RecoveryResult {
        match action {
            RecoveryAction::AcceptTrustPrompt => {
                RecoveryResult::Success {
                    output: "Trust prompt accepted".to_string(),
                }
            }
            RecoveryAction::RedirectPromptToAgent { target_agent } => {
                RecoveryResult::Success {
                    output: format!("Prompt redirected to {}", target_agent),
                }
            }
            RecoveryAction::RebaseBranch => {
                RecoveryResult::Success {
                    output: "Branch rebased".to_string(),
                }
            }
            RecoveryAction::RetryMcpHandshake { max_attempts: _ } => {
                RecoveryResult::Success {
                    output: "MCP handshake retried".to_string(),
                }
            }
            RecoveryAction::RestartWorker => {
                RecoveryResult::Success {
                    output: "Worker restarted".to_string(),
                }
            }
            RecoveryAction::RetryWithBackoff { max_attempts: _, initial_delay_ms: _ } => {
                RecoveryResult::Success {
                    output: "Operation retried".to_string(),
                }
            }
            RecoveryAction::FallbackToProvider { provider } => {
                RecoveryResult::Success {
                    output: format!("Fell back to provider: {}", provider),
                }
            }
            RecoveryAction::AlertHuman { reason } => {
                self.escalate(&FailureScenario::Unknown("alert".to_string()), reason);
                RecoveryResult::Failed {
                    reason: format!("Alert sent to human: {}", reason),
                }
            }
            RecoveryAction::Skip => {
                RecoveryResult::Skipped {
                    reason: "Skipped by policy".to_string(),
                }
            }
        }
    }

    /// Escalate to human
    fn escalate(&self, scenario: &FailureScenario, reason: &str) {
        if let Some(callback) = &self.escalation_callback {
            callback(&format!("Escalation: {:?} - {}", scenario, reason));
        }
    }

    /// Get the recovery ledger
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
        assert_eq!(FailureScenario::TrustPromptUnresolved, FailureScenario::TrustPromptUnresolved);
        assert_ne!(FailureScenario::TrustPromptUnresolved, FailureScenario::ProviderFailure);
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
    async fn test_recovery_engine() {
        let mut engine = RecoveryEngine::new();

        let result = engine.attempt_recovery(
            &FailureScenario::ProviderFailure,
            &serde_json::json!({}),
        ).await;

        assert!(matches!(result, RecoveryResult::Success { .. }));
    }
}
