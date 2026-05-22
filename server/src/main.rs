use axum::Json;
use axum::Router;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::IntoResponse;
use axum::routing::{get, post};

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
use maple_kb::vector_store::VectorStore;
use maple_kb::retriever::HybridRetriever;
use maple_kb::memory::{MemoryStore, MemoryEntry, MemoryType};
use maple_kb::prompt_version::PromptVersionManager;
use maple_llm::embedding::{Embedder, OllamaEmbedder, FallbackEmbedder};
use maple_collab::workspace::WorkspaceManager;
use serde::{Deserialize, Serialize};
use std::sync:: Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub struct AppState {
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
    pub vector_store: Arc<VectorStore>,
    pub hybrid_retriever: Arc<HybridRetriever>,
    pub indexer: Arc<Indexer>,
    pub embedder: Arc<dyn Embedder>,
    pub memory_store: Arc<tokio::sync::Mutex<MemoryStore>>,
    pub prompt_version_mgr: Arc<PromptVersionManager>,
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

    tracing::info!("Database migrations completed");
    Ok(())
}

fn build_llm_router() -> Arc<LlmRouter> {
    let usage_tracker = Arc::new(UsageTracker::new(50.0));
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
            Ok(serde_json::json!({
                "results": [],
                "query": query,
                "message": "Web search not yet configured - connect an MCP server for real search"
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
            Ok(serde_json::json!({
                "stdout": "",
                "stderr": format!("Code execution not available for {} in current environment", language),
                "exit_code": 1,
                "code_preview": &code[..code.len().min(200)]
            }))
        }
    }

    struct FileOpsSkill;
    impl Skill for FileOpsSkill {
        fn id(&self) -> &str { "file_ops" }
        fn description(&self) -> &str { "Read, write, and list files" }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            let operation = config["operation"].as_str().unwrap_or("list");
            let path = config["path"].as_str().unwrap_or(".");
            Ok(serde_json::json!({
                "operation": operation,
                "path": path,
                "result": "File operations require MCP server connection"
            }))
        }
    }

    struct HttpRequestSkill;
    impl Skill for HttpRequestSkill {
        fn id(&self) -> &str { "http_request" }
        fn description(&self) -> &str { "Make HTTP requests" }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            let url = config["url"].as_str().unwrap_or("");
            let method = config["method"].as_str().unwrap_or("GET");
            Ok(serde_json::json!({
                "url": url,
                "method": method,
                "status": 0,
                "body": "HTTP requests not available - use webhook node in workflows"
            }))
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
        state.vector_store.upsert_chunk(chunk, embedding).await;
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

async fn sse_events_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl IntoResponse {
    sse_gateway::handle_user_sse(state.event_bus.clone()).await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mapleos_server=debug,maple_engine=debug,maple_llm=debug".into())
        )
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:mapleos.db?mode=rwc".to_string());

    let pool = sqlx::SqlitePool::connect(&database_url).await?;
    run_migrations(&pool).await?;

    let event_bus = Arc::new(EventBus::new());
    let llm_router = build_llm_router();
    let skill_registry = Arc::new(SkillRegistry::new());
    register_builtin_skills(&skill_registry).await;
    let hook_runner = Arc::new(HookRunner::new());
    let checkpoint_mgr = Arc::new(CheckpointManager::new(pool.clone()));
    let agent_registry = Arc::new(AgentRegistry::new());
    let auth_service = Arc::new(AuthService::new(
        std::env::var("JWT_SECRET").unwrap_or_else(|_| "mapleos-dev-secret-change-me".to_string())
    ));
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
    let vector_store = Arc::new(VectorStore::new(pool.clone()));
    let hybrid_retriever = Arc::new(HybridRetriever::new());
    let indexer = Arc::new(Indexer::new(512, 64));

    let embedder: Arc<dyn Embedder> = if let Ok(base_url) = std::env::var("OLLAMA_BASE_URL") {
        Arc::new(OllamaEmbedder::nomic_embed_text().with_base_url(base_url))
    } else {
        Arc::new(FallbackEmbedder::new(128))
    };

    let memory_store = Arc::new(tokio::sync::Mutex::new(MemoryStore::new(pool.clone())));
    let prompt_version_mgr = Arc::new(PromptVersionManager::new(pool.clone()));

    let state = Arc::new(AppState {
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
    });

    let dispatcher = Arc::new(RpcDispatcher::new());
    register_business_handlers(&dispatcher, state.clone()).await;

    let rpc_server = RpcServer::new(dispatcher);
    let rpc_router = rpc_server.router();

    let state_routes = Router::new()
        .route("/health", get(health_handler))
        .route("/ws/agents", get(ws_agent_handler))
        .route("/api/chat", post(chat_handler))
        .route("/api/models", get(models_handler))
        .route("/api/skills", get(skills_handler))
        .route("/api/kb/index", post(kb_index_handler))
        .route("/api/kb/search", post(kb_search_handler))
        .route("/api/sessions", get(sessions_handler))
        .route("/api/memories", post(memory_store_handler))
        .route("/api/memories/search", post(memory_search_handler))
        .route("/api/prompts", post(prompt_create_handler))
        .route("/api/events", get(sse_events_handler))
        .with_state(state);

    let app = Router::new()
        .merge(rpc_router)
        .merge(state_routes)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:7788").await?;
    tracing::info!("MapleOS Server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;
    Ok(())
}

async fn register_business_handlers(dispatcher: &Arc<RpcDispatcher>, state: Arc<AppState>) {
    dispatcher.register_default_handlers().await;

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
}
