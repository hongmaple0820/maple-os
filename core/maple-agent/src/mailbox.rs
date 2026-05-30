use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

/// Mailbox-based Inter-Agent Communication
///
/// Provides async message passing between agents:
/// - Typed messages with sender/receiver routing
/// - Priority levels for urgent messages
/// - Message history with configurable retention
/// - Fan-out (broadcast to multiple receivers)

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MessagePriority {
    Low,
    Normal,
    High,
    Urgent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub priority: MessagePriority,
    pub reply_to: Option<String>,
    pub created_at: i64,
}

/// Individual mailbox for an agent
#[derive(Debug, Default)]
pub struct Mailbox {
    agent_id: String,
    inbox: VecDeque<MailboxMessage>,
    history: Vec<MailboxMessage>,
    max_history: usize,
}

impl Mailbox {
    pub fn new(agent_id: String, max_history: usize) -> Self {
        Self {
            agent_id,
            inbox: VecDeque::new(),
            history: Vec::new(),
            max_history,
        }
    }

    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// Receive next message (highest priority first)
    pub fn receive(&mut self) -> Option<MailboxMessage> {
        // Sort by priority (highest first), then by creation time
        if self.inbox.is_empty() {
            return None;
        }

        let mut best_idx = 0;
        for (i, msg) in self.inbox.iter().enumerate().skip(1) {
            if msg.priority > self.inbox[best_idx].priority
                || (msg.priority == self.inbox[best_idx].priority
                    && msg.created_at < self.inbox[best_idx].created_at)
            {
                best_idx = i;
            }
        }

        let msg = self.inbox.remove(best_idx)?;
        self.add_to_history(msg.clone());
        Some(msg)
    }

    /// Peek at next message without removing
    pub fn peek(&self) -> Option<&MailboxMessage> {
        if self.inbox.is_empty() {
            return None;
        }
        self.inbox.iter().max_by_key(|m| m.priority)
    }

    /// Check if inbox has messages
    pub fn has_messages(&self) -> bool {
        !self.inbox.is_empty()
    }

    /// Number of messages in inbox
    pub fn inbox_len(&self) -> usize {
        self.inbox.len()
    }

    /// Get message history
    pub fn history(&self) -> &[MailboxMessage] {
        &self.history
    }

    /// Clear inbox
    pub fn clear_inbox(&mut self) {
        self.inbox.clear();
    }

    fn deliver(&mut self, message: MailboxMessage) {
        self.inbox.push_back(message);
    }

    fn add_to_history(&mut self, message: MailboxMessage) {
        if self.history.len() >= self.max_history {
            self.history.remove(0);
        }
        self.history.push(message);
    }
}

/// Central message router managing multiple mailboxes
#[derive(Debug, Default)]
pub struct MailboxRouter {
    mailboxes: HashMap<String, Mailbox>,
    default_max_history: usize,
}

impl MailboxRouter {
    pub fn new(default_max_history: usize) -> Self {
        Self {
            mailboxes: HashMap::new(),
            default_max_history,
        }
    }

    /// Register an agent's mailbox
    pub fn register(&mut self, agent_id: &str) {
        self.mailboxes.insert(
            agent_id.to_string(),
            Mailbox::new(agent_id.to_string(), self.default_max_history),
        );
    }

    /// Send a message to a specific agent
    pub fn send(&mut self, message: MailboxMessage) -> Result<(), MailboxError> {
        let mailbox = self
            .mailboxes
            .get_mut(&message.to)
            .ok_or_else(|| MailboxError::AgentNotFound(message.to.clone()))?;
        mailbox.deliver(message);
        Ok(())
    }

    /// Broadcast a message to all registered agents (except sender)
    pub fn broadcast(
        &mut self,
        from: &str,
        subject: String,
        body: String,
        priority: MessagePriority,
    ) {
        let now = chrono::Utc::now().timestamp();
        let agents: Vec<String> = self
            .mailboxes
            .keys()
            .filter(|k| k.as_str() != from)
            .cloned()
            .collect();

        for (idx, agent_id) in agents.iter().enumerate() {
            let msg = MailboxMessage {
                id: format!("{}-broadcast-{}", from, idx),
                from: from.to_string(),
                to: agent_id.clone(),
                subject: subject.clone(),
                body: body.clone(),
                priority,
                reply_to: None,
                created_at: now,
            };
            if let Some(mailbox) = self.mailboxes.get_mut(agent_id) {
                mailbox.deliver(msg);
            }
        }
    }

