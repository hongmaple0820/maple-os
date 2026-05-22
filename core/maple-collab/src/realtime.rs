use serde_json::Value;
use tokio::sync::broadcast;
use anyhow::Result;

const DEFAULT_CHANNEL_CAPACITY: usize = 256;

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastEvent {
    pub workspace_id: String,
    pub event_type: String,
    pub data: Value,
    pub timestamp: i64,
}

pub struct RealtimeSync {
    sender: broadcast::Sender<BroadcastEvent>,
}

impl RealtimeSync {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(DEFAULT_CHANNEL_CAPACITY);
        Self { sender }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub async fn broadcast(&self, workspace_id: &str, event_type: &str, data: &Value) -> Result<()> {
        let event = BroadcastEvent {
            workspace_id: workspace_id.to_string(),
            event_type: event_type.to_string(),
            data: data.clone(),
            timestamp: chrono::Utc::now().timestamp(),
        };

        let receiver_count = self.sender.receiver_count();
        if receiver_count > 0 {
            let _ = self.sender.send(event);
        } else {
            tracing::debug!(
                workspace_id = workspace_id,
                event_type = event_type,
                "No receivers for broadcast event, skipping"
            );
        }

        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BroadcastEvent> {
        self.sender.subscribe()
    }

    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

use serde::Serialize;