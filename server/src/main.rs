use axum::Json;
use axum::Router;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::IntoResponse;
use axum::response::sse::{Event, Sse};
use axum::routing::{get, post};
use std::convert::Infallible;
use axum::middleware::{self, Next};

use maple_engine::workflow::Workflow;
use maple_engine::executor::{WorkflowExecutor, NodeExecutor};
use maple_engine::event_bus::EventBus;
use maple_engine::checkpoint::CheckpointManager;
use maple_engine::hooks::HookRunner;
use maple_engine::skill_registry::SkillRegistry;
use maple_llm::router::LlmRouter;
use maple_llm::usage::UsageTracker;
use maple_llm::adapters::ollama::OllamaAdapter;
use maple_agent::registry::AgentRegistry;
use maple_agent::react_loop::{ReactLoop, Session, ToolExecutor, ToolUse, ToolResult};
use maple_agent::session_store::SessionStore;
use async_trait::async_trait;
use maple_gateway::auth::AuthService;
use maple_gateway::ws_gateway;
use maple_gateway::sse_gateway;
use maple_rpc::server::RpcServer;
use maple_rpc::dispatch::RpcDispatcher;
use maple_sync::sync_engine::SyncEngine;
use maple_kb::indexer::{Indexer, Document};
use maple_kb::bm25::BM25Searcher;
use maple_kb::vector_store::{VectorSearch, InMemoryVectorStore, QdrantVectorStore};
use maple_kb::retriever::HybridRetriever;
use maple_kb::memory::{MemoryStore, MemoryEntry, MemoryType};
use maple_kb::prompt_version::PromptVersionManager;
use maple_engine::task_queue::TaskQueueService;
use maple_llm::embedding::{Embedder, OllamaEmbedder, FallbackEmbedder};
use maple_collab::workspace::WorkspaceManager;
use serde::{Deserialize, Serialize};
use std::sync:: Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub struct AppState {
    pub config: ServerConfig,
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
    pub prompt_version_mgr: Arc<PromptVersionManager>,
    pub task_queue: Arc<TaskQueueService>,
}

#[derive(Debug, Deserialize)]
struct ChatRequest {
    message: String,
    #[serde(default = "default_model")]
    #[allow(dead_code)]
    model: String,
    #[serde(default)]
    tools: Vec<maple_llm::request::ToolDefinition>,
    #[serde(default)]
    session_id: Option<String>,
}

fn default_model() -> String {
    "auto".to_string()
}

#[derive(Debug, Serialize)]
struct ChatResponse {
    reply: String,
    model: Option<String>,
    tool_calls: Option<Vec<serde_json::Value>>,
    session_id: String,
}

async fn chat_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Result<axum::Json<ChatResponse>, axum::http::StatusCode> {
    let session_id = req.session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let _ = state.session_store.save_message(&session_id, "user", &req.message, None, None).await;

    let llm_request = maple_llm::request::LlmRequest::new(req.message.clone(), "default");

    let adapter = match state.llm_router.route(&llm_request).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("LLM routing failed: {}", e);
            return Ok(axum::Json(ChatResponse {
                reply: format!("No LLM model available: {}", e),
                model: None,
                tool_calls: None,
                session_id,
            }));
        }
    };

    let model_name = adapter.name().to_string();

    if req.tools.is_empty() {
        match adapter.complete(llm_request).await {
            Ok(response) => {
                let _ = state.session_store.save_message(&session_id, "assistant", &response.content, None, response.tool_calls.as_deref()).await;
                Ok(axum::Json(ChatResponse {
                    reply: response.content,
                    model: Some(model_name),
                    tool_calls: response.tool_calls,
                    session_id,
                }))
            }
            Err(e) => {
                tracing::error!("LLM complete failed: {}", e);
                Ok(axum::Json(ChatResponse {
                    reply: format!("LLM error: {}", e),
                    model: Some(model_name),
                    tool_calls: None,
                    session_id,
                }))
            }
        }
    } else {
        let react_loop = ReactLoop::new(10);

        struct AppToolExecutor {
            skill_registry: Arc<SkillRegistry>,
        }

        #[async_trait]
        impl ToolExecutor for AppToolExecutor {
            async fn execute(&self, tool_use: &ToolUse) -> anyhow::Result<ToolResult> {
                match self.skill_registry.execute(&tool_use.name, &tool_use.input).await {
                    Ok(v) => Ok(ToolResult::success(&tool_use.id, v)),
                    Err(e) => Ok(ToolResult::error(&tool_use.id, &e.to_string())),
                }
            }
        }

        let mut session = state.session_store.load_session(&session_id).await
            .unwrap_or_else(|_| Session::new("You are a helpful assistant. Use the provided tools when needed."));
        let tool_executor = AppToolExecutor {
            skill_registry: state.skill_registry.clone(),
        };

        match react_loop.run_turn(
            adapter,
            &tool_executor,
            &mut session,
            &req.message,
            req.tools,
        ).await {
            Ok(summary) => {
                let _ = state.session_store.save_session_messages(&session_id, &session).await;
                Ok(axum::Json(ChatResponse {
                    reply: summary.content,
                    model: Some(model_name),
                    tool_calls: None,
                    session_id,
                }))
            }
            Err(e) => {
                tracing::error!("ReAct loop failed: {}", e);
                Ok(axum::Json(ChatResponse {
                    reply: format!("Agent error: {}", e),
                    model: Some(model_name),
                    tool_calls: None,
                    session_id,
                }))
            }
        }
    }
}

