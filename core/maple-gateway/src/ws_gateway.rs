use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use maple_agent::registry::{AgentRegistry, AgentTask};
use maple_engine::event_bus::EventBus;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct AgentMessage {
    pub msg_type: String,
    pub payload: serde_json::Value,
}

pub async fn handle_agent_ws(
    socket: WebSocket,
    registry: Arc<AgentRegistry>,
    event_bus: Arc<EventBus>,
    agent_id: String,
) {
    let (mut sink, mut stream) = socket.split();

    registry.set_online(&agent_id).await;
    tracing::info!(agent_id = %agent_id, "Agent connected via WebSocket");

    let (task_tx, mut task_rx) = mpsc::channel::<AgentTask>(32);
    registry.register_task_channel(&agent_id, task_tx).await;

    loop {
        tokio::select! {
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<serde_json::Value>(&text) {
                            Ok(json) => {
                                let msg_type = json["type"].as_str().unwrap_or("unknown");
                                match msg_type {
                                    "register" => {
                                        tracing::info!(agent_id = %agent_id, "Agent registered capabilities");
                                    }
                                    "task_result" => {
                                        if let Some(task_id) = json["task_id"].as_str() {
                                            let result = json["result"].as_str().unwrap_or("");
                                            tracing::info!(
                                                agent_id = %agent_id,
                                                task_id = task_id,
                                                "Task result received"
                                            );
                                            registry.complete_task(task_id, result.to_string()).await;
                                        }
                                    }
                                    "progress" => {
                                        if let Some(task_id) = json["task_id"].as_str() {
                                            let progress = json["progress"].as_u64().unwrap_or(0);
                                            let output = json["output"].as_str().unwrap_or("");
                                            event_bus.publish(
                                                maple_engine::event_bus::Event::TaskProgress {
                                                    task_id: task_id.to_string(),
                                                    progress: progress as u32,
                                                    output: output.to_string(),
                                                }
                                            ).await;
                                        }
                                    }
                                    "ping" => {
                                        let _ = sink.send(Message::Text("pong".to_string())).await;
                                        registry.update_heartbeat(&agent_id).await;
                                    }
                                    _ => {
                                        tracing::warn!(
                                            agent_id = %agent_id,
                                            msg_type = msg_type,
                                            "Unknown message type"
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(agent_id = %agent_id, error = %e, "Invalid JSON from agent");
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = sink.send(Message::Pong(data)).await;
                        registry.update_heartbeat(&agent_id).await;
                    }
                    _ => break,
                }
            }
            task = task_rx.recv() => {
                match task {
                    Some(task) => {
                        let payload = serde_json::to_string(&task).unwrap_or_default();
                        if sink.send(Message::Text(payload)).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }

    registry.set_offline(&agent_id).await;
    registry.remove_task_channel(&agent_id).await;
    tracing::info!(agent_id = %agent_id, "Agent disconnected");
}
