use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;
use std::pin::Pin;

type AsyncRpcHandler = Box<dyn Fn(Option<Value>) -> Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send>> + Send + Sync>;

pub struct RpcDispatcher {
    handlers: Arc<RwLock<HashMap<String, AsyncRpcHandler>>>,
}

impl RpcDispatcher {
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register<F, Fut>(&self, method: &str, handler: F)
    where
        F: Fn(Option<Value>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<Value>> + Send + 'static,
    {
        let wrapped: AsyncRpcHandler = Box::new(move |params| {
            let fut = handler(params);
            Box::pin(fut) as Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send>>
        });
        let mut handlers = self.handlers.write().await;
        handlers.insert(method.to_string(), wrapped);
    }

    pub async fn dispatch(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let handlers = self.handlers.read().await;
        match handlers.get(method) {
            Some(handler) => handler(params).await,
            None => anyhow::bail!("Method not found: {}", method),
        }
    }

    pub async fn register_default_handlers(&self) {
        self.register("system.info", |_: Option<Value>| async move {
            Ok(serde_json::json!({
                "name": "mapleos",
                "version": env!("CARGO_PKG_VERSION"),
            }))
        }).await;

        self.register("system.health", |_: Option<Value>| async move {
            Ok(serde_json::json!({
                "status": "ok",
            }))
        }).await;
    }
}