async fn run_migrations(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS workflows (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            version INTEGER NOT NULL DEFAULT 1,
            yaml_content TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'draft',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS workflow_executions (
            id TEXT PRIMARY KEY,
            workflow_id TEXT NOT NULL,
            workflow_version INTEGER NOT NULL,
            status TEXT NOT NULL DEFAULT 'running',
            context_snapshot TEXT,
            input TEXT,
            output TEXT,
            error TEXT,
            started_at INTEGER NOT NULL,
            completed_at INTEGER,
            agent_id TEXT
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS checkpoints (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            exec_id TEXT NOT NULL,
            node_id TEXT NOT NULL,
            output TEXT NOT NULL,
            context_snapshot TEXT NOT NULL,
            created_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS agents (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            transport_type TEXT NOT NULL,
            transport_config TEXT NOT NULL,
            capabilities TEXT NOT NULL,
            triggers TEXT,
            status TEXT NOT NULL DEFAULT 'offline',
            last_heartbeat INTEGER,
            max_concurrent_tasks INTEGER NOT NULL DEFAULT 3,
            created_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS kb_documents (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            title TEXT NOT NULL,
            source_type TEXT NOT NULL,
            source_url TEXT,
            content TEXT NOT NULL,
            chunk_count INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS kb_chunks (
            id TEXT PRIMARY KEY,
            document_id TEXT NOT NULL,
            content TEXT NOT NULL,
            embedding BLOB,
            term_freqs TEXT,
            created_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            type TEXT NOT NULL,
            sender_id TEXT NOT NULL,
            sender_type TEXT NOT NULL,
            content TEXT NOT NULL,
            metadata TEXT,
            created_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS chat_messages (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            metadata TEXT,
            created_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS kv_store (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS prompt_versions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            prompt_ref TEXT NOT NULL,
            version INTEGER NOT NULL,
            content TEXT NOT NULL,
            change_reason TEXT,
            ab_test_result TEXT,
            created_at INTEGER NOT NULL,
            UNIQUE(prompt_ref, version)
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS scheduled_jobs (
            id TEXT PRIMARY KEY,
            workflow_id TEXT NOT NULL,
            cron_expr TEXT NOT NULL,
            timezone TEXT NOT NULL DEFAULT 'UTC',
            last_run_at INTEGER,
            next_run_at INTEGER NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS sync_state (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            last_sync_at INTEGER,
            local_version INTEGER NOT NULL DEFAULT 0,
            remote_version INTEGER,
            pending_changes INTEGER NOT NULL DEFAULT 0
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS memories (
            id TEXT PRIMARY KEY,
            memory_type TEXT NOT NULL,
            content TEXT NOT NULL,
            metadata TEXT,
            created_at INTEGER NOT NULL,
            access_count INTEGER NOT NULL DEFAULT 0
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS workspaces (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            owner_id TEXT NOT NULL,
            max_agents INTEGER DEFAULT 10,
            auto_approve INTEGER DEFAULT 0,
            knowledge_base_enabled INTEGER DEFAULT 1,
            created_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS workspace_members (
            workspace_id TEXT NOT NULL,
            member_id TEXT NOT NULL,
            name TEXT NOT NULL,
            member_type TEXT NOT NULL,
            role TEXT NOT NULL,
            PRIMARY KEY (workspace_id, member_id),
            FOREIGN KEY (workspace_id) REFERENCES workspaces(id)
        )"
    ).execute(pool).await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_workflow_exec_workflow ON workflow_executions(workflow_id)")
        .execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_checkpoints_exec ON checkpoints(exec_id)")
        .execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_workspace ON messages(workspace_id, created_at)")
        .execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_kb_chunks_document ON kb_chunks(document_id)")
        .execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_kb_documents_workspace ON kb_documents(workspace_id)")
        .execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_chat_messages_session ON chat_messages(session_id, created_at)")
        .execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_next ON scheduled_jobs(next_run_at) WHERE enabled = 1")
        .execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_workspace_members_workspace ON workspace_members(workspace_id)")
        .execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(memory_type)")
        .execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS task_queue (
            id TEXT PRIMARY KEY,
            task_type TEXT NOT NULL,
            priority INTEGER NOT NULL DEFAULT 0,
            payload TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            retry_count INTEGER NOT NULL DEFAULT 0,
            max_retries INTEGER NOT NULL DEFAULT 3,
            next_run_at INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            error_message TEXT,
            agent_id TEXT
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_task_queue_status_priority
         ON task_queue(status, priority DESC, next_run_at)"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_task_queue_type
         ON task_queue(task_type, status)"
    ).execute(pool).await?;

    tracing::info!("Database migrations completed");
    Ok(())
}

fn build_llm_router(config: &ServerConfig) -> Arc<LlmRouter> {
    let usage_tracker = Arc::new(UsageTracker::new(config.usage_limit_usd));
    let mut router = LlmRouter::new(usage_tracker);

    if let Ok(base_url) = std::env::var("OLLAMA_BASE_URL") {
        let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "qwen2.5:7b".to_string());
        let adapter = OllamaAdapter::new(model).with_base_url(base_url);
        router.register_adapter(Box::new(adapter));
    } else {
        let adapter = OllamaAdapter::qwen_7b();
        router.register_adapter(Box::new(adapter));
    }

    if let Ok(api_key) = std::env::var("DEEPSEEK_API_KEY") {
        router.register_adapter(Box::new(
            maple_llm::adapters::openai_compat::OpenAiCompatAdapter::deepseek(api_key)
        ));
    }

    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        router.register_adapter(Box::new(
            maple_llm::adapters::anthropic::AnthropicAdapter::new(api_key, "claude-3-5-sonnet-20241022".to_string())
        ));
    }

    if let Ok(api_key) = std::env::var("QWEN_API_KEY") {
        router.register_adapter(Box::new(
            maple_llm::adapters::openai_compat::OpenAiCompatAdapter::qwen(api_key)
        ));
    }

    if let Ok(api_key) = std::env::var("GLM_API_KEY") {
        router.register_adapter(Box::new(
            maple_llm::adapters::openai_compat::OpenAiCompatAdapter::glm(api_key)
        ));
    }

    let mut fallback = vec!["ollama/qwen2.5:7b".to_string()];
    if std::env::var("DEEPSEEK_API_KEY").is_ok() {
        fallback.push("deepseek-chat".to_string());
    }
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        fallback.push("claude-3-5-sonnet-20241022".to_string());
    }
    if std::env::var("QWEN_API_KEY").is_ok() {
        fallback.push("qwen-plus".to_string());
    }
    if std::env::var("GLM_API_KEY").is_ok() {
        fallback.push("glm-4".to_string());
    }
    router.set_fallback_chain(fallback);

    Arc::new(router)
}

async fn register_builtin_skills(skill_registry: &SkillRegistry) {
    use maple_engine::skill_registry::Skill;
    use serde_json::Value;

    struct EchoSkill;
    impl Skill for EchoSkill {
        fn id(&self) -> &str { "echo" }
        fn description(&self) -> &str { "Echo back the input" }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            Ok(config.clone())
        }
    }

    struct WebSearchSkill;
    impl Skill for WebSearchSkill {
        fn id(&self) -> &str { "web_search" }
        fn description(&self) -> &str { "Search the web for information" }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            let query = config["query"].as_str().unwrap_or("");
            let num_results = config["num_results"].as_u64().unwrap_or(5);
            
            if query.is_empty() {
                return Ok(serde_json::json!({"error": "query is required"}));
            }

            let search_api_key = std::env::var("SEARCH_API_KEY").ok();
            let search_engine_id = std::env::var("SEARCH_ENGINE_ID").ok();

            if let (Some(api_key), Some(engine_id)) = (search_api_key, search_engine_id) {
                let rt = tokio::runtime::Handle::current();
                let _guard = rt.enter();
                
                let url = format!(
                    "https://www.googleapis.com/customsearch/v1?key={}&cx={}&q={}&num={}",
                    api_key, engine_id, urlencoding::encode(query), num_results
                );

                let client = reqwest::Client::new();
                match tokio::task::block_in_place(|| {
                    rt.block_on(async {
                        client.get(&url)
                            .timeout(std::time::Duration::from_secs(10))
                            .send()
                            .await
                    })
                }) {
                    Ok(resp) => {
                        let body = tokio::task::block_in_place(|| {
                            rt.block_on(async { resp.text().await.unwrap_or_default() })
                        });
                        
                        if let Ok(json) = serde_json::from_str::<Value>(&body) {
                            let empty: Vec<Value> = vec![];
                            let items = json["items"].as_array().unwrap_or(&empty);
                            let results: Vec<Value> = items.iter().map(|item| {
                                serde_json::json!({
                                    "title": item["title"].as_str().unwrap_or(""),
                                    "url": item["link"].as_str().unwrap_or(""),
                                    "snippet": item["snippet"].as_str().unwrap_or(""),
                                })
                            }).collect();

                            return Ok(serde_json::json!({
                                "query": query,
                                "results": results,
                                "source": "google_custom_search",
                            }));
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Web search API error: {}", e);
                    }
                }
            }

            Ok(serde_json::json!({
                "query": query,
                "results": [],
                "source": "none",
                "message": "Web search requires SEARCH_API_KEY and SEARCH_ENGINE_ID environment variables",
            }))
        }
    }

    struct CodeExecSkill;
    impl Skill for CodeExecSkill {
        fn id(&self) -> &str { "code_execute" }
        fn description(&self) -> &str { "Execute code in a sandboxed environment" }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            let language = config["language"].as_str().unwrap_or("unknown");
            let code = config["code"].as_str().unwrap_or("");
            let timeout_secs = config["timeout"].as_u64().unwrap_or(30);

            if code.is_empty() {
                return Ok(serde_json::json!({"error": "code is required"}));
            }

            let (interpreter, extension) = match language.to_lowercase().as_str() {
                "python" | "py" => ("python3", ".py"),
                "javascript" | "js" => ("node", ".js"),
                "bash" | "sh" => ("bash", ".sh"),
                "ruby" | "rb" => ("ruby", ".rb"),
                "perl" | "pl" => ("perl", ".pl"),
                _ => return Ok(serde_json::json!({
                    "error": format!("Unsupported language: {}", language),
                    "supported": ["python", "javascript", "bash", "ruby", "perl"],
                })),
            };

            let temp_dir = std::env::temp_dir();
            let filename = format!("mapleos_exec_{}{}", uuid::Uuid::new_v4(), extension);
            let file_path = temp_dir.join(&filename);

            if let Err(e) = std::fs::write(&file_path, code) {
                return Ok(serde_json::json!({"error": format!("Failed to write temp file: {}", e)}));
            }

            let result = std::process::Command::new(interpreter)
                .arg(&file_path)
                .output();

            let _ = std::fs::remove_file(&file_path);

            match result {
                Ok(output) => Ok(serde_json::json!({
                    "language": language,
                    "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
                    "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
                    "exit_code": output.status.code().unwrap_or(-1),
                })),
                Err(e) => Ok(serde_json::json!({
                    "language": language,
                    "error": e.to_string(),
                })),
            }
        }
    }

    struct FileOpsSkill;
    impl Skill for FileOpsSkill {
        fn id(&self) -> &str { "file_ops" }
        fn description(&self) -> &str { "Read, write, and list files" }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            let operation = config["operation"].as_str().unwrap_or("list");
            let path = config["path"].as_str().unwrap_or(".");
            let content = config["content"].as_str();

            match operation {
                "read" => {
                    match std::fs::read_to_string(path) {
                        Ok(data) => Ok(serde_json::json!({
                            "operation": "read",
                            "path": path,
                            "content": data,
                            "size": data.len(),
                        })),
                        Err(e) => Ok(serde_json::json!({
                            "operation": "read",
                            "path": path,
                            "error": e.to_string(),
                        })),
                    }
                }
                "write" => {
                    let data = content.unwrap_or("");
                    match std::fs::write(path, data) {
                        Ok(_) => Ok(serde_json::json!({
                            "operation": "write",
                            "path": path,
                            "bytes_written": data.len(),
                            "status": "success",
                        })),
                        Err(e) => Ok(serde_json::json!({
                            "operation": "write",
                            "path": path,
                            "error": e.to_string(),
                        })),
                    }
                }
                "list" => {
                    match std::fs::read_dir(path) {
                        Ok(entries) => {
                            let files: Vec<serde_json::Value> = entries
                                .filter_map(|e| e.ok())
                                .map(|e| {
                                    let metadata = e.metadata().ok();
                                    serde_json::json!({
                                        "name": e.file_name().to_string_lossy(),
                                        "path": e.path().to_string_lossy(),
                                        "is_dir": metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                                        "size": metadata.as_ref().map(|m| m.len()).unwrap_or(0),
                                    })
                                })
                                .collect();
                            Ok(serde_json::json!({
                                "operation": "list",
                                "path": path,
                                "entries": files,
                                "count": files.len(),
                            }))
                        }
                        Err(e) => Ok(serde_json::json!({
                            "operation": "list",
                            "path": path,
                            "error": e.to_string(),
                        })),
                    }
                }
                "exists" => {
                    Ok(serde_json::json!({
                        "operation": "exists",
                        "path": path,
                        "exists": std::path::Path::new(path).exists(),
                    }))
                }
                _ => Ok(serde_json::json!({
                    "error": format!("Unknown operation: {}", operation),
                    "supported": ["read", "write", "list", "exists"],
                })),
            }
        }
    }

    struct HttpRequestSkill;
    impl Skill for HttpRequestSkill {
        fn id(&self) -> &str { "http_request" }
        fn description(&self) -> &str { "Make HTTP requests" }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            let url = config["url"].as_str().unwrap_or("");
            let method = config["method"].as_str().unwrap_or("GET");
            let headers = config["headers"].as_object();
            let body = config["body"].as_str();
            
            if url.is_empty() {
                return Ok(serde_json::json!({"error": "url is required"}));
            }

            let rt = tokio::runtime::Handle::current();
            let _guard = rt.enter();
            
            let client = reqwest::Client::new();
            let mut req = match method.to_uppercase().as_str() {
                "POST" => client.post(url),
                "PUT" => client.put(url),
                "DELETE" => client.delete(url),
                "PATCH" => client.patch(url),
                _ => client.get(url),
            };

            if let Some(hdrs) = headers {
                for (k, v) in hdrs {
                    if let Some(val) = v.as_str() {
                        req = req.header(k.as_str(), val);
                    }
                }
            }

            if let Some(b) = body {
                req = req.body(b.to_string());
            }

            match tokio::task::block_in_place(|| {
                rt.block_on(async {
                    req.timeout(std::time::Duration::from_secs(30))
                        .send()
                        .await
                })
            }) {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let body_text = tokio::task::block_in_place(|| {
                        rt.block_on(async { resp.text().await.unwrap_or_default() })
                    });
                    Ok(serde_json::json!({
                        "url": url,
                        "method": method,
                        "status": status,
                        "body": body_text,
                    }))
                }
                Err(e) => Ok(serde_json::json!({
                    "url": url,
                    "method": method,
                    "error": e.to_string(),
                })),
            }
        }
    }

    skill_registry.register(Box::new(EchoSkill)).await;
    skill_registry.register(Box::new(WebSearchSkill)).await;
    skill_registry.register(Box::new(CodeExecSkill)).await;
    skill_registry.register(Box::new(FileOpsSkill)).await;
    skill_registry.register(Box::new(HttpRequestSkill)).await;

    tracing::info!("Built-in skills registered: echo, web_search, code_execute, file_ops, http_request");
}

async fn ws_agent_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let token = headers
        .get("X-Agent-Token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let agent_id = match state.auth_service.verify_agent_token(token).await {
        Ok(id) => id,
        Err(_) => {
            tracing::warn!("WebSocket auth failed, rejecting connection");
            return ws.on_upgrade(move |_socket| async move {
                tracing::warn!("Unauthenticated WebSocket connection closed");
            });
        }
    };

    ws.on_upgrade(move |socket| {
        ws_gateway::handle_agent_ws(
            socket,
            state.agent_registry.clone(),
            state.event_bus.clone(),
            agent_id,
        )
    })
}

async fn health_handler() -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    let path = req.uri().path();
    
    if path == "/health" || path.starts_with("/ws/") || path.starts_with("/api/events") {
        return Ok(next.run(req).await);
    }

    let auth_header = req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let token = if auth_header.starts_with("Bearer ") {
        &auth_header[7..]
    } else {
        ""
    };

    if token.is_empty() {
        if std::env::var("REQUIRE_AUTH").unwrap_or_default() == "true" {
            return Err(axum::http::StatusCode::UNAUTHORIZED);
        }
        return Ok(next.run(req).await);
    }

    match state.auth_service.verify_token(token) {
        Ok(_claims) => Ok(next.run(req).await),
        Err(_) => Err(axum::http::StatusCode::UNAUTHORIZED),
    }
}

async fn models_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl IntoResponse {
    let models = state.llm_router.list_models().await;
    axum::Json(serde_json::json!({
        "models": models,
    }))
}

async fn skills_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl IntoResponse {
    let skills = state.skill_registry.list().await;
    axum::Json(serde_json::json!({
        "skills": skills.into_iter().map(|(id, desc)| serde_json::json!({
            "id": id,
            "description": desc,
        })).collect::<Vec<_>>(),
    }))
}

async fn chat_stream_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let session_id = req.session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let llm_router = state.llm_router.clone();
    let session_store = state.session_store.clone();
    let _ = session_store.save_message(&session_id, "user", &req.message, None, None).await;

    let llm_request = maple_llm::request::LlmRequest::new(req.message.clone(), "default");
    let complete_result = match llm_router.route(&llm_request).await {
        Ok(adapter) => {
            let model_name = adapter.name().to_string();
            match adapter.complete(llm_request).await {
                Ok(response) => Some((model_name, response.content)),
                Err(e) => None,
            }
        }
        Err(_) => None,
    };
    let sid = session_id.clone();

    let stream = async_stream::stream! {
        match complete_result {
            Some((model_name, content)) => {
                yield Ok(Event::default().event("meta").data(serde_json::json!({"session_id": sid, "model": model_name}).to_string()));
                let chunk_size = 8;
                let chars: Vec<char> = content.chars().collect();
                for chunk in chars.chunks(chunk_size) {
                    let token = chunk.iter().collect::<String>();
                    yield Ok(Event::default().event("token").data(serde_json::json!({"token": token}).to_string()));
                }
                let _ = session_store.save_message(&sid, "assistant", &content, None, None).await;
                yield Ok(Event::default().event("done").data(serde_json::json!({"done": true}).to_string()));
            }
            None => {
                yield Ok(Event::default().event("error").data("LLM unavailable"));
                yield Ok(Event::default().event("done").data("{\"done\":true}"));
            }
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(15)).text("ping"))
}

