use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// In-Process Multi-Agent — lightweight task-local agent contexts
///
/// Runs multiple virtual agents within the same tokio task,
/// sharing underlying resources but isolating state via task_local.

/// Lightweight agent context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContext {
    pub agent_id: String,
    pub parent_id: Option<String>,
    pub tools: Vec<String>,
    pub depth: u32,
    pub metadata: HashMap<String, String>,
}

tokio::task_local! {
    static AGENT_CONTEXT: std::cell::RefCell<Option<AgentContext>>;
}

/// Agent execution result
#[derive(Debug, Clone)]
pub struct AgentResult {
    pub agent_id: String,
    pub output: String,
    pub iterations: usize,
    pub tokens_used: u64,
}

/// In-process agent manager
pub struct InProcessAgentManager {
    max_depth: u32,
    active_count: Arc<AtomicU32>,
    max_concurrent: u32,
}

impl InProcessAgentManager {
    pub fn new(max_depth: u32, max_concurrent: u32) -> Self {
        Self {
            max_depth,
            active_count: Arc::new(AtomicU32::new(0)),
            max_concurrent,
        }
    }

    /// Run a closure within an agent context
    pub async fn run_in_context<F, Fut, T>(&self, context: AgentContext, f: F) -> Result<T, AgentError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, AgentError>>,
    {
        // Check depth limit
        if context.depth > self.max_depth {
            return Err(AgentError::DepthExceeded {
                depth: context.depth,
                max: self.max_depth,
            });
        }

        // Check concurrency limit
        let current = self.active_count.load(Ordering::Relaxed);
        if current >= self.max_concurrent {
            return Err(AgentError::ConcurrencyExceeded {
                current,
                max: self.max_concurrent,
            });
        }

        self.active_count.fetch_add(1, Ordering::Relaxed);

        let result = AGENT_CONTEXT
            .scope(std::cell::RefCell::new(Some(context)), f())
            .await;

        self.active_count.fetch_sub(1, Ordering::Relaxed);
        result
    }

    /// Get current agent context (if running within one)
    pub fn current_context() -> Option<AgentContext> {
        AGENT_CONTEXT
            .try_with(|ctx| ctx.borrow().clone())
            .ok()
            .flatten()
    }

    /// Check if currently running within an agent context
    pub fn in_agent_context() -> bool {
        Self::current_context().is_some()
    }

    /// Get current agent depth (0 if not in context)
    pub fn current_depth() -> u32 {
        Self::current_context().map(|c| c.depth).unwrap_or(0)
    }

    /// Spawn a child agent from the current context
    pub async fn spawn_child<F, Fut, T>(
        &self,
        child_id: String,
        tools: Vec<String>,
        f: F,
    ) -> Result<T, AgentError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, AgentError>>,
    {
        let parent = Self::current_context().ok_or(AgentError::NoParentContext)?;

        let child_context = AgentContext {
            agent_id: child_id,
            parent_id: Some(parent.agent_id),
            tools,
            depth: parent.depth + 1,
            metadata: parent.metadata,
        };

        self.run_in_context(child_context, f).await
    }

    /// Get active agent count
    pub fn active_count(&self) -> u32 {
        self.active_count.load(Ordering::Relaxed)
    }
}

impl Default for InProcessAgentManager {
    fn default() -> Self {
        Self::new(5, 10)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("depth exceeded: {depth} > max {max}")]
    DepthExceeded { depth: u32, max: u32 },
    #[error("concurrency exceeded: {current} >= max {max}")]
    ConcurrencyExceeded { current: u32, max: u32 },
    #[error("no parent context — must run within an agent context")]
    NoParentContext,
    #[error("agent error: {0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_run_in_context() {
        let manager = InProcessAgentManager::new(5, 10);

        let ctx = AgentContext {
            agent_id: "agent-1".into(),
            parent_id: None,
            tools: vec!["read".into()],
            depth: 0,
            metadata: HashMap::new(),
        };

        let result = manager
            .run_in_context(ctx, || async {
                let current = InProcessAgentManager::current_context().unwrap();
                assert_eq!(current.agent_id, "agent-1");
                Ok(42)
            })
            .await
            .unwrap();

        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_context_isolation() {
        let manager = InProcessAgentManager::new(5, 10);

        let ctx = AgentContext {
            agent_id: "agent-1".into(),
            parent_id: None,
            tools: vec![],
            depth: 0,
            metadata: HashMap::new(),
        };

        manager
            .run_in_context(ctx, || async {
                assert!(InProcessAgentManager::in_agent_context());
                Ok(())
            })
            .await
            .unwrap();

        // Outside context
        assert!(!InProcessAgentManager::in_agent_context());
    }

    #[tokio::test]
    async fn test_spawn_child() {
        let manager = InProcessAgentManager::new(5, 10);

        let ctx = AgentContext {
            agent_id: "parent".into(),
            parent_id: None,
            tools: vec!["read".into(), "write".into()],
            depth: 0,
            metadata: HashMap::new(),
        };

        manager
            .run_in_context(ctx, || async {
                let manager = InProcessAgentManager::new(5, 10);
                let result = manager
                    .spawn_child(
                        "child".into(),
                        vec!["read".into()],
                        || async {
                            let ctx = InProcessAgentManager::current_context().unwrap();
                            assert_eq!(ctx.agent_id, "child");
                            assert_eq!(ctx.parent_id.as_deref(), Some("parent"));
                            assert_eq!(ctx.depth, 1);
                            Ok("child_result")
                        },
                    )
                    .await
                    .unwrap();
                assert_eq!(result, "child_result");
                Ok(())
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_depth_limit() {
        let manager = InProcessAgentManager::new(2, 10);

        let ctx = AgentContext {
            agent_id: "deep".into(),
            parent_id: None,
            tools: vec![],
            depth: 3, // Exceeds max_depth of 2
            metadata: HashMap::new(),
        };

        let result = manager.run_in_context(ctx, || async { Ok(()) }).await;
        assert!(matches!(result, Err(AgentError::DepthExceeded { .. })));
    }

    #[tokio::test]
    async fn test_no_parent_context() {
        let manager = InProcessAgentManager::new(5, 10);

        let result = manager
            .spawn_child("child".into(), vec![], || async { Ok(()) })
            .await;
        assert!(matches!(result, Err(AgentError::NoParentContext)));
    }

    #[tokio::test]
    async fn test_active_count() {
        let manager = InProcessAgentManager::new(5, 10);
        assert_eq!(manager.active_count(), 0);

        let ctx = AgentContext {
            agent_id: "agent-1".into(),
            parent_id: None,
            tools: vec![],
            depth: 0,
            metadata: HashMap::new(),
        };

        manager
            .run_in_context(ctx, || async {
                // Note: active_count is checked inside context
                Ok(())
            })
            .await
            .unwrap();

        assert_eq!(manager.active_count(), 0);
    }
}
