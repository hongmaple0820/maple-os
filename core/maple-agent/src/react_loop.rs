use crate::context_compressor::{ContextCompressor, ContextCompressorConfig};
use crate::performance::{PerformanceMonitor, ToolResultCache};
use crate::security::{PermissionCheck, SecurityManager};
use crate::streaming_executor::StreamingToolExecutor;
use crate::tool_use_context::ToolUseContext;
use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use maple_engine::hooks::{HookDecision, HookRunner};
use maple_llm::request::{LlmRequest, Message, ToolDefinition};
use maple_llm::router::LlmAdapter;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: Value,
}

pub struct ToolResult {
    pub tool_use_id: String,
    pub tool_name: String,
    pub output: Value,
    pub is_error: bool,
}

impl ToolResult {
    pub fn error(tool_use_id: &str, tool_name: &str, reason: &str) -> Self {
        Self {
            tool_use_id: tool_use_id.to_string(),
            tool_name: tool_name.to_string(),
            output: Value::String(reason.to_string()),
            is_error: true,
        }
    }

    pub fn success(tool_use_id: &str, tool_name: &str, output: Value) -> Self {
        Self {
            tool_use_id: tool_use_id.to_string(),
            tool_name: tool_name.to_string(),
            output,
            is_error: false,
        }
    }
}

pub struct AssistantMessage {
    pub content: String,
    pub tool_uses: Vec<ToolUse>,
}

pub struct TurnSummary {
    pub completed: bool,
    pub content: String,
    pub iterations: usize,
}

impl TurnSummary {
    pub fn completed(msg: &AssistantMessage) -> Self {
        Self {
            completed: true,
            content: msg.content.clone(),
            iterations: 0,
        }
    }

    pub fn max_iterations_reached() -> Self {
        Self {
            completed: false,
            content: "Max iterations reached".to_string(),
            iterations: 0,
        }
    }
}

pub struct Session {
    pub messages: Vec<Message>,
    pub input_token_count: usize,
}

impl Session {
    pub fn new(system_prompt: &str) -> Self {
        Self {
            messages: vec![Message::system(system_prompt)],
            input_token_count: 0,
        }
    }

    pub fn push_message(&mut self, msg: Message) {
        self.input_token_count +=
            maple_llm::token_counter::count_message_tokens(&msg.content, &msg.role);
        self.messages.push(msg);
    }

    pub fn input_tokens(&self) -> usize {
        self.input_token_count
    }

    pub fn build_messages_for_request(&self) -> Vec<Message> {
        self.messages.clone()
    }
}

/// Streaming events emitted during a turn — callers receive these via channel
#[derive(Debug, Clone)]
pub enum TurnEvent {
    /// LLM started generating a response
    LlmStart,
    /// Incremental text delta from LLM streaming
    TextDelta { delta: String },
    /// LLM finished generating, tool calls detected
    LlmComplete { content: String, tool_count: usize },
    /// A tool is about to execute
    ToolStart {
        tool_name: String,
        tool_use_id: String,
    },
    /// A tool finished executing
    ToolComplete {
        tool_name: String,
        tool_use_id: String,
        is_error: bool,
    },
    /// Context was compressed
    ContextCompressed {
        before_messages: usize,
        after_messages: usize,
    },
    /// Turn finished
    TurnComplete { content: String, iterations: usize },
}

pub type EventSender = tokio::sync::mpsc::Sender<TurnEvent>;

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, tool_use: &ToolUse) -> Result<ToolResult>;
}

pub struct ReactLoop {
    max_iterations: usize,
    max_concurrent_tools: usize,
    hook_runner: HookRunner,
    context_compressor: ContextCompressor,
    tool_use_context: Option<ToolUseContext>,
    security_manager: Option<Arc<RwLock<SecurityManager>>>,
    streaming_executor: Option<StreamingToolExecutor>,
    performance_monitor: Option<PerformanceMonitor>,
    tool_result_cache: Option<Arc<RwLock<ToolResultCache>>>,
    event_sender: Option<EventSender>,
}

impl ReactLoop {
    pub fn new(max_iterations: usize) -> Self {
        Self {
            max_iterations,
            max_concurrent_tools: 4,
            hook_runner: HookRunner::new(),
            context_compressor: ContextCompressor::new(ContextCompressorConfig::default()),
            tool_use_context: None,
            security_manager: None,
            streaming_executor: None,
            performance_monitor: None,
            tool_result_cache: None,
            event_sender: None,
        }
    }

    pub fn with_max_concurrent_tools(mut self, n: usize) -> Self {
        self.max_concurrent_tools = n;
        self
    }

    pub fn with_hook_runner(mut self, runner: HookRunner) -> Self {
        self.hook_runner = runner;
        self
    }

    pub fn with_context_compressor(mut self, compressor: ContextCompressor) -> Self {
        self.context_compressor = compressor;
        self
    }

    pub fn with_tool_use_context(mut self, ctx: ToolUseContext) -> Self {
        self.tool_use_context = Some(ctx);
        self
    }

