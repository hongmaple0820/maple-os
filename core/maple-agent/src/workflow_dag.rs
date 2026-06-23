use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Workflow DAG Engine — visual workflow execution
///
/// Node types:
/// - Agent: Delegate to an agent
/// - LLM: Direct LLM call
/// - Tool: Execute a tool
/// - Condition: Conditional branching
/// - Parallel: Parallel execution of children
/// - Loop: Loop until condition met
/// - Start/End: Entry/exit points
///
///   Node type in the workflow DAG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    /// Start node (entry point)
    Start,
    /// End node (exit point)
    End,
    /// Delegate to an agent
    Agent { agent_id: String },
    /// Direct LLM call
    Llm { prompt_template: String },
    /// Execute a tool
    Tool { tool_name: String },
    /// Conditional branching
    Condition { expression: String },
    /// Parallel execution of downstream nodes
    Parallel,
    /// Loop until condition is false
    Loop { condition: String, max_iterations: u32 },
    /// Transform/map data
    Transform { expression: String },
}

/// Workflow node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub name: String,
    pub node_type: NodeType,
    /// Downstream node IDs
    pub next: Vec<String>,
    /// For Condition nodes: true_branch, false_branch
    pub branches: Option<(String, String)>,
    /// Input mapping (variable name → expression)
    pub input_mapping: HashMap<String, String>,
    /// Output variable name
    pub output_var: Option<String>,
}

/// Workflow definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub nodes: Vec<WorkflowNode>,
    pub variables: HashMap<String, serde_json::Value>,
}

/// Node execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

/// Node execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResult {
    pub node_id: String,
    pub status: NodeStatus,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

/// Workflow execution state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowState {
    pub workflow_id: String,
    pub status: WorkflowStatus,
    pub variables: HashMap<String, serde_json::Value>,
    pub node_results: HashMap<String, NodeResult>,
    pub execution_order: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowStatus {
    NotStarted,
    Running,
    Completed,
    Failed,
    Paused,
}

/// Workflow DAG executor
pub struct WorkflowExecutor {
    definition: WorkflowDefinition,
    state: WorkflowState,
}

impl WorkflowExecutor {
    pub fn new(definition: WorkflowDefinition) -> Self {
        let variables = definition.variables.clone();
        let workflow_id = definition.id.clone();

        Self {
            definition,
            state: WorkflowState {
                workflow_id,
                status: WorkflowStatus::NotStarted,
                variables,
                node_results: HashMap::new(),
                execution_order: Vec::new(),
            },
        }
    }

    /// Get current workflow state
    pub fn state(&self) -> &WorkflowState {
        &self.state
    }

    /// Validate the DAG (check for cycles, missing nodes, etc.)
    pub fn validate(&self) -> Result<(), WorkflowError> {
        let node_ids: HashSet<&str> = self
            .definition
            .nodes
            .iter()
            .map(|n| n.id.as_str())
            .collect();

        // Check all referenced nodes exist
        for node in &self.definition.nodes {
            for next_id in &node.next {
                if !node_ids.contains(next_id.as_str()) {
                    return Err(WorkflowError::MissingNode(next_id.clone()));
                }
            }
            if let Some((ref true_id, ref false_id)) = node.branches {
                if !node_ids.contains(true_id.as_str()) {
                    return Err(WorkflowError::MissingNode(true_id.clone()));
                }
                if !node_ids.contains(false_id.as_str()) {
                    return Err(WorkflowError::MissingNode(false_id.clone()));
                }
            }
        }

        // Check for cycles using DFS
        if self.has_cycle() {
            return Err(WorkflowError::CycleDetected);
        }

        // Check for exactly one start node
        let start_count = self
            .definition
            .nodes
            .iter()
            .filter(|n| matches!(n.node_type, NodeType::Start))
            .count();
        if start_count != 1 {
            return Err(WorkflowError::InvalidStartNode(start_count));
        }

        Ok(())
    }

    /// Get the start node
    pub fn start_node(&self) -> Option<&WorkflowNode> {
        self.definition
            .nodes
            .iter()
            .find(|n| matches!(n.node_type, NodeType::Start))
    }

    /// Get next executable nodes (dependencies satisfied)
    pub fn ready_nodes(&self) -> Vec<&WorkflowNode> {
        self.definition
            .nodes
            .iter()
            .filter(|n| {
                // Must be pending
                let result = self.state.node_results.get(&n.id);
                result.is_none_or(|r| r.status == NodeStatus::Pending)
            })
            .filter(|n| {
                // All predecessors must be completed
                self.predecessors(&n.id)
                    .iter()
                    .all(|pred_id| {
                        self.state
                            .node_results
                            .get(pred_id)
                            .is_some_and(|r| r.status == NodeStatus::Completed)
                    })
            })
            .collect()
    }

