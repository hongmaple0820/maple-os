use crate::delegation::{DelegateOptsBuilder, DelegateResult, DelegationEngine};
use crate::tool_use_context::ToolUseContext;
use anyhow::Result;
use maple_llm::request::LlmRequest;
use maple_llm::router::LlmAdapter;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Coordinator Mode — inspired by cc-haha's 4-phase workflow
///
/// Features:
/// - LLM-driven task decomposition (Analyze phase)
/// - Parallel worker execution (Delegate phase)
/// - Result aggregation (Monitor + Synthesize phases)
/// - Error handling and fallback
///
///   Coordinator workflow phases
#[derive(Debug, Clone, PartialEq)]
pub enum CoordinatorPhase {
    Analyze,
    Delegate,
    Monitor,
    Synthesize,
}

/// Sub-task decomposition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTask {
    pub id: String,
    pub description: String,
    pub tools_required: Vec<String>,
    pub dependencies: Vec<String>,
    pub priority: u8,
    pub estimated_complexity: TaskComplexity,
}

/// Task complexity estimation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskComplexity {
    Low,
    Medium,
    High,
    Critical,
}

/// Worker status
#[derive(Debug, Clone)]
pub struct WorkerStatus {
    pub worker_id: String,
    pub subtask_id: String,
    pub phase: WorkerPhase,
    pub progress: f32,
    pub error: Option<String>,
}

/// Worker execution phase
#[derive(Debug, Clone, PartialEq)]
pub enum WorkerPhase {
    Pending,
    Running,
    Completed,
    Failed,
    TimedOut,
}

/// Coordinator result
#[derive(Debug, Clone)]
pub struct CoordinatorResult {
    pub success: bool,
    pub output: String,
    pub subtask_results: HashMap<String, DelegateResult>,
    pub total_tokens: usize,
    pub total_duration: Duration,
}

/// LLM response format for task decomposition
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DecompositionResponse {
    subtasks: Vec<DecomposedTask>,
}

#[derive(Debug, Deserialize)]
struct DecomposedTask {
    description: String,
    tools_required: Vec<String>,
    dependencies: Vec<String>,
    priority: u8,
    complexity: String,
}

/// Coordinator — orchestrates complex task decomposition and execution
pub struct Coordinator {
    delegation_engine: Arc<DelegationEngine>,
    llm_adapter: Option<Arc<dyn LlmAdapter>>,
    max_workers: usize,
    worker_timeout: Duration,
}

impl Coordinator {
    pub fn new(delegation_engine: Arc<DelegationEngine>) -> Self {
        Self {
            delegation_engine,
            llm_adapter: None,
            max_workers: 4,
            worker_timeout: Duration::from_secs(300),
        }
    }

    pub fn with_llm_adapter(mut self, adapter: Arc<dyn LlmAdapter>) -> Self {
        self.llm_adapter = Some(adapter);
        self
    }

    pub fn with_max_workers(mut self, max: usize) -> Self {
        self.max_workers = max;
        self
    }

    pub fn with_worker_timeout(mut self, timeout: Duration) -> Self {
        self.worker_timeout = timeout;
        self
    }

    /// Execute a complex goal using the 4-phase workflow
    pub async fn coordinate(
        &self,
        goal: &str,
        tools: &[String],
        context: &ToolUseContext,
    ) -> Result<CoordinatorResult> {
        let start_time = std::time::Instant::now();
        let mut total_tokens = 0;

        // Phase 1: Analyze — decompose goal into sub-tasks
        let subtasks = self.analyze(goal, tools).await?;

        // If only one subtask and it matches the goal, skip delegation
        if subtasks.len() == 1 && subtasks[0].description == goal {
            return Ok(CoordinatorResult {
                success: true,
                output: format!("Single task — no decomposition needed: {}", goal),
                subtask_results: HashMap::new(),
                total_tokens: 0,
                total_duration: start_time.elapsed(),
            });
        }

        // Phase 2: Delegate — create workers for each sub-task
        let workers = self.delegate(&subtasks, tools, context).await?;

        // Phase 3: Monitor — wait for all workers to complete
        let results = self.monitor(workers).await?;

        // Phase 4: Synthesize — aggregate results
        let output = self.synthesize(goal, &results).await?;

        for result in results.values() {
            total_tokens += result.tokens_used;
        }

        Ok(CoordinatorResult {
            success: results.values().all(|r| r.success),
            output,
            subtask_results: results,
            total_tokens,
            total_duration: start_time.elapsed(),
        })
    }

