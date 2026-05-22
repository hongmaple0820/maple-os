
use maple_llm::request::{LlmRequest, Message, ToolDefinition};
use maple_llm::router::LlmAdapter;
use async_trait::async_trait;
use serde_json::Value;
use anyhow::Result;

pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: Value,
}

pub struct ToolResult {
    pub tool_use_id: String,
    pub output: Value,
    pub is_error: bool,
}

impl ToolResult {
    pub fn error(tool_use_id: &str, reason: &str) -> Self {
        Self {
            tool_use_id: tool_use_id.to_string(),
            output: Value::String(reason.to_string()),
            is_error: true,
        }
    }

    pub fn success(tool_use_id: &str, output: Value) -> Self {
        Self {
            tool_use_id: tool_use_id.to_string(),
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
        self.input_token_count += msg.content.len() / 4;
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
    auto_compaction_threshold: u32,
}

impl ReactLoop {
    pub fn new(max_iterations: usize) -> Self {
        Self {
            max_iterations,
            auto_compaction_threshold: 100_000,
        }
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
                return Ok(TurnSummary::max_iterations_reached());
            }

            let mut request = LlmRequest::new(
                String::new(),
                "default",
            );
            request.messages = session.build_messages_for_request();

            if !tools.is_empty() {
                request.tools = Some(tools.clone());
            }

            let response = adapter.complete(request).await?;

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

            for tool_use in &assistant_msg.tool_uses {
                match tool_executor.execute(tool_use).await {
                    Ok(result) => {
                        let output_str = serde_json::to_string(&result.output).unwrap_or_default();
                        session.push_message(Message::tool_result(
                            &tool_use.id,
                            &output_str,
                            result.is_error,
                        ));
                    }
                    Err(e) => {
                        session.push_message(Message::tool_result(
                            &tool_use.id,
                            &format!("Error: {}", e),
                            true,
                        ));
                    }
                }
            }

            if session.input_tokens() > self.auto_compaction_threshold as usize {
                self.auto_compact(session);
            }
        }
    }

    fn auto_compact(&self, session: &mut Session) {
        if session.messages.len() <= 4 {
            return;
        }

        let mut compacted = Vec::new();

        if !session.messages.is_empty() && session.messages[0].role == "system" {
            compacted.push(session.messages[0].clone());
        }

        compacted.push(Message::system("[Earlier conversation context has been compacted to save tokens. Key information is preserved.]"));

        let recent_count = session.messages.len().min(6);
        for msg in session.messages.iter().rev().take(recent_count).rev() {
            compacted.push(msg.clone());
        }

        session.messages = compacted;
        session.input_token_count = session.messages.iter().map(|m| m.content.len() / 4).sum();
    }
}