    /// Mark a node as completed with output
    pub fn complete_node(
        &mut self,
        node_id: &str,
        output: Option<serde_json::Value>,
    ) -> Result<(), WorkflowError> {
        let node = self
            .definition
            .nodes
            .iter()
            .find(|n| n.id == node_id)
            .ok_or_else(|| WorkflowError::MissingNode(node_id.into()))?;

        let now = chrono::Utc::now().timestamp();

        // Store output in variables if specified
        if let Some(ref var_name) = node.output_var
            && let Some(val) = output.clone()
        {
            self.state.variables.insert(var_name.clone(), val);
        }

        self.state.node_results.insert(
            node_id.to_string(),
            NodeResult {
                node_id: node_id.into(),
                status: NodeStatus::Completed,
                output,
                error: None,
                started_at: Some(now),
                completed_at: Some(now),
            },
        );

        self.state.execution_order.push(node_id.into());

        // Transition status
        if self.is_end_node(node_id) {
            self.state.status = WorkflowStatus::Completed;
        } else if self.state.status == WorkflowStatus::NotStarted {
            self.state.status = WorkflowStatus::Running;
        }

        Ok(())
    }

    /// Mark a node as failed
    pub fn fail_node(&mut self, node_id: &str, error: String) -> Result<(), WorkflowError> {
        let now = chrono::Utc::now().timestamp();

        self.state.node_results.insert(
            node_id.to_string(),
            NodeResult {
                node_id: node_id.into(),
                status: NodeStatus::Failed,
                output: None,
                error: Some(error),
                started_at: Some(now),
                completed_at: Some(now),
            },
        );

        self.state.status = WorkflowStatus::Failed;
        Ok(())
    }

    /// Get node by ID
    pub fn get_node(&self, id: &str) -> Option<&WorkflowNode> {
        self.definition.nodes.iter().find(|n| n.id == id)
    }

    /// Get all nodes
    pub fn nodes(&self) -> &[WorkflowNode] {
        &self.definition.nodes
    }

    /// Resolve condition and return branch node ID
    pub fn resolve_condition(&self, node_id: &str, result: bool) -> Option<String> {
        let node = self.get_node(node_id)?;
        if let Some((ref true_id, ref false_id)) = node.branches {
            Some(if result { true_id.clone() } else { false_id.clone() })
        } else {
            None
        }
    }

    /// Get predecessors of a node
    fn predecessors(&self, node_id: &str) -> Vec<String> {
        self.definition
            .nodes
            .iter()
            .filter(|n| {
                n.next.contains(&node_id.to_string())
                    || n.branches.as_ref().is_some_and(|(t, f)| {
                        t == node_id || f == node_id
                    })
            })
            .map(|n| n.id.clone())
            .collect()
    }

    fn is_end_node(&self, node_id: &str) -> bool {
        self.get_node(node_id)
            .map(|n| matches!(n.node_type, NodeType::End))
            .unwrap_or(false)
    }

    fn has_cycle(&self) -> bool {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for node in &self.definition.nodes {
            if self.dfs_cycle(&node.id, &mut visited, &mut rec_stack) {
                return true;
            }
        }
        false
    }

    fn dfs_cycle(
        &self,
        node_id: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> bool {
        if rec_stack.contains(node_id) {
            return true;
        }
        if visited.contains(node_id) {
            return false;
        }

        visited.insert(node_id.into());
        rec_stack.insert(node_id.into());

        if let Some(node) = self.get_node(node_id) {
            for next_id in &node.next {
                if self.dfs_cycle(next_id, visited, rec_stack) {
                    return true;
                }
            }
            if let Some((ref t, ref f)) = node.branches {
                if self.dfs_cycle(t, visited, rec_stack) {
                    return true;
                }
                if self.dfs_cycle(f, visited, rec_stack) {
                    return true;
                }
            }
        }

        rec_stack.remove(node_id);
        false
    }
}

/// Builder for workflow definitions
pub struct WorkflowBuilder {
    id: String,
    name: String,
    description: String,
    nodes: Vec<WorkflowNode>,
    variables: HashMap<String, serde_json::Value>,
}

impl WorkflowBuilder {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            nodes: Vec::new(),
            variables: HashMap::new(),
        }
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.description = desc.into();
        self
    }

    pub fn variable(mut self, key: &str, value: serde_json::Value) -> Self {
        self.variables.insert(key.into(), value);
        self
    }

    pub fn add_node(mut self, node: WorkflowNode) -> Self {
        self.nodes.push(node);
        self
    }

    pub fn build(self) -> WorkflowDefinition {
        WorkflowDefinition {
            id: self.id,
            name: self.name,
            description: self.description,
            nodes: self.nodes,
            variables: self.variables,
        }
    }
}

