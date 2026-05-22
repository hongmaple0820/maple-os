use crate::registry::{AgentRegistry, AgentRole, AgentTask};
use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;

const DELEGATE_EXCLUDED_TOOLS: &[&str] = &["delegate", "approve"];

pub struct DelegateEngine {
    agent_registry: Arc<AgentRegistry>,
    #[allow(dead_code)]
    max_depth: usize,
}

impl DelegateEngine {
    pub fn new(agent_registry: Arc<AgentRegistry>, max_depth: usize) -> Self {
        Self { agent_registry, max_depth }
    }

    pub async fn delegate(
        &self,
        goal: &str,
        role: AgentRole,
        parent_tools: &[String],
    ) -> Result<String> {
        let child_tools = match role {
            AgentRole::Orchestrator => parent_tools.to_vec(),
            AgentRole::Leaf => {
                parent_tools.iter()
                    .filter(|t| !DELEGATE_EXCLUDED_TOOLS.contains(&t.as_str()))
                    .cloned()
                    .collect()
            }
            _ => parent_tools.to_vec(),
        };

        let agent = self.agent_registry.find_available(&child_tools).await
            .ok_or_else(|| anyhow::anyhow!("No available agent for delegation"))?;

        let task_id = uuid::Uuid::new_v4().to_string();
        let timeout_secs = 600;

        let child_task = AgentTask {
            task_id: task_id.clone(),
            goal: goal.to_string(),
            tools: child_tools,
            role,
            timeout_secs,
        };

        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        self.agent_registry.register_result_channel(&task_id, result_tx).await;

        let tx = self.agent_registry.get_task_channel(&agent).await
            .ok_or_else(|| anyhow::anyhow!("Agent {} has no task channel", agent))?;

        tx.send(child_task).await
            .map_err(|_| anyhow::anyhow!("Failed to send task to agent {}", agent))?;

        let result = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            result_rx,
        ).await
        .map_err(|_| anyhow::anyhow!("Task {} timed out after {}s", task_id, timeout_secs))?
        .map_err(|_| anyhow::anyhow!("Task {} result channel closed", task_id))?;

        Ok(result)
    }
}
