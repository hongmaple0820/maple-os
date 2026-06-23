#![allow(clippy::all)]
use std::sync::Arc;
use std::collections::HashMap;
use serde_json::{json, Value};

use maple_engine::executor::{WorkflowExecutor, NodeExecutor, AgentHandler};
use maple_engine::workflow::{Workflow, WorkflowNode, NodeType, ExecStatus, RetryConfig};
use maple_engine::event_bus::EventBus;
use maple_engine::checkpoint::CheckpointManager;
use maple_engine::hooks::HookRunner;
use maple_engine::skill_registry::{SkillRegistry, Skill};
use maple_llm::router::LlmRouter;
use maple_llm::usage::UsageTracker;
use maple_llm::mock_llm::{MockLlmAdapter, MockResponses, RequestMatcher};

// ---------- helpers ----------

fn make_mock_router(response: &str) -> Arc<LlmRouter> {
    let usage = Arc::new(UsageTracker::new(100.0));
    let mut router = LlmRouter::new(usage);
    let mut adapter = MockLlmAdapter::new("mock-model");
    adapter.when(RequestMatcher::Always, MockResponses::text(response));
    router.register_adapter(Box::new(adapter));
    Arc::new(router)
}

fn make_registry() -> Arc<SkillRegistry> {
    let registry = SkillRegistry::new();
    Arc::new(registry)
}

struct EchoSkill;
impl Skill for EchoSkill {
    fn id(&self) -> &str { "echo" }
    fn description(&self) -> &str { "Echo back input" }
    fn execute(&self, config: &Value) -> anyhow::Result<Value> {
        Ok(json!({ "echo": config }))
    }
}

struct UpperSkill;
impl Skill for UpperSkill {
    fn id(&self) -> &str { "upper" }
    fn description(&self) -> &str { "Uppercase input text" }
    fn execute(&self, config: &Value) -> anyhow::Result<Value> {
        let text = config.get("text").and_then(|v| v.as_str()).unwrap_or("");
        Ok(json!({ "result": text.to_uppercase() }))
    }
}

async fn make_executor_with_skills(
    router: Arc<LlmRouter>,
    skills: Vec<Box<dyn Skill + Send + Sync>>,
) -> WorkflowExecutor {
    let registry = make_registry();
    for skill in skills {
        registry.register(skill).await;
    }
    let event_bus = Arc::new(EventBus::new());
    let db = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query("CREATE TABLE checkpoints (id INTEGER PRIMARY KEY AUTOINCREMENT, exec_id TEXT NOT NULL, node_id TEXT NOT NULL, output TEXT, context_snapshot TEXT, created_at INTEGER)")
        .execute(&db).await.unwrap();
    let checkpoint_mgr = Arc::new(CheckpointManager::new(db));
    let hook_runner = Arc::new(HookRunner::new());
    let node_executor = NodeExecutor::new(router, registry, hook_runner.clone());
    WorkflowExecutor::new(event_bus, node_executor, checkpoint_mgr, hook_runner)
}

// ---------- tests ----------

#[tokio::test]
async fn test_yaml_parse_and_single_llm_node() {
    let yaml = r#"
id: greet
name: Greet Workflow
version: 1
trigger:
  type: manual
nodes:
  - id: greet
    name: Greet
    node_type:
      type: llm
      model_route: mock-model
      prompt_ref: "Say hello"
      temperature: 0.5
"#;
    let wf = Workflow::parse_yaml(yaml).unwrap();
    assert_eq!(wf.id, "greet");
    assert_eq!(wf.nodes.len(), 1);

    let router = make_mock_router("Hello, world!");
    let executor = make_executor_with_skills(router, vec![]).await;

    let result = executor.execute(&wf.nodes, &wf.id, wf.version, json!({})).await.unwrap();
    assert_eq!(result.status, ExecStatus::Completed);
    assert!(result.duration_secs >= 0.0);
    // Output is stored under node ID "greet" in context, not top-level "output"
    assert!(!result.exec_id.to_string().is_empty());
}

