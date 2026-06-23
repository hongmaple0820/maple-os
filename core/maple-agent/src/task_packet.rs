use serde::{Deserialize, Serialize};

/// Task Packets — structured handoff with acceptance tests
///
/// Inspired by claw-code's task packet system:
/// - Structured task definition with inputs/outputs
/// - Built-in acceptance criteria
/// - Dependency tracking between packets
/// - Status lifecycle management

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PacketStatus {
    /// Created, waiting for dependencies
    Pending,
    /// Dependencies met, ready for execution
    Ready,
    /// Currently being executed
    InProgress,
    /// Completed, awaiting verification
    AwaitingVerification,
    /// Verified and accepted
    Accepted,
    /// Failed verification or execution
    Rejected,
    /// Cancelled
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceCriterion {
    /// Description of what must be true
    pub description: String,
    /// Optional verification command or check
    pub verify_with: Option<String>,
    /// Whether this criterion is mandatory
    pub mandatory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPacket {
    pub id: String,
    pub title: String,
    pub description: String,
    /// Input data/context required
    pub inputs: Vec<String>,
    /// Expected outputs
    pub outputs: Vec<String>,
    /// Acceptance criteria
    pub acceptance: Vec<AcceptanceCriterion>,
    /// IDs of packets this depends on
    pub depends_on: Vec<String>,
    pub status: PacketStatus,
    /// Maximum time allowed (seconds)
    pub timeout_secs: Option<u64>,
    /// Agent/worker assigned
    pub assigned_to: Option<String>,
    /// Error message if rejected
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl TaskPacket {
    pub fn new(id: String, title: String, description: String) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id,
            title,
            description,
            inputs: Vec::new(),
            outputs: Vec::new(),
            acceptance: Vec::new(),
            depends_on: Vec::new(),
            status: PacketStatus::Pending,
            timeout_secs: None,
            assigned_to: None,
            error: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_input(mut self, input: String) -> Self {
        self.inputs.push(input);
        self
    }

    pub fn with_output(mut self, output: String) -> Self {
        self.outputs.push(output);
        self
    }

    pub fn with_acceptance(mut self, criterion: AcceptanceCriterion) -> Self {
        self.acceptance.push(criterion);
        self
    }

    pub fn depends_on(mut self, dep_id: String) -> Self {
        self.depends_on.push(dep_id);
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    pub fn assign_to(mut self, agent_id: String) -> Self {
        self.assigned_to = Some(agent_id);
        self
    }
}

/// Manages a collection of task packets with dependency resolution
#[derive(Debug)]
pub struct PacketManager {
    packets: Vec<TaskPacket>,
}

impl PacketManager {
    pub fn new() -> Self {
        Self {
            packets: Vec::new(),
        }
    }

    /// Add a packet
    pub fn add(&mut self, packet: TaskPacket) {
        self.packets.push(packet);
    }

    /// Get a packet by ID
    pub fn get(&self, id: &str) -> Option<&TaskPacket> {
        self.packets.iter().find(|p| p.id == id)
    }

    /// Get packets that are ready (dependencies all Accepted)
    pub fn ready_packets(&self) -> Vec<&TaskPacket> {
        self.packets
            .iter()
            .filter(|p| p.status == PacketStatus::Pending)
            .filter(|p| {
                p.depends_on
                    .iter()
                    .all(|dep_id| {
                        self.packets
                            .iter()
                            .any(|d| d.id == *dep_id && d.status == PacketStatus::Accepted)
                    })
            })
            .collect()
    }

    /// Mark a packet as ready (Pending → Ready)
    pub fn mark_ready(&mut self, id: &str) -> Result<(), PacketError> {
        let packet = self.find_mut(id)?;
        if packet.status != PacketStatus::Pending {
            return Err(PacketError::InvalidTransition {
                from: packet.status,
                to: PacketStatus::Ready,
            });
        }
        packet.status = PacketStatus::Ready;
        packet.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    /// Start execution (Ready → InProgress)
    pub fn start(&mut self, id: &str) -> Result<(), PacketError> {
        let packet = self.find_mut(id)?;
        if packet.status != PacketStatus::Ready {
            return Err(PacketError::InvalidTransition {
                from: packet.status,
                to: PacketStatus::InProgress,
            });
        }
        packet.status = PacketStatus::InProgress;
        packet.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    /// Submit for verification (InProgress → AwaitingVerification)
    pub fn submit(&mut self, id: &str) -> Result<(), PacketError> {
        let packet = self.find_mut(id)?;
        if packet.status != PacketStatus::InProgress {
            return Err(PacketError::InvalidTransition {
                from: packet.status,
                to: PacketStatus::AwaitingVerification,
            });
        }
        packet.status = PacketStatus::AwaitingVerification;
        packet.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    /// Accept (AwaitingVerification → Accepted)
    pub fn accept(&mut self, id: &str) -> Result<(), PacketError> {
        let packet = self.find_mut(id)?;
        if packet.status != PacketStatus::AwaitingVerification {
            return Err(PacketError::InvalidTransition {
                from: packet.status,
                to: PacketStatus::Accepted,
            });
        }
        packet.status = PacketStatus::Accepted;
        packet.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    /// Reject (AwaitingVerification → Rejected)
    pub fn reject(&mut self, id: &str, reason: String) -> Result<(), PacketError> {
        let packet = self.find_mut(id)?;
        if packet.status != PacketStatus::AwaitingVerification {
            return Err(PacketError::InvalidTransition {
                from: packet.status,
                to: PacketStatus::Rejected,
            });
        }
        packet.status = PacketStatus::Rejected;
        packet.error = Some(reason);
        packet.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    /// Cancel (any non-terminal → Cancelled)
    pub fn cancel(&mut self, id: &str) -> Result<(), PacketError> {
        let packet = self.find_mut(id)?;
        if matches!(
            packet.status,
            PacketStatus::Accepted | PacketStatus::Rejected | PacketStatus::Cancelled
        ) {
            return Err(PacketError::InvalidTransition {
                from: packet.status,
                to: PacketStatus::Cancelled,
            });
        }
        packet.status = PacketStatus::Cancelled;
        packet.updated_at = chrono::Utc::now().timestamp();
        Ok(())
    }

    /// Get all packets sorted by status
    pub fn all(&self) -> &[TaskPacket] {
        &self.packets
    }

    /// Get count by status
    pub fn counts(&self) -> std::collections::HashMap<PacketStatus, usize> {
        let mut counts = std::collections::HashMap::new();
        for p in &self.packets {
            *counts.entry(p.status).or_insert(0) += 1;
        }
        counts
    }

    fn find_mut(&mut self, id: &str) -> Result<&mut TaskPacket, PacketError> {
        self.packets
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| PacketError::NotFound(id.to_string()))
    }
}

impl Default for PacketManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum PacketError {
    #[error("packet not found: {0}")]
    NotFound(String),
    #[error("invalid transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: PacketStatus,
        to: PacketStatus,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_lifecycle() {
        let mut mgr = PacketManager::new();
        let packet = TaskPacket::new("p1".into(), "Build API".into(), "Create REST API".into())
            .with_input("spec.md".into())
            .with_output("api.rs".into())
            .with_acceptance(AcceptanceCriterion {
                description: "All endpoints return 200".into(),
                verify_with: Some("curl test".into()),
                mandatory: true,
            });

        mgr.add(packet);

        mgr.mark_ready("p1").unwrap();
        mgr.start("p1").unwrap();
        mgr.submit("p1").unwrap();
        mgr.accept("p1").unwrap();

        assert_eq!(mgr.get("p1").unwrap().status, PacketStatus::Accepted);
    }

    #[test]
    fn test_dependency_resolution() {
        let mut mgr = PacketManager::new();
        mgr.add(TaskPacket::new("p1".into(), "Base".into(), "".into()));
        mgr.add(
            TaskPacket::new("p2".into(), "Dependent".into(), "".into())
                .depends_on("p1".into()),
        );

        // p1 is ready (no deps), p2 is not (p1 not accepted)
        let ready = mgr.ready_packets();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "p1");

        // Accept p1
        mgr.mark_ready("p1").unwrap();
        mgr.start("p1").unwrap();
        mgr.submit("p1").unwrap();
        mgr.accept("p1").unwrap();

        // Now p2 is ready
        let ready = mgr.ready_packets();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "p2");
    }

    #[test]
    fn test_reject_and_error() {
        let mut mgr = PacketManager::new();
        mgr.add(TaskPacket::new("p1".into(), "Task".into(), "".into()));

        mgr.mark_ready("p1").unwrap();
        mgr.start("p1").unwrap();
        mgr.submit("p1").unwrap();
        mgr.reject("p1", "Tests failed".into()).unwrap();

        let packet = mgr.get("p1").unwrap();
        assert_eq!(packet.status, PacketStatus::Rejected);
        assert_eq!(packet.error.as_deref(), Some("Tests failed"));
    }

    #[test]
    fn test_cancel() {
        let mut mgr = PacketManager::new();
        mgr.add(TaskPacket::new("p1".into(), "Task".into(), "".into()));

        mgr.cancel("p1").unwrap();
        assert_eq!(mgr.get("p1").unwrap().status, PacketStatus::Cancelled);
    }

    #[test]
    fn test_invalid_transition() {
        let mut mgr = PacketManager::new();
        mgr.add(TaskPacket::new("p1".into(), "Task".into(), "".into()));

        // Can't start a pending packet
        let result = mgr.start("p1");
        assert!(matches!(
            result,
            Err(PacketError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn test_counts() {
        let mut mgr = PacketManager::new();
        mgr.add(TaskPacket::new("p1".into(), "A".into(), "".into()));
        mgr.add(TaskPacket::new("p2".into(), "B".into(), "".into()));

        let counts = mgr.counts();
        assert_eq!(counts.get(&PacketStatus::Pending), Some(&2));
    }
}
