use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// IterationBudget — controls resource consumption per session/request
///
/// Inspired by hermes-agent's budget system with grace call mechanism.
/// Prevents runaway loops and cost overruns while allowing graceful completion.

#[derive(Debug, Clone)]
pub struct IterationBudget {
    /// Maximum iterations (tool use rounds)
    pub max_iterations: usize,
    /// Maximum total tokens (input + output)
    pub max_total_tokens: usize,
    /// Maximum input tokens
    pub max_input_tokens: usize,
    /// Maximum output tokens
    pub max_output_tokens: usize,
    /// Maximum cost in USD
    pub max_cost_usd: f64,
    /// Maximum wall-clock time
    pub max_duration: Duration,
    /// Grace calls allowed after budget exceeded (for cleanup/finalization)
    pub grace_calls: usize,
}

impl Default for IterationBudget {
    fn default() -> Self {
        Self {
            max_iterations: 25,
            max_total_tokens: 100_000,
            max_input_tokens: 80_000,
            max_output_tokens: 20_000,
            max_cost_usd: 1.0,
            max_duration: Duration::from_secs(300), // 5 minutes
            grace_calls: 2,
        }
    }
}

impl IterationBudget {
    /// Create a new budget with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a builder for fluent configuration
    pub fn builder() -> IterationBudgetBuilder {
        IterationBudgetBuilder::new()
    }

    /// Check if the budget allows another iteration
    pub fn can_continue(&self, state: &BudgetState) -> BudgetDecision {
        // Check grace calls first
        if state.in_grace_period {
            if state.grace_calls_remaining > 0 {
                return BudgetDecision::GraceCall {
                    remaining: state.grace_calls_remaining,
                };
            } else {
                return BudgetDecision::Exhausted {
                    reason: "Grace calls exhausted".to_string(),
                };
            }
        }

        // Check iteration limit
        if state.iterations >= self.max_iterations {
            return BudgetDecision::StartGrace {
                reason: format!(
                    "Max iterations reached ({}/{})",
                    state.iterations, self.max_iterations
                ),
            };
        }

        // Check token limits
        let total_tokens = state.input_tokens + state.output_tokens;
        if total_tokens >= self.max_total_tokens {
            return BudgetDecision::StartGrace {
                reason: format!(
                    "Max total tokens reached ({}/{})",
                    total_tokens, self.max_total_tokens
                ),
            };
        }

        if state.input_tokens >= self.max_input_tokens {
            return BudgetDecision::StartGrace {
                reason: format!(
                    "Max input tokens reached ({}/{})",
                    state.input_tokens, self.max_input_tokens
                ),
            };
        }

        if state.output_tokens >= self.max_output_tokens {
            return BudgetDecision::StartGrace {
                reason: format!(
                    "Max output tokens reached ({}/{})",
                    state.output_tokens, self.max_output_tokens
                ),
            };
        }

        // Check cost limit
        if state.cost_usd >= self.max_cost_usd {
            return BudgetDecision::StartGrace {
                reason: format!(
                    "Max cost reached (${:.4}/${:.4})",
                    state.cost_usd, self.max_cost_usd
                ),
            };
        }

        // Check duration
        if state.elapsed() >= self.max_duration {
            return BudgetDecision::StartGrace {
                reason: format!(
                    "Max duration reached ({:.1}s/{:.1}s)",
                    state.elapsed().as_secs_f64(),
                    self.max_duration.as_secs_f64()
                ),
            };
        }

        BudgetDecision::Continue {
            remaining_iterations: self.max_iterations - state.iterations,
            remaining_tokens: self.max_total_tokens - total_tokens,
            remaining_cost: self.max_cost_usd - state.cost_usd,
        }
    }

    /// Get warning thresholds (80% of limits)
    pub fn warnings(&self, state: &BudgetState) -> Vec<BudgetWarning> {
        let mut warnings = Vec::new();

        let iteration_pct = state.iterations as f64 / self.max_iterations as f64;
        if iteration_pct >= 0.8 {
            warnings.push(BudgetWarning::IterationHigh {
                current: state.iterations,
                max: self.max_iterations,
                pct: iteration_pct * 100.0,
            });
        }

        let total_tokens = state.input_tokens + state.output_tokens;
        let token_pct = total_tokens as f64 / self.max_total_tokens as f64;
        if token_pct >= 0.8 {
            warnings.push(BudgetWarning::TokenHigh {
                current: total_tokens,
                max: self.max_total_tokens,
                pct: token_pct * 100.0,
            });
        }

        let cost_pct = state.cost_usd / self.max_cost_usd;
        if cost_pct >= 0.8 {
            warnings.push(BudgetWarning::CostHigh {
                current: state.cost_usd,
                max: self.max_cost_usd,
                pct: cost_pct * 100.0,
            });
        }

        warnings
    }
}