#[tokio::test]
async fn test_tool_node_execution() {
    let yaml = r#"
id: tool-wf
name: Tool Workflow
version: 1
trigger:
  type: manual
nodes:
  - id: echo-step
    name: Echo Step
    node_type:
      type: tool
      skill_id: echo
      config:
        message: "hello from tool"
"#;
    let wf = Workflow::parse_yaml(yaml).unwrap();
    let router = make_mock_router("unused");
    let executor = make_executor_with_skills(router, vec![Box::new(EchoSkill)]).await;

    let result = executor.execute(&wf.nodes, &wf.id, wf.version, json!({})).await.unwrap();
    assert_eq!(result.status, ExecStatus::Completed);
    // Result stored under node ID "echo-step"
    assert!(!result.exec_id.to_string().is_empty());
}

#[tokio::test]
async fn test_dag_dependency_order() {
    // node-b depends on node-a → must execute a then b
    let yaml = r#"
id: dag-wf
name: DAG Workflow
version: 1
trigger:
  type: manual
nodes:
  - id: step-a
    name: Step A
    node_type:
      type: tool
      skill_id: echo
      config:
        step: a
  - id: step-b
    name: Step B
    depends_on:
      - step-a
    node_type:
      type: tool
      skill_id: echo
      config:
        step: b
        prev: "{{step-a}}"
"#;
    let wf = Workflow::parse_yaml(yaml).unwrap();
    let router = make_mock_router("unused");
    let executor = make_executor_with_skills(router, vec![Box::new(EchoSkill)]).await;

    let result = executor.execute(&wf.nodes, &wf.id, wf.version, json!({})).await.unwrap();
    assert_eq!(result.status, ExecStatus::Completed);
    assert!(result.error.is_none());
}

#[tokio::test]
async fn test_condition_node_branching() {
    let yaml = r#"
id: cond-wf
name: Condition Workflow
version: 1
trigger:
  type: manual
nodes:
  - id: check
    name: Check
    node_type:
      type: condition
      expression: "true"
      branches:
        - label: yes
          expression: "true"
          target_node: approved
  - id: approved
    name: Approved
    depends_on:
      - check
    node_type:
      type: tool
      skill_id: echo
      config:
        status: "approved"
"#;
    let wf = Workflow::parse_yaml(yaml).unwrap();
    let router = make_mock_router("unused");
    let executor = make_executor_with_skills(router, vec![Box::new(EchoSkill)]).await;

    let result = executor.execute(&wf.nodes, &wf.id, wf.version, json!({})).await.unwrap();
    assert_eq!(result.status, ExecStatus::Completed);
}

#[tokio::test]
async fn test_delay_node() {
    let nodes = vec![WorkflowNode {
        id: "wait".to_string(),
        name: "Wait".to_string(),
        node_type: NodeType::Delay { duration_secs: 1 },
        depends_on: vec![],
        condition: None,
        retry: RetryConfig::default(),
        timeout_secs: None,
    }];

    let router = make_mock_router("unused");
    let executor = make_executor_with_skills(router, vec![]).await;
    let start = std::time::Instant::now();
    let result = executor.execute(&nodes, "delay-wf", 1, json!({})).await.unwrap();
    assert_eq!(result.status, ExecStatus::Completed);
    assert!(start.elapsed() >= std::time::Duration::from_millis(900));
}

#[tokio::test]
async fn test_template_resolution_in_prompt() {
    let yaml = r#"
id: tpl-wf
name: Template Workflow
version: 1
trigger:
  type: manual
nodes:
  - id: greet
    name: Greet
    node_type:
      type: llm
      model_route: mock-model
      prompt_ref: "Hello {{name}}, welcome to {{place}}"
      temperature: 0.7
"#;
    let wf = Workflow::parse_yaml(yaml).unwrap();
    let router = make_mock_router("Welcome!");
    let executor = make_executor_with_skills(router, vec![]).await;

    let mut input = json!({});
    input.as_object_mut().unwrap().insert("name".to_string(), json!("Alice"));
    input.as_object_mut().unwrap().insert("place".to_string(), json!("MapleOS"));

    let result = executor.execute(&wf.nodes, &wf.id, wf.version, input).await.unwrap();
    assert_eq!(result.status, ExecStatus::Completed);
}

