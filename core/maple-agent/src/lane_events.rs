use serde::{Deserialize, Serialize};

/// Lane Events + Policy Engine — parallel workstream management
///
/// Manages parallel execution lanes with policy-based control:
/// - Lanes: independent workstreams with ordered steps
/// - Policies: rules that govern lane behavior
/// - Events: lifecycle notifications for monitoring
///
///   Lane status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaneStatus {
    Idle,
    Running,
    Completed,
    Failed,
    Paused,
}

/// Lane definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lane {
    pub id: String,
    pub name: String,
    pub steps: Vec<String>,
    pub policy: LanePolicy,
    pub status: LaneStatus,
    pub current_step_idx: usize,
}

/// Lane policy constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanePolicy {
    pub max_concurrent_tools: Option<usize>,
    pub on_failure: FailurePolicy,
    pub requires_approval: Vec<String>,
    pub allowed_tools: Option<Vec<String>>,
}

impl Default for LanePolicy {
    fn default() -> Self {
        Self {
            max_concurrent_tools: None,
            on_failure: FailurePolicy::Abort,
            requires_approval: Vec::new(),
            allowed_tools: None,
        }
    }
}

/// Failure handling policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailurePolicy {
    Abort,
    Skip,
    Retry { max_attempts: u32 },
}

/// Lane event for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LaneEvent {
    LaneStarted { lane_id: String },
    StepStarted { lane_id: String, step_id: String },
    StepCompleted { lane_id: String, step_id: String },
    StepFailed { lane_id: String, step_id: String, error: String },
    LaneCompleted { lane_id: String },
    LaneFailed { lane_id: String, error: String },
    LanePaused { lane_id: String, reason: String },
}

/// Policy rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub id: String,
    pub name: String,
    pub condition: PolicyCondition,
    pub action: PolicyAction,
}

/// Policy condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyCondition {
    OnLaneComplete(String),
    OnStepFailure(String),
    OnToolUse(String),
    Always,
    Never,
}

/// Policy action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyAction {
    Allow,
    Deny { reason: String },
    Pause { reason: String },
    Retry,
}

/// Lane manager
pub struct LaneManager {
    lanes: Vec<Lane>,
    rules: Vec<PolicyRule>,
    events: Vec<LaneEvent>,
}

