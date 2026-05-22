use maple_llm::request::Message;
use maple_llm::router::LlmRouter;
use maple_llm::request::LlmRequest;
use std::sync::Arc;


pub struct ConversationManager {
    max_context_tokens: usize,
    llm_router: Option<Arc<LlmRouter>>,
    summary_model: String,
}

impl ConversationManager {
    pub fn new(max_context_tokens: usize) -> Self {
        Self {
            max_context_tokens,
            llm_router: None,
            summary_model: "default".to_string(),
        }
    }

    pub fn with_llm_router(mut self, router: Arc<LlmRouter>) -> Self {
        self.llm_router = Some(router);
        self
    }

    pub fn with_summary_model(mut self, model: String) -> Self {
        self.summary_model = model;
        self
    }

    pub async fn compact(&self, messages: &[Message]) -> Vec<Message> {
        if messages.len() <= 6 {
            return messages.to_vec();
        }

        let system_msg = messages.iter().find(|m| m.role == "system").cloned();
        let non_system = messages.iter().filter(|m| m.role != "system").collect::<Vec<&Message>>();

        if non_system.len() <= 4 {
            return messages.to_vec();
        }

        let recent_count = non_system.len().min(4);
        let older = &non_system[..non_system.len() - recent_count];
        let recent = &non_system[non_system.len() - recent_count..];

        let summary_text = self.generate_summary(older).await;

        let mut compacted = Vec::new();
        if let Some(sys) = system_msg {
            compacted.push(sys);
        }

        let summary = Message::system(&format!(
            "Summary of earlier conversation:\n{}",
            summary_text
        ));
        compacted.push(summary);

        for msg in recent.iter() {
            compacted.push((*msg).clone());
        }

        compacted
    }

    async fn generate_summary(&self, messages: &[&Message]) -> String {
        if let Some(router) = &self.llm_router {
            let conversation_text = messages.iter()
                .map(|m| format!("{}: {}", m.role, m.content))
                .collect::<Vec<String>>()
                .join("\n");

            let prompt = format!(
                "Summarize the following conversation concisely, preserving key decisions, facts, and outcomes. Omit redundant details:\n\n{}",
                conversation_text
            );

            let request = LlmRequest::quick_qa(&prompt);

            match router.route(&request).await {
                Ok(adapter) => {
                    match adapter.complete(request).await {
                        Ok(response) => response.text(),
                        Err(e) => {
                            tracing::warn!("LLM summary generation failed: {}", e);
                            self.fallback_summary(messages)
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("LLM router failed for summary: {}", e);
                    self.fallback_summary(messages)
                }
            }
        } else {
            self.fallback_summary(messages)
        }
    }

    fn fallback_summary(&self, messages: &[&Message]) -> String {
        let mut key_points = Vec::new();
        for msg in messages {
            let content = msg.content.clone();
            if content.len() > 200 {
                key_points.push(format!("{}: {}...", msg.role, &content[..200]));
            } else {
                key_points.push(format!("{}: {}", msg.role, content));
            }
        }
        format!("Earlier conversation had {} exchanges. Key points:\n{}", messages.len(), key_points.join("\n"))
    }

    pub fn estimate_tokens(&self, messages: &[Message]) -> usize {
        messages.iter().map(|m| m.content.len() / 4 + 10).sum()
    }

    pub fn needs_compaction(&self, messages: &[Message]) -> bool {
        self.estimate_tokens(messages) > self.max_context_tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_compact_short_conversation() {
        let mgr = ConversationManager::new(4096);
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hi"),
            Message::assistant("Hello!"),
        ];
        let result = mgr.compact(&messages).await;
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_needs_compaction() {
        let mgr = ConversationManager::new(50);
        let messages = vec![
            Message::user("This is a very long message that should exceed the token limit when combined with other messages and more text here"),
            Message::assistant("And this is also a pretty long response that adds to the total with additional content"),
        ];
        assert!(mgr.needs_compaction(&messages));
    }

    #[test]
    fn test_estimate_tokens() {
        let mgr = ConversationManager::new(4096);
        let messages = vec![
            Message::user("Hello world"),
            Message::assistant("Hi there"),
        ];
        let tokens = mgr.estimate_tokens(&messages);
        assert!(tokens > 0);
        assert!(tokens < 100);
    }
}