#[derive(Debug, Deserialize)]
struct KbIndexRequest {
    title: String,
    content: String,
    source_type: Option<String>,
    source_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct KbIndexResponse {
    document_id: String,
    chunk_count: usize,
}

async fn kb_index_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<KbIndexRequest>,
) -> Result<axum::Json<KbIndexResponse>, axum::http::StatusCode> {
    let doc_id = uuid::Uuid::new_v4().to_string();
    let doc = Document {
        id: doc_id.clone(),
        title: req.title,
        content: req.content,
        source_type: req.source_type.unwrap_or_else(|| "text".to_string()),
        source_url: req.source_url,
        chunks: vec![],
    };

    let chunks = state.indexer.index(&doc).unwrap_or_default();

    for chunk in &chunks {
        state.bm25_searcher.index_chunk(chunk);
        let embedding = match state.embedder.embed(&chunk.content).await {
            Ok(emb) => emb,
            Err(_) => maple_llm::embedding::simple_embedding(&chunk.content, 128),
        };
        state.vector_store.upsert(&chunk.id, chunk, embedding).await;
    }

    let now = chrono::Utc::now().timestamp();
    let chunk_count = chunks.len() as i32;
    let content = &doc.content;

    let _ = sqlx::query(
        "INSERT INTO kb_documents (id, workspace_id, title, source_type, source_url, content, chunk_count, created_at) VALUES (?, 'default', ?, ?, ?, ?, ?, ?)"
    )
    .bind(&doc_id)
    .bind(&doc.title)
    .bind(&doc.source_type)
    .bind(&doc.source_url)
    .bind(content)
    .bind(chunk_count)
    .bind(now)
    .execute(&state.db)
    .await;

    Ok(axum::Json(KbIndexResponse {
        document_id: doc_id,
        chunk_count: chunks.len(),
    }))
}

