
use maple_llm::request::{LlmRequest, Message, ToolDefinition};
use maple_llm::router::LlmAdapter;
use maple_engine::hooks::{HookRunner, HookDecision};
use crate::context_compressor::{ContextCompressor, ContextCompressorConfig};
use async_trait::async_trait;
use serde_json::Value;
use anyhow::Result;
use futures::StreamExt;

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
        self.input_token_count += maple_llm::token_counter::count_message_tokens(&msg.content, &msg.role);
        self.messages.push(msg);
    }

    pub fn input_tokens(&self) -> usize {
        self.input_token_count
    }

    pub fn build_messages_for_request(&self) -> Vec<Message> {
        self.messages.clone()
    }
}

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, tool_use: &ToolUse) -> Result<ToolResult>;
}

pub struct ReactLoop {
    max_iterations: usize,
    max_concurrent_tools: usize,
    hook_runner: HookRunner,
    context_compressor: ContextCompressor,
}

impl ReactLoop {
    pub fn new(max_iterations: usize) -> Self {
        Self {
            max_iterations,
            max_concurrent_tools: 4,
            hook_runner: HookRunner::new(),
            context_compressor: ContextCompressor::new(ContextCompressorConfig::default()),
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

    pub async fn run_turn(
        &self,
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
                return Ok(TurnSummary::max_iterations_reached());
            }

            // Hook: pre-LLM call
            if let HookDecision::Deny(reason) = self.hook_runner
                .run_pre_llm_call("default", session.messages.len()).await?
            {
                self.hook_runner.run_error(&reason).await;
                return Ok(TurnSummary {
                    completed: false,
                    content: reason,
                    iterations,
                });
            }

            let mut request = LlmRequest::new(
                String::new(),
                "default",
            );
            request.messages = session.build_messages_for_request();

            if !tools.is_empty() {
                request.tools = Some(tools.clone());
            }

            let response = match adapter.complete(request).await {
                Ok(resp) => resp,
                Err(e) => {
                    self.hook_runner.run_error(&e.to_string()).await;
                    return Err(e);
                }
            };

            // Hook: post-LLM call
            self.hook_runner.run_post_llm_call(
                "default",
                response.input_tokens,
                response.output_tokens,
            ).await;

            let tool_calls = response.parse_tool_calls();

            let assistant_msg = AssistantMessage {
                content: response.text(),
                tool_uses: tool_calls.iter().map(|tc| ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.arguments.clone(),
                }).collect(),
            };

            if assistant_msg.tool_uses.is_empty() {
                session.push_message(Message::assistant(&assistant_msg.content));
                return Ok(TurnSummary::completed(&assistant_msg));
            }

            session.push_message(Message::assistant_with_tool_calls(
                &response.content,
                response.tool_calls.clone().unwrap_or_default(),
            ));

            // Concurrent tool execution — inspired by rig's buffer_unordered pattern
            // Execute tools concurrently, then emit results in original index order
            let tool_uses = &assistant_msg.tool_uses;
            let max_concurrent = self.max_concurrent_tools.max(1);

            // Hook: pre-tool-use for each tool (check if any should be blocked)
            let mut blocked_tool: Option<(usize, String)> = None;
            for (idx, tool_use) in tool_uses.iter().enumerate() {
                if let HookDecision::Deny(reason) = self.hook_runner
                    .run_pre_tool_use(&tool_use.name, &tool_use.input).await?
                {
                    blocked_tool = Some((idx, reason));
                    break;
                }
            }

            if let Some((idx, reason)) = blocked_tool {
                let tool_use = &tool_uses[idx];
                session.push_message(Message::tool_result(
                    &tool_use.id,
                    &format!("Blocked by hook: {}", reason),
                    true,
                ));
                self.hook_runner.run_error(&format!("Tool {} blocked: {}", tool_use.name, reason)).await;
                continue;
            }

            let mut indexed_results: Vec<(usize, ToolResult)> = futures::stream::iter(
                tool_uses.iter().enumerate()
            )
            .map(|(idx, tool_use)| async move {
                let result = match tool_executor.execute(tool_use).await {
                    Ok(r) => r,
                    Err(e) => ToolResult::error(&tool_use.id, &tool_use.name, &format!("Error: {}", e)),
                };
                (idx, result)
            })
            .buffer_unordered(max_concurrent)
            .collect::<Vec<_>>()
            .await;

            // Sort by original index to preserve tool_use order (not completion order)
            indexed_results.sort_by_key(|(idx, _)| *idx);

            // Hook: post-tool-use for each result
            for (_, result) in &indexed_results {
                self.hook_runner.run_post_tool_use(
                    &result.tool_name,
                    &result.output,
                ).await;
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
                let compressed = self.context_compressor.compress(&session.messages);
                session.messages = compressed;
                session.input_token_count = session.messages.iter().map(|m| m.content.len() / 4).sum();
            }
        }
    }
}