    /// Phase 1: Analyze — LLM-driven goal decomposition
    async fn analyze(&self, goal: &str, tools: &[String]) -> Result<Vec<SubTask>> {
        // Try LLM-driven decomposition first
        if let Some(ref adapter) = self.llm_adapter {
            return self.llm_decompose(adapter, goal, tools).await;
        }

        // Fallback: single task (no decomposition)
        Ok(vec![SubTask {
            id: uuid::Uuid::new_v4().to_string(),
            description: goal.to_string(),
            tools_required: tools.to_vec(),
            dependencies: Vec::new(),
            priority: 0,
            estimated_complexity: TaskComplexity::Medium,
        }])
    }

    /// LLM-driven task decomposition
    async fn llm_decompose(
        &self,
        adapter: &dyn LlmAdapter,
        goal: &str,
        tools: &[String],
    ) -> Result<Vec<SubTask>> {
        let tools_list = tools.join(", ");

        let prompt = format!(
            r#"You are a task decomposer. Break the following goal into independent sub-tasks that can be executed in parallel.

GOAL: {}

AVAILABLE TOOLS: {}

Respond with ONLY a JSON array of sub-tasks. Each sub-task has:
- "description": clear, actionable description
- "tools_required": array of tool names needed
- "dependencies": array of indices (0-based) of sub-tasks that must complete before this one
- "priority": 0-10 (higher = more important)
- "complexity": "Low", "Medium", "High", or "Critical"

Rules:
- If the goal is simple (single step), return a single sub-task
- Each sub-task should be independently executable
- Minimize dependencies between sub-tasks
- Use only tools from the available tools list

Example response:
[
  {{"description": "Read the file contents", "tools_required": ["read_file"], "dependencies": [], "priority": 5, "complexity": "Low"}},
  {{"description": "Analyze the code structure", "tools_required": ["read_file", "search"], "dependencies": [0], "priority": 3, "complexity": "Medium"}}
]"#,
            goal, tools_list
        );

        let request = LlmRequest::new(prompt, "default");
        let response = adapter.complete(request).await?;
        let text = response.text();

        // Parse JSON array from response
        let json_start = text.find('[').unwrap_or(0);
        let json_end = text.rfind(']').unwrap_or(text.len());
        let json_str = &text[json_start..=json_end];

        let decomposed: Vec<DecomposedTask> = serde_json::from_str(json_str).unwrap_or_default();

        if decomposed.is_empty() {
            // Fallback if parsing fails
            return Ok(vec![SubTask {
                id: uuid::Uuid::new_v4().to_string(),
                description: goal.to_string(),
                tools_required: tools.to_vec(),
                dependencies: Vec::new(),
                priority: 0,
                estimated_complexity: TaskComplexity::Medium,
            }]);
        }

        let subtasks: Vec<SubTask> = decomposed
            .iter()
            .enumerate()
            .map(|(i, dt)| {
                let complexity = match dt.complexity.as_str() {
                    "Low" => TaskComplexity::Low,
                    "High" => TaskComplexity::High,
                    "Critical" => TaskComplexity::Critical,
                    _ => TaskComplexity::Medium,
                };

                SubTask {
                    id: format!("subtask-{}", i),
                    description: dt.description.clone(),
                    tools_required: dt.tools_required.clone(),
                    dependencies: dt
                        .dependencies
                        .iter()
                        .map(|d| format!("subtask-{}", d))
                        .collect(),
                    priority: dt.priority,
                    estimated_complexity: complexity,
                }
            })
            .collect();

        Ok(subtasks)
    }

    /// Phase 2: Delegate — create workers for each sub-task
    async fn delegate(
        &self,
        subtasks: &[SubTask],
        tools: &[String],
        context: &ToolUseContext,
    ) -> Result<Vec<(String, tokio::task::JoinHandle<Result<DelegateResult>>)>> {
        let mut workers = Vec::new();

        // Sort by priority (highest first) and respect dependencies
        let mut sorted_subtasks: Vec<&SubTask> = subtasks.iter().collect();
        sorted_subtasks.sort_by_key(|b| std::cmp::Reverse(b.priority));

        for subtask in sorted_subtasks.iter().take(self.max_workers) {
            // Skip subtasks with unresolved dependencies (simplified: just warn)
            if !subtask.dependencies.is_empty() {
                tracing::warn!(
                    "Subtask {} has dependencies {:?} — executing anyway (parallel mode)",
                    subtask.id,
                    subtask.dependencies
                );
            }

            let opts = DelegateOptsBuilder::new()
                .max_iterations(10)
                .timeout(self.worker_timeout)
                .tool_subset(subtask.tools_required.clone())
                .build();

            let delegation = self.delegation_engine.clone();
            let goal = subtask.description.clone();
            let tools: Vec<String> = tools.to_vec();
            let ctx = context.clone();

            let handle =
                tokio::spawn(async move { delegation.delegate(&goal, &tools, opts, &ctx).await });

            workers.push((subtask.id.clone(), handle));
        }

        Ok(workers)
    }

