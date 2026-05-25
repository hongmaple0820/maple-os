use crate::registry::{AgentRegistry, AgentRole};
use crate::delegate::DelegateEngine;
use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::watch;

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct PlanStep {
    pub step_id: String,
    pub description: String,
    pub required_tools: Vec<String>,
    pub depends_on: Vec<String>,
    pub assigned_agent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub goal: String,
    pub steps: Vec<PlanStep>,
}

pub struct Orchestrator {
    registry: Arc<AgentRegistry>,
    delegate: Arc<DelegateEngine>,
    max_sub_tasks: usize,
    approval_channels: Arc<DashMap<String, watch::Sender<bool>>>,
}

impl Orchestrator {
    pub fn new(registry: Arc<AgentRegistry>, delegate: Arc<DelegateEngine>) -> Self {
        Self {
            registry,
            delegate,
            max_sub_tasks: 8,
            approval_channels: Arc::new(DashMap::new()),
        }
    }

    pub fn with_max_sub_tasks(mut self, max: usize) -> Self {
        self.max_sub_tasks = max;
        Self { ..self }
    }

    pub async fn plan_and_execute(&self, goal: &str, tools: &[String]) -> Result<String> {
        let plan = self.create_plan(goal, tools).await?;

        if plan.steps.is_empty() {
            return self.delegate.delegate(goal, AgentRole::Executor, tools).await;
        }

        if plan.steps.len() == 1 {
            let step = &plan.steps[0];
            return self.delegate.delegate(&step.description, AgentRole::Executor, &step.required_tools).await;
        }

        self.execute_plan(&plan).await
    }

    pub async fn create_plan(&self, goal: &str, tools: &[String]) -> Result<ExecutionPlan> {
        let available_agents = self.registry.list_agents().await;
        let online_agents: Vec<_> = available_agents.iter()
            .filter(|(_, _, s)| *s == crate::registry::AgentStatus::Online)
            .collect();

        if online_agents.len() <= 1 {
            return Ok(ExecutionPlan {
                goal: goal.to_string(),
                steps: vec![PlanStep {
                    step_id: "step_1".to_string(),
                    description: goal.to_string(),
                    required_tools: tools.to_vec(),
                    depends_on: Vec::new(),
                    assigned_agent: online_agents.first().map(|(id, _, _)| id.clone()),
                }],
            });
        }

        let mut sub_tasks = self.decompose_goal(goal, tools);
        
        for step in &mut sub_tasks {
            step.assigned_agent = self.find_best_agent(&step.required_tools, &online_agents).await;
        }

        Ok(ExecutionPlan {
            goal: goal.to_string(),
            steps: sub_tasks,
        })
    }

    async fn find_best_agent(
        &self,
        required_tools: &[String],
        agents: &[(String, String, crate::registry::AgentStatus)],
    ) -> Option<String> {
        if required_tools.is_empty() {
            return agents.first().map(|(id, _, _)| id.clone());
        }

        let mut best_agent: Option<String> = None;
        let mut best_score = 0;

        for (agent_id, _, _) in agents {
            let agent = self.registry.get_agent(agent_id).await;
            if let Some(agent_info) = agent {
                let capabilities = &agent_info.capabilities;
                let score = required_tools.iter()
                    .filter(|tool| capabilities.iter().any(|cap| cap.contains(tool.as_str()) || tool.contains(cap.as_str())))
                    .count();
                
                if score > best_score {
                    best_score = score;
                    best_agent = Some(agent_id.clone());
                }
            }
        }

        best_agent.or_else(|| agents.first().map(|(id, _, _)| id.clone()))
    }

    fn decompose_goal(&self, goal: &str, tools: &[String]) -> Vec<PlanStep> {
        let keywords = self.extract_task_keywords(goal);
        let mut steps = Vec::new();
        let tool_count = tools.len();

        if tool_count <= self.max_sub_tasks {
            for (i, tool) in tools.iter().enumerate() {
                steps.push(PlanStep {
                    step_id: format!("step_{}", i + 1),
                    description: format!("Execute {} for: {}", tool, goal),
                    required_tools: vec![tool.clone()],
                    depends_on: if i > 0 && keywords.contains(&"sequential".to_string()) {
                        vec![format!("step_{}", i)]
                    } else {
                        Vec::new()
                    },
                    assigned_agent: None,
                });
            }
        } else {
            let chunk_size = tool_count.div_ceil(self.max_sub_tasks);
            for (i, chunk) in tools.chunks(chunk_size).enumerate() {
                steps.push(PlanStep {
                    step_id: format!("step_{}", i + 1),
                    description: format!("Execute tools [{}] for: {}", chunk.join(", "), goal),
                    required_tools: chunk.to_vec(),
                    depends_on: Vec::new(),
                    assigned_agent: None,
                });
            }
        }

        steps.push(PlanStep {
            step_id: "step_aggregate".to_string(),
            description: format!("Aggregate results for: {}", goal),
            required_tools: tools.to_vec(),
            depends_on: steps.iter().map(|s| s.step_id.clone()).collect(),
            assigned_agent: None,
        });

        steps
    }