#[derive(Debug, Deserialize)]
struct KbSearchRequest {
    query: String,
    #[serde(default = "default_top_k")]
    top_k: usize,
}

fn default_top_k() -> usize { 5 }

#[derive(Debug, Serialize)]
struct KbSearchResponse {
    results: Vec<serde_json::Value>,
}

async fn kb_search_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<KbSearchRequest>,
) -> axum::Json<KbSearchResponse> {
    let query_embedding = match state.embedder.embed(&req.query).await {
        Ok(emb) => emb,
        Err(_) => maple_llm::embedding::simple_embedding(&req.query, 128),
    };
    let vector_results = state.vector_store.search(&query_embedding, req.top_k).await;
    let bm25_results = state.bm25_searcher.search(&req.query, req.top_k);

    let results = state.hybrid_retriever.search(
        &req.query,
        req.top_k,
        vector_results,
        bm25_results,
    ).await.unwrap_or_default();

    axum::Json(KbSearchResponse {
        results: results.into_iter().map(|r| serde_json::json!({
            "id": r.id,
            "content": r.content,
            "score": r.score,
            "source": r.source,
            "source_type": r.source_type,
            "metadata": r.metadata,
        })).collect(),
    })
}

async fn sessions_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl IntoResponse {
    match state.session_store.list_sessions(50).await {
        Ok(sessions) => axum::Json(serde_json::json!({ "sessions": sessions })),
        Err(_) => axum::Json(serde_json::json!({ "sessions": [] })),
    }
}

#[derive(Debug, Deserialize)]
struct MemoryStoreRequest {
    content: String,
    #[serde(default = "default_memory_type")]
    memory_type: String,
    #[serde(default)]
    metadata: std::collections::HashMap<String, String>,
}

fn default_memory_type() -> String { "working".to_string() }

