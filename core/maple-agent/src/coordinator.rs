use crate::delegation::{DelegationEngine, DelegateOpts, DelegateResult};
use crate::tool_use_context::ToolUseContext;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;

/// Coordinator Mode — inspired by cc-haha's 4-phase workflow
///
/// Features:
/// - Analyze → Delegate → Monitor → Synthesize workflow
/// - Parallel worker execution
/// - Result aggregation
/// - Error handling and fallback

/// Coordinator workflow phases
#[derive(Debug, Clone, PartialEq)]
pub enum CoordinatorPhase {
    /// Analyze the goal and decompose into sub-tasks
    Analyze,
    /// Delegate sub-tasks to workers
    Delegate,
    /// Monitor worker execution
    Monitor,
    /// Synthesize results into final output
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

/// Coordinator — orchestrates complex task decomposition and execution
pub struct Coordinator {
    delegation_engine: Arc<DelegationEngine>,
    max_workers: usize,
    worker_timeout: Duration,
}

impl Coordinator {
    pub fn new(delegation_engine: Arc<DelegationEngine>) -> Self {
        Self {
            delegation_engine,
            max_workers: 4,
            worker_timeout: Duration::from_secs(300),
        }
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

        // Phase 2: Delegate — create workers for each sub-task
        let workers = self.delegate(&subtasks, tools, context).await?;

        // Phase 3: Monitor — wait for all workers to complete
        let results = self.monitor(workers).await?;

        // Phase 4: Synthesize — aggregate results
        let output = self.synthesize(&results).await?;

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

    /// Phase 1: Analyze — decompose goal into sub-tasks
    async fn analyze(&self, goal: &str, tools: &[String]) -> Result<Vec<SubTask>> {
        // Simple decomposition based on goal analysis
        // In a real implementation, this would use LLM to decompose
        let subtasks = vec![
            SubTask {
                id: uuid::Uuid::new_v4().to_string(),
                description: goal.to_string(),
                tools_required: tools.clone(),
                dependencies: Vec::new(),
                priority: 0,
                estimated_complexity: TaskComplexity::Medium,
            },
        ];

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

        for subtask in subtasks.iter().take(self.max_workers) {
            let opts = DelegateOpts::builder()
                .max_iterations(10)
                .timeout(self.worker_timeout)
                .build();

            let delegation = self.delegation_engine.clone();
            let goal = subtask.description.clone();
            let tools = tools.clone();
            let ctx = context.clone();

            let handle = tokio::spawn(async move {
                delegation.delegate(&goal, &tools, opts, &ctx).await
            });

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
                    results.insert(task_id, DelegateResult {
                        task_id: task_id.clone(),
                        success: false,
                        output: format!("Error: {}", e),
                        iterations_used: 0,
                        tokens_used: 0,
                    });
                }
                Err(e) => {
                    results.insert(task_id, DelegateResult {
                        task_id: task_id.clone(),
                        success: false,
                        output: format!("Worker panicked: {}", e),
                        iterations_used: 0,
                        tokens_used: 0,
                    });
                }
            }
        }

        Ok(results)
    }

    /// Phase 4: Synthesize — aggregate results into final output
    async fn synthesize(&self, results: &HashMap<String, DelegateResult>) -> Result<String> {
        let mut output = String::new();

        for (task_id, result) in results {
            if result.success {
                output.push_str(&format!("## Task {}\n{}\n\n", task_id, result.output));
            } else {
                output.push_str(&format!("## Task {} (Failed)\n{}\n\n", task_id, result.output));
            }
        }

        Ok(output)
    }
}

/// Builder for Coordinator
pub struct CoordinatorBuilder {
    delegation_engine: Arc<DelegationEngine>,
    max_workers: usize,
    worker_timeout: Duration,
}

impl CoordinatorBuilder {
    pub fn new(delegation_engine: Arc<DelegationEngine>) -> Self {
        Self {
            delegation_engine,
            max_workers: 4,
            worker_timeout: Duration::from_secs(300),
        }
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
        Coordinator::new(self.delegation_engine)
            .with_max_workers(self.max_workers)
            .with_worker_timeout(self.worker_timeout)
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