    fn extract_task_keywords(&self, goal: &str) -> Vec<String> {
        let sequential_indicators = ["then", "after", "next", "sequentially", "step by step"];
        let parallel_indicators = ["parallel", "simultaneously", "concurrently", "at the same time", "independently"];

        let goal_lower = goal.to_lowercase();
        let mut keywords = Vec::new();

        for indicator in &sequential_indicators {
            if goal_lower.contains(indicator) {
                keywords.push("sequential".to_string());
                break;
            }
        }

        for indicator in &parallel_indicators {
            if goal_lower.contains(indicator) {
                keywords.push("parallel".to_string());
                break;
            }
        }

        keywords
    }

    async fn execute_plan(&self, plan: &ExecutionPlan) -> Result<String> {
        let mut results: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut completed_steps: std::collections::HashSet<String> = std::collections::HashSet::new();

        let mut remaining: Vec<&PlanStep> = plan.steps.iter().collect();
        let max_iterations = remaining.len() * 2;
        let mut iteration = 0;

        while !remaining.is_empty() && iteration < max_iterations {
            iteration += 1;

            let ready: Vec<&PlanStep> = remaining.iter()
                .filter(|step| step.depends_on.iter().all(|dep| completed_steps.contains(dep)))
                .copied()
                .collect();

            if ready.is_empty() {
                anyhow::bail!("Deadlock detected in execution plan - no steps are ready");
            }

            let mut concurrent_handles: Vec<tokio::task::JoinHandle<Result<(String, String)>>> = Vec::new();

            for step in &ready {
                let step_id = step.step_id.clone();
                let description = step.description.clone();
                let required_tools = step.required_tools.clone();
                let delegate = self.delegate.clone();

                let has_unmet_deps = step.depends_on.iter().any(|dep| !completed_steps.contains(dep));
                if has_unmet_deps {
                    continue;
                }

                let dep_results: Vec<String> = step.depends_on.iter()
                    .filter_map(|dep| results.get(dep))
                    .cloned()
                    .collect();

                let enriched_description = if dep_results.is_empty() {
                    description.clone()
                } else {
                    format!("{}\n\nContext from previous steps:\n{}", description, dep_results.join("\n---\n"))
                };

                concurrent_handles.push(tokio::spawn(async move {
                    let result = delegate.delegate(&enriched_description, AgentRole::Executor, &required_tools).await;
                    result.map(|r| (step_id, r))
                }));
            }

            for handle in concurrent_handles {
                match handle.await {
                    Ok(Ok((step_id, result))) => {
                        results.insert(step_id.clone(), result);
                        completed_steps.insert(step_id);
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("Plan step execution failed: {}", e);
                    }
                    Err(e) => {
                        tracing::warn!("Plan step task join error: {}", e);
                    }
                }
            }

            remaining.retain(|step| !completed_steps.contains(&step.step_id));
        }

        let aggregate_step = plan.steps.iter().find(|s| s.step_id == "step_aggregate");
        if let Some(agg) = aggregate_step
            && let Some(agg_result) = results.get(&agg.step_id)
        {
            return Ok(agg_result.clone());
        }

        let final_results: Vec<String> = plan.steps.iter()
            .filter_map(|step| results.get(&step.step_id))
            .cloned()
            .collect();

        Ok(format!("Plan execution completed with {} steps:\n{}", final_results.len(), final_results.join("\n\n---\n\n")))
    }

    pub async fn resolve_approval(&self, approval_id: &str, approved: bool) -> bool {
        if let Some(entry) = self.approval_channels.get(approval_id) {
            let _ = entry.send(approved);
            true
        } else {
            tracing::warn!(approval_id, "Approval channel not found");
            false
        }
    }

    pub fn create_approval_channel(&self, approval_id: &str) -> watch::Receiver<bool> {
        let (tx, rx) = watch::channel(false);
        self.approval_channels.insert(approval_id.to_string(), tx);
        rx
    }

    pub fn remove_approval_channel(&self, approval_id: &str) {
        self.approval_channels.remove(approval_id);
    }
}
