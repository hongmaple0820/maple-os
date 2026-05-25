use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeType {
    Llm {
        model_route: String,
        prompt_ref: String,
        temperature: Option<f32>,
    },
    Tool {
        skill_id: String,
        config: Value,
    },
    Condition {
        expression: String,
        branches: Vec<Branch>,
    },
    Parallel {
        nodes: Vec<String>,
        wait_strategy: WaitStrategy,
    },
    Loop {
        each: Option<String>,
        condition: Option<String>,
        body: String,
        max_iterations: usize,
    },
    HumanApproval {
        notify: Vec<NotifyChannel>,
        timeout_secs: u64,
        on_timeout: TimeoutAction,
    },
    SubWorkflow {
        workflow_id: String,
        input_mapping: HashMap<String, String>,
    },
    Webhook {
        url: String,
        method: HttpMethod,
        headers: HashMap<String, String>,
        body_template: Option<String>,
    },
    Delay {
        duration_secs: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub label: String,
    pub expression: Option<String>,
    pub target_node: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitStrategy {
    WaitAll,
    WaitAny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeoutAction {
    AutoApprove,
    AutoReject,
    FailWorkflow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyChannel {
    pub channel_type: String,
    pub url: String,
    pub message_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: usize,
    pub backoff: BackoffStrategy,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff: BackoffStrategy::Exponential { base_secs: 1 },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackoffStrategy {
    Fixed { secs: u64 },
    Exponential { base_secs: u64 },
}

impl BackoffStrategy {
    pub fn delay(&self, attempt: usize) -> std::time::Duration {
        match self {
            BackoffStrategy::Fixed { secs } => {
                std::time::Duration::from_secs(*secs)
            }
            BackoffStrategy::Exponential { base_secs } => {
                let secs = base_secs * 2u64.saturating_pow(attempt as u32 - 1);
                std::time::Duration::from_secs(secs)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub name: String,
    pub node_type: NodeType,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub condition: Option<String>,
    #[serde(default)]
    pub retry: RetryConfig,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerConfig {
    Cron { expression: String, timezone: String },
    Webhook { path: String, method: String },
    Event { event_type: String, filter: Option<Value> },
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookConfig {
    pub on_start: Option<Vec<String>>,
    pub on_error: Option<Vec<String>>,
    pub on_complete: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub version: u32,
    pub trigger: TriggerConfig,
    #[serde(default)]
    pub variables: HashMap<String, Value>,
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub hooks: HookConfig,
}

impl Workflow {
    pub fn get_node(&self, id: &str) -> Option<&WorkflowNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn get_node_mut(&mut self, id: &str) -> Option<&mut WorkflowNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn parse_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    pub fn parse_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn parse_definition(content: &str) -> Result<Self, String> {
        if let Ok(wf) = Self::parse_json(content) {
            return Ok(wf);
        }
        if let Ok(wf) = Self::parse_yaml(content) {
            return Ok(wf);
        }
        Err("Failed to parse as JSON or YAML".to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub node_id: String,
    pub output: Value,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExecution {
    pub exec_id: Uuid,
    pub workflow_id: String,
    pub workflow_version: u32,
    pub status: ExecStatus,
    pub context: HashMap<String, Value>,
    pub checkpoints: Vec<Checkpoint>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub agent_id: Option<String>,
    pub error: Option<String>,
}

impl WorkflowExecution {
    pub fn new(workflow_id: &str, workflow_version: u32, input: Value) -> Self {
        let mut context = HashMap::new();
        context.insert("input".to_string(), input);

        Self {
            exec_id: Uuid::new_v4(),
            workflow_id: workflow_id.to_string(),
            workflow_version,
            status: ExecStatus::Pending,
            context,
            checkpoints: Vec::new(),
            started_at: Utc::now(),
            completed_at: None,
            agent_id: None,
            error: None,
        }
    }

    pub fn set_running(&mut self) {
        self.status = ExecStatus::Running;
    }

    pub fn set_completed(&mut self) {
        self.status = ExecStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    pub fn set_failed(&mut self, error: &str) {
        self.status = ExecStatus::Failed;
        self.error = Some(error.to_string());
        self.completed_at = Some(Utc::now());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    pub exec_id: Uuid,
    pub workflow_id: String,
    pub status: ExecStatus,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub duration_secs: f64,
}

impl ExecResult {
    pub fn from_execution(exec: &WorkflowExecution) -> Self {
        let duration = exec.completed_at
            .map(|end| (end - exec.started_at).num_seconds() as f64)
            .unwrap_or(0.0);

        Self {
            exec_id: exec.exec_id,
            workflow_id: exec.workflow_id.clone(),
            status: exec.status.clone(),
            output: exec.context.get("output").cloned(),
            error: exec.error.clone(),
            duration_secs: duration,
        }
    }
}