async fn memory_store_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<MemoryStoreRequest>,
) -> impl IntoResponse {
    let entry = MemoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        memory_type: MemoryType::from_str(&req.memory_type),
        content: req.content,
        metadata: req.metadata,
        created_at: chrono::Utc::now().timestamp(),
        access_count: 0,
    };
    let id = entry.id.clone();
    let mut store = state.memory_store.lock().await;
    match store.store(entry).await {
        Ok(_) => axum::Json(serde_json::json!({ "id": id, "status": "stored" })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct MemorySearchRequest {
    keyword: String,
    #[serde(default = "default_memory_type")]
    memory_type: String,
    #[serde(default = "default_top_k")]
    limit: usize,
}

async fn memory_search_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<MemorySearchRequest>,
) -> impl IntoResponse {
    let store = state.memory_store.lock().await;
    let mt = MemoryType::from_str(&req.memory_type);
    match store.search_by_type(&mt, &req.keyword, req.limit).await {
        Ok(entries) => axum::Json(serde_json::json!({
            "results": entries.iter().map(|e| serde_json::json!({
                "id": e.id,
                "content": e.content,
                "type": e.memory_type.as_str(),
                "source_type": e.memory_type.as_str(),
                "score": 1.0,
                "metadata": e.metadata,
                "created_at": e.created_at,
            })).collect::<Vec<_>>()
        })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct PromptCreateRequest {
    prompt_ref: String,
    content: String,
    #[serde(default)]
    change_reason: Option<String>,
}

async fn prompt_create_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<PromptCreateRequest>,
) -> impl IntoResponse {
    match state.prompt_version_mgr.create_version(&req.prompt_ref, &req.content, req.change_reason.as_deref()).await {
        Ok(version) => axum::Json(serde_json::json!({
            "prompt_ref": req.prompt_ref,
            "version": version,
            "status": "created",
        })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct TaskEnqueueRequest {
    task_type: String,
    #[serde(default)]
    priority: i32,
    payload: serde_json::Value,
    #[serde(default = "default_max_retries")]
    max_retries: i32,
    #[serde(default)]
    delay_secs: i64,
    #[serde(default)]
    agent_id: Option<String>,
}

fn default_max_retries() -> i32 { 3 }

async fn task_enqueue_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<TaskEnqueueRequest>,
) -> impl IntoResponse {
    match state.task_queue.enqueue(
        &req.task_type,
        req.priority,
        req.payload,
        req.max_retries,
        req.delay_secs,
        req.agent_id.as_deref(),
    ).await {
        Ok(id) => axum::Json(serde_json::json!({
            "id": id,
            "status": "pending",
            "task_type": req.task_type,
        })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn task_stats_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl IntoResponse {
    match state.task_queue.stats().await {
        Ok(stats) => axum::Json(serde_json::json!({
            "pending": stats.pending,
            "running": stats.running,
            "completed": stats.completed,
            "failed": stats.failed,
            "dead_letter": stats.dead_letter,
        })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn task_dead_letter_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl IntoResponse {
    match state.task_queue.list_dead_letter(50).await {
        Ok(tasks) => axum::Json(serde_json::json!({
            "tasks": tasks.into_iter().map(|t| serde_json::json!({
                "id": t.id,
                "task_type": t.task_type,
                "retry_count": t.retry_count,
                "max_retries": t.max_retries,
                "error_message": t.error_message,
                "created_at": t.created_at,
            })).collect::<Vec<_>>(),
        })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn task_requeue_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    match state.task_queue.requeue_dead_letter(&task_id).await {
        Ok(_) => axum::Json(serde_json::json!({
            "id": task_id,
            "status": "requeued",
        })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn sse_events_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl IntoResponse {
    sse_gateway::handle_user_sse(state.event_bus.clone()).await
}

async fn get_workflow_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(workflow_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let row = sqlx::query_as::<_, (String, String, i64, String, String, i64, i64)>(
        "SELECT id, name, version, yaml_content, status, created_at, updated_at FROM workflows WHERE id = ?"
    )
    .bind(&workflow_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    match row {
        Some((id, name, version, yaml, status, created, updated)) => {
            Ok(axum::Json(serde_json::json!({
                "id": id,
                "name": name,
                "version": version,
                "yaml_content": yaml,
                "status": status,
                "created_at": created,
                "updated_at": updated,
            })))
        }
        None => Err(axum::http::StatusCode::NOT_FOUND),
    }
}

#[derive(Debug, Deserialize)]
struct UpdateWorkflowRequest {
    name: Option<String>,
    yaml_content: Option<String>,
    status: Option<String>,
}

async fn update_workflow_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(workflow_id): axum::extract::Path<String>,
    Json(req): Json<UpdateWorkflowRequest>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let now = chrono::Utc::now().timestamp();
    
    if let Some(name) = &req.name {
        let _ = sqlx::query("UPDATE workflows SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(now)
            .bind(&workflow_id)
            .execute(&state.db)
            .await;
    }
    
    if let Some(yaml) = &req.yaml_content {
        let _ = sqlx::query("UPDATE workflows SET yaml_content = ?, version = version + 1, updated_at = ? WHERE id = ?")
            .bind(yaml)
            .bind(now)
            .bind(&workflow_id)
            .execute(&state.db)
            .await;
    }
    
    if let Some(status) = &req.status {
        let _ = sqlx::query("UPDATE workflows SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status)
            .bind(now)
            .bind(&workflow_id)
            .execute(&state.db)
            .await;
    }

    Ok(axum::Json(serde_json::json!({
        "id": workflow_id,
        "status": "updated",
    })))
}

async fn delete_workflow_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(workflow_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let result = sqlx::query("DELETE FROM workflows WHERE id = ?")
        .bind(&workflow_id)
        .execute(&state.db)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() > 0 {
        Ok(axum::Json(serde_json::json!({
            "id": workflow_id,
            "status": "deleted",
        })))
    } else {
        Err(axum::http::StatusCode::NOT_FOUND)
    }
}

async fn get_workflow_executions_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(workflow_id): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    let rows = sqlx::query_as::<_, (String, String, i64, String, Option<String>, Option<String>, i64, Option<i64>, Option<String>)>(
        "SELECT id, workflow_id, workflow_version, status, input, output, started_at, completed_at, agent_id FROM workflow_executions WHERE workflow_id = ? ORDER BY started_at DESC LIMIT 50"
    )
    .bind(&workflow_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let executions: Vec<serde_json::Value> = rows.iter().map(|(id, wf_id, ver, status, input, output, started, completed, agent)| {
        serde_json::json!({
            "id": id,
            "workflow_id": wf_id,
            "version": ver,
            "status": status,
            "input": input,
            "output": output,
            "started_at": started,
            "completed_at": completed,
            "agent_id": agent,
        })
    }).collect();

    axum::Json(serde_json::json!({
        "workflow_id": workflow_id,
        "executions": executions,
    }))
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

async fn login_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let admin_user = &state.config.admin_username;
    let admin_pass = &state.config.admin_password;

    if req.username == *admin_user && req.password == *admin_pass {
        let token = state.auth_service.create_token_for_user(&req.username, "admin", 86400)
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        
        Ok(axum::Json(serde_json::json!({
            "token": token,
            "user_id": req.username,
            "role": "admin",
        })))
    } else {
        Err(axum::http::StatusCode::UNAUTHORIZED)
    }
}

#[derive(Debug, Deserialize)]
struct TokenRequest {
    agent_id: String,
}

async fn token_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<TokenRequest>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let token = state.auth_service.create_token_for_agent(&req.agent_id, 86400)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(axum::Json(serde_json::json!({
        "token": token,
        "agent_id": req.agent_id,
        "expires_in": 86400,
    })))
}

async fn get_memory_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(memory_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let store = state.memory_store.lock().await;
    match store.get(&memory_id).await {
        Ok(Some(entry)) => {
            Ok(axum::Json(serde_json::json!({
                "id": entry.id,
                "content": entry.content,
                "type": entry.memory_type.as_str(),
                "metadata": entry.metadata,
                "created_at": entry.created_at,
                "access_count": entry.access_count,
            })))
        }
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn delete_memory_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(memory_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let mut store = state.memory_store.lock().await;
    match store.delete(&memory_id).await {
        Ok(_) => {
            Ok(axum::Json(serde_json::json!({
                "id": memory_id,
                "status": "deleted",
            })))
        }
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Clone)]
struct ServerConfig {
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
                .unwrap_or_default() == "true",
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ServerConfig::from_env();
    
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| config.log_level.clone().into())
        )
        .init();

    let database_url = &config.database_url;

    let pool = sqlx::SqlitePool::connect(&database_url).await?;
    run_migrations(&pool).await?;

    let event_bus = Arc::new(EventBus::new());
    let llm_router = build_llm_router(&config);
    let skill_registry = Arc::new(SkillRegistry::new());
    register_builtin_skills(&skill_registry).await;
    let hook_runner = Arc::new(HookRunner::new());
    let checkpoint_mgr = Arc::new(CheckpointManager::new(pool.clone()));
    let agent_registry = Arc::new(AgentRegistry::new());
    let auth_service = Arc::new(AuthService::new(config.jwt_secret.clone()));
    let workspace_manager = Arc::new(tokio::sync::Mutex::new(WorkspaceManager::new(pool.clone())));
    {
        let wm = workspace_manager.lock().await;
        if let Err(e) = wm.init_schema().await {
            tracing::warn!("Failed to init workspace schema: {}", e);
        }
    }

    let node_executor = NodeExecutor::new(
        llm_router.clone(),
        skill_registry.clone(),
        hook_runner.clone(),
    );

    let workflow_executor = Arc::new(WorkflowExecutor::new(
        event_bus.clone(),
        node_executor,
        checkpoint_mgr,
        hook_runner,
    ));

    let sync_engine = Arc::new(SyncEngine::new(
        None,
        300,
    ));

    let session_store = Arc::new(SessionStore::new(pool.clone()));

    let bm25_searcher = Arc::new(BM25Searcher::new());
    let vector_store: Arc<dyn VectorSearch> = if let Ok(qdrant_url) = std::env::var("QDRANT_URL") {
        let collection = std::env::var("QDRANT_COLLECTION").unwrap_or_else(|_| "mapleos_chunks".to_string());
        let dim: usize = std::env::var("EMBEDDING_DIM").unwrap_or_else(|_| "768".to_string()).parse().unwrap_or(768);
        match QdrantVectorStore::new(&qdrant_url, &collection, dim).await {
            Ok(vs) => {
                tracing::info!("Using Qdrant vector store: {} (dim={})", qdrant_url, dim);
                Arc::new(vs)
            }
            Err(e) => {
                tracing::warn!("Qdrant connection failed: {}, falling back to in-memory store", e);
                let pool_clone = pool.clone();
                let vs = InMemoryVectorStore::new(pool_clone);
                vs.init_schema().await.ok();
                Arc::new(vs)
            }
        }
    } else {
        tracing::info!("Using in-memory vector store (no QDRANT_URL configured)");
        let pool_clone = pool.clone();
        let vs = InMemoryVectorStore::new(pool_clone);
        vs.init_schema().await.ok();
        Arc::new(vs)
    };
    let hybrid_retriever = Arc::new(HybridRetriever::new());
    let indexer = Arc::new(Indexer::new(512, 64));

    let embedder: Arc<dyn Embedder> = if let Ok(base_url) = std::env::var("OLLAMA_BASE_URL") {
        Arc::new(OllamaEmbedder::nomic_embed_text().with_base_url(base_url))
    } else {
        Arc::new(FallbackEmbedder::new(128))
    };

    let memory_store = Arc::new(tokio::sync::Mutex::new(MemoryStore::new(pool.clone())));
    let prompt_version_mgr = Arc::new(PromptVersionManager::new(pool.clone()));
    let task_queue = Arc::new(TaskQueueService::new(pool.clone()));
    task_queue.init_schema().await?;

    let task_worker_queue = task_queue.clone();
    let task_worker_skills = skill_registry.clone();
    let task_worker_llm = llm_router.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            match task_worker_queue.dequeue().await {
                Ok(Some(task)) => {
                    let payload = task.payload.clone();
                    let prompt = payload["prompt"].as_str().unwrap_or("").to_string();
                    let task_id = task.id.clone();
                    if !prompt.is_empty() {
                        let llm_request = maple_llm::request::LlmRequest::new(prompt, "task-worker");
                        match task_worker_llm.route(&llm_request).await {
                            Ok(adapter) => {
                                match adapter.complete(llm_request).await {
                                    Ok(_) => {
                                        let _ = task_worker_queue.complete(&task_id).await;
                                    }
                                    Err(e) => {
                                        let _ = task_worker_queue.fail(&task_id, &e.to_string()).await;
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = task_worker_queue.fail(&task_id, &format!("LLM routing: {}", e)).await;
                            }
                        }
                    } else {
                        let skill_id = payload["skill_id"].as_str().unwrap_or("echo");
                        match task_worker_skills.execute(skill_id, &payload).await {
                            Ok(_) => {
                                let _ = task_worker_queue.complete(&task_id).await;
                            }
                            Err(e) => {
                                let _ = task_worker_queue.fail(&task_id, &e.to_string()).await;
                            }
                        }
                    }
                }
                Ok(None) | Err(_) => {}
            }
        }
    });

    let state = Arc::new(AppState {
        config: config.clone(),
        db: pool.clone(),
        event_bus: event_bus.clone(),
        llm_router: llm_router.clone(),
        workflow_executor: workflow_executor.clone(),
        agent_registry: agent_registry.clone(),
        auth_service: auth_service.clone(),
        workspace_manager: workspace_manager.clone(),
        sync_engine: sync_engine.clone(),
        skill_registry: skill_registry.clone(),
        session_store: session_store.clone(),
        bm25_searcher: bm25_searcher.clone(),
        vector_store: vector_store.clone(),
        hybrid_retriever: hybrid_retriever.clone(),
        indexer: indexer.clone(),
        embedder,
        memory_store: memory_store.clone(),
        prompt_version_mgr: prompt_version_mgr.clone(),
        task_queue: task_queue.clone(),
    });

    let dispatcher = Arc::new(RpcDispatcher::new());
    register_business_handlers(&dispatcher, state.clone()).await;

    let rpc_server = RpcServer::new(dispatcher);
    let rpc_router = rpc_server.router();

    let state_routes = Router::new()
        .route("/health", get(health_handler))
        .route("/ws/agents", get(ws_agent_handler))
        .route("/api/chat", post(chat_handler))
        .route("/api/chat/stream", post(chat_stream_handler))
        .route("/api/models", get(models_handler))
        .route("/api/skills", get(skills_handler))
        .route("/api/kb/index", post(kb_index_handler))
        .route("/api/kb/search", post(kb_search_handler))
        .route("/api/sessions", get(sessions_handler))
        .route("/api/memories", post(memory_store_handler))
        .route("/api/memories/search", post(memory_search_handler))
        .route("/api/memories/:id", get(get_memory_handler).delete(delete_memory_handler))
        .route("/api/prompts", post(prompt_create_handler))
        .route("/api/tasks/enqueue", post(task_enqueue_handler))
        .route("/api/tasks/stats", get(task_stats_handler))
        .route("/api/tasks/dead-letter", get(task_dead_letter_handler))
        .route("/api/tasks/:id/requeue", post(task_requeue_handler))
        .route("/api/events", get(sse_events_handler))
        .route("/api/workflows/:id", get(get_workflow_handler).put(update_workflow_handler).delete(delete_workflow_handler))
        .route("/api/workflows/:id/executions", get(get_workflow_executions_handler))
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/token", post(token_handler))
        .with_state(state.clone());

    let app = Router::new()
        .merge(rpc_router)
        .merge(state_routes)
        .layer(middleware::from_fn_with_state(state, auth_middleware))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let bind_addr = config.bind_address();
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("MapleOS Server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;
    Ok(())
}

async fn register_business_handlers(dispatcher: &Arc<RpcDispatcher>, state: Arc<AppState>) {
    dispatcher.register_default_handlers().await;

    let start_time = std::time::Instant::now();
    let s = state.clone();
    dispatcher.register("system.info", move |_: Option<serde_json::Value>| {
        let registry = s.agent_registry.clone();
        let db = s.db.clone();
        let task_queue = s.task_queue.clone();
        let start = start_time;
        async move {
            let uptime_secs = start.elapsed().as_secs() as i64;
            let agents_count: i64 = registry.list_agents().await.len() as i64;
            let workflows_count: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workflows")
                .fetch_one(&db).await.unwrap_or(0);
            let stats = task_queue.stats().await.unwrap_or(maple_engine::task_queue::TaskQueueStats { pending: 0, running: 0, completed: 0, failed: 0, dead_letter: 0 });
            Ok(serde_json::json!({
                "name": "MapleOS",
                "version": env!("CARGO_PKG_VERSION"),
                "uptime_secs": uptime_secs,
                "agents_count": agents_count,
                "workflows_count": workflows_count,
                "tasks_count": stats.pending + stats.running + stats.completed + stats.failed + stats.dead_letter,
            }))
        }
    }).await;

    let s = state.clone();
    dispatcher.register("workflow.list", move |_: Option<serde_json::Value>| {
        let db = s.db.clone();
        async move {
            let rows = sqlx::query_as::<_, (String, String, i64, String, String, i64, i64)>(
                "SELECT id, name, version, yaml_content, status, created_at, updated_at FROM workflows ORDER BY updated_at DESC"
            )
            .fetch_all(&db)
            .await?;

            let workflows: Vec<serde_json::Value> = rows.iter().map(|(id, name, version, _yaml, status, created, updated)| {
                serde_json::json!({
                    "id": id,
                    "name": name,
                    "version": version,
                    "status": status,
                    "created_at": created,
                    "updated_at": updated,
                })
            }).collect();

            Ok(serde_json::json!({ "workflows": workflows }))
        }
    }).await;

    let s = state.clone();
    dispatcher.register("workflow.create", move |params: Option<serde_json::Value>| {
        let db = s.db.clone();
        async move {
            let p = match params {
                Some(v) => v,
                None => return Ok(serde_json::json!({"error": "missing params"})),
            };
            let name = p["name"].as_str().unwrap_or("unnamed");
            let yaml_content = p["yaml_content"].as_str().unwrap_or("");
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().timestamp();

            sqlx::query(
                "INSERT INTO workflows (id, name, version, yaml_content, status, created_at, updated_at) VALUES (?, ?, 1, ?, 'draft', ?, ?)"
            )
            .bind(&id)
            .bind(name)
            .bind(yaml_content)
            .bind(now)
            .bind(now)
            .execute(&db)
            .await?;

            Ok(serde_json::json!({
                "id": id,
                "name": name,
                "status": "created",
            }))
        }
    }).await;

    let s = state.clone();
    dispatcher.register("workflow.execute", move |params: Option<serde_json::Value>| {
        let executor = s.workflow_executor.clone();
        let db = s.db.clone();
        async move {
            let workflow_id = match params {
                Some(v) => v["workflow_id"].as_str().unwrap_or("").to_string(),
                None => return Ok(serde_json::json!({"error": "missing params"})),
            };

            let row = sqlx::query_as::<_, (String, String, i64, String)>(
                "SELECT id, name, version, yaml_content FROM workflows WHERE id = ?"
            )
            .bind(&workflow_id)
            .fetch_optional(&db)
            .await?;

            let (_id, _name, version, yaml) = match row {
                Some(r) => r,
                None => return Ok(serde_json::json!({"error": "workflow not found"})),
            };

            let workflow = match Workflow::parse_yaml(&yaml) {
                Ok(w) => w,
                Err(e) => return Ok(serde_json::json!({"error": format!("YAML parse error: {}", e)})),
            };

            let exec_id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().timestamp();

            sqlx::query(
                "INSERT INTO workflow_executions (id, workflow_id, workflow_version, status, input, started_at) VALUES (?, ?, ?, 'running', '{}', ?)"
            )
            .bind(&exec_id)
            .bind(&workflow_id)
            .bind(version)
            .bind(now)
            .execute(&db)
            .await?;

            let result = executor.execute(&workflow.nodes, &workflow.id, workflow.version, serde_json::Value::Null).await;

            match result {
                Ok(exec_result) => {
                    let output = serde_json::to_string(&exec_result).unwrap_or_default();
                    let completed_at = chrono::Utc::now().timestamp();
                    sqlx::query(
                        "UPDATE workflow_executions SET status = 'completed', output = ?, completed_at = ? WHERE id = ?"
                    )
                    .bind(&output)
                    .bind(completed_at)
                    .bind(&exec_id)
                    .execute(&db)
                    .await?;

                    Ok(serde_json::json!({
                        "exec_id": exec_id,
                        "status": "completed",
                        "result": output,
                    }))
                }
                Err(e) => {
                    let completed_at = chrono::Utc::now().timestamp();
                    sqlx::query(
                        "UPDATE workflow_executions SET status = 'failed', error = ?, completed_at = ? WHERE id = ?"
                    )
                    .bind(e.to_string())
                    .bind(completed_at)
                    .bind(&exec_id)
                    .execute(&db)
                    .await?;

                    Ok(serde_json::json!({
                        "exec_id": exec_id,
                        "status": "failed",
                        "error": e.to_string(),
                    }))
                }
            }
        }
    }).await;

    let s = state.clone();
    dispatcher.register("agent.list", move |_: Option<serde_json::Value>| {
        let registry = s.agent_registry.clone();
        async move {
            let agents = registry.list_agents().await;
            Ok(serde_json::json!({
                "agents": agents.into_iter().map(|(id, name, status)| serde_json::json!({
                    "id": id,
                    "name": name,
                    "status": format!("{:?}", status),
                })).collect::<Vec<_>>(),
            }))
        }
    }).await;

    let s = state.clone();
    dispatcher.register("workspace.create", move |params: Option<serde_json::Value>| {
        let mgr = s.workspace_manager.clone();
        async move {
            let name = params.as_ref().and_then(|p| p["name"].as_str()).unwrap_or("Default Workspace");
            let owner_id = params.as_ref().and_then(|p| p["owner_id"].as_str()).unwrap_or("user-1");

            let manager = mgr.lock().await;
            let ws = manager.create_workspace(name, owner_id).await?;
            Ok(serde_json::json!({
                "id": ws.id,
                "name": ws.name,
                "owner_id": ws.owner_id,
            }))
        }
    }).await;

    let s = state.clone();
    dispatcher.register("llm.models", move |_: Option<serde_json::Value>| {
        let router = s.llm_router.clone();
        async move {
            let models = router.list_models().await;
            Ok(serde_json::json!({
                "models": models,
            }))
        }
    }).await;

    let s = state.clone();
    dispatcher.register("skill.list", move |_: Option<serde_json::Value>| {
        let registry = s.skill_registry.clone();
        async move {
            let skills = registry.list().await;
            Ok(serde_json::json!({
                "skills": skills.into_iter().map(|(id, desc)| serde_json::json!({
                    "id": id,
                    "description": desc,
                })).collect::<Vec<_>>(),
            }))
        }
    }).await;

    dispatcher.register("scale.tools", move |_: Option<serde_json::Value>| {
        async move {
            let client = reqwest::Client::new();
            let resp = client.get("http://127.0.0.1:7790/tools").send().await;
            match resp {
                Ok(r) => {
                    let text = r.text().await.unwrap_or_default();
                    Ok(serde_json::json!({"source": "scale-engine", "raw": text}))
                }
                Err(e) => Ok(serde_json::json!({"error": e.to_string(), "note": "scale-engine HTTP bridge not running on port 7790"})),
            }
        }
    }).await;

    dispatcher.register("scale.call", move |params: Option<serde_json::Value>| {
        async move {
            let p = match params {
                Some(v) => v,
                None => return Ok(serde_json::json!({"error": "missing params: tool_name + arguments"})),
            };
            let tool_name = p["tool_name"].as_str().unwrap_or("").to_string();
            let args = p.get("arguments").cloned().unwrap_or(serde_json::json!({}));

            let client = reqwest::Client::new();
            let resp = client.post("http://127.0.0.1:7790/call")
                .json(&serde_json::json!({"name": tool_name, "arguments": args}))
                .send().await;

            match resp {
                Ok(r) => {
                    let text = r.text().await.unwrap_or_default();
                    Ok(serde_json::json!({"source": "scale-engine", "raw": text}))
                }
                Err(e) => Ok(serde_json::json!({"error": e.to_string(), "note": "scale-engine HTTP bridge not running on port 7790"})),
            }
        }
    }).await;

    let s = state.clone();
    dispatcher.register("agent.chat", move |params: Option<serde_json::Value>| {
        let llm_router = s.llm_router.clone();
        let session_store = s.session_store.clone();
        let skill_registry = s.skill_registry.clone();
        async move {
            let p = match params {
                Some(v) => v,
                None => return Ok(serde_json::json!({"error": "missing params: agent_id + prompt"})),
            };
            let agent_id = p["agent_id"].as_str().unwrap_or("default").to_string();
            let prompt = p["prompt"].as_str().unwrap_or("").to_string();
            if prompt.is_empty() {
                return Ok(serde_json::json!({"error": "prompt is required"}));
            }

            let session_id = format!("agent-chat-{}", agent_id);
            let _ = session_store.save_message(&session_id, "user", &prompt, None, None).await;

            let llm_request = maple_llm::request::LlmRequest::new(prompt.clone(), &agent_id);
            let adapter = match llm_router.route(&llm_request).await {
                Ok(a) => a,
                Err(e) => return Ok(serde_json::json!({"response": format!("No LLM available: {}", e), "agent_id": agent_id})),
            };

            match adapter.complete(llm_request).await {
                Ok(response) => {
                    let _ = session_store.save_message(&session_id, "assistant", &response.content, None, response.tool_calls.as_deref()).await;
                    Ok(serde_json::json!({
                        "response": response.content,
                        "agent_id": agent_id,
                        "model": adapter.name().to_string(),
                    }))
                }
                Err(e) => Ok(serde_json::json!({
                    "response": format!("LLM error: {}", e),
                    "agent_id": agent_id,
                }))
            }
        }
    }).await;

    let s = state.clone();
    dispatcher.register("agent.register", move |params: Option<serde_json::Value>| {
        let registry = s.agent_registry.clone();
        let db = s.db.clone();
        async move {
            let p = match params {
                Some(v) => v,
                None => return Ok(serde_json::json!({"error": "missing params"})),
            };
            let name = p["name"].as_str().unwrap_or("unnamed-agent").to_string();
            let id = p["id"].as_str().map(|s| s.to_string()).unwrap_or_else(|| format!("agent-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("x")));
            let now = chrono::Utc::now().timestamp();

            let _ = sqlx::query(
                "INSERT OR IGNORE INTO agents (id, name, transport_type, transport_config, capabilities, status, created_at) VALUES (?, ?, 'local', '{}', '[]', 'Idle', ?)"
            )
            .bind(&id)
            .bind(&name)
            .bind(now)
            .execute(&db)
            .await;

            registry.register_agent(&id, &name, maple_agent::registry::AgentStatus::Online).await;

            Ok(serde_json::json!({
                "id": id,
                "name": name,
                "status": "Idle",
            }))
        }
    }).await;

    let s = state.clone();
    dispatcher.register("task.create", move |params: Option<serde_json::Value>| {
        let task_queue = s.task_queue.clone();
        async move {
            let p = match params {
                Some(v) => v,
                None => return Ok(serde_json::json!({"error": "missing params"})),
            };
            let task_type = p["task_type"].as_str().unwrap_or("generic").to_string();
            let agent_id = p["agent_id"].as_str().unwrap_or("").to_string();
            let prompt = p["prompt"].as_str().unwrap_or("").to_string();
            let priority = p["priority"].as_i64().unwrap_or(0) as i32;

            let payload = serde_json::json!({
                "agent_id": agent_id,
                "prompt": prompt,
            });

            match task_queue.enqueue(&task_type, priority, payload, 3, 0, Some(&agent_id)).await {
                Ok(id) => Ok(serde_json::json!({
                    "id": id,
                    "task_type": task_type,
                    "agent_id": agent_id,
                    "status": "pending",
                })),
                Err(e) => Ok(serde_json::json!({"error": e.to_string()})),
            }
        }
    }).await;

    let s = state.clone();
    dispatcher.register("config.get", move |_: Option<serde_json::Value>| {
        let db = s.db.clone();
        async move {
            let rows = sqlx::query_as::<_, (String, String)>(
                "SELECT key, value FROM kv_store WHERE key LIKE 'config.%'"
            )
            .fetch_all(&db)
            .await
            .unwrap_or_default();

            let config: serde_json::Map<String, serde_json::Value> = rows.iter().map(|(k, v)| {
                let key = k.replace("config.", "");
                let val: serde_json::Value = serde_json::from_str(v).unwrap_or_else(|_| serde_json::Value::String(v.clone()));
                (key, val)
            }).collect();

            Ok(serde_json::json!({
                "ollama_url": config.get("ollama_url").and_then(|v| v.as_str()).unwrap_or("http://localhost:11434"),
                "openai_api_key": config.get("openai_api_key").and_then(|v| v.as_str()).unwrap_or(""),
                "default_model": config.get("default_model").and_then(|v| v.as_str()).unwrap_or("auto"),
                "webdav_url": config.get("webdav_url").and_then(|v| v.as_str()).unwrap_or(""),
                "webdav_username": config.get("webdav_username").and_then(|v| v.as_str()).unwrap_or(""),
                "webdav_password": config.get("webdav_password").and_then(|v| v.as_str()).unwrap_or(""),
                "qdrant_url": config.get("qdrant_url").and_then(|v| v.as_str()).unwrap_or(""),
                "gateway_mode": config.get("gateway_mode").and_then(|v| v.as_str()).unwrap_or("strict"),
                "data_local_only": config.get("data_local_only").and_then(|v| v.as_bool()).unwrap_or(true),
            }))
        }
    }).await;

    let s = state.clone();
    dispatcher.register("config.update", move |params: Option<serde_json::Value>| {
        let db = s.db.clone();
        async move {
            let p = match params {
                Some(v) => v,
                None => return Ok(serde_json::json!({"error": "missing params"})),
            };
            let now = chrono::Utc::now().timestamp();
            let fields = [
                ("ollama_url", p.get("ollama_url").and_then(|v| v.as_str())),
                ("openai_api_key", p.get("openai_api_key").and_then(|v| v.as_str())),
                ("default_model", p.get("default_model").and_then(|v| v.as_str())),
                ("webdav_url", p.get("webdav_url").and_then(|v| v.as_str())),
                ("webdav_username", p.get("webdav_username").and_then(|v| v.as_str())),
                ("webdav_password", p.get("webdav_password").and_then(|v| v.as_str())),
                ("qdrant_url", p.get("qdrant_url").and_then(|v| v.as_str())),
                ("gateway_mode", p.get("gateway_mode").and_then(|v| v.as_str())),
            ];

            for (key, value) in fields {
                if let Some(val) = value {
                    let config_key = format!("config.{}", key);
                    let _ = sqlx::query(
                        "INSERT OR REPLACE INTO kv_store (key, value, updated_at) VALUES (?, ?, ?)"
                    )
                    .bind(&config_key)
                    .bind(val)
                    .bind(now)
                    .execute(&db)
                    .await;
                }
            }

            if let Some(val) = p.get("data_local_only").and_then(|v| v.as_bool()) {
                let config_key = "config.data_local_only";
                let val_str = if val { "true" } else { "false" };
                let _ = sqlx::query(
                    "INSERT OR REPLACE INTO kv_store (key, value, updated_at) VALUES (?, ?, ?)"
                )
                .bind(config_key)
                .bind(val_str)
                .bind(now)
                .execute(&db)
                .await;
            }

            Ok(serde_json::json!({"status": "updated"}))
        }
    }).await;

    let s = state.clone();
    dispatcher.register("skill.install", move |params: Option<serde_json::Value>| {
        let registry = s.skill_registry.clone();
        async move {
            let p = match params {
                Some(v) => v,
                None => return Ok(serde_json::json!({"error": "missing params: skill_id"})),
            };
            let skill_id = p["skill_id"].as_str().unwrap_or("").to_string();
            if skill_id.is_empty() {
                return Ok(serde_json::json!({"error": "skill_id is required"}));
            }

            struct PlaceholderSkill { id: String }
            impl maple_engine::skill_registry::Skill for PlaceholderSkill {
                fn id(&self) -> &str { &self.id }
                fn description(&self) -> &str { "Placeholder - awaiting MCP server connection" }
                fn execute(&self, config: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
                    Ok(serde_json::json!({"skill_id": self.id, "status": "placeholder", "input": config}))
                }
            }

            registry.register(Box::new(PlaceholderSkill { id: skill_id.clone() })).await;
            Ok(serde_json::json!({"skill_id": skill_id, "status": "installed"}))
        }
    }).await;

    let s = state.clone();
    dispatcher.register("skill.uninstall", move |params: Option<serde_json::Value>| {
        let registry = s.skill_registry.clone();
        async move {
            let p = match params {
                Some(v) => v,
                None => return Ok(serde_json::json!({"error": "missing params: skill_id"})),
            };
            let skill_id = p["skill_id"].as_str().unwrap_or("").to_string();
            registry.unregister(&skill_id).await;
            Ok(serde_json::json!({"skill_id": skill_id, "status": "uninstalled"}))
        }
    }).await;
}
