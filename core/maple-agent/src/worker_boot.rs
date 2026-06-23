use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Worker Boot State Machine — lifecycle management for subagent workers
///
/// States: Spawn → Trust → Permission → Ready → Running → Stopped
/// Each transition validates prerequisites before allowing progression.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootPhase {
    /// Worker process spawned, no trust established
    Spawn,
    /// Trust verification in progress (identity, provenance)
    Trust,
    /// Permission assignment in progress (tool subset, file access)
    Permission,
    /// Worker ready to accept tasks
    Ready,
    /// Worker actively executing a task
    Running,
    /// Worker stopped (completed, failed, or terminated)
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    /// Task completed successfully
    Completed,
    /// Task failed with error
    Failed,
    /// Worker was terminated by orchestrator
    Terminated,
    /// Worker timed out
    TimedOut,
    /// Health check failed
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootConfig {
    /// Maximum time allowed for boot sequence
    pub boot_timeout: Duration,
    /// Maximum time allowed for a single task
    pub task_timeout: Duration,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Maximum consecutive health check failures before stop
    pub max_health_failures: u32,
}

impl Default for BootConfig {
    fn default() -> Self {
        Self {
            boot_timeout: Duration::from_secs(30),
            task_timeout: Duration::from_secs(300),
            health_check_interval: Duration::from_secs(10),
            max_health_failures: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerState {
    pub phase: BootPhase,
    pub worker_id: String,
    pub task_id: Option<String>,
    pub stop_reason: Option<StopReason>,
    pub health_failures: u32,
    pub started_at: Option<i64>,
    pub phase_entered_at: i64,
}

/// State machine for worker boot lifecycle
#[derive(Debug)]
pub struct WorkerBootMachine {
    config: BootConfig,
    state: WorkerState,
    boot_start: Instant,
    phase_start: Instant,
}

impl WorkerBootMachine {
    pub fn new(worker_id: String, config: BootConfig) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            config,
            state: WorkerState {
                phase: BootPhase::Spawn,
                worker_id,
                task_id: None,
                stop_reason: None,
                health_failures: 0,
                started_at: None,
                phase_entered_at: now,
            },
            boot_start: Instant::now(),
            phase_start: Instant::now(),
        }
    }

    pub fn state(&self) -> &WorkerState {
        &self.state
    }

    pub fn phase(&self) -> BootPhase {
        self.state.phase
    }

    /// Transition to Trust phase (Spawn → Trust)
    pub fn begin_trust(&mut self) -> Result<(), BootError> {
        self.require_phase(BootPhase::Spawn)?;
        self.transition(BootPhase::Trust)
    }

    /// Complete trust verification (Trust → Permission)
    pub fn complete_trust(&mut self) -> Result<(), BootError> {
        self.require_phase(BootPhase::Trust)?;
        self.transition(BootPhase::Permission)
    }

    /// Complete permission assignment (Permission → Ready)
    pub fn complete_permission(&mut self) -> Result<(), BootError> {
        self.require_phase(BootPhase::Permission)?;
        self.transition(BootPhase::Ready)
    }

    /// Start task execution (Ready → Running)
    pub fn start_task(&mut self, task_id: String) -> Result<(), BootError> {
        self.require_phase(BootPhase::Ready)?;
        self.state.task_id = Some(task_id);
        self.state.started_at = Some(chrono::Utc::now().timestamp());
        self.transition(BootPhase::Running)
    }

    /// Complete task (Running → Ready)
    pub fn complete_task(&mut self) -> Result<(), BootError> {
        self.require_phase(BootPhase::Running)?;
        self.state.task_id = None;
        self.state.started_at = None;
        self.transition(BootPhase::Ready)
    }

    /// Stop worker (any active phase → Stopped)
    pub fn stop(&mut self, reason: StopReason) -> Result<(), BootError> {
        if self.state.phase == BootPhase::Stopped {
            return Err(BootError::AlreadyStopped);
        }
        self.state.stop_reason = Some(reason);
        self.transition(BootPhase::Stopped)
    }

    /// Record a health check failure; auto-stop if threshold exceeded
    pub fn record_health_failure(&mut self) -> Result<Option<StopReason>, BootError> {
        if self.state.phase == BootPhase::Stopped {
            return Err(BootError::AlreadyStopped);
        }
        self.state.health_failures += 1;
        if self.state.health_failures >= self.config.max_health_failures {
            self.stop(StopReason::Unhealthy)?;
            Ok(Some(StopReason::Unhealthy))
        } else {
            Ok(None)
        }
    }

    /// Reset health failure counter
    pub fn reset_health_failures(&mut self) {
        self.state.health_failures = 0;
    }

    /// Check if boot sequence has timed out
    pub fn is_boot_timeout(&self) -> bool {
        matches!(
            self.state.phase,
            BootPhase::Spawn | BootPhase::Trust | BootPhase::Permission
        ) && self.boot_start.elapsed() > self.config.boot_timeout
    }

    /// Check if current task has timed out
    pub fn is_task_timeout(&self) -> bool {
        if self.state.phase != BootPhase::Running {
            return false;
        }
        if let Some(started) = self.state.started_at {
            let elapsed = chrono::Utc::now().timestamp() - started;
            elapsed > self.config.task_timeout.as_secs() as i64
        } else {
            false
        }
    }

    /// Duration in current phase
    pub fn phase_duration(&self) -> Duration {
        self.phase_start.elapsed()
    }

    fn require_phase(&self, expected: BootPhase) -> Result<(), BootError> {
        if self.state.phase != expected {
            Err(BootError::InvalidTransition {
                from: self.state.phase,
                to: expected,
            })
        } else {
            Ok(())
        }
    }

    fn transition(&mut self, new_phase: BootPhase) -> Result<(), BootError> {
        self.state.phase = new_phase;
        self.state.phase_entered_at = chrono::Utc::now().timestamp();
        self.phase_start = Instant::now();
        Ok(())
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum BootError {
    #[error("invalid transition from {from:?} to {to:?}")]
    InvalidTransition { from: BootPhase, to: BootPhase },
    #[error("worker is already stopped")]
    AlreadyStopped,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_lifecycle() {
        let mut boot = WorkerBootMachine::new("w1".into(), BootConfig::default());
        assert_eq!(boot.phase(), BootPhase::Spawn);

        boot.begin_trust().unwrap();
        assert_eq!(boot.phase(), BootPhase::Trust);

        boot.complete_trust().unwrap();
        assert_eq!(boot.phase(), BootPhase::Permission);

        boot.complete_permission().unwrap();
        assert_eq!(boot.phase(), BootPhase::Ready);

        boot.start_task("task-1".into()).unwrap();
        assert_eq!(boot.phase(), BootPhase::Running);
        assert_eq!(boot.state().task_id.as_deref(), Some("task-1"));

        boot.complete_task().unwrap();
        assert_eq!(boot.phase(), BootPhase::Ready);

        boot.stop(StopReason::Completed).unwrap();
        assert_eq!(boot.phase(), BootPhase::Stopped);
        assert_eq!(boot.state().stop_reason, Some(StopReason::Completed));
    }

    #[test]
    fn test_invalid_transition() {
        let mut boot = WorkerBootMachine::new("w1".into(), BootConfig::default());
        // Can't go directly from Spawn to Running
        let result = boot.start_task("t1".into());
        assert!(result.is_err());
    }

    #[test]
    fn test_stop_from_any_phase() {
        let mut boot = WorkerBootMachine::new("w1".into(), BootConfig::default());
        boot.stop(StopReason::Terminated).unwrap();
        assert_eq!(boot.phase(), BootPhase::Stopped);
    }

    #[test]
    fn test_already_stopped() {
        let mut boot = WorkerBootMachine::new("w1".into(), BootConfig::default());
        boot.stop(StopReason::Completed).unwrap();
        let result = boot.stop(StopReason::Failed);
        assert!(matches!(result, Err(BootError::AlreadyStopped)));
    }

    #[test]
    fn test_health_failure_threshold() {
        let config = BootConfig {
            max_health_failures: 2,
            ..Default::default()
        };
        let mut boot = WorkerBootMachine::new("w1".into(), config);
        boot.begin_trust().unwrap();
        boot.complete_trust().unwrap();
        boot.complete_permission().unwrap();

        // First failure
        let result = boot.record_health_failure().unwrap();
        assert!(result.is_none());

        // Second failure → auto-stop
        let result = boot.record_health_failure().unwrap();
        assert_eq!(result, Some(StopReason::Unhealthy));
        assert_eq!(boot.phase(), BootPhase::Stopped);
    }

    #[test]
    fn test_task_lifecycle() {
        let mut boot = WorkerBootMachine::new("w1".into(), BootConfig::default());
        boot.begin_trust().unwrap();
        boot.complete_trust().unwrap();
        boot.complete_permission().unwrap();

        boot.start_task("t1".into()).unwrap();
        boot.complete_task().unwrap();
        boot.start_task("t2".into()).unwrap();
        assert_eq!(boot.state().task_id.as_deref(), Some("t2"));
    }
}