/// Helper to create common node types
pub fn start_node(id: &str, next: &str) -> WorkflowNode {
    WorkflowNode {
        id: id.into(),
        name: "Start".into(),
        node_type: NodeType::Start,
        next: vec![next.into()],
        branches: None,
        input_mapping: HashMap::new(),
        output_var: None,
    }
}

pub fn end_node(id: &str) -> WorkflowNode {
    WorkflowNode {
        id: id.into(),
        name: "End".into(),
        node_type: NodeType::End,
        next: vec![],
        branches: None,
        input_mapping: HashMap::new(),
        output_var: None,
    }
}

pub fn tool_node(id: &str, tool_name: &str, next: &str) -> WorkflowNode {
    WorkflowNode {
        id: id.into(),
        name: format!("Tool: {}", tool_name),
        node_type: NodeType::Tool {
            tool_name: tool_name.into(),
        },
        next: vec![next.into()],
        branches: None,
        input_mapping: HashMap::new(),
        output_var: None,
    }
}

pub fn condition_node(id: &str, expr: &str, true_branch: &str, false_branch: &str) -> WorkflowNode {
    WorkflowNode {
        id: id.into(),
        name: format!("Condition: {}", expr),
        node_type: NodeType::Condition {
            expression: expr.into(),
        },
        next: vec![true_branch.into(), false_branch.into()],
        branches: Some((true_branch.into(), false_branch.into())),
        input_mapping: HashMap::new(),
        output_var: None,
    }
}

pub fn llm_node(id: &str, prompt: &str, next: &str) -> WorkflowNode {
    WorkflowNode {
        id: id.into(),
        name: "LLM Call".into(),
        node_type: NodeType::Llm {
            prompt_template: prompt.into(),
        },
        next: vec![next.into()],
        branches: None,
        input_mapping: HashMap::new(),
        output_var: None,
    }
}

pub fn agent_node(id: &str, agent_id: &str, next: &str) -> WorkflowNode {
    WorkflowNode {
        id: id.into(),
        name: format!("Agent: {}", agent_id),
        node_type: NodeType::Agent {
            agent_id: agent_id.into(),
        },
        next: vec![next.into()],
        branches: None,
        input_mapping: HashMap::new(),
        output_var: None,
    }
}

pub fn parallel_node(id: &str, branches: Vec<&str>) -> WorkflowNode {
    WorkflowNode {
        id: id.into(),
        name: "Parallel".into(),
        node_type: NodeType::Parallel,
        next: branches.iter().map(|b| (*b).to_string()).collect(),
        branches: None,
        input_mapping: HashMap::new(),
        output_var: None,
    }
}

