use crate::workflow::{WorkflowExecution, WorkflowNode, ExecStatus, ExecResult, Checkpoint};
use crate::event_bus::{EventBus, Event};
use crate::checkpoint::CheckpointManager;
use crate::hooks::HookRunner;
use crate::skill_registry::SkillRegistry;
use maple_llm::router::LlmRouter;
use maple_llm::request::LlmRequest;
use std::sync::Arc;
use std::collections::HashMap;
use serde_json::Value;
use anyhow::Result;
use petgraph::graph::DiGraph;
use petgraph::algo::toposort;
use tokio::sync::watch;
use dashmap::DashMap;

type ApprovalMap = Arc<DashMap<String, watch::Sender<bool>>>;

/// Async callback for executing an agent task.
/// Takes (agent_id, goal) and returns the agent's response as a String.
pub type AgentHandler = Arc<
    dyn Fn(String, String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send>>
        + Send
        + Sync,
>;

pub struct NodeExecutor {
    llm_router: Arc<LlmRouter>,
    skill_registry: Arc<SkillRegistry>,
    hook_runner: Arc<HookRunner>,
    workflow_nodes: Vec<WorkflowNode>,
    approval_channels: ApprovalMap,
    agent_handler: Option<AgentHandler>,
}

impl Clone for NodeExecutor {
    fn clone(&self) -> Self {
        Self {
            llm_router: self.llm_router.clone(),
            skill_registry: self.skill_registry.clone(),
            hook_runner: self.hook_runner.clone(),
            workflow_nodes: self.workflow_nodes.clone(),
            approval_channels: self.approval_channels.clone(),
            agent_handler: self.agent_handler.clone(),
        }
    }
}

impl NodeExecutor {
    pub fn new(
        llm_router: Arc<LlmRouter>,
        skill_registry: Arc<SkillRegistry>,
        hook_runner: Arc<HookRunner>,
    ) -> Self {
        Self {
            llm_router,
            skill_registry,
            hook_runner,
            workflow_nodes: Vec::new(),
            approval_channels: Arc::new(DashMap::new()),
            agent_handler: None,
        }
    }

    pub fn set_workflow_nodes(&mut self, nodes: Vec<WorkflowNode>) {
        self.workflow_nodes = nodes;
    }

    pub fn with_agent_handler(mut self, handler: AgentHandler) -> Self {
        self.agent_handler = Some(handler);
        self
    }

    fn find_node(&self, node_id: &str) -> Option<&WorkflowNode> {
        self.workflow_nodes.iter().find(|n| n.id == node_id)
    }

    pub fn execute<'a>(
        &'a self,
        node: &'a WorkflowNode,
        ctx: &'a mut WorkflowExecution,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send + 'a>> {
        Box::pin(async move {
            if let Some(cond) = &node.condition
                && !eval_condition(cond, &ctx.context)
            {
                return Ok(Value::Null);
            }

            let result = match &node.node_type {
                crate::workflow::NodeType::Llm { model_route, prompt_ref, temperature } => {
                    self.execute_llm(model_route, prompt_ref, *temperature, &ctx.context).await?
                }
                crate::workflow::NodeType::Tool { skill_id, config } => {
                    self.execute_tool(skill_id, config, &ctx.context).await?
                }
                crate::workflow::NodeType::Condition { expression: _, branches } => {
                    self.execute_condition(branches, &ctx.context)?
                }
                crate::workflow::NodeType::Parallel { nodes, wait_strategy } => {
                    self.execute_parallel(nodes, wait_strategy, ctx).await?
                }
                crate::workflow::NodeType::Loop { each, condition, body, max_iterations } => {
                    self.execute_loop(each.as_deref(), condition.as_deref(), body, *max_iterations, ctx).await?
                }
                crate::workflow::NodeType::HumanApproval { notify, timeout_secs, on_timeout } => {
                    self.wait_for_approval(notify, *timeout_secs, on_timeout, ctx).await?
                }
                crate::workflow::NodeType::SubWorkflow { workflow_id, input_mapping } => {
                    self.execute_sub_workflow(workflow_id, input_mapping, ctx).await?
                }
                crate::workflow::NodeType::Webhook { url, method, headers, body_template } => {
                    self.execute_webhook(url, method, headers, body_template.as_deref(), &ctx.context).await?
                }
                crate::workflow::NodeType::Delay { duration_secs } => {
                    tokio::time::sleep(std::time::Duration::from_secs(*duration_secs)).await;
                    Value::String("delay_completed".to_string())
                }
                crate::workflow::NodeType::Agent { agent_id, goal, timeout_secs } => {
                    self.execute_agent(agent_id, goal, *timeout_secs).await?
                }
            };

            ctx.checkpoints.push(Checkpoint {
                node_id: node.id.clone(),
                output: result.clone(),
                timestamp: chrono::Utc::now(),
            });

            Ok(result)
        })
    }

    async fn execute_llm(
        &self,
        model_route: &str,
        prompt_ref: &str,
        temperature: Option<f32>,
        context: &HashMap<String, Value>,
    ) -> Result<Value> {
        let prompt = resolve_template(prompt_ref, context);
        let request = LlmRequest::new(prompt, model_route)
            .with_temperature(temperature.unwrap_or(0.7));

        let adapter = self.llm_router.route(&request).await?;
        let response = adapter.complete(request).await?;
        Ok(Value::String(response.text()))
    }

    async fn execute_tool(
        &self,
        skill_id: &str,
        config: &Value,
        context: &HashMap<String, Value>,
    ) -> Result<Value> {
        let resolved_config = resolve_value(config, context);
        let result = self.skill_registry.execute(skill_id, &resolved_config).await?;
        Ok(result)
    }

    async fn execute_agent(
        &self,
        agent_id: &str,
        goal: &str,
        timeout_secs: Option<u64>,
    ) -> Result<Value> {
        if let Some(handler) = &self.agent_handler {
            let timeout = timeout_secs.unwrap_or(120);
            let agent_id_owned = agent_id.to_string();
            let goal = goal.to_string();
            match tokio::time::timeout(
                std::time::Duration::from_secs(timeout),
                handler(agent_id_owned, goal),
            ).await {
                Ok(result) => result.map(Value::String),
                Err(_) => anyhow::bail!("Agent {} timed out after {}s", agent_id, timeout),
            }
        } else {
            anyhow::bail!("No agent handler registered; cannot execute agent node '{}'", agent_id)
        }
    }

    fn execute_condition(
        &self,
        branches: &[crate::workflow::Branch],
        context: &HashMap<String, Value>,
    ) -> Result<Value> {
        for branch in branches {
            if let Some(expr) = &branch.expression
                && eval_condition(expr, context)
            {
                return Ok(Value::String(branch.target_node.clone()));
            }
        }
        Ok(Value::String("default".to_string()))
    }

    async fn execute_parallel(
        &self,
        node_ids: &[String],
        wait_strategy: &crate::workflow::WaitStrategy,
        ctx: &mut WorkflowExecution,
    ) -> Result<Value> {
        let nodes_to_execute: Vec<WorkflowNode> = node_ids.iter()
            .filter(|id| !ctx.context.contains_key(*id))
            .filter_map(|id| self.find_node(id).cloned())
            .collect();

        let context_snapshot: HashMap<String, Value> = ctx.context.clone();
        let mut handles = Vec::new();

        for node in &nodes_to_execute {
            let node_id = node.id.clone();
            let mut sub_exec = WorkflowExecution::new(
                &node_id,
                0,
                Value::Object(context_snapshot.clone().into_iter().collect()),
            );
            sub_exec.set_running();

            let executor = self.clone();
            let node_clone = node.clone();

            handles.push(tokio::spawn(async move {
                let result = executor.execute(&node_clone, &mut sub_exec).await;
                (node_id, result, sub_exec)
            }));
        }

        let mut results: serde_json::Map<String, Value> = serde_json::Map::new();
        let mut completed_count = 0;
        let total = nodes_to_execute.len();
        let mut first_completed_id = String::new();
        let mut first_completed_result = Value::Null;

        match wait_strategy {
            crate::workflow::WaitStrategy::WaitAll => {
                for handle in handles {
                    match handle.await {
                        Ok((node_id, Ok(result), _)) => {
                            results.insert(node_id.clone(), result.clone());
                            ctx.context.insert(node_id, result);
                            completed_count += 1;
                        }
                        Ok((node_id, Err(e), _)) => {
                            tracing::warn!(node_id, error = %e, "Parallel node failed");
                            results.insert(node_id, Value::Null);
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Parallel task join error");
                        }
                    }
                }
            }
            crate::workflow::WaitStrategy::WaitAny => {
                let all_results = futures::future::join_all(handles).await;
                for result in all_results {
                    match result {
                        Ok((node_id, Ok(val), _)) => {
                            if first_completed_id.is_empty() {
                                first_completed_id = node_id.clone();
                                first_completed_result = val.clone();
                            }
                            results.insert(node_id.clone(), val.clone());
                            ctx.context.insert(node_id, val);
                            completed_count += 1;
                        }
                        Ok((node_id, Err(e), _)) => {
                            tracing::warn!(node_id, error = %e, "Parallel node failed");
                            results.insert(node_id, Value::Null);
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Parallel task join error");
                        }
                    }
                }
            }
        }

        for node_id in node_ids {
            if !results.contains_key(node_id) {
                results.insert(node_id.clone(), Value::Null);
            }
        }

        match wait_strategy {
            crate::workflow::WaitStrategy::WaitAll => {
                Ok(Value::Object(results))
            }
            crate::workflow::WaitStrategy::WaitAny => {
                Ok(serde_json::json!({
                    "first_completed": first_completed_id,
                    "result": first_completed_result,
                    "all": Value::Object(results),
                    "completed": completed_count,
                    "total": total,
                }))
            }
        }
    }

    async fn execute_loop(
        &self,
        each: Option<&str>,
        condition: Option<&str>,
        body: &str,
        max_iterations: usize,
        ctx: &mut WorkflowExecution,
    ) -> Result<Value> {
        let mut results = Vec::new();
        let body_node = self.find_node(body).cloned();

        if let Some(each_path) = each {
            let items = ctx.context.get(each_path)
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            for (i, item) in items.iter().enumerate() {
                if i >= max_iterations {
                    break;
                }
                ctx.context.insert("loop_item".to_string(), item.clone());
                ctx.context.insert("loop_index".to_string(), Value::Number(i.into()));

                if let Some(ref node) = body_node {
                    let iter_result = self.execute(node, ctx).await.unwrap_or_else(|_| {
                        serde_json::json!({ "iteration": i, "status": "failed" })
                    });
                    ctx.context.insert(format!("{}_{}", body, i), iter_result.clone());
                    results.push(iter_result);
                } else {
                    let iter_result = serde_json::json!({
                        "iteration": i,
                        "body_node": body,
                        "status": "referenced_node_not_found"
                    });
                    ctx.context.insert(format!("{}_{}", body, i), iter_result.clone());
                    results.push(iter_result);
                }
            }
        } else if let Some(cond) = condition {
            let mut iteration = 0;
            while iteration < max_iterations && eval_condition(cond, &ctx.context) {
                if let Some(ref node) = body_node {
                    let iter_result = self.execute(node, ctx).await.unwrap_or_else(|_| {
                        serde_json::json!({ "iteration": iteration, "status": "failed" })
                    });
                    ctx.context.insert(format!("{}_{}", body, iteration), iter_result.clone());
                    results.push(iter_result);
                } else {
                    let iter_result = serde_json::json!({
                        "iteration": iteration,
                        "body_node": body,
                        "status": "referenced_node_not_found"
                    });
                    results.push(iter_result);
                }
                iteration += 1;
            }
        }

        Ok(Value::Array(results))
    }

    async fn wait_for_approval(
        &self,
        notify: &[crate::workflow::NotifyChannel],
        timeout_secs: u64,
        on_timeout: &crate::workflow::TimeoutAction,
        ctx: &mut WorkflowExecution,
    ) -> Result<Value> {
        let approval_id = format!("approval_{}", uuid::Uuid::new_v4());
        let (tx, mut rx) = watch::channel(false);

        self.approval_channels.insert(approval_id.clone(), tx);

        for channel in notify {
            tracing::info!(
                channel_type = %channel.channel_type,
                url = %channel.url,
                approval_id = %approval_id,
                "Sending approval request notification"
            );
            let client = reqwest::Client::new();
            let msg = channel.message_template.as_deref().unwrap_or("Approval required for workflow node");
            let _ = client
                .post(&channel.url)
                .json(&serde_json::json!({
                    "message": msg,
                    "approval_id": approval_id,
                    "timeout_secs": timeout_secs,
                    "context": serde_json::Value::Object(ctx.context.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                }))
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await;
        }

        ctx.context.insert("approval_id".to_string(), Value::String(approval_id.clone()));

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
        let approved = tokio::select! {
            change = rx.changed() => {
                match change {
                    Ok(()) => *rx.borrow(),
                    Err(_) => false,
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                tracing::info!(timeout_secs, "Approval timed out");
                match on_timeout {
                    crate::workflow::TimeoutAction::AutoApprove => true,
                    crate::workflow::TimeoutAction::AutoReject => false,
                    crate::workflow::TimeoutAction::FailWorkflow => false,
                }
            }
        };

        self.approval_channels.remove(&approval_id);

        if !approved && matches!(on_timeout, crate::workflow::TimeoutAction::FailWorkflow) {
            anyhow::bail!("Approval was rejected or timed out with FailWorkflow policy");
        }

        Ok(Value::Bool(approved))
    }

    pub fn resolve_approval(&self, approval_id: &str, approved: bool) -> bool {
        if let Some(entry) = self.approval_channels.get(approval_id) {
            let _ = entry.send(approved);
            true
        } else {
            false
        }
    }

    async fn execute_sub_workflow(
        &self,
        workflow_id: &str,
        input_mapping: &HashMap<String, String>,
        ctx: &mut WorkflowExecution,
    ) -> Result<Value> {
        let mut sub_input = serde_json::Map::new();
        for (local_key, remote_key) in input_mapping {
            if let Some(val) = ctx.context.get(local_key) {
                sub_input.insert(remote_key.clone(), val.clone());
            } else {
                sub_input.insert(remote_key.clone(), Value::String(resolve_template(local_key, &ctx.context)));
            }
        }

        let prefix = format!("{}::", workflow_id);
        let sub_nodes: Vec<WorkflowNode> = self.workflow_nodes.iter()
            .filter(|n| n.id.starts_with(&prefix) || n.id == workflow_id)
            .cloned()
            .collect();

        if sub_nodes.is_empty() {
            tracing::warn!(workflow_id, "SubWorkflow has no matching nodes, returning mapped input");
            return Ok(Value::Object(sub_input));
        }

        let mut sub_exec = WorkflowExecution::new(
            workflow_id,
            0,
            Value::Object(sub_input.clone()),
        );
        sub_exec.set_running();

        let topo = build_topological_order(&sub_nodes)?;
        for node_id in topo {
            if let Some(node) = self.find_node(&node_id) {
                match self.execute(node, &mut sub_exec).await {
                    Ok(result) => {
                        sub_exec.context.insert(node_id, result);
                    }
                    Err(e) => {
                        tracing::warn!(node_id, error = %e, "SubWorkflow node failed");
                        sub_exec.set_failed(&e.to_string());
                        break;
                    }
                }
            }
        }

        let output = sub_exec
            .context
            .get("output")
            .cloned()
            .unwrap_or(Value::Object(sub_input));

        ctx.context.insert(format!("sub_workflow_{}_result", workflow_id), output.clone());

        if sub_exec.status == ExecStatus::Failed {
            anyhow::bail!("SubWorkflow {} failed: {}", workflow_id, sub_exec.error.unwrap_or_default());
        }

        Ok(output)
    }

    async fn execute_webhook(
        &self,
        url: &str,
        method: &crate::workflow::HttpMethod,
        headers: &HashMap<String, String>,
        body_template: Option<&str>,
        context: &HashMap<String, Value>,
    ) -> Result<Value> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()?;
        let resolved_url = resolve_template(url, context);
        let resolved_body = body_template.map(|t| resolve_template(t, context));

        let method_str = match method {
            crate::workflow::HttpMethod::GET => "GET",
            crate::workflow::HttpMethod::POST => "POST",
            crate::workflow::HttpMethod::PUT => "PUT",
            crate::workflow::HttpMethod::DELETE => "DELETE",
            crate::workflow::HttpMethod::PATCH => "PATCH",
        };

        let mut req = client.request(
            reqwest::Method::from_bytes(method_str.as_bytes())?,
            &resolved_url,
        );

        for (k, v) in headers {
            req = req.header(k, resolve_template(v, context));
        }

        if let Some(body) = resolved_body {
            req = req.body(body);
        }

        let resp = req.send().await?;
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();

        Ok(serde_json::json!({
            "status": status,
            "body": body,
        }))
    }
}

pub struct WorkflowExecutor {
    event_bus: Arc<EventBus>,
    node_executor: Arc<tokio::sync::RwLock<NodeExecutor>>,
    checkpoint_mgr: Arc<CheckpointManager>,
    hook_runner: Arc<HookRunner>,
}

impl WorkflowExecutor {
    pub fn new(
        event_bus: Arc<EventBus>,
        node_executor: NodeExecutor,
        checkpoint_mgr: Arc<CheckpointManager>,
        hook_runner: Arc<HookRunner>,
    ) -> Self {
        Self {
            event_bus,
            node_executor: Arc::new(tokio::sync::RwLock::new(node_executor)),
            checkpoint_mgr,
            hook_runner,
        }
    }

    pub async fn execute(
        &self,
        workflow_nodes: &[WorkflowNode],
        workflow_id: &str,
        workflow_version: u32,
        input: Value,
    ) -> Result<ExecResult> {
        self.execute_with_group(workflow_nodes, workflow_id, workflow_version, input, None).await
    }

    /// Execute workflow with optional group chat integration.
    /// When `group_id` is set, workflow lifecycle events are published as group messages.
    pub async fn execute_with_group(
        &self,
        workflow_nodes: &[WorkflowNode],
        workflow_id: &str,
        workflow_version: u32,
        input: Value,
        group_id: Option<String>,
    ) -> Result<ExecResult> {
        {
            let mut ne = self.node_executor.write().await;
            ne.set_workflow_nodes(workflow_nodes.to_vec());
        }

        let mut exec = WorkflowExecution::new(workflow_id, workflow_version, input);
        exec.group_id = group_id.clone();
        exec.set_running();

        self.event_bus.publish(Event::WorkflowStarted {
            workflow_id: workflow_id.to_string(),
            exec_id: exec.exec_id,
        }).await;

        if let Some(ref gid) = group_id {
            self.publish_group_msg(gid, "system", &serde_json::json!({
                "type": "workflow_run",
                "workflow_id": workflow_id,
                "exec_id": exec.exec_id.to_string(),
                "status": "started"
            }).to_string()).await;
        }

        let topo = build_topological_order(workflow_nodes)?;

        for node_id in topo {
            let node = match workflow_nodes.iter().find(|n| n.id == node_id) {
                Some(n) => n.clone(),
                None => {
                    exec.set_failed(&format!("Node not found: {}", node_id));
                    break;
                }
            };

            self.event_bus.publish(Event::NodeStarted {
                workflow_id: workflow_id.to_string(),
                exec_id: exec.exec_id,
                node_id: node_id.clone(),
            }).await;

            if let Some(ref gid) = group_id {
                let node_type_str = match &node.node_type {
                    crate::workflow::NodeType::Llm { .. } => "llm",
                    crate::workflow::NodeType::Tool { .. } => "tool",
                    crate::workflow::NodeType::Condition { .. } => "condition",
                    crate::workflow::NodeType::Parallel { .. } => "parallel",
                    crate::workflow::NodeType::Loop { .. } => "loop",
                    crate::workflow::NodeType::HumanApproval { .. } => "human_approval",
                    crate::workflow::NodeType::SubWorkflow { .. } => "sub_workflow",
                    crate::workflow::NodeType::Webhook { .. } => "webhook",
                    crate::workflow::NodeType::Delay { .. } => "delay",
                    crate::workflow::NodeType::Agent { .. } => "agent",
                };
                self.publish_group_msg(gid, "system", &serde_json::json!({
                    "type": "workflow_step",
                    "workflow_id": workflow_id,
                    "exec_id": exec.exec_id.to_string(),
                    "node_id": node_id,
                    "node_type": node_type_str,
                    "status": "running"
                }).to_string()).await;
            }

            let node_executor = self.node_executor.read().await;
            match self.execute_with_retry(&node_executor, &node, &mut exec).await {
                Ok(result) => {
                    exec.context.insert(node_id.clone(), result);

                    let _ = self.hook_runner.run_post_tool_use(&node.id, &exec.context.get(&node_id).cloned().unwrap_or(Value::Null)).await;

                    self.checkpoint_mgr.save(&exec, &node_id).await?;

                    self.event_bus.publish(Event::NodeCompleted {
                        workflow_id: workflow_id.to_string(),
                        exec_id: exec.exec_id,
                        node_id: node_id.clone(),
                    }).await;

                    if let Some(ref gid) = group_id {
                        self.publish_group_msg(gid, "system", &serde_json::json!({
                            "type": "workflow_step",
                            "workflow_id": workflow_id,
                            "exec_id": exec.exec_id.to_string(),
                            "node_id": node_id,
                            "status": "completed"
                        }).to_string()).await;
                    }
                }
                Err(e) => {
                    exec.set_failed(&e.to_string());

                    self.event_bus.publish(Event::NodeFailed {
                        workflow_id: workflow_id.to_string(),
                        exec_id: exec.exec_id,
                        node_id: node_id.clone(),
                        error: e.to_string(),
                    }).await;

                    if let Some(ref gid) = group_id {
                        self.publish_group_msg(gid, "system", &serde_json::json!({
                            "type": "workflow_failed",
                            "workflow_id": workflow_id,
                            "exec_id": exec.exec_id.to_string(),
                            "node_id": node_id,
                            "error": e.to_string()
                        }).to_string()).await;
                    }
                    break;
                }
            }
        }

        if exec.status == ExecStatus::Running {
            exec.set_completed();
            self.event_bus.publish(Event::WorkflowCompleted {
                workflow_id: workflow_id.to_string(),
                exec_id: exec.exec_id,
            }).await;

            if let Some(ref gid) = group_id {
                self.publish_group_msg(gid, "system", &serde_json::json!({
                    "type": "workflow_complete",
                    "workflow_id": workflow_id,
                    "exec_id": exec.exec_id.to_string(),
                    "status": "completed"
                }).to_string()).await;
            }
        } else {
            self.event_bus.publish(Event::WorkflowFailed {
                workflow_id: workflow_id.to_string(),
                exec_id: exec.exec_id,
                error: exec.error.clone().unwrap_or_default(),
            }).await;
        }

        Ok(ExecResult::from_execution(&exec))
    }

    async fn publish_group_msg(&self, group_id: &str, sender_id: &str, content: &str) {
        let _ = self.event_bus.publish(Event::GroupMessageSent {
            group_id: group_id.to_string(),
            message_id: uuid::Uuid::new_v4().to_string(),
            sender_id: sender_id.to_string(),
            content: content.to_string(),
        }).await;
    }

    pub async fn resolve_approval(&self, approval_id: &str, approved: bool) -> bool {
        let ne = self.node_executor.read().await;
        ne.resolve_approval(approval_id, approved)
    }

    async fn execute_with_retry(
        &self,
        node_executor: &NodeExecutor,
        node: &WorkflowNode,
        exec: &mut WorkflowExecution,
    ) -> Result<Value> {
        let max_attempts = node.retry.max_attempts;
        let mut attempt = 0;

        loop {
            attempt += 1;
            match node_executor.execute(node, exec).await {
                Ok(v) => return Ok(v),
                Err(e) if attempt < max_attempts => {
                    let delay = node.retry.backoff.delay(attempt);
                    tracing::warn!(attempt, max_attempts, error = %e, "Node execution failed, retrying");
                    tokio::time::sleep(delay).await;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

fn build_topological_order(nodes: &[WorkflowNode]) -> Result<Vec<String>> {
    let mut graph = DiGraph::<String, ()>::new();
    let mut node_index = HashMap::new();

    for node in nodes {
        let idx = graph.add_node(node.id.clone());
        node_index.insert(node.id.clone(), idx);
    }

    for node in nodes {
        if let Some(&src) = node_index.get(&node.id) {
            for dep in &node.depends_on {
                if let Some(&dst) = node_index.get(dep) {
                    graph.add_edge(dst, src, ());
                }
            }
        }
    }

    let sorted = toposort(&graph, None)
        .map_err(|e| anyhow::anyhow!("Cycle detected in workflow DAG: {:?}", e))?;

    Ok(sorted.iter().map(|idx| graph[*idx].clone()).collect())
}

fn eval_condition(expression: &str, context: &HashMap<String, Value>) -> bool {
    let expr = expression.trim();

    if expr.starts_with("{{") && expr.ends_with("}}") {
        let path = &expr[2..expr.len()-2].trim();
        return context.get(path as &str).is_some_and(|v| !v.is_null());
    }

    if let Some(idx) = expr.find("&&") {
        let left = &expr[..idx];
        let right = &expr[idx + 2..];
        return eval_condition(left, context) && eval_condition(right, context);
    }

    if let Some(idx) = expr.find("||") {
        let left = &expr[..idx];
        let right = &expr[idx + 2..];
        return eval_condition(left, context) || eval_condition(right, context);
    }

    if expr.starts_with('(') && expr.ends_with(')') {
        return eval_condition(&expr[1..expr.len()-1], context);
    }

    if let Some(idx) = expr.find(">=") {
        let left = resolve_template(expr[..idx].trim(), context);
        let right = resolve_template(expr[idx + 2..].trim(), context);
        if let (Ok(l), Ok(r)) = (left.parse::<f64>(), right.parse::<f64>()) {
            return l >= r;
        }
        return left >= right;
    }

    if let Some(idx) = expr.find("<=") {
        let left = resolve_template(expr[..idx].trim(), context);
        let right = resolve_template(expr[idx + 2..].trim(), context);
        if let (Ok(l), Ok(r)) = (left.parse::<f64>(), right.parse::<f64>()) {
            return l <= r;
        }
        return left <= right;
    }

    if let Some(idx) = expr.find("!=") {
        let left = resolve_template(expr[..idx].trim(), context);
        let right = resolve_template(expr[idx + 2..].trim(), context);
        return left != right;
    }

    if let Some(idx) = expr.find("==") {
        let left = resolve_template(expr[..idx].trim(), context);
        let right = resolve_template(expr[idx + 2..].trim(), context);
        return left == right;
    }

    if let Some(idx) = expr.find('>') {
        let left = resolve_template(expr[..idx].trim(), context);
        let right = resolve_template(expr[idx + 1..].trim(), context);
        if let (Ok(l), Ok(r)) = (left.parse::<f64>(), right.parse::<f64>()) {
            return l > r;
        }
        return left > right;
    }

    if let Some(idx) = expr.find('<') {
        let left = resolve_template(expr[..idx].trim(), context);
        let right = resolve_template(expr[idx + 1..].trim(), context);
        if let (Ok(l), Ok(r)) = (left.parse::<f64>(), right.parse::<f64>()) {
            return l < r;
        }
        return left < right;
    }

    let resolved = resolve_template(expr, context);
    match resolved.as_str() {
        "true" | "1" | "yes" => true,
        "false" | "0" | "no" | "" => false,
        _ => context.get(expr).is_some_and(|v| !v.is_null()),
    }
}

fn resolve_template(template: &str, context: &HashMap<String, Value>) -> String {
    let mut result = template.to_string();
    for (key, val) in context {
        let placeholder = format!("{{{{{}}}}}", key);
        if result.contains(&placeholder) {
            result = result.replace(&placeholder, &val.to_string());
        }
    }
    result
}

fn resolve_value(value: &Value, context: &HashMap<String, Value>) -> Value {
    match value {
        Value::String(s) => {
            Value::String(resolve_template(s, context))
        }
        Value::Object(map) => {
            let resolved: serde_json::Map<String, Value> = map
                .into_iter()
                .map(|(k, v)| (k.clone(), resolve_value(v, context)))
                .collect();
            Value::Object(resolved)
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(|v| resolve_value(v, context)).collect())
        }
        other => other.clone(),
    }
}