/// Current budget consumption state
#[derive(Debug, Clone)]
pub struct BudgetState {
    pub iterations: usize,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost_usd: f64,
    pub in_grace_period: bool,
    pub grace_calls_remaining: usize,
    started_at: Instant,
}

impl BudgetState {
    pub fn new(grace_calls: usize) -> Self {
        Self {
            iterations: 0,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            in_grace_period: false,
            grace_calls_remaining: grace_calls,
            started_at: Instant::now(),
        }
    }

    /// Record a completed iteration
    pub fn record_iteration(&mut self, input_tokens: usize, output_tokens: usize, cost_usd: f64) {
        self.iterations += 1;
        self.input_tokens += input_tokens;
        self.output_tokens += output_tokens;
        self.cost_usd += cost_usd;
    }

    /// Enter grace period
    pub fn enter_grace(&mut self) {
        self.in_grace_period = true;
    }

    /// Use a grace call
    pub fn use_grace_call(&mut self) {
        if self.grace_calls_remaining > 0 {
            self.grace_calls_remaining -= 1;
        }
    }

    /// Get elapsed time since start
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }
}

/// Budget decision — what to do next
#[derive(Debug, Clone)]
pub enum BudgetDecision {
    /// Budget OK, can continue
    Continue {
        remaining_iterations: usize,
        remaining_tokens: usize,
        remaining_cost: f64,
    },
    /// Budget exceeded, start grace period
    StartGrace { reason: String },
    /// In grace period, one more call allowed
    GraceCall { remaining: usize },
    /// Budget fully exhausted
    Exhausted { reason: String },
}

/// Budget warnings (80%+ thresholds)
#[derive(Debug, Clone)]
pub enum BudgetWarning {
    IterationHigh {
        current: usize,
        max: usize,
        pct: f64,
    },
    TokenHigh {
        current: usize,
        max: usize,
        pct: f64,
    },
    CostHigh {
        current: f64,
        max: f64,
        pct: f64,
    },
}

impl std::fmt::Display for BudgetWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BudgetWarning::IterationHigh {
                current,
                max,
                pct,
            } => write!(
                f,
                "Iteration budget at {:.0}% ({}/{})",
                pct, current, max
            ),
            BudgetWarning::TokenHigh {
                current,
                max,
                pct,
            } => write!(f, "Token budget at {:.0}% ({}/{})", pct, current, max),
            BudgetWarning::CostHigh {
                current,
                max,
                pct,
            } => write!(
                f,
                "Cost budget at {:.0}% (${:.4}/${:.4})",
                pct, current, max
            ),
        }
    }
}

/// Builder for IterationBudget
pub struct IterationBudgetBuilder {
    budget: IterationBudget,
}

impl IterationBudgetBuilder {
    pub fn new() -> Self {
        Self {
            budget: IterationBudget::default(),
        }
    }

    pub fn max_iterations(mut self, max: usize) -> Self {
        self.budget.max_iterations = max;
        self
    }

    pub fn max_total_tokens(mut self, max: usize) -> Self {
        self.budget.max_total_tokens = max;
        self
    }

    pub fn max_input_tokens(mut self, max: usize) -> Self {
        self.budget.max_input_tokens = max;
        self
    }

    pub fn max_output_tokens(mut self, max: usize) -> Self {
        self.budget.max_output_tokens = max;
        self
    }

    pub fn max_cost_usd(mut self, max: f64) -> Self {
        self.budget.max_cost_usd = max;
        self
    }

    pub fn max_duration(mut self, duration: Duration) -> Self {
        self.budget.max_duration = duration;
        self
    }

    pub fn grace_calls(mut self, calls: usize) -> Self {
        self.budget.grace_calls = calls;
        self
    }