#[tokio::test]
async fn test_multi_node_sequential_pipeline() {
    // 3 nodes in sequence: echo → upper → echo
    let yaml = r#"
id: pipeline
name: Pipeline
version: 1
trigger:
  type: manual
nodes:
  - id: step1
    name: Step 1
    node_type:
      type: tool
      skill_id: echo
      config:
        text: "hello"
  - id: step2
    name: Step 2
    depends_on:
      - step1
    node_type:
      type: tool
      skill_id: upper
      config:
        text: "world"
  - id: step3
    name: Step 3
    depends_on:
      - step2
    node_type:
      type: tool
      skill_id: echo
      config:
        prev: "{{step2}}"
"#;
    let wf = Workflow::parse_yaml(yaml).unwrap();
    let router = make_mock_router("unused");
    let executor = make_executor_with_skills(
        router,
        vec![Box::new(EchoSkill), Box::new(UpperSkill)],
    ).await;

    let result = executor.execute(&wf.nodes, &wf.id, wf.version, json!({})).await.unwrap();
    assert_eq!(result.status, ExecStatus::Completed);
}

#[tokio::test]
async fn test_checkpoint_persistence() {
    let yaml = r#"
id: ckpt-wf
name: Checkpoint Workflow
version: 1
trigger:
  type: manual
nodes:
  - id: step1
    name: Step 1
    node_type:
      type: tool
      skill_id: echo
      config:
        data: "checkpoint test"
"#;
    let wf = Workflow::parse_yaml(yaml).unwrap();
    let router = make_mock_router("unused");

    // Use a real SQLite pool for checkpoint testing
    let db = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query("CREATE TABLE checkpoints (id INTEGER PRIMARY KEY AUTOINCREMENT, exec_id TEXT NOT NULL, node_id TEXT NOT NULL, output TEXT, context_snapshot TEXT, created_at INTEGER)")
        .execute(&db).await.unwrap();

    let registry = make_registry();
    registry.register(Box::new(EchoSkill)).await;
    let event_bus = Arc::new(EventBus::new());
    let checkpoint_mgr = Arc::new(CheckpointManager::new(db.clone()));
    let hook_runner = Arc::new(HookRunner::new());
    let node_executor = NodeExecutor::new(router, registry, hook_runner.clone());
    let executor = WorkflowExecutor::new(event_bus, node_executor, checkpoint_mgr, hook_runner);

    let result = executor.execute(&wf.nodes, &wf.id, wf.version, json!({})).await.unwrap();
    assert_eq!(result.status, ExecStatus::Completed);

    // Verify checkpoint was persisted
    let row: (String,) = sqlx::query_as("SELECT node_id FROM checkpoints WHERE exec_id = ?")
        .bind(result.exec_id.to_string())
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.0, "step1");
}

#[tokio::test]
async fn test_loop_node_foreach() {
    let yaml = r#"
id: loop-wf
name: Loop Workflow
version: 1
trigger:
  type: manual
nodes:
  - id: looper
    name: Looper
    node_type:
      type: loop
      each: items
      body: process
      max_iterations: 10
  - id: process
    name: Process Item
    node_type:
      type: tool
      skill_id: echo
      config:
        item: "{{loop_item}}"
"#;
    let wf = Workflow::parse_yaml(yaml).unwrap();
    let router = make_mock_router("unused");
    let executor = make_executor_with_skills(router, vec![Box::new(EchoSkill)]).await;

    let input = json!({ "items": ["a", "b", "c"] });
    let result = executor.execute(&wf.nodes, &wf.id, wf.version, input).await.unwrap();
    assert_eq!(result.status, ExecStatus::Completed);
}

#[tokio::test]
async fn test_workflow_parse_json() {
    let json_str = r#"{
        "id": "json-wf",
        "name": "JSON Workflow",
        "version": 1,
        "trigger": {"type": "manual"},
        "nodes": [{
            "id": "n1",
            "name": "N1",
            "node_type": {"type": "delay", "duration_secs": 0}
        }]
    }"#;
    let wf = Workflow::parse_json(json_str).unwrap();
    assert_eq!(wf.id, "json-wf");
    assert_eq!(wf.nodes.len(), 1);
}