impl LaneManager {
    pub fn new() -> Self {
        Self {
            lanes: Vec::new(),
            rules: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Add a lane
    pub fn add_lane(&mut self, lane: Lane) {
        self.lanes.push(lane);
    }

    /// Add a policy rule
    pub fn add_rule(&mut self, rule: PolicyRule) {
        self.rules.push(rule);
    }

    /// Get all lanes
    pub fn lanes(&self) -> &[Lane] {
        &self.lanes
    }

    /// Get event log
    pub fn events(&self) -> &[LaneEvent] {
        &self.events
    }

    /// Start a lane
    pub fn start_lane(&mut self, lane_id: &str) -> Result<(), LaneError> {
        let lane = self.find_lane_mut(lane_id)?;
        if lane.status != LaneStatus::Idle {
            return Err(LaneError::InvalidState(lane.status));
        }
        lane.status = LaneStatus::Running;
        lane.current_step_idx = 0;
        self.events.push(LaneEvent::LaneStarted {
            lane_id: lane_id.into(),
        });
        Ok(())
    }

    /// Get the current step for a lane
    pub fn current_step(&self, lane_id: &str) -> Option<&str> {
        let lane = self.lanes.iter().find(|l| l.id == lane_id)?;
        if lane.status != LaneStatus::Running {
            return None;
        }
        lane.steps.get(lane.current_step_idx).map(|s| s.as_str())
    }

    /// Complete current step and advance
    pub fn complete_step(&mut self, lane_id: &str) -> Result<Option<String>, LaneError> {
        let lane_idx = self
            .lanes
            .iter()
            .position(|l| l.id == lane_id)
            .ok_or_else(|| LaneError::NotFound(lane_id.into()))?;

        if self.lanes[lane_idx].status != LaneStatus::Running {
            return Err(LaneError::InvalidState(self.lanes[lane_idx].status));
        }

        let step_id = self.lanes[lane_idx]
            .steps
            .get(self.lanes[lane_idx].current_step_idx)
            .cloned()
            .ok_or(LaneError::NoMoreSteps)?;

        self.events.push(LaneEvent::StepCompleted {
            lane_id: lane_id.into(),
            step_id,
        });

        self.lanes[lane_idx].current_step_idx += 1;

        if self.lanes[lane_idx].current_step_idx >= self.lanes[lane_idx].steps.len() {
            self.lanes[lane_idx].status = LaneStatus::Completed;
            self.events.push(LaneEvent::LaneCompleted {
                lane_id: lane_id.into(),
            });
            Ok(None)
        } else {
            let next = self.lanes[lane_idx].steps[self.lanes[lane_idx].current_step_idx].clone();
            self.events.push(LaneEvent::StepStarted {
                lane_id: lane_id.into(),
                step_id: next.clone(),
            });
            Ok(Some(next))
        }
    }

    /// Fail current step
    pub fn fail_step(&mut self, lane_id: &str, error: String) -> Result<(), LaneError> {
        let lane_idx = self
            .lanes
            .iter()
            .position(|l| l.id == lane_id)
            .ok_or_else(|| LaneError::NotFound(lane_id.into()))?;

        if self.lanes[lane_idx].status != LaneStatus::Running {
            return Err(LaneError::InvalidState(self.lanes[lane_idx].status));
        }

        let step_id = self.lanes[lane_idx]
            .steps
            .get(self.lanes[lane_idx].current_step_idx)
            .cloned()
            .unwrap_or_default();

        self.events.push(LaneEvent::StepFailed {
            lane_id: lane_id.into(),
            step_id,
            error: error.clone(),
        });

        match self.lanes[lane_idx].policy.on_failure {
            FailurePolicy::Abort => {
                self.lanes[lane_idx].status = LaneStatus::Failed;
                self.events.push(LaneEvent::LaneFailed {
                    lane_id: lane_id.into(),
                    error,
                });
            }
            FailurePolicy::Skip => {
                self.lanes[lane_idx].current_step_idx += 1;
                if self.lanes[lane_idx].current_step_idx >= self.lanes[lane_idx].steps.len() {
                    self.lanes[lane_idx].status = LaneStatus::Completed;
                }
            }
            FailurePolicy::Retry { .. } => {
                // Stay on current step for retry
            }
        }

        Ok(())
    }

    /// Check if a step requires approval
    pub fn requires_approval(&self, lane_id: &str, step_id: &str) -> bool {
        self.lanes
            .iter()
            .find(|l| l.id == lane_id)
            .map(|l| l.policy.requires_approval.contains(&step_id.to_string()))
            .unwrap_or(false)
    }

    /// Evaluate policy rules
    pub fn evaluate_rules(&self, condition: &PolicyCondition) -> Vec<&PolicyAction> {
        self.rules
            .iter()
            .filter(|r| match (&r.condition, condition) {
                (PolicyCondition::Always, _) => true,
                (PolicyCondition::Never, _) => false,
                (a, b) => std::mem::discriminant(a) == std::mem::discriminant(b),
            })
            .map(|r| &r.action)
            .collect()
    }

    /// Get lanes that have completed
    pub fn completed_lanes(&self) -> Vec<&Lane> {
        self.lanes
            .iter()
            .filter(|l| l.status == LaneStatus::Completed)
            .collect()
    }

    /// Get lanes that have failed
    pub fn failed_lanes(&self) -> Vec<&Lane> {
        self.lanes
            .iter()
            .filter(|l| l.status == LaneStatus::Failed)
            .collect()
    }

    fn find_lane_mut(&mut self, id: &str) -> Result<&mut Lane, LaneError> {
        self.lanes
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or_else(|| LaneError::NotFound(id.into()))
    }
}

impl Default for LaneManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LaneError {
    #[error("lane not found: {0}")]
    NotFound(String),
    #[error("invalid lane state: {0:?}")]
    InvalidState(LaneStatus),
    #[error("no more steps in lane")]
    NoMoreSteps,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_lane(id: &str, steps: Vec<&str>) -> Lane {
        Lane {
            id: id.into(),
            name: format!("Lane {}", id),
            steps: steps.into_iter().map(String::from).collect(),
            policy: LanePolicy::default(),
            status: LaneStatus::Idle,
            current_step_idx: 0,
        }
    }

    #[test]
    fn test_lane_lifecycle() {
        let mut mgr = LaneManager::new();
        mgr.add_lane(test_lane("l1", vec!["s1", "s2", "s3"]));

        mgr.start_lane("l1").unwrap();
        assert_eq!(mgr.current_step("l1"), Some("s1"));

        let next = mgr.complete_step("l1").unwrap();
        assert_eq!(next.as_deref(), Some("s2"));

        let next = mgr.complete_step("l1").unwrap();
        assert_eq!(next.as_deref(), Some("s3"));

        let next = mgr.complete_step("l1").unwrap();
        assert!(next.is_none());
        assert_eq!(mgr.lanes()[0].status, LaneStatus::Completed);
    }

    #[test]
    fn test_failure_abort() {
        let mut mgr = LaneManager::new();
        mgr.add_lane(Lane {
            policy: LanePolicy {
                on_failure: FailurePolicy::Abort,
                ..Default::default()
            },
            ..test_lane("l1", vec!["s1", "s2"])
        });

        mgr.start_lane("l1").unwrap();
        mgr.fail_step("l1", "error".into()).unwrap();

        assert_eq!(mgr.lanes()[0].status, LaneStatus::Failed);
    }

    #[test]
    fn test_failure_skip() {
        let mut mgr = LaneManager::new();
        mgr.add_lane(Lane {
            policy: LanePolicy {
                on_failure: FailurePolicy::Skip,
                ..Default::default()
            },
            ..test_lane("l1", vec!["s1", "s2"])
        });

        mgr.start_lane("l1").unwrap();
        mgr.fail_step("l1", "error".into()).unwrap();

        // Skipped to next step
        assert_eq!(mgr.current_step("l1"), Some("s2"));
    }

    #[test]
    fn test_requires_approval() {
        let mut mgr = LaneManager::new();
        mgr.add_lane(Lane {
            policy: LanePolicy {
                requires_approval: vec!["deploy".into()],
                ..Default::default()
            },
            ..test_lane("l1", vec!["build", "test", "deploy"])
        });

        assert!(mgr.requires_approval("l1", "deploy"));
        assert!(!mgr.requires_approval("l1", "build"));
    }

    #[test]
    fn test_policy_rules() {
        let mut mgr = LaneManager::new();
        mgr.add_rule(PolicyRule {
            id: "r1".into(),
            name: "Always allow".into(),
            condition: PolicyCondition::Always,
            action: PolicyAction::Allow,
        });
        mgr.add_rule(PolicyRule {
            id: "r2".into(),
            name: "Never deny".into(),
            condition: PolicyCondition::Never,
            action: PolicyAction::Deny {
                reason: "blocked".into(),
            },
        });
        mgr.add_rule(PolicyRule {
            id: "r3".into(),
            name: "On failure".into(),
            condition: PolicyCondition::OnStepFailure("s1".into()),
            action: PolicyAction::Retry,
        });

        // Always rule always matches (1 result: r1)
        let actions = mgr.evaluate_rules(&PolicyCondition::Always);
        assert_eq!(actions.len(), 1);

        // OnStepFailure: Always + OnStepFailure both match (2 results)
        let actions = mgr.evaluate_rules(&PolicyCondition::OnStepFailure("s1".into()));
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn test_events() {
        let mut mgr = LaneManager::new();
        mgr.add_lane(test_lane("l1", vec!["s1"]));

        mgr.start_lane("l1").unwrap();
        mgr.complete_step("l1").unwrap();

        assert!(mgr.events().iter().any(|e| matches!(
            e,
            LaneEvent::LaneStarted { lane_id } if lane_id == "l1"
        )));
        assert!(mgr.events().iter().any(|e| matches!(
            e,
            LaneEvent::LaneCompleted { lane_id } if lane_id == "l1"
        )));
    }

    #[test]
    fn test_completed_failed_lanes() {
        let mut mgr = LaneManager::new();
        mgr.add_lane(test_lane("l1", vec!["s1"]));
        mgr.add_lane(Lane {
            policy: LanePolicy {
                on_failure: FailurePolicy::Abort,
                ..Default::default()
            },
            ..test_lane("l2", vec!["s1"])
        });

        mgr.start_lane("l1").unwrap();
        mgr.complete_step("l1").unwrap();

        mgr.start_lane("l2").unwrap();
        mgr.fail_step("l2", "err".into()).unwrap();

        assert_eq!(mgr.completed_lanes().len(), 1);
        assert_eq!(mgr.failed_lanes().len(), 1);
    }
}