    /// Get a mutable reference to an agent's mailbox
    pub fn mailbox_mut(&mut self, agent_id: &str) -> Option<&mut Mailbox> {
        self.mailboxes.get_mut(agent_id)
    }

    /// Get inbox count for an agent
    pub fn inbox_count(&self, agent_id: &str) -> usize {
        self.mailboxes
            .get(agent_id)
            .map(|m| m.inbox_len())
            .unwrap_or(0)
    }

    /// Get all registered agent IDs
    pub fn agents(&self) -> Vec<&str> {
        self.mailboxes.keys().map(|s| s.as_str()).collect()
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum MailboxError {
    #[error("agent not found: {0}")]
    AgentNotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(id: &str, from: &str, to: &str, priority: MessagePriority) -> MailboxMessage {
        MailboxMessage {
            id: id.into(),
            from: from.into(),
            to: to.into(),
            subject: "test".into(),
            body: "body".into(),
            priority,
            reply_to: None,
            created_at: 1000,
        }
    }

    #[test]
    fn test_send_and_receive() {
        let mut router = MailboxRouter::new(100);
        router.register("agent-a");
        router.register("agent-b");

        let msg = make_msg("m1", "agent-a", "agent-b", MessagePriority::Normal);
        router.send(msg).unwrap();

        let received = router.mailbox_mut("agent-b").unwrap().receive().unwrap();
        assert_eq!(received.id, "m1");
        assert_eq!(received.from, "agent-a");
    }

    #[test]
    fn test_priority_ordering() {
        let mut router = MailboxRouter::new(100);
        router.register("agent-a");

        router
            .send(make_msg("low", "x", "agent-a", MessagePriority::Low))
            .unwrap();
        router
            .send(make_msg("urgent", "x", "agent-a", MessagePriority::Urgent))
            .unwrap();
        router
            .send(make_msg("normal", "x", "agent-a", MessagePriority::Normal))
            .unwrap();

        let m1 = router.mailbox_mut("agent-a").unwrap().receive().unwrap();
        assert_eq!(m1.id, "urgent");

        let m2 = router.mailbox_mut("agent-a").unwrap().receive().unwrap();
        assert_eq!(m2.id, "normal");

        let m3 = router.mailbox_mut("agent-a").unwrap().receive().unwrap();
        assert_eq!(m3.id, "low");
    }

    #[test]
    fn test_broadcast() {
        let mut router = MailboxRouter::new(100);
        router.register("a");
        router.register("b");
        router.register("c");

        router.broadcast(
            "a",
            "announcement".into(),
            "hello all".into(),
            MessagePriority::Normal,
        );

        // b and c should have messages, a should not
        assert_eq!(router.inbox_count("a"), 0);
        assert_eq!(router.inbox_count("b"), 1);
        assert_eq!(router.inbox_count("c"), 1);
    }

    #[test]
    fn test_agent_not_found() {
        let mut router = MailboxRouter::new(100);
        let msg = make_msg("m1", "a", "unknown", MessagePriority::Normal);
        let result = router.send(msg);
        assert!(matches!(result, Err(MailboxError::AgentNotFound(_))));
    }

    #[test]
    fn test_history() {
        let mut mailbox = Mailbox::new("a".into(), 10);
        mailbox.deliver(make_msg("m1", "x", "a", MessagePriority::Normal));
        mailbox.deliver(make_msg("m2", "x", "a", MessagePriority::Normal));

        mailbox.receive();
        assert_eq!(mailbox.history().len(), 1);

        mailbox.receive();
        assert_eq!(mailbox.history().len(), 2);
    }

    #[test]
    fn test_history_eviction() {
        let mut mailbox = Mailbox::new("a".into(), 2);
        for i in 0..5 {
            mailbox.deliver(make_msg(&format!("m{}", i), "x", "a", MessagePriority::Normal));
        }

        for _ in 0..5 {
            mailbox.receive();
        }

        assert_eq!(mailbox.history().len(), 2);
        assert_eq!(mailbox.history()[0].id, "m3");
        assert_eq!(mailbox.history()[1].id, "m4");
    }

    #[test]
    fn test_peek() {
        let mut mailbox = Mailbox::new("a".into(), 10);
        mailbox.deliver(make_msg("m1", "x", "a", MessagePriority::Low));
        mailbox.deliver(make_msg("m2", "x", "a", MessagePriority::High));

        let peeked = mailbox.peek().unwrap();
        assert_eq!(peeked.id, "m2");
        assert_eq!(mailbox.inbox_len(), 2); // not removed
    }
}
