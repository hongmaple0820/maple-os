use serde::{Deserialize, Serialize};

/// Post-Ready Step Queue — ordered initialization steps after agent is ready
///
/// Manages a queue of initialization steps that execute after the agent
/// reaches the Ready phase. Supports:
/// - Ordered execution with dependencies
/// - Retry on failure
/// - Timeout per step
/// - Skip on failure (continue to next step)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitStep {
    pub id: String,
    pub name: String,
    /// Step IDs that must complete before this one
    pub depends_on: Vec<String>,
    pub status: StepStatus,
    /// Maximum time allowed (seconds)
    pub timeout_secs: Option<u64>,
    /// Whether to skip on failure (continue to next step)
    pub skip_on_failure: bool,
    /// Number of retry attempts on failure
    pub max_retries: u32,
    pub attempt: u32,
    pub error: Option<String>,
}

impl InitStep {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            depends_on: Vec::new(),
            status: StepStatus::Pending,
            timeout_secs: None,
            skip_on_failure: false,
            max_retries: 0,
            attempt: 0,
            error: None,
        }
    }

    pub fn depends_on(mut self, dep_id: String) -> Self {
        self.depends_on.push(dep_id);
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    pub fn skip_on_failure(mut self) -> Self {
        self.skip_on_failure = true;
        self
    }

    pub fn with_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }
}

/// Manages ordered initialization steps
#[derive(Debug)]
pub struct StepQueue {
    steps: Vec<InitStep>,
}

impl StepQueue {
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// Add an initialization step
    pub fn add(&mut self, step: InitStep) {
        self.steps.push(step);
    }

    /// Get the next step that is ready to execute
    /// (all dependencies completed, step is Pending)
    pub fn next_ready(&self) -> Option<&InitStep> {
        self.steps.iter().find(|s| {
            s.status == StepStatus::Pending
                && s.depends_on.iter().all(|dep_id| {
                    self.steps
                        .iter()
                        .any(|d| d.id == *dep_id && d.status == StepStatus::Completed)
                })
        })
    }

    /// Mark a step as running
    pub fn start(&mut self, id: &str) -> Result<(), StepError> {
        let step = self.find_mut(id)?;
        if step.status != StepStatus::Pending {
            return Err(StepError::InvalidTransition {
                from: step.status,
                to: StepStatus::Running,
            });
        }
        step.status = StepStatus::Running;
        step.attempt += 1;
        Ok(())
    }

    /// Mark a step as completed
    pub fn complete(&mut self, id: &str) -> Result<(), StepError> {
        let step = self.find_mut(id)?;
        if step.status != StepStatus::Running {
            return Err(StepError::InvalidTransition {
                from: step.status,
                to: StepStatus::Completed,
            });
        }
        step.status = StepStatus::Completed;
        Ok(())
    }

    /// Mark a step as failed; retry or skip if configured
    pub fn fail(&mut self, id: &str, error: String) -> Result<StepAction, StepError> {
        let step = self.find_mut(id)?;
        if step.status != StepStatus::Running {
            return Err(StepError::InvalidTransition {
                from: step.status,
                to: StepStatus::Failed,
            });
        }

        step.error = Some(error);

        // Check if we can retry
        if step.attempt <= step.max_retries {
            step.status = StepStatus::Pending;
            return Ok(StepAction::Retry);
        }

        // Check if we should skip
        if step.skip_on_failure {
            step.status = StepStatus::Skipped;
            return Ok(StepAction::Skip);
        }

        step.status = StepStatus::Failed;
        Ok(StepAction::Abort)
    }

    /// Check if all steps are in a terminal state
    pub fn is_done(&self) -> bool {
        self.steps.iter().all(|s| {
            matches!(
                s.status,
                StepStatus::Completed | StepStatus::Failed | StepStatus::Skipped
            )
        })
    }

    /// Check if any step failed (not skipped)
    pub fn has_failures(&self) -> bool {
        self.steps.iter().any(|s| s.status == StepStatus::Failed)
    }