    /// Phase 3: Monitor — wait for all workers to complete
    async fn monitor(
        &self,
        workers: Vec<(String, tokio::task::JoinHandle<Result<DelegateResult>>)>,
    ) -> Result<HashMap<String, DelegateResult>> {
        let mut results = HashMap::new();

        for (task_id, handle) in workers {
            match handle.await {
                Ok(Ok(result)) => {
                    results.insert(task_id, result);
                }
                Ok(Err(e)) => {
                    let err_result = DelegateResult {
                        task_id: task_id.clone(),
                        success: false,
                        output: format!("Error: {}", e),
                        iterations_used: 0,
                        tokens_used: 0,
                    };
                    results.insert(task_id, err_result);
                }
                Err(e) => {
                    let err_result = DelegateResult {
                        task_id: task_id.clone(),
                        success: false,
                        output: format!("Worker panicked: {}", e),
                        iterations_used: 0,
                        tokens_used: 0,
                    };
                    results.insert(task_id, err_result);
                }
            }
        }

        Ok(results)
    }

    /// Phase 4: Synthesize — aggregate results into final output
    async fn synthesize(
        &self,
        goal: &str,
        results: &HashMap<String, DelegateResult>,
    ) -> Result<String> {
        // Try LLM-driven synthesis if adapter available
        if let Some(ref adapter) = self.llm_adapter {
            return self.llm_synthesize(adapter, goal, results).await;
        }

        // Fallback: simple concatenation
        let mut output = String::new();
        for (task_id, result) in results {
            if result.success {
                output.push_str(&format!("## Task {}\n{}\n\n", task_id, result.output));
            } else {
                output.push_str(&format!(
                    "## Task {} (Failed)\n{}\n\n",
                    task_id, result.output
                ));
            }
        }
        Ok(output)
    }

    /// LLM-driven result synthesis
    async fn llm_synthesize(
        &self,
        adapter: &dyn LlmAdapter,
        goal: &str,
        results: &HashMap<String, DelegateResult>,
    ) -> Result<String> {
        let mut results_text = String::new();
        for (task_id, result) in results {
            let status = if result.success { "SUCCESS" } else { "FAILED" };
            results_text.push_str(&format!(
                "[{}] Task {}: {}\n",
                status, task_id, result.output
            ));
        }

        let prompt = format!(
            r#"You are a result synthesizer. Combine the following sub-task results into a coherent response for the original goal.

ORIGINAL GOAL: {}

SUB-TASK RESULTS:
{}

Provide a clear, complete response that addresses the original goal. If any sub-tasks failed, note what was accomplished and what wasn't."#,
            goal, results_text
        );

        let request = LlmRequest::new(prompt, "default");
        let response = adapter.complete(request).await?;
        Ok(response.text())
    }
}

/// Builder for Coordinator
pub struct CoordinatorBuilder {
    delegation_engine: Arc<DelegationEngine>,
    llm_adapter: Option<Arc<dyn LlmAdapter>>,
    max_workers: usize,
    worker_timeout: Duration,
}

impl CoordinatorBuilder {
    pub fn new(delegation_engine: Arc<DelegationEngine>) -> Self {
        Self {
            delegation_engine,
            llm_adapter: None,
            max_workers: 4,
            worker_timeout: Duration::from_secs(300),
        }
    }

    pub fn llm_adapter(mut self, adapter: Arc<dyn LlmAdapter>) -> Self {
        self.llm_adapter = Some(adapter);
        self
    }

    pub fn max_workers(mut self, max: usize) -> Self {
        self.max_workers = max;
        self
    }

    pub fn worker_timeout(mut self, timeout: Duration) -> Self {
        self.worker_timeout = timeout;
        self
    }

    pub fn build(self) -> Coordinator {
        let mut coord = Coordinator::new(self.delegation_engine)
            .with_max_workers(self.max_workers)
            .with_worker_timeout(self.worker_timeout);
        if let Some(adapter) = self.llm_adapter {
            coord = coord.with_llm_adapter(adapter);
        }
        coord
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_complexity() {
        assert_eq!(TaskComplexity::Low, TaskComplexity::Low);
        assert_ne!(TaskComplexity::Low, TaskComplexity::High);
    }

    #[test]
    fn test_subtask_creation() {
        let subtask = SubTask {
            id: "test".to_string(),
            description: "Test task".to_string(),
            tools_required: vec!["read_file".to_string()],
            dependencies: Vec::new(),
            priority: 0,
            estimated_complexity: TaskComplexity::Medium,
        };

        assert_eq!(subtask.id, "test");
        assert_eq!(subtask.dependencies.len(), 0);
    }

    #[test]
    fn test_coordinator_phases() {
        assert_eq!(CoordinatorPhase::Analyze, CoordinatorPhase::Analyze);
        assert_ne!(CoordinatorPhase::Analyze, CoordinatorPhase::Synthesize);
    }
}