pub fn loop_node(id: &str, condition: &str, body: &str, max_iter: u32) -> WorkflowNode {
    WorkflowNode {
        id: id.into(),
        name: format!("Loop: {}", condition),
        node_type: NodeType::Loop {
            condition: condition.into(),
            max_iterations: max_iter,
        },
        next: vec![body.into()],
        branches: None,
        input_mapping: HashMap::new(),
        output_var: None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("missing node: {0}")]
    MissingNode(String),
    #[error("cycle detected in workflow DAG")]
    CycleDetected,
    #[error("invalid start node count: expected 1, found {0}")]
    InvalidStartNode(usize),
    #[error("node not found: {0}")]
    NodeNotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_linear_workflow() {
        let def = WorkflowBuilder::new("w1", "Test Workflow")
            .add_node(start_node("start", "tool1"))
            .add_node(tool_node("tool1", "read_file", "end"))
            .add_node(end_node("end"))
            .build();

        let executor = WorkflowExecutor::new(def);
        executor.validate().unwrap();

        let start = executor.start_node().unwrap();
        assert_eq!(start.id, "start");
    }

    #[test]
    fn test_conditional_workflow() {
        let def = WorkflowBuilder::new("w1", "Conditional")
            .add_node(start_node("start", "check"))
            .add_node(condition_node("check", "success", "end_ok", "end_fail"))
            .add_node(tool_node("end_ok", "report_success", "end"))
            .add_node(tool_node("end_fail", "report_failure", "end"))
            .add_node(end_node("end"))
            .build();

        let executor = WorkflowExecutor::new(def);
        executor.validate().unwrap();
    }

    #[test]
    fn test_cycle_detection() {
        let def = WorkflowDefinition {
            id: "w1".into(),
            name: "Cyclic".into(),
            description: "".into(),
            nodes: vec![
                WorkflowNode {
                    id: "a".into(),
                    name: "A".into(),
                    node_type: NodeType::Start,
                    next: vec!["b".into()],
                    branches: None,
                    input_mapping: HashMap::new(),
                    output_var: None,
                },
                WorkflowNode {
                    id: "b".into(),
                    name: "B".into(),
                    node_type: NodeType::Tool {
                        tool_name: "test".into(),
                    },
                    next: vec!["a".into()], // cycle!
                    branches: None,
                    input_mapping: HashMap::new(),
                    output_var: None,
                },
            ],
            variables: HashMap::new(),
        };

        let executor = WorkflowExecutor::new(def);
        assert!(matches!(
            executor.validate(),
            Err(WorkflowError::CycleDetected)
        ));
    }

    #[test]
    fn test_missing_node() {
        let def = WorkflowDefinition {
            id: "w1".into(),
            name: "Bad".into(),
            description: "".into(),
            nodes: vec![WorkflowNode {
                id: "a".into(),
                name: "A".into(),
                node_type: NodeType::Start,
                next: vec!["nonexistent".into()],
                branches: None,
                input_mapping: HashMap::new(),
                output_var: None,
            }],
            variables: HashMap::new(),
        };

        let executor = WorkflowExecutor::new(def);
        assert!(matches!(
            executor.validate(),
            Err(WorkflowError::MissingNode(_))
        ));
    }

    #[test]
    fn test_ready_nodes() {
        let def = WorkflowBuilder::new("w1", "Test")
            .add_node(start_node("start", "tool1"))
            .add_node(tool_node("tool1", "read", "end"))
            .add_node(end_node("end"))
            .build();

        let mut executor = WorkflowExecutor::new(def);
        executor.validate().unwrap();

        // Initially, only start is ready (no predecessors)
        let ready = executor.ready_nodes();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "start");

        // Complete start
        executor.complete_node("start", None).unwrap();

        // Now tool1 is ready
        let ready = executor.ready_nodes();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "tool1");
    }

    #[test]
    fn test_workflow_completion() {
        let def = WorkflowBuilder::new("w1", "Simple")
            .add_node(start_node("start", "end"))
            .add_node(end_node("end"))
            .build();

        let mut executor = WorkflowExecutor::new(def);
        executor.validate().unwrap();

        executor.complete_node("start", None).unwrap();
        assert_eq!(executor.state().status, WorkflowStatus::Running);

        executor.complete_node("end", None).unwrap();
        assert_eq!(executor.state().status, WorkflowStatus::Completed);
    }

    #[test]
    fn test_variable_output() {
        let def = WorkflowBuilder::new("w1", "Vars")
            .add_node(start_node("start", "tool1"))
            .add_node({
                let mut n = tool_node("tool1", "read", "end");
                n.output_var = Some("file_content".into());
                n
            })
            .add_node(end_node("end"))
            .build();

        let mut executor = WorkflowExecutor::new(def);
        executor.validate().unwrap();

        executor.complete_node("start", None).unwrap();
        executor
            .complete_node(
                "tool1",
                Some(serde_json::json!("hello world")),
            )
            .unwrap();

        assert_eq!(
            executor.state().variables.get("file_content"),
            Some(&serde_json::json!("hello world"))
        );
    }

    #[test]
    fn test_node_helpers() {
        let start = start_node("s", "next");
        assert!(matches!(start.node_type, NodeType::Start));

        let end = end_node("e");
        assert!(matches!(end.node_type, NodeType::End));

        let tool = tool_node("t", "bash", "next");
        assert!(matches!(tool.node_type, NodeType::Tool { .. }));

        let cond = condition_node("c", "true", "t", "f");
        assert!(cond.branches.is_some());

        let llm = llm_node("l", "prompt", "next");
        assert!(matches!(llm.node_type, NodeType::Llm { .. }));

        let agent = agent_node("a", "agent-1", "next");
        assert!(matches!(agent.node_type, NodeType::Agent { .. }));

        let par = parallel_node("p", vec!["a", "b"]);
        assert_eq!(par.next.len(), 2);

        let lp = loop_node("lp", "cond", "body", 10);
        assert!(matches!(lp.node_type, NodeType::Loop { .. }));
    }
}