    /// Get all steps
    pub fn steps(&self) -> &[InitStep] {
        &self.steps
    }

    /// Get step by ID
    pub fn get(&self, id: &str) -> Option<&InitStep> {
        self.steps.iter().find(|s| s.id == id)
    }

    fn find_mut(&mut self, id: &str) -> Result<&mut InitStep, StepError> {
        self.steps
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| StepError::NotFound(id.to_string()))
    }
}

impl Default for StepQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Action to take after a step failure
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepAction {
    /// Retry the step
    Retry,
    /// Skip and continue to next
    Skip,
    /// Abort the queue
    Abort,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum StepError {
    #[error("step not found: {0}")]
    NotFound(String),
    #[error("invalid transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: StepStatus,
        to: StepStatus,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequential_steps() {
        let mut queue = StepQueue::new();
        queue.add(InitStep::new("s1".into(), "Load config".into()));
        queue.add(InitStep::new("s2".into(), "Connect DB".into()));

        // s1 is ready
        let next = queue.next_ready().unwrap();
        assert_eq!(next.id, "s1");

        queue.start("s1").unwrap();
        queue.complete("s1").unwrap();

        // s2 is now ready
        let next = queue.next_ready().unwrap();
        assert_eq!(next.id, "s2");

        queue.start("s2").unwrap();
        queue.complete("s2").unwrap();

        assert!(queue.is_done());
        assert!(!queue.has_failures());
    }

    #[test]
    fn test_dependency_ordering() {
        let mut queue = StepQueue::new();
        queue.add(
            InitStep::new("s2".into(), "Second".into()).depends_on("s1".into()),
        );
        queue.add(InitStep::new("s1".into(), "First".into()));

        // s2 is not ready (s1 not done)
        let next = queue.next_ready().unwrap();
        assert_eq!(next.id, "s1");
    }

    #[test]
    fn test_retry_on_failure() {
        let mut queue = StepQueue::new();
        queue.add(InitStep::new("s1".into(), "Step".into()).with_retries(2));

        queue.start("s1").unwrap();
        let action = queue.fail("s1", "error".into()).unwrap();
        assert_eq!(action, StepAction::Retry);

        // Retry
        queue.start("s1").unwrap();
        let action = queue.fail("s1", "error".into()).unwrap();
        assert_eq!(action, StepAction::Retry);

        // Third attempt (max_retries=2, attempt starts at 1, so attempt=3 > 1+2=3, abort)
        queue.start("s1").unwrap();
        let action = queue.fail("s1", "error".into()).unwrap();
        assert_eq!(action, StepAction::Abort);
    }

    #[test]
    fn test_skip_on_failure() {
        let mut queue = StepQueue::new();
        queue.add(InitStep::new("s1".into(), "Optional".into()).skip_on_failure());
        queue.add(InitStep::new("s2".into(), "Next".into()));

        queue.start("s1").unwrap();
        let action = queue.fail("s1", "error".into()).unwrap();
        assert_eq!(action, StepAction::Skip);

        // s2 should still be ready
        let next = queue.next_ready().unwrap();
        assert_eq!(next.id, "s2");
    }

    #[test]
    fn test_abort_on_failure() {
        let mut queue = StepQueue::new();
        queue.add(InitStep::new("s1".into(), "Critical".into()));

        queue.start("s1").unwrap();
        let action = queue.fail("s1", "error".into()).unwrap();
        assert_eq!(action, StepAction::Abort);

        assert!(queue.has_failures());
        assert!(queue.is_done()); // Failed is terminal
    }

    #[test]
    fn test_invalid_transition() {
        let mut queue = StepQueue::new();
        queue.add(InitStep::new("s1".into(), "Step".into()));

        // Can't complete a pending step
        let result = queue.complete("s1");
        assert!(matches!(
            result,
            Err(StepError::InvalidTransition { .. })
        ));
    }
}