    pub fn with_security_manager(mut self, manager: SecurityManager) -> Self {
        self.security_manager = Some(Arc::new(RwLock::new(manager)));
        self
    }

    pub fn with_streaming_executor(mut self, executor: StreamingToolExecutor) -> Self {
        self.streaming_executor = Some(executor);
        self
    }

    pub fn with_performance_monitor(mut self, monitor: PerformanceMonitor) -> Self {
        self.performance_monitor = Some(monitor);
        self
    }

    pub fn with_tool_result_cache(mut self, cache: ToolResultCache) -> Self {
        self.tool_result_cache = Some(Arc::new(RwLock::new(cache)));
        self
    }

    /// Set an event sender for streaming turn events to the caller
    pub fn with_event_sender(mut self, sender: EventSender) -> Self {
        self.event_sender = Some(sender);
        self
    }

    async fn emit_event(&self, event: TurnEvent) {
        if let Some(ref sender) = self.event_sender {
            let _ = sender.send(event).await;
        }
    }

    /// Check if a tool is allowed by ToolUseContext and SecurityManager
    async fn check_tool_permission(&self, tool_use: &ToolUse) -> Option<String> {
        // Check ToolUseContext permission
        if let Some(ref ctx) = self.tool_use_context {
            if !ctx.is_tool_allowed(&tool_use.name) {
                return Some(format!(
                    "Tool '{}' denied by permission level {:?}",
                    tool_use.name, ctx.permission_level
                ));
            }

            // Check feature flags for specific tool categories
            if (tool_use.name.contains("shell")
                || tool_use.name.contains("bash")
                || tool_use.name.contains("execute"))
                && !ctx.is_feature_enabled("shell")
            {
                return Some(format!(
                    "Tool '{}' denied: shell feature disabled",
                    tool_use.name
                ));
            }
            if tool_use.name.contains("browser")
                && !ctx.is_feature_enabled("browser")
            {
                return Some(format!(
                    "Tool '{}' denied: browser feature disabled",
                    tool_use.name
                ));
            }

            // Check path boundaries for file operations
            if let Some(path_val) = tool_use.input.get("path").and_then(|v| v.as_str()) {
                let path = std::path::PathBuf::from(path_val);
                if !ctx.is_path_allowed(&path) {
                    return Some(format!("Path '{}' is outside allowed boundaries", path_val));
                }
            }
            if let Some(file_path_val) = tool_use.input.get("file_path").and_then(|v| v.as_str()) {
                let path = std::path::PathBuf::from(file_path_val);
                if !ctx.is_path_allowed(&path) {
                    return Some(format!(
                        "Path '{}' is outside allowed boundaries",
                        file_path_val
                    ));
                }
            }
        }

        // Check SecurityManager permission
        if let Some(ref sm) = self.security_manager {
            let sm_guard = sm.read().await;
            match sm_guard.check_permission(&tool_use.name, &tool_use.input) {
                Ok(PermissionCheck::Allowed) => {}
                Ok(PermissionCheck::Denied { reason }) => {
                    return Some(reason);
                }
                Ok(PermissionCheck::RequiresApproval { reason }) => {
                    return Some(format!("Approval required: {}", reason));
                }
                Err(e) => {
                    return Some(format!("Security check error: {}", e));
                }
            }
        }

        None
    }