    pub fn build(self) -> IterationBudget {
        self.budget
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_budget() {
        let budget = IterationBudget::default();
        assert_eq!(budget.max_iterations, 25);
        assert_eq!(budget.max_total_tokens, 100_000);
        assert_eq!(budget.grace_calls, 2);
    }

    #[test]
    fn test_budget_continue() {
        let budget = IterationBudget::default();
        let state = BudgetState::new(budget.grace_calls);

        match budget.can_continue(&state) {
            BudgetDecision::Continue {
                remaining_iterations,
                remaining_tokens,
                ..
            } => {
                assert_eq!(remaining_iterations, 25);
                assert_eq!(remaining_tokens, 100_000);
            }
            _ => panic!("Expected Continue"),
        }
    }

    #[test]
    fn test_budget_iteration_limit() {
        let budget = IterationBudget::builder()
            .max_iterations(5)
            .build();
        let mut state = BudgetState::new(budget.grace_calls);

        for _ in 0..5 {
            state.record_iteration(100, 50, 0.01);
        }

        match budget.can_continue(&state) {
            BudgetDecision::StartGrace { reason } => {
                assert!(reason.contains("Max iterations"));
            }
            _ => panic!("Expected StartGrace"),
        }
    }

    #[test]
    fn test_budget_grace_period() {
        let budget = IterationBudget::builder()
            .max_iterations(3)
            .grace_calls(2)
            .build();
        let mut state = BudgetState::new(budget.grace_calls);

        for _ in 0..3 {
            state.record_iteration(100, 50, 0.01);
        }

        // Should start grace
        assert!(matches!(
            budget.can_continue(&state),
            BudgetDecision::StartGrace { .. }
        ));

        // Enter grace
        state.enter_grace();

        // First grace call
        match budget.can_continue(&state) {
            BudgetDecision::GraceCall { remaining } => {
                assert_eq!(remaining, 2);
            }
            _ => panic!("Expected GraceCall"),
        }

        state.use_grace_call();

        // Second grace call
        match budget.can_continue(&state) {
            BudgetDecision::GraceCall { remaining } => {
                assert_eq!(remaining, 1);
            }
            _ => panic!("Expected GraceCall"),
        }

        state.use_grace_call();

        // Exhausted
        assert!(matches!(
            budget.can_continue(&state),
            BudgetDecision::Exhausted { .. }
        ));
    }

    #[test]
    fn test_budget_token_limit() {
        let budget = IterationBudget::builder()
            .max_total_tokens(1000)
            .build();
        let mut state = BudgetState::new(budget.grace_calls);

        state.record_iteration(500, 500, 0.01);

        assert!(matches!(
            budget.can_continue(&state),
            BudgetDecision::StartGrace { .. }
        ));
    }

    #[test]
    fn test_budget_cost_limit() {
        let budget = IterationBudget::builder()
            .max_cost_usd(0.10)
            .build();
        let mut state = BudgetState::new(budget.grace_calls);

        state.record_iteration(100, 50, 0.15);

        assert!(matches!(
            budget.can_continue(&state),
            BudgetDecision::StartGrace { .. }
        ));
    }

    #[test]
    fn test_budget_warnings() {
        let budget = IterationBudget::builder()
            .max_iterations(10)
            .max_total_tokens(1000)
            .max_cost_usd(1.0)
            .build();
        let mut state = BudgetState::new(budget.grace_calls);

        state.record_iteration(400, 400, 0.85);

        let warnings = budget.warnings(&state);
        assert!(!warnings.is_empty());

        // Should have cost warning (85%)
        assert!(warnings.iter().any(|w| matches!(w, BudgetWarning::CostHigh { .. })));

        // Should have token warning (80%)
        assert!(warnings.iter().any(|w| matches!(w, BudgetWarning::TokenHigh { .. })));
    }

    #[test]
    fn test_builder() {
        let budget = IterationBudget::builder()
            .max_iterations(50)
            .max_total_tokens(200_000)
            .max_input_tokens(150_000)
            .max_output_tokens(50_000)
            .max_cost_usd(5.0)
            .max_duration(Duration::from_secs(600))
            .grace_calls(3)
            .build();

        assert_eq!(budget.max_iterations, 50);
        assert_eq!(budget.max_total_tokens, 200_000);
        assert_eq!(budget.max_input_tokens, 150_000);
        assert_eq!(budget.max_output_tokens, 50_000);
        assert_eq!(budget.max_cost_usd, 5.0);
        assert_eq!(budget.max_duration, Duration::from_secs(600));
        assert_eq!(budget.grace_calls, 3);
    }
}
