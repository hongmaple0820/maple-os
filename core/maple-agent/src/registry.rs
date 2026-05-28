use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use anyhow::Result;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Online,
    Busy,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    WebSocket,
    Webhook { url: String, secret: String },
    Mcp { command: Vec<String> },
    Rest { url: String },
    Sse { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapabilities {
    pub tools: Vec<String>,
    pub skills: Vec<String>,
    pub max_context_length: usize,
    pub supports_streaming: bool,
    pub supports_image: bool,
    pub supports_function_calling: bool,
}

impl Default for AgentCapabilities {
    fn default() -> Self {
        Self {
            tools: Vec::new(),
            skills: Vec::new(),
            max_context_length: 128_000,
            supports_streaming: true,
            supports_image: false,
            supports_function_calling: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentTriggers {
    pub events: Vec<String>,
    pub keywords: Vec<String>,
    pub cron: Option<String>,
    pub workflow_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSchema {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub transport: Transport,
    pub capabilities: AgentCapabilities,
    pub triggers: AgentTriggers,
    pub max_concurrent_tasks: u32,
}

struct AgentEntry {
    schema: AgentSchema,
    status: AgentStatus,
    last_heartbeat: Instant,
    #[allow(dead_code)]
    current_task_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub task_id: String,
    pub goal: String,
    pub tools: Vec<String>,
    pub role: AgentRole,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Orchestrator,
    Executor,
    Reviewer,
    Leaf,
}

pub struct AgentRegistry {
    agents: DashMap<String, AgentEntry>,
    task_channels: DashMap<String, mpsc::Sender<AgentTask>>,
    result_channels: DashMap<String, oneshot::Sender<String>>,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: DashMap::new(),
            task_channels: DashMap::new(),
            result_channels: DashMap::new(),
        }
    }

    pub async fn register(&self, schema: AgentSchema) -> Result<()> {
        let id = schema.id.clone();
        self.agents.insert(
            id,
            AgentEntry {
                schema,
                status: AgentStatus::Offline,
                last_heartbeat: Instant::now(),
                current_task_count: 0,
            },
        );
        Ok(())
    }

    pub async fn deregister_agent(&self, id: &str) {
        self.agents.remove(id);
        self.task_channels.remove(id);
    }

    pub async fn register_agent(&self, id: &str, name: &str, status: AgentStatus) {
        let schema = AgentSchema {
            id: id.to_string(),
            name: name.to_string(),
            description: None,
            avatar_url: None,
            transport: Transport::WebSocket,
            capabilities: AgentCapabilities::default(),
            triggers: AgentTriggers::default(),
            max_concurrent_tasks: 3,
        };
        self.agents.insert(
            id.to_string(),
            AgentEntry {
                schema,
                status,
                last_heartbeat: Instant::now(),
                current_task_count: 0,
            },
        );
    }

    pub async fn set_online(&self, agent_id: &str) {
        if let Some(mut entry) = self.agents.get_mut(agent_id) {
            entry.status = AgentStatus::Online;
            entry.last_heartbeat = Instant::now();
        }
    }

    pub async fn set_offline(&self, agent_id: &str) {
        if let Some(mut entry) = self.agents.get_mut(agent_id) {
            entry.status = AgentStatus::Offline;
        }
    }

    pub async fn set_busy(&self, agent_id: &str) {
        if let Some(mut entry) = self.agents.get_mut(agent_id) {
            entry.status = AgentStatus::Busy;
        }
    }

    pub async fn update_heartbeat(&self, agent_id: &str) {
        if let Some(mut entry) = self.agents.get_mut(agent_id) {
            entry.last_heartbeat = Instant::now();
        }
    }

    pub async fn update_capabilities(&self, agent_id: &str, capabilities: AgentCapabilities) {
        if let Some(mut entry) = self.agents.get_mut(agent_id) {
            entry.schema.capabilities = capabilities;
        }
    }

    pub async fn register_task_channel(&self, agent_id: &str, tx: mpsc::Sender<AgentTask>) {
        self.task_channels.insert(agent_id.to_string(), tx);
    }

    pub async fn remove_task_channel(&self, agent_id: &str) {
        self.task_channels.remove(agent_id);
    }

    pub async fn find_available(&self, required_tools: &[String]) -> Option<String> {
        for entry in self.agents.iter() {
            if entry.status == AgentStatus::Online {
                let has_tools = required_tools
                    .iter()
                    .all(|t| entry.schema.capabilities.tools.contains(t));
                if has_tools {
                    return Some(entry.schema.id.clone());
                }
            }
        }
        None
    }

    pub async fn list_agents(&self) -> Vec<(String, String, AgentStatus)> {
        self.agents
            .iter()
            .map(|e| (e.schema.id.clone(), e.schema.name.clone(), e.status.clone()))
            .collect()
    }

    pub async fn register_result_channel(&self, task_id: &str, tx: oneshot::Sender<String>) {
        self.result_channels.insert(task_id.to_string(), tx);
    }

    pub async fn get_task_channel(&self, agent_id: &str) -> Option<mpsc::Sender<AgentTask>> {
        self.task_channels.get(agent_id).map(|tx| tx.clone())
    }

    pub async fn get_agent(&self, agent_id: &str) -> Option<AgentSchema> {
        self.agents.get(agent_id).map(|entry| entry.schema.clone())
    }

    pub async fn complete_task(&self, task_id: &str, result: String) -> bool {
        if let Some((_, tx)) = self.result_channels.remove(task_id) {
            tx.send(result).is_ok()
        } else {
            false
        }
    }
}