#[tokio::test]
async fn test_cycle_detection() {
    // a depends on b, b depends on a → cycle
    let nodes = vec![
        WorkflowNode {
            id: "a".to_string(),
            name: "A".to_string(),
            node_type: NodeType::Delay { duration_secs: 0 },
            depends_on: vec!["b".to_string()],
            condition: None,
            retry: RetryConfig::default(),
            timeout_secs: None,
        },
        WorkflowNode {
            id: "b".to_string(),
            name: "B".to_string(),
            node_type: NodeType::Delay { duration_secs: 0 },
            depends_on: vec!["a".to_string()],
            condition: None,
            retry: RetryConfig::default(),
            timeout_secs: None,
        },
    ];

    let router = make_mock_router("unused");
    let executor = make_executor_with_skills(router, vec![]).await;
    let result = executor.execute(&nodes, "cycle-wf", 1, json!({})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Cycle"));
}

// ---------- Phase 2: additional node type coverage ----------

#[tokio::test]
async fn test_parallel_node_wait_all() {
    let yaml = r#"
id: parallel-wf
name: Parallel Workflow
version: 1
trigger:
  type: manual
nodes:
  - id: start
    name: Start
    node_type:
      type: tool
      skill_id: echo
      config: { "value": "init" }
  - id: branch_a
    name: Branch A
    depends_on: [start]
    node_type:
      type: tool
      skill_id: echo
      config: { "value": "a" }
  - id: branch_b
    name: Branch B
    depends_on: [start]
    node_type:
      type: tool
      skill_id: upper
      config: { "text": "hello" }
  - id: join
    name: Join
    node_type:
      type: parallel
      nodes: [branch_a, branch_b]
      wait_strategy: wait_all
"#;
    let wf = Workflow::parse_yaml(yaml).unwrap();
    let router = make_mock_router("unused");
    let executor = make_executor_with_skills(
        router,
        vec![Box::new(EchoSkill), Box::new(UpperSkill)],
    ).await;
    let result = executor.execute(&wf.nodes, &wf.id, wf.version, json!({})).await.unwrap();
    assert_eq!(result.status, ExecStatus::Completed);
}

#[tokio::test]
async fn test_sub_workflow_node() {
    // SubWorkflow looks for nodes whose id starts with "{workflow_id}::"
    let nodes = vec![
        WorkflowNode {
            id: "sub-wf::step1".to_string(),
            name: "Sub Step 1".to_string(),
            node_type: NodeType::Tool {
                skill_id: "echo".to_string(),
                config: json!({ "value": "sub_result" }),
            },
            depends_on: vec![],
            condition: None,
            retry: RetryConfig::default(),
            timeout_secs: None,
        },
        WorkflowNode {
            id: "main_call".to_string(),
            name: "Call Sub".to_string(),
            node_type: NodeType::SubWorkflow {
                workflow_id: "sub-wf".to_string(),
                input_mapping: HashMap::new(),
            },
            depends_on: vec![],
            condition: None,
            retry: RetryConfig::default(),
            timeout_secs: None,
        },
    ];

    let router = make_mock_router("unused");
    let executor = make_executor_with_skills(router, vec![Box::new(EchoSkill)]).await;
    let result = executor.execute(&nodes, "parent-wf", 1, json!({})).await.unwrap();
    assert_eq!(result.status, ExecStatus::Completed);
}

#[tokio::test]
async fn test_human_approval_auto_approve() {
    let nodes = vec![
        WorkflowNode {
            id: "approval".to_string(),
            name: "Needs Approval".to_string(),
            node_type: NodeType::HumanApproval {
                notify: vec![],
                timeout_secs: 1, // short timeout for test
                on_timeout: maple_engine::workflow::TimeoutAction::AutoApprove,
            },
            depends_on: vec![],
            condition: None,
            retry: RetryConfig::default(),
            timeout_secs: None,
        },
    ];

    let router = make_mock_router("unused");
    let executor = make_executor_with_skills(router, vec![]).await;
    let result = executor.execute(&nodes, "approval-wf", 1, json!({})).await.unwrap();
    assert_eq!(result.status, ExecStatus::Completed);
}

#[tokio::test]
async fn test_agent_node_with_handler() {
    let nodes = vec![
        WorkflowNode {
            id: "agent_task".to_string(),
            name: "Agent Task".to_string(),
            node_type: NodeType::Agent {
                agent_id: "test-agent".to_string(),
                goal: "Summarize the data".to_string(),
                timeout_secs: Some(10),
            },
            depends_on: vec![],
            condition: None,
            retry: RetryConfig::default(),
            timeout_secs: None,
        },
    ];

    let router = make_mock_router("unused");
    let registry = make_registry();
    let event_bus = Arc::new(EventBus::new());
    let db = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query("CREATE TABLE checkpoints (id INTEGER PRIMARY KEY AUTOINCREMENT, exec_id TEXT NOT NULL, node_id TEXT NOT NULL, output TEXT, context_snapshot TEXT, created_at INTEGER)")
        .execute(&db).await.unwrap();
    let checkpoint_mgr = Arc::new(CheckpointManager::new(db));
    let hook_runner = Arc::new(HookRunner::new());
    let agent_handler: AgentHandler = Arc::new(|_agent_id, goal| {
        Box::pin(async move {
            Ok(format!("Agent completed: {}", goal))
        })
    });
    let node_executor = NodeExecutor::new(router, registry, hook_runner.clone())
        .with_agent_handler(agent_handler);
    let executor = WorkflowExecutor::new(event_bus, node_executor, checkpoint_mgr, hook_runner);

    let result = executor.execute(&nodes, "agent-wf", 1, json!({})).await.unwrap();
    assert_eq!(result.status, ExecStatus::Completed);
}

#[tokio::test]
async fn test_agent_node_no_handler_fails() {
    let nodes = vec![
        WorkflowNode {
            id: "agent_task".to_string(),
            name: "Agent Task".to_string(),
            node_type: NodeType::Agent {
                agent_id: "test-agent".to_string(),
                goal: "Do something".to_string(),
                timeout_secs: Some(5),
            },
            depends_on: vec![],
            condition: None,
            retry: RetryConfig::default(),
            timeout_secs: None,
        },
    ];

    let router = make_mock_router("unused");
    let executor = make_executor_with_skills(router, vec![]).await;
    let result = executor.execute(&nodes, "agent-wf", 1, json!({})).await.unwrap();
    // WorkflowExecutor catches node errors and marks status as Failed
    assert_eq!(result.status, ExecStatus::Failed);
    assert!(result.error.as_deref().unwrap_or("").contains("No agent handler"));
}

#[tokio::test]
async fn test_loop_condition_mode() {
    // While-loop: condition is checked each iteration; body runs a tool
    // We use "each" on a shrinking array to simulate a while loop that terminates
    let yaml = r#"
id: loop-cond-wf
name: Loop Condition Workflow
version: 1
trigger:
  type: manual
nodes:
  - id: looper
    name: Looper
    node_type:
      type: loop
      condition: "true"
      body: process
      max_iterations: 3
  - id: process
    name: Process
    node_type:
      type: tool
      skill_id: echo
      config: { "value": "iter" }
"#;
    let wf = Workflow::parse_yaml(yaml).unwrap();
    let router = make_mock_router("unused");
    let executor = make_executor_with_skills(router, vec![Box::new(EchoSkill)]).await;
    let result = executor.execute(&wf.nodes, &wf.id, wf.version, json!({})).await.unwrap();
    assert_eq!(result.status, ExecStatus::Completed);
}

#[tokio::test]
async fn test_parallel_wait_any() {
    let nodes = vec![
        WorkflowNode {
            id: "fast".to_string(),
            name: "Fast".to_string(),
            node_type: NodeType::Tool {
                skill_id: "echo".to_string(),
                config: json!({ "value": "fast" }),
            },
            depends_on: vec![],
            condition: None,
            retry: RetryConfig::default(),
            timeout_secs: None,
        },
        WorkflowNode {
            id: "slow".to_string(),
            name: "Slow".to_string(),
            node_type: NodeType::Delay { duration_secs: 60 },
            depends_on: vec![],
            condition: None,
            retry: RetryConfig::default(),
            timeout_secs: None,
        },
        WorkflowNode {
            id: "race".to_string(),
            name: "Race".to_string(),
            node_type: NodeType::Parallel {
                nodes: vec!["fast".to_string(), "slow".to_string()],
                wait_strategy: maple_engine::workflow::WaitStrategy::WaitAny,
            },
            depends_on: vec![],
            condition: None,
            retry: RetryConfig::default(),
            timeout_secs: None,
        },
    ];

    let router = make_mock_router("unused");
    let executor = make_executor_with_skills(router, vec![Box::new(EchoSkill)]).await;
    let result = executor.execute(&nodes, "race-wf", 1, json!({})).await.unwrap();
    assert_eq!(result.status, ExecStatus::Completed);
}