    pub async fn run_turn(
        &mut self,
        adapter: &dyn LlmAdapter,
        tool_executor: &dyn ToolExecutor,
        session: &mut Session,
        user_input: &str,
        tools: Vec<ToolDefinition>,
    ) -> Result<TurnSummary> {
        session.push_message(Message::user(user_input));

        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > self.max_iterations {
                self.hook_runner.run_error("Max iterations reached").await;
                self.emit_event(TurnEvent::TurnComplete {
                    content: "Max iterations reached".to_string(),
                    iterations,
                })
                .await;
                return Ok(TurnSummary::max_iterations_reached());
            }

            // Hook: pre-LLM call
            if let HookDecision::Deny(reason) = self
                .hook_runner
                .run_pre_llm_call("default", session.messages.len())
                .await?
            {
                self.hook_runner.run_error(&reason).await;
                return Ok(TurnSummary {
                    completed: false,
                    content: reason,
                    iterations,
                });
            }

            self.emit_event(TurnEvent::LlmStart).await;

            let mut request = LlmRequest::new(String::new(), "default");
            request.messages = session.build_messages_for_request();

            if !tools.is_empty() {
                request.tools = Some(tools.clone());
            }

            let llm_start = Instant::now();
            let response = match adapter.complete(request).await {
                Ok(resp) => {
                    if let Some(ref monitor) = self.performance_monitor {
                        monitor.record_llm_call(llm_start.elapsed()).await;
                    }
                    resp
                }
                Err(e) => {
                    self.hook_runner.run_error(&e.to_string()).await;
                    return Err(e);
                }
            };

            // Hook: post-LLM call
            self.hook_runner
                .run_post_llm_call("default", response.input_tokens, response.output_tokens)
                .await;

            let tool_calls = response.parse_tool_calls();

            let assistant_msg = AssistantMessage {
                content: response.text(),
                tool_uses: tool_calls
                    .iter()
                    .map(|tc| ToolUse {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        input: tc.arguments.clone(),
                    })
                    .collect(),
            };

            if assistant_msg.tool_uses.is_empty() {
                session.push_message(Message::assistant(&assistant_msg.content));
                self.emit_event(TurnEvent::TurnComplete {
                    content: assistant_msg.content.clone(),
                    iterations,
                })
                .await;
                return Ok(TurnSummary::completed(&assistant_msg));
            }

            self.emit_event(TurnEvent::LlmComplete {
                content: assistant_msg.content.clone(),
                tool_count: assistant_msg.tool_uses.len(),
            })
            .await;

            session.push_message(Message::assistant_with_tool_calls(
                &response.content,
                response.tool_calls.clone().unwrap_or_default(),
            ));

            // Pre-tool-use phase: hook + permission + security checks
            let tool_uses = &assistant_msg.tool_uses;
            let mut blocked_tool: Option<(usize, String)> = None;

            for (idx, tool_use) in tool_uses.iter().enumerate() {
                // Hook check
                if let HookDecision::Deny(reason) = self
                    .hook_runner
                    .run_pre_tool_use(&tool_use.name, &tool_use.input)
                    .await?
                {
                    blocked_tool = Some((idx, reason));
                    break;
                }

                // ToolUseContext + SecurityManager permission check
                if let Some(reason) = self.check_tool_permission(tool_use).await {
                    blocked_tool = Some((idx, reason));
                    break;
                }
            }

            if let Some((idx, reason)) = blocked_tool {
                let tool_use = &tool_uses[idx];
                session.push_message(Message::tool_result(
                    &tool_use.id,
                    &format!("Blocked: {}", reason),
                    true,
                ));
                self.hook_runner
                    .run_error(&format!("Tool {} blocked: {}", tool_use.name, reason))
                    .await;
                continue;
            }

            // Emit tool start events
            for tool_use in tool_uses {
                self.emit_event(TurnEvent::ToolStart {
                    tool_name: tool_use.name.clone(),
                    tool_use_id: tool_use.id.clone(),
                })
                .await;
            }

            // Tool execution — use StreamingToolExecutor if available, otherwise inline buffer_unordered
            let indexed_results: Vec<(usize, ToolResult)> =
                if let Some(ref executor) = self.streaming_executor {
                    let results = executor.execute_all(tool_uses).await;
                    results.into_iter().enumerate().collect()
                } else {
                    let max_concurrent = self.max_concurrent_tools.max(1);
                    let owned_tool_uses: Vec<(usize, ToolUse)> = tool_uses
                        .iter()
                        .enumerate()
                        .map(|(idx, tu)| {
                            (
                                idx,
                                ToolUse {
                                    id: tu.id.clone(),
                                    name: tu.name.clone(),
                                    input: tu.input.clone(),
                                },
                            )
                        })
                        .collect();
                    let mut results: Vec<(usize, ToolResult)> =
                        futures::stream::iter(owned_tool_uses)
                            .map(|(idx, tool_use)| async move {
                                let result = match tool_executor.execute(&tool_use).await {
                                    Ok(r) => r,
                                    Err(e) => ToolResult::error(
                                        &tool_use.id,
                                        &tool_use.name,
                                        &format!("Error: {}", e),
                                    ),
                                };
                                (idx, result)
                            })
                            .buffer_unordered(max_concurrent)
                            .collect::<Vec<_>>()
                            .await;
                    results.sort_by_key(|(idx, _)| *idx);
                    results
                };

            // Post-tool-use hooks + performance recording + emit events
            for (_, result) in &indexed_results {
                let _ = self.hook_runner
                    .run_post_tool_use(&result.tool_name, &result.output)
                    .await;

                if let Some(ref monitor) = self.performance_monitor {
                    monitor
                        .record_tool_call(std::time::Duration::from_millis(0))
                        .await;
                }

                self.emit_event(TurnEvent::ToolComplete {
                    tool_name: result.tool_name.clone(),
                    tool_use_id: result.tool_use_id.clone(),
                    is_error: result.is_error,
                })
                .await;
            }

            for (_, result) in indexed_results {
                let output_str = serde_json::to_string(&result.output).unwrap_or_default();
                session.push_message(Message::tool_result(
                    &result.tool_use_id,
                    &output_str,
                    result.is_error,
                ));
            }

            // Context compression — inspired by hermes-agent's head/tail protection
            if self.context_compressor.needs_compression(&session.messages) {
                let before_count = session.messages.len();
                let compressed = self.context_compressor.compress(&session.messages);
                let after_count = compressed.len();
                session.messages = compressed;
                session.input_token_count = session
                    .messages
                    .iter()
                    .map(|m| maple_llm::token_counter::count_message_tokens(&m.content, &m.role))
                    .sum();
                self.emit_event(TurnEvent::ContextCompressed {
                    before_messages: before_count,
                    after_messages: after_count,
                })
                .await;
            }
        }
    }
}
