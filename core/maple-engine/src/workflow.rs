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
    Agent {
        agent_id: String,
        goal: String,
        timeout_secs: Option<u64>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_workflow_creation() {
        let workflow = Workflow {
            id: "test-workflow".to_string(),
            name: "Test Workflow".to_string(),
            description: None,
            version: 1,
            trigger: TriggerConfig::Manual,
            variables: HashMap::new(),
            nodes: vec![],
            hooks: HookConfig::default(),
        };

        assert_eq!(workflow.id, "test-workflow");
        assert_eq!(workflow.name, "Test Workflow");
        assert_eq!(workflow.version, 1);
        assert!(workflow.nodes.is_empty());
    }

    #[test]
    fn test_workflow_node_creation() {
        let node = WorkflowNode {
            id: "node-1".to_string(),
            name: "LLM Call".to_string(),
            node_type: NodeType::Llm {
                model_route: "auto".to_string(),
                prompt_ref: "default".to_string(),
                temperature: Some(0.7),
            },
            depends_on: vec![],
            condition: None,
            retry: RetryConfig::default(),
            timeout_secs: Some(300),
        };

        assert_eq!(node.id, "node-1");
        assert_eq!(node.name, "LLM Call");
        assert!(node.condition.is_none());
        assert_eq!(node.timeout_secs, Some(300));
    }

    #[test]
    fn test_workflow_parse_definition() {
        let yaml = r#"
id: test-workflow
name: Test Workflow
version: 1
nodes:
  - id: node-1
    name: LLM Call
    node_type:
      type: llm
      model_route: auto
      prompt_ref: default
      temperature: 0.7
trigger:
  type: manual
"#;

        let workflow = Workflow::parse_yaml(yaml).unwrap();
        assert_eq!(workflow.id, "test-workflow");
        assert_eq!(workflow.name, "Test Workflow");
        assert_eq!(workflow.version, 1);
        assert_eq!(workflow.nodes.len(), 1);
        
        let node = &workflow.nodes[0];
        assert_eq!(node.id, "node-1");
        assert_eq!(node.name, "LLM Call");
        
        if let NodeType::Llm { model_route, prompt_ref, temperature } = &node.node_type {
            assert_eq!(model_route, "auto");
            assert_eq!(prompt_ref, "default");
            assert_eq!(*temperature, Some(0.7));
        } else {
            panic!("Expected LLM node type");
        }
    }

    #[test]
    fn test_workflow_parse_invalid_yaml() {
        let yaml = "invalid: yaml: content: [";
        let result = Workflow::parse_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_node_type_variants() {
        let llm_node = NodeType::Llm {
            model_route: "auto".to_string(),
            prompt_ref: "default".to_string(),
            temperature: None,
        };

        let tool_node = NodeType::Tool {
            skill_id: "web_search".to_string(),
            config: json!({}),
        };

        let condition_node = NodeType::Condition {
            expression: "true".to_string(),
            branches: vec![],
        };

        assert!(matches!(llm_node, NodeType::Llm { .. }));
        assert!(matches!(tool_node, NodeType::Tool { .. }));
        assert!(matches!(condition_node, NodeType::Condition { .. }));
    }

    #[test]
    fn test_trigger_config() {
        let trigger = TriggerConfig::Manual;
        assert!(matches!(trigger, TriggerConfig::Manual));
    }

    #[test]
    fn test_workflow_execution_status() {
        let statuses = vec![
            ExecStatus::Pending,
            ExecStatus::Running,
            ExecStatus::Completed,
            ExecStatus::Failed,
            ExecStatus::Cancelled,
        ];

        for status in statuses {
            let exec = WorkflowExecution {
                exec_id: Uuid::new_v4(),
                workflow_id: "test".to_string(),
                workflow_version: 1,
                status: status.clone(),
                context: HashMap::new(),
                checkpoints: Vec::new(),
                started_at: Utc::now(),
                completed_at: None,
                agent_id: None,
                error: None,
            };

            assert_eq!(exec.status, status);
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
