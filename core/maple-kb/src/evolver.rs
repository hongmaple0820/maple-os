use maple_engine::workflow::WorkflowExecution;
use maple_llm::router::LlmRouter;
use maple_llm::request::LlmRequest;
use crate::memory::{MemoryStore, MemoryEntry, MemoryType};
use std::sync::Arc;
use std::collections::HashMap;
use anyhow::Result;

pub struct Evolver {
    llm_router: Arc<LlmRouter>,
    memory_store: Option<Arc<tokio::sync::Mutex<MemoryStore>>>,
}

impl Evolver {
    pub fn new(llm_router: Arc<LlmRouter>) -> Self {
        Self { llm_router, memory_store: None }
    }

    pub fn with_memory_store(mut self, store: Arc<tokio::sync::Mutex<MemoryStore>>) -> Self {
        self.memory_store = Some(store);
        self
    }

    pub async fn on_execution_complete(&self, exec: &WorkflowExecution) -> Result<()> {
        let score = self.score_execution(exec).await?;

        if score >= 7.0 {
            tracing::info!(workflow_id = %exec.workflow_id, score = score, "High score, extracting experience");
            self.extract_experience(exec).await?;
        } else if score < 5.0 {
            tracing::warn!(workflow_id = %exec.workflow_id, score = score, "Low score, recording failure");
            self.record_failure_pattern(exec).await?;
        }

        Ok(())
    }

    async fn extract_experience(&self, exec: &WorkflowExecution) -> Result<()> {
        let output = exec.context.get("output").map(|v| v.to_string()).unwrap_or_default();
        let experience = self.generate_takeaway(exec, &output, "success").await?;
        self.store_experience(&experience, MemoryType::Episodic, HashMap::from([
            ("workflow_id".to_string(), exec.workflow_id.clone()),
            ("outcome".to_string(), "success".to_string()),
        ])).await
    }

    async fn record_failure_pattern(&self, exec: &WorkflowExecution) -> Result<()> {
        let error = exec.error.clone().unwrap_or_default();
        let experience = self.generate_takeaway(exec, &error, "failure").await?;
        self.store_experience(&experience, MemoryType::Episodic, HashMap::from([
            ("workflow_id".to_string(), exec.workflow_id.clone()),
            ("outcome".to_string(), "failure".to_string()),
        ])).await
    }

    async fn store_experience(&self, content: &str, memory_type: MemoryType, metadata: HashMap<String, String>) -> Result<()> {
        if let Some(memory_store) = &self.memory_store {
            let mut store = memory_store.lock().await;
            let entry = MemoryEntry {
                id: uuid::Uuid::new_v4().to_string(),
                memory_type,
                content: content.to_string(),
                metadata,
                created_at: chrono::Utc::now().timestamp(),
                access_count: 0,
            };
            store.store(entry).await?;
        } else {
            tracing::info!(content = %content, "Experience extracted (no memory store)");
        }
        Ok(())
    }

    async fn generate_takeaway(&self, exec: &WorkflowExecution, result: &str, outcome: &str) -> Result<String> {
        let prompt = format!(
            "Summarize the key takeaway from this workflow execution in one concise sentence:\n\
             Workflow: {}\n\
             Outcome: {}\n\
             Result: {}\n\
             Provide only the takeaway sentence.",
            exec.workflow_id, outcome, result
        );

        let request = LlmRequest::quick_qa(&prompt);
        let adapter = self.llm_router.route(&request).await?;
        let response = adapter.complete(request).await?;
        Ok(response.text())
    }

    /// Score and optionally extract knowledge from a completed chat conversation.
    /// Runs LLM scoring; if the conversation is valuable (score >= 7), stores a
    /// one-sentence takeaway as an Episodic memory entry.
    pub async fn on_chat_complete(
        &self,
        session_id: &str,
        user_msg: &str,
        assistant_msg: &str,
    ) -> Result<()> {
        let score = self.score_chat(session_id, user_msg, assistant_msg).await?;
        if score >= 7.0 {
            tracing::info!(session_id = %session_id, score = score, "Valuable chat, extracting knowledge");
            let takeaway = self.generate_chat_takeaway(user_msg, assistant_msg).await?;
            self.store_experience(&takeaway, MemoryType::Episodic, HashMap::from([
                ("source".to_string(), "chat".to_string()),
                ("session_id".to_string(), session_id.to_string()),
            ])).await?;
        }
        Ok(())
    }

    async fn score_chat(&self, session_id: &str, user_msg: &str, assistant_msg: &str) -> Result<f32> {
        let prompt = format!(
            "Score this chat exchange on a scale of 0-10 for knowledge value:\n\
             Session: {}\n\
             User: {}\n\
             Assistant: {}\n\
             Return only a number.",
            session_id,
            if user_msg.len() > 500 { &user_msg[..500] } else { user_msg },
            if assistant_msg.len() > 500 { &assistant_msg[..500] } else { assistant_msg },
        );
        let request = LlmRequest::quick_qa(&prompt);
        let adapter = self.llm_router.route(&request).await?;
        let response = adapter.complete(request).await?;
        let text = response.text().trim().to_string();
        text.parse::<f32>().or_else(|_| {
            let digits: String = text.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
            digits.parse::<f32>().map_err(|_| anyhow::anyhow!("Invalid score: {}", text))
        })
    }

    async fn generate_chat_takeaway(&self, user_msg: &str, assistant_msg: &str) -> Result<String> {
        let prompt = format!(
            "Extract one concise knowledge takeaway from this conversation:\n\
             User: {}\n\
             Assistant: {}\n\
             Provide only the takeaway sentence.",
            if user_msg.len() > 500 { &user_msg[..500] } else { user_msg },
            if assistant_msg.len() > 500 { &assistant_msg[..500] } else { assistant_msg },
        );
        let request = LlmRequest::quick_qa(&prompt);
        let adapter = self.llm_router.route(&request).await?;
        let response = adapter.complete(request).await?;
        Ok(response.text())
    }

    async fn score_execution(&self, exec: &WorkflowExecution) -> Result<f32> {
        let output = exec.context.get("output").map(|v| v.to_string()).unwrap_or_default();
        let error_str = exec.error.clone().unwrap_or_default();

        let prompt = format!(
            "Score this workflow execution (0-10):\n\
             Workflow: {}\n\
             Status: {:?}\n\
             Output: {}\n\
             Error: {}\n\
             Return only a number.",
            exec.workflow_id, exec.status, output, error_str
        );

        let request = LlmRequest::quick_qa(&prompt);
        let adapter = self.llm_router.route(&request).await?;
        let response = adapter.complete(request).await?;

        let text = response.text();
        let trimmed = text.trim();
        trimmed.parse::<f32>().or_else(|_| {
            let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
            digits.parse::<f32>().map_err(|_| anyhow::anyhow!("Invalid score: {}", trimmed))
        })
    }
}