use serde::{Deserialize, Serialize};
use std::sync::Arc;
use maple_engine::event_bus::EventBus;
use maple_engine::executor::WorkflowExecutor;
use maple_engine::skill_registry::SkillRegistry;
use maple_engine::task_queue::TaskQueueService;
use maple_engine::scheduler::Scheduler;
use maple_engine::hooks::HookRunner;
use maple_engine::checkpoint::CheckpointManager;
use maple_llm::router::LlmRouter;
use maple_agent::registry::AgentRegistry;
use maple_agent::session_store::SessionStore;
use maple_gateway::auth::AuthService;
use maple_gateway::mcp_host::McpHostManager;
use maple_sync::sync_engine::SyncEngine;
use maple_kb::memory::MemoryStore;
use maple_kb::evolver::Evolver;
use maple_kb::prompt_version::PromptVersionManager;
use maple_collab::workspace::WorkspaceManager;
use maple_kb::retriever::HybridRetriever;
use maple_kb::bm25::BM25Searcher;
use maple_kb::vector_store::VectorSearch;
use maple_kb::indexer::Indexer;
use maple_llm::embedding::Embedder;

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[allow(dead_code)]
impl ApiError {
    pub fn new(error: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: code.into(),
            details: None,
        }
    }

    pub fn with_details(error: impl Into<String>, code: impl Into<String>, details: serde_json::Value) -> Self {
        Self {
            error: error.into(),
            code: code.into(),
            details: Some(details),
        }
    }
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let status = match self.code.as_str() {
            "NOT_FOUND" => axum::http::StatusCode::NOT_FOUND,
            "UNAUTHORIZED" => axum::http::StatusCode::UNAUTHORIZED,
            "FORBIDDEN" => axum::http::StatusCode::FORBIDDEN,
            "BAD_REQUEST" => axum::http::StatusCode::BAD_REQUEST,
            "CONFLICT" => axum::http::StatusCode::CONFLICT,
            _ => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, axum::Json(self)).into_response()
    }
}

pub struct AppState {
    pub config: Arc<tokio::sync::RwLock<ServerConfig>>,
    pub db: sqlx::SqlitePool,
    pub event_bus: Arc<EventBus>,
    pub llm_router: Arc<LlmRouter>,
    pub workflow_executor: Arc<WorkflowExecutor>,
    pub agent_registry: Arc<AgentRegistry>,
    pub auth_service: Arc<AuthService>,
    pub workspace_manager: Arc<tokio::sync::Mutex<WorkspaceManager>>,
    pub sync_engine: Arc<SyncEngine>,
    pub skill_registry: Arc<SkillRegistry>,
    pub session_store: Arc<SessionStore>,
    pub bm25_searcher: Arc<BM25Searcher>,
    pub vector_store: Arc<dyn VectorSearch>,
    pub hybrid_retriever: Arc<HybridRetriever>,
    pub indexer: Arc<Indexer>,
    pub embedder: Arc<dyn Embedder>,
    pub memory_store: Arc<tokio::sync::Mutex<MemoryStore>>,
    pub evolver: Arc<Evolver>,
    pub prompt_version_mgr: Arc<PromptVersionManager>,
    pub task_queue: Arc<TaskQueueService>,
    pub mcp_host: Arc<McpHostManager>,
    pub rate_limiter: RateLimiter,
    pub cache: crate::cache::AppCache,
    pub metrics: crate::metrics::AppMetrics,
}

impl AppState {
    pub async fn get_config(&self) -> ServerConfig {
        self.config.read().await.clone()
    }
}

#[derive(Clone)]
pub struct RateLimiter {
    pub requests: Arc<tokio::sync::RwLock<std::collections::HashMap<String, Vec<std::time::Instant>>>>,
    pub max_requests: usize,
    pub window_secs: u64,
}

impl RateLimiter {
    pub fn new(max_requests: usize, window_secs: u64) -> Self {
        Self {
            requests: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            max_requests,
            window_secs,
        }
    }

    pub async fn check(&self, key: &str) -> bool {
        let mut requests = self.requests.write().await;
        let now = std::time::Instant::now();
        let window = std::time::Duration::from_secs(self.window_secs);

        let entry = requests.entry(key.to_string()).or_insert_with(Vec::new);
        entry.retain(|t| now.duration_since(*t) < window);

        if entry.len() >= self.max_requests {
            false
        } else {
            entry.push(now);
            true
        }
    }
}

#[derive(Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub jwt_secret: String,
    pub require_auth: bool,
    pub admin_username: String,
    pub admin_password: String,
    pub usage_limit_usd: f64,
    pub log_level: String,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        Self {
            host: std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "7788".to_string())
                .parse()
                .unwrap_or(7788),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:mapleos.db?mode=rwc".to_string()),
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "mapleos-dev-secret-change-me".to_string()),
            require_auth: std::env::var("REQUIRE_AUTH")
                .unwrap_or_else(|_| "true".to_string()) == "true",
            admin_username: std::env::var("ADMIN_USERNAME")
                .unwrap_or_else(|_| "admin".to_string()),
            admin_password: std::env::var("ADMIN_PASSWORD")
                .unwrap_or_else(|_| "mapleos".to_string()),
            usage_limit_usd: std::env::var("USAGE_LIMIT_USD")
                .unwrap_or_else(|_| "50.0".to_string())
                .parse()
                .unwrap_or(50.0),
            log_level: std::env::var("LOG_LEVEL")
                .unwrap_or_else(|_| "mapleos_server=debug,maple_engine=debug,maple_llm=debug".to_string()),
        }
    }

    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
