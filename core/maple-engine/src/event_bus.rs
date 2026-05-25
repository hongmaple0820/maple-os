use serde::Serialize;


use tokio::sync::mpsc;
use dashmap::DashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    WorkflowStarted { workflow_id: String, exec_id: Uuid },
    NodeStarted { workflow_id: String, exec_id: Uuid, node_id: String },
    NodeCompleted { workflow_id: String, exec_id: Uuid, node_id: String },
    NodeFailed { workflow_id: String, exec_id: Uuid, node_id: String, error: String },
    WorkflowCompleted { workflow_id: String, exec_id: Uuid },
    WorkflowFailed { workflow_id: String, exec_id: Uuid, error: String },
    MessageReceived { channel: String, sender: String, content: String },
    AgentOnline { agent_id: String },
    AgentOffline { agent_id: String },
    ApprovalRequested { request_id: String, workflow_id: String, node_id: String },
    ApprovalCompleted { request_id: String, approved: bool },
    TaskProgress { task_id: String, progress: u32, output: String },
}

impl Event {
    pub fn event_type(&self) -> String {
        match self {
            Event::WorkflowStarted { .. } => "workflow.started",
            Event::NodeStarted { .. } => "node.started",
            Event::NodeCompleted { .. } => "node.completed",
            Event::NodeFailed { .. } => "node.failed",
            Event::WorkflowCompleted { .. } => "workflow.completed",
            Event::WorkflowFailed { .. } => "workflow.failed",
            Event::MessageReceived { .. } => "message.received",
            Event::AgentOnline { .. } => "agent.online",
            Event::AgentOffline { .. } => "agent.offline",
            Event::ApprovalRequested { .. } => "approval.requested",
            Event::ApprovalCompleted { .. } => "approval.completed",
            Event::TaskProgress { .. } => "task.progress",
        }.to_string()
    }
}

pub struct EventBus {
    subscribers: DashMap<String, Vec<mpsc::Sender<Event>>>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscribers: DashMap::new(),
        }
    }

    pub async fn publish(&self, event: Event) {
        let event_type = event.event_type();
        if let Some(subs) = self.subscribers.get(&event_type) {
            for tx in subs.iter() {
                let _ = tx.send(event.clone()).await;
            }
        }
    }

    pub async fn subscribe(&self, event_type: &str) -> mpsc::Receiver<Event> {
        let (tx, rx) = mpsc::channel(256);
        self.subscribers
            .entry(event_type.to_string())
            .or_default()
            .push(tx);
        rx
    }

    pub async fn subscribe_all(&self) -> mpsc::Receiver<Event> {
        let (tx, rx) = mpsc::channel(1024);
        for event_type in &[
            "workflow.started",
            "node.started",
            "node.completed",
            "node.failed",
            "workflow.completed",
            "workflow.failed",
            "message.received",
            "agent.online",
            "agent.offline",
            "approval.requested",
            "approval.completed",
            "task.progress",
        ] {
            self.subscribers
                .entry(event_type.to_string())
                .or_default()
                .push(tx.clone());
        }
        rx
    }
}
