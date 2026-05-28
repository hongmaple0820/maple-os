mod cache;
mod config;
mod db;
mod metrics;
mod middleware;
mod sandbox;
mod skills;
mod state;

use axum::Json;
use axum::Router;
use axum::extract::ws::WebSocketUpgrade;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::sse::{Event, Sse};
use axum::routing::{delete, get, post, put};
use std::convert::Infallible;

use async_trait::async_trait;
use maple_agent::health::HealthMonitor;
use maple_agent::performance::PerformanceMonitor;
use maple_agent::react_loop::{ReactLoop, Session, ToolExecutor, ToolResult, ToolUse};
use maple_agent::registry::AgentRegistry;
use maple_agent::security::SecurityManager;
use maple_agent::session_store::SessionStore;
use maple_agent::tool_use_context::ToolUseContext;
use maple_collab::workspace::WorkspaceManager;
use maple_engine::checkpoint::CheckpointManager;
use maple_engine::event_bus::EventBus;
use maple_engine::executor::{NodeExecutor, WorkflowExecutor};
use maple_engine::hooks::HookRunner;
use maple_engine::scheduler::{ScheduledJob, Scheduler};
use maple_engine::skill_registry::SkillRegistry;
use maple_engine::task_queue::TaskQueueService;
use maple_engine::workflow::Workflow;
use maple_gateway::auth::AuthService;
use maple_gateway::mcp_host::McpHostManager;
use maple_gateway::sse_gateway;
use maple_gateway::ws_gateway;
use maple_kb::bm25::BM25Searcher;
use maple_kb::indexer::{Document, Indexer};
use maple_kb::memory::{MemoryEntry, MemoryStore, MemoryType};
use maple_kb::prompt_version::PromptVersionManager;
use maple_kb::retriever::HybridRetriever;
use maple_kb::vector_store::{InMemoryVectorStore, QdrantVectorStore, VectorSearch};
use maple_llm::embedding::{Embedder, FallbackEmbedder, OllamaEmbedder};
use maple_llm::router::LlmRouter;
use maple_llm::router::ProviderHealthChecker;
use maple_rpc::dispatch::RpcDispatcher;
use maple_rpc::server::RpcServer;
use maple_sync::sync_engine::SyncEngine;
use serde::{Deserialize, Serialize};
use state::{ApiError, AppState, ServerConfig};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

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
    #[serde(default)]
    agent_id: Option<String>,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    kb_sources: Vec<serde_json::Value>,
}

async fn chat_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Result<axum::Json<ChatResponse>, axum::http::StatusCode> {
    // 输入验证
    if req.message.trim().is_empty() {
        return Err(axum::http::StatusCode::BAD_REQUEST);
    }

    let session_id = req
        .session_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let _ = state
        .session_store
        .save_message(&session_id, "user", &req.message, None, None)
        .await;

    let route_key = if req.model != "auto" {
        req.model.as_str()
    } else {
        req.agent_id.as_deref().unwrap_or("default")
    };

    let mut enhanced_message = req.message.clone();
    let mut kb_sources: Vec<serde_json::Value> = Vec::new();
    {
        let query_embedding = match state.embedder.embed(&req.message).await {
            Ok(emb) => emb,
            Err(_) => maple_llm::embedding::simple_embedding(&req.message, 128),
        };
        let vector_results = state.vector_store.search(&query_embedding, 3).await;
        let bm25_results = state.bm25_searcher.search(&req.message, 3);
        let kb_results = state
            .hybrid_retriever
            .search(&req.message, 3, vector_results, bm25_results)
            .await
            .unwrap_or_default();

        if !kb_results.is_empty() {
            let mut context_parts = Vec::new();
            for r in &kb_results {
                context_parts.push(r.content.clone());
                let snippet = if r.content.len() > 200 {
                    r.content[..200].to_string() + "..."
                } else {
                    r.content.clone()
                };
                kb_sources.push(serde_json::json!({
                    "id": r.id,
                    "snippet": snippet,
                    "score": r.score,
                    "source": r.source,
                    "source_type": r.source_type,
                }));
            }
            let kb_context = context_parts.join("\n---\n");
            enhanced_message = format!(
                "[Knowledge Base Context]\n{}\n---\n[User Question]\n{}",
                kb_context, req.message
            );
        }
    }

    // Inject relevant memories from previous sessions
    let memory_scope = maple_agent::MemoryScope::User(session_id.clone());
    let memory_mgr = maple_agent::MemoryManager::new(state.memory_store.clone());
    match memory_mgr
        .build_context_injection(&req.message, Some(&memory_scope))
        .await
    {
        Ok(memory_context) if !memory_context.is_empty() => {
            enhanced_message = format!("{}\n---\n{}", memory_context, enhanced_message);
        }
        _ => {}
    }

    let llm_request = maple_llm::request::LlmRequest::new(enhanced_message, route_key);

    let adapter = match state.llm_router.route(&llm_request).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("LLM routing failed: {}", e);
            return Ok(axum::Json(ChatResponse {
                reply: format!("No LLM model available: {}", e),
                model: None,
                tool_calls: None,
                session_id,
                kb_sources: Vec::new(),
            }));
        }
    };

    let model_name = adapter.name().to_string();

    if req.tools.is_empty() {
        match adapter.complete(llm_request).await {
            Ok(response) => {
                let _ = state
                    .session_store
                    .save_message(
                        &session_id,
                        "assistant",
                        &response.content,
                        None,
                        response.tool_calls.as_deref(),
                    )
                    .await;
                Ok(axum::Json(ChatResponse {
                    reply: response.content,
                    model: Some(model_name),
                    tool_calls: response.tool_calls,
                    session_id,
                    kb_sources,
                }))
            }
            Err(e) => {
                tracing::error!("LLM complete failed: {}", e);
                Ok(axum::Json(ChatResponse {
                    reply: format!("LLM error: {}", e),
                    model: Some(model_name),
                    tool_calls: None,
                    session_id,
                    kb_sources: Vec::new(),
                }))
            }
        }
    } else {
        // Wire in ToolUseContext, SecurityManager, and PerformanceMonitor
        let tool_ctx = ToolUseContext::new(&session_id, std::path::PathBuf::from("."));
        let security_mgr = SecurityManager::new(Default::default());
        let perf_monitor = PerformanceMonitor::new();

        let react_loop = ReactLoop::new(10)
            .with_tool_use_context(tool_ctx)
            .with_security_manager(security_mgr)
            .with_performance_monitor(perf_monitor);

        // Merge MCP-discovered tools into the tool list
        let mcp_tool_defs = state.mcp_host.as_tool_definitions();
        let mut all_tools = req.tools;
        if !mcp_tool_defs.is_empty() {
            tracing::info!(mcp_tool_count = mcp_tool_defs.len(), "MCP tools available");
            all_tools.extend(mcp_tool_defs);
        }

        struct AppToolExecutor {
            skill_registry: Arc<SkillRegistry>,
            mcp_host: Arc<McpHostManager>,
        }

        #[async_trait]
        impl ToolExecutor for AppToolExecutor {
            async fn execute(&self, tool_use: &ToolUse) -> anyhow::Result<ToolResult> {
                // Route MCP tool calls to McpHostManager
                if tool_use.name.starts_with("mcp__") {
                    if let Some((server_name, raw_name)) =
                        self.mcp_host.resolve_tool_name(&tool_use.name)
                    {
                        match self
                            .mcp_host
                            .call_tool(&server_name, &raw_name, tool_use.input.clone())
                            .await
                        {
                            Ok(v) => {
                                return Ok(ToolResult::success(&tool_use.id, &tool_use.name, v));
                            }
                            Err(e) => {
                                return Ok(ToolResult::error(
                                    &tool_use.id,
                                    &tool_use.name,
                                    &e.to_string(),
                                ));
                            }
                        }
                    }
                    return Ok(ToolResult::error(
                        &tool_use.id,
                        &tool_use.name,
                        &format!("MCP tool not found: {}", tool_use.name),
                    ));
                }
                // Route all other tools to skill registry
                match self
                    .skill_registry
                    .execute(&tool_use.name, &tool_use.input)
                    .await
                {
                    Ok(v) => Ok(ToolResult::success(&tool_use.id, &tool_use.name, v)),
                    Err(e) => Ok(ToolResult::error(
                        &tool_use.id,
                        &tool_use.name,
                        &e.to_string(),
                    )),
                }
            }
        }

        let mut session = state
            .session_store
            .load_session(&session_id)
            .await
            .unwrap_or_else(|_| {
                Session::new("You are a helpful assistant. Use the provided tools when needed.")
            });
        let tool_executor = AppToolExecutor {
            skill_registry: state.skill_registry.clone(),
            mcp_host: state.mcp_host.clone(),
        };

        match react_loop
            .run_turn(
                adapter,
                &tool_executor,
                &mut session,
                &req.message,
                all_tools,
            )
            .await
        {
            Ok(summary) => {
                let _ = state
                    .session_store
                    .save_session_messages(&session_id, &session)
                    .await;

                // Extract memories from this conversation turn
                let extract_scope = maple_agent::MemoryScope::User(session_id.clone());
                let _ = memory_mgr
                    .extract_from_turn(&req.message, &summary.content, &extract_scope)
                    .await;

                Ok(axum::Json(ChatResponse {
                    reply: summary.content,
                    model: Some(model_name),
                    tool_calls: None,
                    session_id,
                    kb_sources: Vec::new(),
                }))
            }
            Err(e) => {
                tracing::error!("ReAct loop failed: {}", e);
                Ok(axum::Json(ChatResponse {
                    reply: format!("Agent error: {}", e),
                    model: Some(model_name),
                    tool_calls: None,
                    session_id,
                    kb_sources: Vec::new(),
                }))
            }
        }
    }
}

async fn ws_agent_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let token = headers
        .get("X-Agent-Token")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            headers
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
        })
        .or_else(|| params.get("token").map(|s| s.as_str()))
        .unwrap_or("");

    let agent_id = match state.auth_service.verify_agent_token(token).await {
        Ok(id) => id,
        Err(_) => {
            if state.config.read().await.require_auth {
                tracing::warn!("WebSocket auth failed, rejecting connection");
                return ws.on_upgrade(move |_socket| async move {
                    tracing::warn!("Unauthenticated WebSocket connection closed");
                });
            }
            "anonymous-agent".to_string()
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
    // 尝试从缓存获取
    let cache_key = "models_list".to_string();
    let cached: Option<Vec<serde_json::Value>> = state.cache.models.get(&cache_key);
    if let Some(cached) = cached {
        return axum::Json(serde_json::json!({
            "models": cached,
            "cached": true,
        }));
    }

    // 缓存未命中，从LLM路由获取
    let models = state.llm_router.list_models().await;
    let models_json: Vec<serde_json::Value> =
        models.into_iter().map(|m| serde_json::json!(m)).collect();

    // 存入缓存
    state.cache.models.insert(cache_key, models_json.clone());

    axum::Json(serde_json::json!({
        "models": models_json,
        "cached": false,
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
    let session_id = req
        .session_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let llm_router = state.llm_router.clone();
    let session_store = state.session_store.clone();
    let _ = session_store
        .save_message(&session_id, "user", &req.message, None, None)
        .await;

    let route_key = if req.model != "auto" {
        &req.model
    } else {
        req.agent_id.as_deref().unwrap_or("default")
    };

    let mut enhanced_message = req.message.clone();
    let kb_sources_json: Vec<serde_json::Value> = {
        let query_embedding = match state.embedder.embed(&req.message).await {
            Ok(emb) => emb,
            Err(_) => maple_llm::embedding::simple_embedding(&req.message, 128),
        };
        let vector_results = state.vector_store.search(&query_embedding, 3).await;
        let bm25_results = state.bm25_searcher.search(&req.message, 3);
        let kb_results = state
            .hybrid_retriever
            .search(&req.message, 3, vector_results, bm25_results)
            .await
            .unwrap_or_default();

        if !kb_results.is_empty() {
            let mut context_parts = Vec::new();
            let mut sources = Vec::new();
            for r in &kb_results {
                context_parts.push(r.content.clone());
                let snippet = if r.content.len() > 200 {
                    r.content[..200].to_string() + "..."
                } else {
                    r.content.clone()
                };
                sources.push(serde_json::json!({
                    "id": r.id,
                    "snippet": snippet,
                    "score": r.score,
                    "source": r.source,
                    "source_type": r.source_type,
                }));
            }
            let kb_context = context_parts.join("\n---\n");
            enhanced_message = format!(
                "[Knowledge Base Context]\n{}\n---\n[User Question]\n{}",
                kb_context, req.message
            );
            sources
        } else {
            Vec::new()
        }
    };

    let llm_request = maple_llm::request::LlmRequest::new(enhanced_message, route_key);
    let sid = session_id.clone();
    let evolver = state.evolver.clone();

    let stream = async_stream::stream! {
        if !kb_sources_json.is_empty() {
            yield Ok(Event::default().event("kb_sources").data(serde_json::json!({"sources": kb_sources_json}).to_string()));
        }
        let stream_result = llm_router.route(&llm_request).await;
        match stream_result {
            Ok(adapter) => {
                let model_name = adapter.name().to_string();
                yield Ok(Event::default().event("meta").data(serde_json::json!({"session_id": sid, "model": model_name}).to_string()));

                match adapter.stream(llm_request).await {
                    Ok(mut llm_stream) => {
                        let mut full_content = String::new();
                        loop {
                            match llm_stream.next_chunk().await {
                                Ok(Some(chunk)) => {
                                    if !chunk.delta.is_empty() {
                                        full_content.push_str(&chunk.delta);
                                        yield Ok(Event::default().event("token").data(serde_json::json!({"token": chunk.delta}).to_string()));
                                    }
                                    if chunk.finish_reason.is_some() {
                                        break;
                                    }
                                }
                                Ok(None) => break,
                                Err(e) => {
                                    yield Ok(Event::default().event("error").data(format!("Stream error: {}", e)));
                                    break;
                                }
                            }
                        }
                        let _ = session_store.save_message(&sid, "assistant", &full_content, None, None).await;
                        // Fire-and-forget: extract knowledge from valuable conversations
                        let evolver_for_bg = evolver.clone();
                        let sid_for_bg = sid.clone();
                        let user_msg_for_bg = req.message.clone();
                        let assistant_msg_for_bg = full_content.clone();
                        tokio::spawn(async move {
                            if let Err(e) = evolver_for_bg.on_chat_complete(&sid_for_bg, &user_msg_for_bg, &assistant_msg_for_bg).await {
                                tracing::warn!(error = %e, "Chat knowledge precipitation failed");
                            }
                        });
                        yield Ok(Event::default().event("done").data(serde_json::json!({"done": true}).to_string()));
                    }
                    Err(e) => {
                        yield Ok(Event::default().event("error").data(format!("Stream init error: {}", e)));
                        yield Ok(Event::default().event("done").data("{\"done\":true}"));
                    }
                }
            }
            Err(e) => {
                yield Ok(Event::default().event("error").data(format!("No LLM available: {}", e)));
                yield Ok(Event::default().event("done").data("{\"done\":true}"));
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}

#[allow(dead_code)]
async fn kb_upload_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    mut multipart: axum::extract::Multipart,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let mut results = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?
    {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name != "file" {
            continue;
        }

        let filename = field.file_name().unwrap_or("untitled").to_string();
        let bytes = field
            .bytes()
            .await
            .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
        let content = String::from_utf8_lossy(&bytes).to_string();

        let source_type = if filename.ends_with(".pdf") {
            "pdf"
        } else if filename.ends_with(".md") {
            "markdown"
        } else if filename.ends_with(".txt") {
            "text"
        } else {
            "file"
        };

        let doc_id = uuid::Uuid::new_v4().to_string();
        let doc = Document {
            id: doc_id.clone(),
            title: filename.clone(),
            content: content.clone(),
            source_type: source_type.to_string(),
            source_url: None,
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
        let _ = sqlx::query(
            "INSERT INTO kb_documents (id, workspace_id, title, source_type, source_url, content, chunk_count, created_at) VALUES (?, 'default', ?, ?, NULL, ?, ?, ?)"
        )
        .bind(&doc_id)
        .bind(&filename)
        .bind(source_type)
        .bind(&content)
        .bind(chunk_count)
        .bind(now)
        .execute(&state.db)
        .await;

        results.push(serde_json::json!({
            "document_id": doc_id,
            "filename": filename,
            "chunk_count": chunks.len(),
            "source_type": source_type,
        }));
    }

    Ok(axum::Json(serde_json::json!({ "uploaded": results })))
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

fn default_top_k() -> usize {
    5
}

#[derive(Debug, Serialize)]
struct KbSearchResponse {
    results: Vec<serde_json::Value>,
}

#[allow(dead_code)]
async fn kb_documents_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    let rows: Vec<(String, String, String, i32, i64)> = sqlx::query_as(
        "SELECT id, title, source_type, chunk_count, created_at FROM kb_documents ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    axum::Json(serde_json::json!({
        "documents": rows.into_iter().map(|(id, title, source_type, chunk_count, created_at)| {
            serde_json::json!({
                "id": id,
                "title": title,
                "source_type": source_type,
                "chunk_count": chunk_count,
                "created_at": created_at,
            })
        }).collect::<Vec<_>>()
    }))
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

    let results = state
        .hybrid_retriever
        .search(&req.query, req.top_k, vector_results, bm25_results)
        .await
        .unwrap_or_default();

    let doc_rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, title, source_type FROM kb_documents")
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();
    let doc_map: std::collections::HashMap<String, (String, String)> = doc_rows
        .into_iter()
        .map(|(id, title, st)| (id, (title, st)))
        .collect();

    axum::Json(KbSearchResponse {
        results: results
            .into_iter()
            .map(|r| {
                let snippet = if r.content.len() > 200 {
                    r.content[..200].to_string() + "..."
                } else {
                    r.content.clone()
                };
                let doc_id = r.source.strip_prefix("document:").unwrap_or(&r.id);
                let (title, doc_source_type) = doc_map
                    .get(doc_id)
                    .cloned()
                    .unwrap_or_else(|| (r.id.clone(), r.source_type.clone()));
                serde_json::json!({
                    "id": r.id,
                    "title": title,
                    "content": r.content,
                    "snippet": snippet,
                    "score": r.score,
                    "source": r.source,
                    "source_type": doc_source_type,
                    "metadata": r.metadata,
                })
            })
            .collect(),
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

fn default_memory_type() -> String {
    "working".to_string()
}

async fn memory_store_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<MemoryStoreRequest>,
) -> impl IntoResponse {
    let entry = MemoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        memory_type: req
            .memory_type
            .parse::<MemoryType>()
            .unwrap_or(MemoryType::Working),
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
    let mt = req
        .memory_type
        .parse::<MemoryType>()
        .unwrap_or(MemoryType::Working);
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
    match state
        .prompt_version_mgr
        .create_version(&req.prompt_ref, &req.content, req.change_reason.as_deref())
        .await
    {
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

fn default_max_retries() -> i32 {
    3
}

async fn task_enqueue_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<TaskEnqueueRequest>,
) -> impl IntoResponse {
    match state
        .task_queue
        .enqueue(
            &req.task_type,
            req.priority,
            req.payload,
            req.max_retries,
            req.delay_secs,
            req.agent_id.as_deref(),
        )
        .await
    {
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
            "total": stats.pending + stats.running + stats.completed + stats.failed + stats.dead_letter,
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

async fn get_execution_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(exec_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let row = sqlx::query_as::<_, (String, String, i64, String, Option<String>, Option<String>, i64, Option<i64>, Option<String>)>(
        "SELECT id, workflow_id, workflow_version, status, input, output, started_at, completed_at, agent_id FROM workflow_executions WHERE id = ?"
    )
    .bind(&exec_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    match row {
        Some((id, wf_id, ver, status, input, output, started, completed, agent)) => {
            let checkpoints = sqlx::query_as::<_, (String, String, String, i64)>(
                "SELECT exec_id, node_id, output, created_at FROM checkpoints WHERE exec_id = ? ORDER BY created_at"
            )
            .bind(&exec_id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

            Ok(axum::Json(serde_json::json!({
                "id": id,
                "workflow_id": wf_id,
                "version": ver,
                "status": status,
                "input": input,
                "output": output,
                "started_at": started,
                "completed_at": completed,
                "agent_id": agent,
                "checkpoints": checkpoints.iter().map(|(_, node_id, output, created)| serde_json::json!({
                    "node_id": node_id,
                    "output": output,
                    "created_at": created,
                })).collect::<Vec<_>>(),
            })))
        }
        None => Err(axum::http::StatusCode::NOT_FOUND),
    }
}

async fn get_checkpoints_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(exec_id): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    let checkpoints = sqlx::query_as::<_, (String, String, String, i64)>(
        "SELECT exec_id, node_id, output, created_at FROM checkpoints WHERE exec_id = ? ORDER BY created_at"
    )
    .bind(&exec_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    axum::Json(serde_json::json!({
        "exec_id": exec_id,
        "checkpoints": checkpoints.iter().map(|(_, node_id, output, created)| serde_json::json!({
            "node_id": node_id,
            "output": output,
            "created_at": created,
        })).collect::<Vec<_>>(),
    }))
}

async fn workflow_stats_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(workflow_id): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM workflow_executions WHERE workflow_id = ?")
            .bind(&workflow_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

    let completed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM workflow_executions WHERE workflow_id = ? AND status = 'completed'",
    )
    .bind(&workflow_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let failed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM workflow_executions WHERE workflow_id = ? AND status = 'failed'",
    )
    .bind(&workflow_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let avg_duration: Option<f64> = sqlx::query_scalar(
        "SELECT AVG(CAST((completed_at - started_at) AS REAL)) FROM workflow_executions WHERE workflow_id = ? AND status = 'completed' AND completed_at IS NOT NULL"
    )
    .bind(&workflow_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(None);

    axum::Json(serde_json::json!({
        "workflow_id": workflow_id,
        "total_executions": total,
        "completed": completed,
        "failed": failed,
        "success_rate": if total > 0 { (completed as f64 / total as f64 * 100.0).round() } else { 0.0 },
        "avg_duration_secs": avg_duration.map(|d| d.round()),
    }))
}

async fn system_metrics_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    let db_ok = sqlx::query("SELECT 1").execute(&state.db).await.is_ok();
    let agents = state.agent_registry.list_agents().await;
    let tasks = state.task_queue.stats().await.unwrap_or_default();

    let total_workflows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workflows")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let total_executions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workflow_executions")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let total_sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_messages")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let total_memories: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let total_documents: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kb_documents")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    axum::Json(serde_json::json!({
        "status": if db_ok { "healthy" } else { "degraded" },
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "database": {
            "connected": db_ok,
            "workflows": total_workflows,
            "executions": total_executions,
            "sessions": total_sessions,
            "memories": total_memories,
            "documents": total_documents,
        },
        "agents": {
            "total": agents.len(),
            "online": agents.iter().filter(|(_, _, s)| *s == maple_agent::registry::AgentStatus::Online).count(),
            "offline": agents.iter().filter(|(_, _, s)| *s == maple_agent::registry::AgentStatus::Offline).count(),
            "busy": agents.iter().filter(|(_, _, s)| *s == maple_agent::registry::AgentStatus::Busy).count(),
        },
        "tasks": {
            "pending": tasks.pending,
            "running": tasks.running,
            "completed": tasks.completed,
            "failed": tasks.failed,
            "dead_letter": tasks.dead_letter,
        },
    }))
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

    let executions: Vec<serde_json::Value> = rows
        .iter()
        .map(
            |(id, wf_id, ver, status, input, output, started, completed, agent)| {
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
            },
        )
        .collect();

    axum::Json(serde_json::json!({
        "workflow_id": workflow_id,
        "executions": executions,
    }))
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    username: String,
    password: String,
    email: Option<String>,
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
    let config = state.config.read().await;
    let admin_user = &config.admin_username;
    let admin_pass = &config.admin_password;

    // First try admin login with plaintext password comparison
    if req.username == *admin_user && req.password == *admin_pass {
        let user_id = format!("admin-{}", req.username);
        let token = state
            .auth_service
            .create_token_for_user(&user_id, "admin", 3600)
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        let refresh_token = uuid::Uuid::new_v4().to_string();
        let refresh_hash = bcrypt::hash(&refresh_token, bcrypt::DEFAULT_COST)
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        let now = chrono::Utc::now().timestamp();
        let _ = sqlx::query(
            "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, created_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&user_id)
        .bind(&refresh_hash)
        .bind(now + 7 * 86400)
        .bind(now)
        .execute(&state.db)
        .await;

        return Ok(axum::Json(serde_json::json!({
            "token": token,
            "refresh_token": refresh_token,
            "user_id": user_id,
            "username": req.username,
            "role": "admin",
            "expires_in": 3600,
        })));
    }

    // If admin login fails, try database user login
    let user_row: Option<(String, String, String, String)> =
        sqlx::query_as("SELECT id, username, password_hash, role FROM users WHERE username = ?")
            .bind(&req.username)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some((user_id, username, password_hash, role)) = user_row {
        // Verify password with bcrypt
        if bcrypt::verify(&req.password, &password_hash).unwrap_or(false) {
            let token = state
                .auth_service
                .create_token_for_user(&user_id, &role, 3600)
                .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
            let refresh_token = uuid::Uuid::new_v4().to_string();
            let refresh_hash = bcrypt::hash(&refresh_token, bcrypt::DEFAULT_COST)
                .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
            let now = chrono::Utc::now().timestamp();
            let _ = sqlx::query(
                "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, created_at) VALUES (?, ?, ?, ?, ?)"
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&user_id)
            .bind(&refresh_hash)
            .bind(now + 7 * 86400)
            .bind(now)
            .execute(&state.db)
            .await;

            return Ok(axum::Json(serde_json::json!({
                "token": token,
                "refresh_token": refresh_token,
                "user_id": user_id,
                "username": username,
                "role": role,
                "expires_in": 3600,
            })));
        }
    }

    // Neither admin nor database user credentials matched
    Err(axum::http::StatusCode::UNAUTHORIZED)
}

async fn register_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    if req.username.trim().is_empty() || req.password.len() < 6 {
        return Ok(axum::Json(serde_json::json!({
            "error": "Username required, password min 6 chars"
        })));
    }

    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE username = ?")
        .bind(&req.username)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    if existing.is_some() {
        return Ok(axum::Json(serde_json::json!({
            "error": "Username already exists"
        })));
    }

    let user_id = uuid::Uuid::new_v4().to_string();
    let password_hash = bcrypt::hash(&req.password, bcrypt::DEFAULT_COST)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        "INSERT INTO users (id, username, password_hash, email, role, created_at) VALUES (?, ?, ?, ?, 'user', ?)"
    )
    .bind(&user_id)
    .bind(&req.username)
    .bind(&password_hash)
    .bind(&req.email)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let access_token = state
        .auth_service
        .create_token_for_user(&user_id, "user", 3600)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(axum::Json(serde_json::json!({
        "token": access_token,
        "user_id": user_id,
        "username": req.username,
        "role": "user",
        "expires_in": 3600,
    })))
}

#[derive(Debug, Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

async fn refresh_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let now = chrono::Utc::now().timestamp();
    let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT id, user_id, token_hash, expires_at FROM refresh_tokens WHERE expires_at > ?",
    )
    .bind(now)
    .fetch_all(&state.db)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut matched: Option<(String, String)> = None;
    for (_id, user_id, token_hash, _expires) in &rows {
        if bcrypt::verify(&req.refresh_token, token_hash).unwrap_or(false) {
            matched = Some((user_id.clone(), _id.clone()));
            break;
        }
    }

    if let Some((user_id, token_id)) = matched {
        let user_row: Option<(String, String)> =
            sqlx::query_as("SELECT username, role FROM users WHERE id = ?")
                .bind(&user_id)
                .fetch_optional(&state.db)
                .await
                .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

        let (username, role) = match user_row {
            Some(r) => r,
            None => return Err(axum::http::StatusCode::UNAUTHORIZED),
        };

        let access_token = state
            .auth_service
            .create_token_for_user(&user_id, &role, 3600)
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

        let new_refresh = uuid::Uuid::new_v4().to_string();
        let new_hash = bcrypt::hash(&new_refresh, bcrypt::DEFAULT_COST)
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

        let _ = sqlx::query("DELETE FROM refresh_tokens WHERE id = ?")
            .bind(&token_id)
            .execute(&state.db)
            .await;

        let _ = sqlx::query(
            "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, created_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&user_id)
        .bind(&new_hash)
        .bind(now + 7 * 86400)
        .bind(now)
        .execute(&state.db)
        .await;

        Ok(axum::Json(serde_json::json!({
            "token": access_token,
            "refresh_token": new_refresh,
            "user_id": user_id,
            "username": username,
            "role": role,
            "expires_in": 3600,
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
    let token = state
        .auth_service
        .create_token_for_agent(&req.agent_id, 86400)
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
        Ok(Some(entry)) => Ok(axum::Json(serde_json::json!({
            "id": entry.id,
            "content": entry.content,
            "type": entry.memory_type.as_str(),
            "metadata": entry.metadata,
            "created_at": entry.created_at,
            "access_count": entry.access_count,
        }))),
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
        Ok(_) => Ok(axum::Json(serde_json::json!({
            "id": memory_id,
            "status": "deleted",
        }))),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn delete_session_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    match state.session_store.delete_session(&session_id).await {
        Ok(true) => Ok(axum::Json(serde_json::json!({
            "session_id": session_id,
            "status": "deleted",
        }))),
        Ok(false) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn get_session_messages_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    match state.session_store.get_session_messages(&session_id).await {
        Ok(messages) => axum::Json(serde_json::json!({
            "session_id": session_id,
            "messages": messages.iter().map(|m| serde_json::json!({
                "role": m.role,
                "content": m.content,
                "created_at": m.created_at,
            })).collect::<Vec<_>>(),
        })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_prompt_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(prompt_ref): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    match state.prompt_version_mgr.get_latest(&prompt_ref).await {
        Ok(Some(version)) => axum::Json(serde_json::json!({
            "prompt_ref": prompt_ref,
            "version": version.version,
            "content": version.content,
            "created_at": version.created_at,
        })),
        Ok(None) => axum::Json(serde_json::json!({
            "error": "Prompt not found",
            "prompt_ref": prompt_ref,
        })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn rollback_prompt_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(prompt_ref): axum::extract::Path<String>,
    Json(req): Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    let version = req["version"].as_i64().unwrap_or(0) as i32;
    match state
        .prompt_version_mgr
        .rollback(&prompt_ref, version)
        .await
    {
        Ok(new_version) => axum::Json(serde_json::json!({
            "prompt_ref": prompt_ref,
            "rolled_back_to": version,
            "new_version": new_version,
            "status": "rolled_back",
        })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn trigger_sync_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    match state.sync_engine.full_sync().await {
        Ok(result) => axum::Json(serde_json::json!({
            "status": "completed",
            "pushed": result.pushed_count,
            "pulled": result.pulled_count,
            "conflicts": result.conflicts,
        })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn sync_status_handler(
    axum::extract::State(_state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "ok",
        "engine": "webdav",
        "interval_secs": 300,
    }))
}

async fn get_workspace_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(workspace_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let row = sqlx::query_as::<_, (String, String, Option<String>, String, i64, i64, i64, i64)>(
        "SELECT id, name, description, owner_id, max_agents, auto_approve, knowledge_base_enabled, created_at FROM workspaces WHERE id = ?"
    )
    .bind(&workspace_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    match row {
        Some((id, name, desc, owner, max_agents, auto_approve, kb_enabled, created)) => {
            Ok(axum::Json(serde_json::json!({
                "id": id,
                "name": name,
                "description": desc,
                "owner_id": owner,
                "max_agents": max_agents,
                "auto_approve": auto_approve,
                "knowledge_base_enabled": kb_enabled,
                "created_at": created,
            })))
        }
        None => Err(axum::http::StatusCode::NOT_FOUND),
    }
}

#[derive(Debug, Deserialize)]
struct UpdateWorkspaceRequest {
    name: Option<String>,
    description: Option<String>,
    max_agents: Option<i64>,
    auto_approve: Option<bool>,
    knowledge_base_enabled: Option<bool>,
}

async fn update_workspace_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(workspace_id): axum::extract::Path<String>,
    Json(req): Json<UpdateWorkspaceRequest>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    if let Some(name) = &req.name {
        let _ = sqlx::query("UPDATE workspaces SET name = ? WHERE id = ?")
            .bind(name)
            .bind(&workspace_id)
            .execute(&state.db)
            .await;
    }
    if let Some(desc) = &req.description {
        let _ = sqlx::query("UPDATE workspaces SET description = ? WHERE id = ?")
            .bind(desc)
            .bind(&workspace_id)
            .execute(&state.db)
            .await;
    }
    if let Some(max) = req.max_agents {
        let _ = sqlx::query("UPDATE workspaces SET max_agents = ? WHERE id = ?")
            .bind(max)
            .bind(&workspace_id)
            .execute(&state.db)
            .await;
    }
    if let Some(auto) = req.auto_approve {
        let _ = sqlx::query("UPDATE workspaces SET auto_approve = ? WHERE id = ?")
            .bind(auto as i64)
            .bind(&workspace_id)
            .execute(&state.db)
            .await;
    }
    if let Some(kb) = req.knowledge_base_enabled {
        let _ = sqlx::query("UPDATE workspaces SET knowledge_base_enabled = ? WHERE id = ?")
            .bind(kb as i64)
            .bind(&workspace_id)
            .execute(&state.db)
            .await;
    }

    Ok(axum::Json(serde_json::json!({
        "id": workspace_id,
        "status": "updated",
    })))
}

async fn delete_workspace_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(workspace_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let result = sqlx::query("DELETE FROM workspaces WHERE id = ?")
        .bind(&workspace_id)
        .execute(&state.db)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() > 0 {
        Ok(axum::Json(serde_json::json!({
            "id": workspace_id,
            "status": "deleted",
        })))
    } else {
        Err(axum::http::StatusCode::NOT_FOUND)
    }
}

// ── Board Task handlers ──

#[derive(serde::Deserialize)]
struct CreateTaskRequest {
    title: String,
    description: Option<String>,
    status: Option<String>,
    priority: Option<String>,
    assignee_name: Option<String>,
    assignee_avatar: Option<String>,
    due_date: Option<String>,
    tags: Option<Vec<String>>,
}

async fn list_tasks_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    let rows: Vec<(String, String, Option<String>, String, String, Option<String>, Option<String>, Option<String>, String, i64)> =
        sqlx::query_as("SELECT id, title, description, status, priority, assignee_name, assignee_avatar, due_date, tags, sort_order FROM board_tasks ORDER BY sort_order, created_at")
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

    let tasks: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(
                id,
                title,
                desc,
                status,
                priority,
                assignee_name,
                assignee_avatar,
                due_date,
                tags,
                _sort,
            )| {
                let tags_vec: Vec<String> = serde_json::from_str(&tags).unwrap_or_default();
                serde_json::json!({
                    "id": id,
                    "title": title,
                    "description": desc,
                    "status": status,
                    "priority": priority,
                    "assignee": if let Some(name) = assignee_name {
                        Some(serde_json::json!({"name": name, "avatar": assignee_avatar}))
                    } else { None },
                    "due_date": due_date,
                    "tags": tags_vec,
                })
            },
        )
        .collect();

    axum::Json(serde_json::json!({ "tasks": tasks }))
}

async fn create_task_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<CreateTaskRequest>,
) -> axum::Json<serde_json::Value> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let status = req.status.unwrap_or_else(|| "todo".to_string());
    let priority = req.priority.unwrap_or_else(|| "medium".to_string());
    let tags =
        serde_json::to_string(&req.tags.unwrap_or_default()).unwrap_or_else(|_| "[]".to_string());

    let _ = sqlx::query(
        "INSERT INTO board_tasks (id, workspace_id, title, description, status, priority, assignee_name, assignee_avatar, due_date, tags, sort_order, created_at, updated_at) VALUES (?, 'default', ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)"
    )
    .bind(&id).bind(&req.title).bind(&req.description).bind(&status).bind(&priority)
    .bind(&req.assignee_name).bind(&req.assignee_avatar).bind(&req.due_date)
    .bind(&tags).bind(now).bind(now)
    .execute(&state.db).await;

    axum::Json(serde_json::json!({ "id": id, "status": status }))
}

#[derive(serde::Deserialize)]
struct UpdateTaskRequest {
    title: Option<String>,
    description: Option<String>,
    status: Option<String>,
    priority: Option<String>,
    assignee_name: Option<String>,
    assignee_avatar: Option<String>,
    due_date: Option<String>,
    tags: Option<Vec<String>>,
    sort_order: Option<i64>,
}

async fn update_task_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let now = chrono::Utc::now().timestamp();
    let mut updates = vec!["updated_at = ?".to_string()];
    let mut bind_values: Vec<String> = vec![now.to_string()];

    if let Some(v) = &req.title {
        updates.push("title = ?".to_string());
        bind_values.push(v.clone());
    }
    if let Some(v) = &req.description {
        updates.push("description = ?".to_string());
        bind_values.push(v.clone());
    }
    if let Some(v) = &req.status {
        updates.push("status = ?".to_string());
        bind_values.push(v.clone());
    }
    if let Some(v) = &req.priority {
        updates.push("priority = ?".to_string());
        bind_values.push(v.clone());
    }
    if let Some(v) = &req.assignee_name {
        updates.push("assignee_name = ?".to_string());
        bind_values.push(v.clone());
    }
    if let Some(v) = &req.assignee_avatar {
        updates.push("assignee_avatar = ?".to_string());
        bind_values.push(v.clone());
    }
    if let Some(v) = &req.due_date {
        updates.push("due_date = ?".to_string());
        bind_values.push(v.clone());
    }
    if let Some(v) = &req.tags {
        updates.push("tags = ?".to_string());
        bind_values.push(serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string()));
    }
    if let Some(v) = req.sort_order {
        updates.push("sort_order = ?".to_string());
        bind_values.push(v.to_string());
    }

    let sql = format!("UPDATE board_tasks SET {} WHERE id = ?", updates.join(", "));
    let mut query = sqlx::query(&sql);
    for val in &bind_values {
        query = query.bind(val);
    }
    query = query.bind(&task_id);

    let result = query
        .execute(&state.db)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() > 0 {
        Ok(axum::Json(
            serde_json::json!({ "id": task_id, "updated": true }),
        ))
    } else {
        Err(axum::http::StatusCode::NOT_FOUND)
    }
}

async fn delete_task_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let result = sqlx::query("DELETE FROM board_tasks WHERE id = ?")
        .bind(&task_id)
        .execute(&state.db)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() > 0 {
        Ok(axum::Json(
            serde_json::json!({ "id": task_id, "deleted": true }),
        ))
    } else {
        Err(axum::http::StatusCode::NOT_FOUND)
    }
}

// ── Board Comment handlers ──

#[derive(serde::Deserialize)]
struct CreateCommentRequest {
    task_id: String,
    parent_id: Option<String>,
    author_name: String,
    author_avatar: Option<String>,
    author_role: Option<String>,
    content: String,
}

async fn list_comments_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    let rows: Vec<(String, Option<String>, String, Option<String>, Option<String>, String, i64, i64)> =
        sqlx::query_as("SELECT id, parent_id, author_name, author_avatar, author_role, content, likes, created_at FROM board_comments WHERE task_id = ? ORDER BY created_at DESC")
            .bind(&task_id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

    let comments: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(
                id,
                parent_id,
                author_name,
                author_avatar,
                author_role,
                content,
                likes,
                created_at,
            )| {
                serde_json::json!({
                    "id": id,
                    "parent_id": parent_id,
                    "author": { "name": author_name, "avatar": author_avatar, "role": author_role },
                    "content": content,
                    "likes": likes,
                    "created_at": created_at,
                })
            },
        )
        .collect();

    axum::Json(serde_json::json!({ "comments": comments }))
}

async fn create_comment_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<CreateCommentRequest>,
) -> axum::Json<serde_json::Value> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();

    let _ = sqlx::query(
        "INSERT INTO board_comments (id, task_id, parent_id, author_name, author_avatar, author_role, content, likes, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?)"
    )
    .bind(&id).bind(&req.task_id).bind(&req.parent_id).bind(&req.author_name)
    .bind(&req.author_avatar).bind(&req.author_role).bind(&req.content).bind(now)
    .execute(&state.db).await;

    axum::Json(serde_json::json!({ "id": id }))
}

async fn delete_comment_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(comment_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let result = sqlx::query("DELETE FROM board_comments WHERE id = ?")
        .bind(&comment_id)
        .execute(&state.db)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() > 0 {
        Ok(axum::Json(
            serde_json::json!({ "id": comment_id, "deleted": true }),
        ))
    } else {
        Err(axum::http::StatusCode::NOT_FOUND)
    }
}

async fn like_comment_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(comment_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let result = sqlx::query("UPDATE board_comments SET likes = likes + 1 WHERE id = ?")
        .bind(&comment_id)
        .execute(&state.db)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() > 0 {
        Ok(axum::Json(
            serde_json::json!({ "id": comment_id, "liked": true }),
        ))
    } else {
        Err(axum::http::StatusCode::NOT_FOUND)
    }
}

async fn deep_health_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    let db_ok = sqlx::query("SELECT 1").execute(&state.db).await.is_ok();
    let agents = state.agent_registry.list_agents().await;
    let tasks = state.task_queue.stats().await.unwrap_or_default();

    axum::Json(serde_json::json!({
        "status": if db_ok { "healthy" } else { "degraded" },
        "version": env!("CARGO_PKG_VERSION"),
        "checks": {
            "database": db_ok,
            "agents_online": agents.iter().filter(|(_, _, s)| *s == maple_agent::registry::AgentStatus::Online).count(),
            "agents_total": agents.len(),
            "tasks": {
                "pending": tasks.pending,
                "running": tasks.running,
                "completed": tasks.completed,
                "failed": tasks.failed,
            }
        },
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

async fn agent_status_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    let agents = state.agent_registry.list_agents().await;
    let tasks = state.task_queue.stats().await.unwrap_or_default();

    let agent_details: Vec<serde_json::Value> = agents
        .iter()
        .map(|(id, name, status)| {
            serde_json::json!({
                "id": id,
                "name": name,
                "status": format!("{:?}", status),
                "is_online": *status == maple_agent::registry::AgentStatus::Online,
            })
        })
        .collect();

    axum::Json(serde_json::json!({
        "agents": agent_details,
        "summary": {
            "total": agents.len(),
            "online": agents.iter().filter(|(_, _, s)| *s == maple_agent::registry::AgentStatus::Online).count(),
            "offline": agents.iter().filter(|(_, _, s)| *s == maple_agent::registry::AgentStatus::Offline).count(),
            "busy": agents.iter().filter(|(_, _, s)| *s == maple_agent::registry::AgentStatus::Busy).count(),
        },
        "tasks": {
            "pending": tasks.pending,
            "running": tasks.running,
            "completed": tasks.completed,
            "failed": tasks.failed,
        },
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

async fn agent_heartbeat_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(agent_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    match state.agent_registry.get_agent(&agent_id).await {
        Some(_) => {
            state.agent_registry.update_heartbeat(&agent_id).await;
            Ok(axum::Json(serde_json::json!({
                "agent_id": agent_id,
                "status": "ok",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            })))
        }
        None => Err(axum::http::StatusCode::NOT_FOUND),
    }
}

#[derive(Debug, Deserialize)]
struct UpdateConfigRequest {
    host: Option<String>,
    port: Option<u16>,
    jwt_secret: Option<String>,
    require_auth: Option<bool>,
    admin_username: Option<String>,
    admin_password: Option<String>,
    usage_limit_usd: Option<f64>,
    log_level: Option<String>,
}

async fn get_config_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    let config = state.config.read().await;
    axum::Json(serde_json::json!({
        "host": config.host,
        "port": config.port,
        "require_auth": config.require_auth,
        "usage_limit_usd": config.usage_limit_usd,
        "log_level": config.log_level,
    }))
}

async fn update_config_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<UpdateConfigRequest>,
) -> axum::Json<serde_json::Value> {
    let mut config = state.config.write().await;

    if let Some(host) = req.host {
        config.host = host;
    }
    if let Some(port) = req.port {
        config.port = port;
    }
    if let Some(secret) = req.jwt_secret {
        config.jwt_secret = secret;
    }
    if let Some(auth) = req.require_auth {
        config.require_auth = auth;
    }
    if let Some(user) = req.admin_username {
        config.admin_username = user;
    }
    if let Some(pass) = req.admin_password {
        config.admin_password = pass;
    }
    if let Some(limit) = req.usage_limit_usd {
        config.usage_limit_usd = limit;
    }
    if let Some(level) = req.log_level {
        config.log_level = level;
    }

    axum::Json(serde_json::json!({
        "status": "updated",
        "message": "Configuration updated. Some changes may require restart.",
    }))
}

#[derive(Debug, Deserialize)]
struct AgentRegisterRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    transport_type: Option<String>,
    #[serde(default)]
    transport_config: Option<serde_json::Value>,
    #[serde(default)]
    capabilities: Option<Vec<String>>,
    #[serde(default)]
    max_concurrent_tasks: Option<u32>,
}

async fn register_agent_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<AgentRegisterRequest>,
) -> axum::Json<serde_json::Value> {
    // 输入验证
    if req.name.trim().is_empty() {
        return axum::Json(serde_json::json!({
            "error": "Agent name is required",
            "code": "INVALID_INPUT"
        }));
    }

    let agent_id = format!(
        "agent-{}",
        uuid::Uuid::new_v4()
            .to_string()
            .split('-')
            .next()
            .unwrap_or("x")
    );
    let now = chrono::Utc::now().timestamp();

    let capabilities_json = serde_json::json!({
        "tools": req.capabilities.unwrap_or_default(),
        "skills": [],
        "max_context_length": 128000,
        "supports_streaming": true,
        "supports_function_calling": true,
    });

    let transport_type = req
        .transport_type
        .unwrap_or_else(|| "websocket".to_string());
    let transport_config = req
        .transport_config
        .unwrap_or_else(|| serde_json::json!({}));

    let _ = sqlx::query(
        "INSERT INTO agents (id, name, transport_type, transport_config, capabilities, status, max_concurrent_tasks, created_at) VALUES (?, ?, ?, ?, ?, 'offline', ?, ?)"
    )
    .bind(&agent_id)
    .bind(&req.name)
    .bind(&transport_type)
    .bind(transport_config.to_string())
    .bind(capabilities_json.to_string())
    .bind(req.max_concurrent_tasks.unwrap_or(3) as i64)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to insert agent into database: {}", e);
        e
    });

    state
        .agent_registry
        .register_agent(
            &agent_id,
            &req.name,
            maple_agent::registry::AgentStatus::Offline,
        )
        .await;

    axum::Json(serde_json::json!({
        "id": agent_id,
        "name": req.name,
        "description": req.description,
        "status": "registered",
    }))
}

async fn list_agents_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    let agents = state.agent_registry.list_agents().await;
    axum::Json(serde_json::json!({
        "agents": agents.into_iter().map(|(id, name, status)| serde_json::json!({
            "id": id,
            "name": name,
            "status": format!("{:?}", status),
        })).collect::<Vec<_>>(),
    }))
}

async fn get_agent_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(agent_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    match state.agent_registry.get_agent(&agent_id).await {
        Some(agent) => Ok(axum::Json(serde_json::json!({
            "id": agent.id,
            "name": agent.name,
            "description": agent.description,
            "transport": agent.transport,
            "capabilities": agent.capabilities,
            "max_concurrent_tasks": agent.max_concurrent_tasks,
        }))),
        None => Err(axum::http::StatusCode::NOT_FOUND),
    }
}

async fn delete_agent_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(agent_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    state.agent_registry.set_offline(&agent_id).await;
    state.agent_registry.remove_task_channel(&agent_id).await;

    let result = sqlx::query("DELETE FROM agents WHERE id = ?")
        .bind(&agent_id)
        .execute(&state.db)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() > 0 {
        Ok(axum::Json(serde_json::json!({
            "id": agent_id,
            "status": "deleted",
        })))
    } else {
        Err(axum::http::StatusCode::NOT_FOUND)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ServerConfig::from_env();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| config.log_level.clone().into()),
        )
        .init();

    let database_url = &config.database_url;

    let pool = sqlx::SqlitePool::connect(database_url).await?;
    db::run_migrations(&pool).await?;

    let event_bus = Arc::new(EventBus::new());
    let llm_router = config::build_llm_router(&config);
    let skill_registry = Arc::new(SkillRegistry::new());
    skills::register_builtin_skills(&skill_registry).await;
    let hook_runner = Arc::new(HookRunner::new());
    let checkpoint_mgr = Arc::new(CheckpointManager::new(pool.clone()));
    let agent_registry = Arc::new(AgentRegistry::new());
    {
        let rows: Vec<(String, String, String)> =
            sqlx::query_as("SELECT id, name, status FROM agents")
                .fetch_all(&pool)
                .await
                .unwrap_or_default();

        for (id, name, status) in &rows {
            let agent_status = if status == "Online" || status == "Idle" {
                maple_agent::registry::AgentStatus::Online
            } else {
                maple_agent::registry::AgentStatus::Offline
            };
            agent_registry.register_agent(id, name, agent_status).await;
        }
        tracing::info!("Restored {} agents from DB", rows.len());
    }
    let auth_service = Arc::new(AuthService::new(config.jwt_secret.clone()));
    let workspace_manager = Arc::new(tokio::sync::Mutex::new(WorkspaceManager::new(pool.clone())));
    {
        let wm = workspace_manager.lock().await;
        if let Err(e) = wm.init_schema().await {
            tracing::warn!("Failed to init workspace schema: {}", e);
        }
    }

    let agent_registry_for_handler = agent_registry.clone();
    let llm_router_for_handler = llm_router.clone();
    let agent_handler: maple_engine::executor::AgentHandler =
        Arc::new(move |agent_id: String, goal: String| {
            let reg = agent_registry_for_handler.clone();
            let llm = llm_router_for_handler.clone();
            Box::pin(async move {
                // Try to dispatch to a connected agent via task channel
                if let Some(tx) = reg.get_task_channel(&agent_id).await {
                    let (result_tx, result_rx) = tokio::sync::oneshot::channel();
                    let task_id = uuid::Uuid::new_v4().to_string();
                    reg.register_result_channel(&task_id, result_tx).await;
                    let task = maple_agent::registry::AgentTask {
                        task_id: task_id.clone(),
                        goal: goal.clone(),
                        tools: Vec::new(),
                        role: maple_agent::registry::AgentRole::Executor,
                        timeout_secs: 120,
                    };
                    tx.send(task)
                        .await
                        .map_err(|_| anyhow::anyhow!("Agent channel closed"))?;
                    match tokio::time::timeout(std::time::Duration::from_secs(120), result_rx).await
                    {
                        Ok(Ok(result)) => return Ok(result),
                        Ok(Err(_)) => return Err(anyhow::anyhow!("Agent result channel closed")),
                        Err(_) => return Err(anyhow::anyhow!("Agent {} timed out", agent_id)),
                    }
                }
                // Fallback: use LLM with agent context
                let agent = reg.get_agent(&agent_id).await;
                let agent_desc = agent
                    .as_ref()
                    .map(|a| a.description.as_deref().unwrap_or(&a.name))
                    .unwrap_or("assistant");
                let prompt = format!(
                    "You are an AI agent named '{}'. {}\n\nTask: {}",
                    agent_id, agent_desc, goal
                );
                let request = maple_llm::request::LlmRequest::new(prompt, "default");
                let adapter = llm.route(&request).await?;
                let response = adapter.complete(request).await?;
                Ok(response.text())
            })
        });

    let node_executor = NodeExecutor::new(
        llm_router.clone(),
        skill_registry.clone(),
        hook_runner.clone(),
    )
    .with_agent_handler(agent_handler);

    let workflow_executor = Arc::new(WorkflowExecutor::new(
        event_bus.clone(),
        node_executor,
        checkpoint_mgr,
        hook_runner,
    ));

    let sync_engine = Arc::new(SyncEngine::new(None, 300));

    let session_store = Arc::new(SessionStore::new(pool.clone()));

    let bm25_searcher = Arc::new(BM25Searcher::new());
    let vector_store: Arc<dyn VectorSearch> = if let Ok(qdrant_url) = std::env::var("QDRANT_URL") {
        let collection =
            std::env::var("QDRANT_COLLECTION").unwrap_or_else(|_| "mapleos_chunks".to_string());
        let dim: usize = std::env::var("EMBEDDING_DIM")
            .unwrap_or_else(|_| "768".to_string())
            .parse()
            .unwrap_or(768);
        match QdrantVectorStore::new(&qdrant_url, &collection, dim).await {
            Ok(vs) => {
                tracing::info!("Using Qdrant vector store: {} (dim={})", qdrant_url, dim);
                Arc::new(vs)
            }
            Err(e) => {
                tracing::warn!(
                    "Qdrant connection failed: {}, falling back to in-memory store",
                    e
                );
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
    let evolver = Arc::new(
        maple_kb::evolver::Evolver::new(llm_router.clone()).with_memory_store(memory_store.clone()),
    );
    let prompt_version_mgr = Arc::new(PromptVersionManager::new(pool.clone()));
    let task_queue = Arc::new(TaskQueueService::new(pool.clone()));
    task_queue.init_schema().await?;

    let scheduler = Arc::new(Scheduler::new());
    let job_count: usize;
    {
        type ScheduledJobRow = (String, String, String, String, Option<i64>, i64, bool);
        let rows: Vec<ScheduledJobRow> = sqlx::query_as(
            "SELECT id, workflow_id, cron_expr, timezone, last_run_at, next_run_at, enabled FROM scheduled_jobs WHERE enabled = 1"
        ).fetch_all(&pool).await?;
        job_count = rows.len();
        for row in rows {
            scheduler
                .add_job(ScheduledJob {
                    id: row.0,
                    workflow_id: row.1,
                    cron_expr: row.2,
                    timezone: row.3,
                    last_run_at: row.4,
                    next_run_at: row.5,
                    enabled: row.6,
                })
                .await?;
        }
    }
    tracing::info!("Scheduler loaded {} active jobs from DB", job_count);

    let task_worker_queue = task_queue.clone();
    let task_worker_skills = skill_registry.clone();
    let task_worker_llm = llm_router.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if let Ok(Some(task)) = task_worker_queue.dequeue().await {
                let payload = task.payload.clone();
                let prompt = payload["prompt"].as_str().unwrap_or("").to_string();
                let task_id = task.id.clone();
                if !prompt.is_empty() {
                    let llm_request = maple_llm::request::LlmRequest::new(prompt, "task-worker");
                    match task_worker_llm.route(&llm_request).await {
                        Ok(adapter) => match adapter.complete(llm_request).await {
                            Ok(_) => {
                                let _ = task_worker_queue.complete(&task_id).await;
                            }
                            Err(e) => {
                                let _ = task_worker_queue.fail(&task_id, &e.to_string()).await;
                            }
                        },
                        Err(e) => {
                            let _ = task_worker_queue
                                .fail(&task_id, &format!("LLM routing: {}", e))
                                .await;
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
        }
    });

    let rate_limiter = state::RateLimiter::new(100, 60);

    let state = Arc::new(AppState {
        config: Arc::new(tokio::sync::RwLock::new(config.clone())),
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
        evolver: evolver.clone(),
        prompt_version_mgr: prompt_version_mgr.clone(),
        task_queue: task_queue.clone(),
        mcp_host: Arc::new(McpHostManager::new()),
        rate_limiter,
        cache: cache::AppCache::new(),
        metrics: metrics::AppMetrics::new(),
    });

    let scheduler_wf = workflow_executor.clone();
    let scheduler_db = pool.clone();
    scheduler.start_loop(60, move |job: ScheduledJob| {
        let wf = scheduler_wf.clone();
        let db = scheduler_db.clone();
        async move {
            tracing::info!(job_id = %job.id, workflow_id = %job.workflow_id, "Scheduled job triggered");
            let yaml_str: Option<String> = sqlx::query_scalar(
                "SELECT yaml_content FROM workflows WHERE id = ?"
            ).bind(&job.workflow_id).fetch_optional(&db).await.ok().flatten();
            if let Some(yaml) = yaml_str {
                if let Ok(parsed) = Workflow::parse_definition(&yaml) {
                    let _ = wf.execute(&parsed.nodes, &job.workflow_id, parsed.version, serde_json::Value::Null).await;
                }
            }
            Ok(())
        }
    }).await;

    let dispatcher = Arc::new(RpcDispatcher::new());
    register_business_handlers(&dispatcher, state.clone()).await;

    let rpc_server = RpcServer::new(dispatcher);
    let rpc_router = rpc_server.router();

    let state_routes = Router::new()
        .route("/health", get(health_handler))
        .route("/health/deep", get(deep_health_handler))
        .route("/metrics", get(system_metrics_handler))
        .route("/prometheus", get(metrics::metrics_handler))
        .route("/ws/agents", get(ws_agent_handler))
        .route("/api/chat", post(chat_handler))
        .route("/api/chat/stream", post(chat_stream_handler))
        .route("/api/models", get(models_handler))
        .route("/api/skills", get(skills_handler))
        .route(
            "/api/config",
            get(get_config_handler).put(update_config_handler),
        )
        .route("/api/kb/index", post(kb_index_handler))
        .route("/api/kb/search", post(kb_search_handler))
        .route(
            "/api/agents",
            get(list_agents_handler).post(register_agent_handler),
        )
        .route(
            "/api/agents/:id",
            get(get_agent_handler).delete(delete_agent_handler),
        )
        .route("/api/agents/:id/heartbeat", post(agent_heartbeat_handler))
        .route("/api/agents/status", get(agent_status_handler))
        .route("/api/sessions", get(sessions_handler))
        .route("/api/sessions/:id", delete(delete_session_handler))
        .route(
            "/api/sessions/:id/messages",
            get(get_session_messages_handler),
        )
        .route("/api/memories", post(memory_store_handler))
        .route("/api/memories/search", post(memory_search_handler))
        .route(
            "/api/memories/:id",
            get(get_memory_handler).delete(delete_memory_handler),
        )
        .route("/api/prompts", post(prompt_create_handler))
        .route("/api/prompts/:ref", get(get_prompt_handler))
        .route("/api/prompts/:ref/rollback", post(rollback_prompt_handler))
        .route("/api/sync/trigger", post(trigger_sync_handler))
        .route("/api/sync/status", get(sync_status_handler))
        .route("/api/tasks/enqueue", post(task_enqueue_handler))
        .route("/api/tasks/stats", get(task_stats_handler))
        .route("/api/tasks/dead-letter", get(task_dead_letter_handler))
        .route("/api/tasks/:id/requeue", post(task_requeue_handler))
        .route("/api/events", get(sse_events_handler))
        .route(
            "/api/workflows/:id",
            get(get_workflow_handler)
                .put(update_workflow_handler)
                .delete(delete_workflow_handler),
        )
        .route(
            "/api/workflows/:id/executions",
            get(get_workflow_executions_handler),
        )
        .route("/api/workflows/:id/stats", get(workflow_stats_handler))
        .route("/api/executions/:id", get(get_execution_handler))
        .route(
            "/api/executions/:id/checkpoints",
            get(get_checkpoints_handler),
        )
        .route(
            "/api/workspaces/:id",
            get(get_workspace_handler)
                .put(update_workspace_handler)
                .delete(delete_workspace_handler),
        )
        // Collaboration board APIs
        .route(
            "/api/board/tasks",
            get(list_tasks_handler).post(create_task_handler),
        )
        .route(
            "/api/board/tasks/:id",
            put(update_task_handler).delete(delete_task_handler),
        )
        .route("/api/board/tasks/:id/comments", get(list_comments_handler))
        .route("/api/board/comments", post(create_comment_handler))
        .route("/api/board/comments/:id", delete(delete_comment_handler))
        .route("/api/board/comments/:id/like", post(like_comment_handler))
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/token", post(token_handler))
        .route("/api/auth/register", post(register_handler))
        .route("/api/auth/refresh", post(refresh_handler))
        .with_state(state.clone());

    let cors = if config.require_auth {
        CorsLayer::new()
            .allow_origin([
                "http://localhost:3000".parse().unwrap(),
                "http://localhost:3001".parse().unwrap(),
                "https://mapleos.dev".parse().unwrap(),
            ])
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::PUT,
                axum::http::Method::DELETE,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
                axum::http::header::ACCEPT,
            ])
            .max_age(std::time::Duration::from_secs(3600))
    } else {
        CorsLayer::permissive()
    };

    let app = Router::new()
        .merge(rpc_router)
        .merge(state_routes)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            metrics::metrics_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::rate_limit_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::auth_middleware,
        ))
        .layer(axum::middleware::from_fn(middleware::audit_log_middleware))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let bind_addr = config.bind_address();
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("MapleOS Server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;
    Ok(())
}

async fn register_business_handlers(dispatcher: &Arc<RpcDispatcher>, state: Arc<AppState>) {
    use maple_engine::skill_registry::Skill;
    use serde_json::Value;
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

            let workflow = match Workflow::parse_definition(&yaml) {
                Ok(w) => w,
                Err(e) => return Ok(serde_json::json!({"error": format!("Parse error: {}", e)})),
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
    dispatcher
        .register("agent.list", move |_: Option<serde_json::Value>| {
            let db = s.db.clone();
            let registry = s.agent_registry.clone();
            async move {
                let rows: Vec<(String, String, String)> =
                    sqlx::query_as("SELECT id, name, status FROM agents ORDER BY created_at DESC")
                        .fetch_all(&db)
                        .await
                        .unwrap_or_default();

                for (id, name, status) in &rows {
                    let agent_status = if status == "Online" || status == "Idle" {
                        maple_agent::registry::AgentStatus::Online
                    } else {
                        maple_agent::registry::AgentStatus::Offline
                    };
                    registry.register_agent(id, name, agent_status).await;
                }

                Ok(serde_json::json!({
                    "agents": rows.into_iter().map(|(id, name, status)| serde_json::json!({
                        "id": id,
                        "name": name,
                        "status": status,
                    })).collect::<Vec<_>>(),
                }))
            }
        })
        .await;

    let s = state.clone();
    dispatcher
        .register(
            "workspace.create",
            move |params: Option<serde_json::Value>| {
                let mgr = s.workspace_manager.clone();
                async move {
                    let name = params
                        .as_ref()
                        .and_then(|p| p["name"].as_str())
                        .unwrap_or("Default Workspace");
                    let owner_id = params
                        .as_ref()
                        .and_then(|p| p["owner_id"].as_str())
                        .unwrap_or("user-1");

                    let manager = mgr.lock().await;
                    let ws = manager.create_workspace(name, owner_id).await?;
                    Ok(serde_json::json!({
                        "id": ws.id,
                        "name": ws.name,
                        "owner_id": ws.owner_id,
                    }))
                }
            },
        )
        .await;

    let s = state.clone();
    dispatcher
        .register("llm.models", move |_: Option<serde_json::Value>| {
            let router = s.llm_router.clone();
            async move {
                let models = router.list_models().await;
                Ok(serde_json::json!({
                    "models": models,
                }))
            }
        })
        .await;

    let s = state.clone();
    dispatcher
        .register("skill.list", move |_: Option<serde_json::Value>| {
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
        })
        .await;

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
        async move {
            let p = match params {
                Some(v) => v,
                None => return Ok(serde_json::json!({"error": "missing params: agent_id + message"})),
            };
            let agent_id = p["agent_id"].as_str().unwrap_or("default").to_string();
            let prompt = p["message"].as_str()
                .or_else(|| p["prompt"].as_str())
                .unwrap_or("")
                .to_string();
            if prompt.is_empty() {
                return Ok(serde_json::json!({"error": "message is required"}));
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
                        "reply": response.content,
                        "response": response.content,
                        "agent_id": agent_id,
                        "model": adapter.name().to_string(),
                    }))
                }
                Err(e) => Ok(serde_json::json!({
                    "reply": format!("LLM error: {}", e),
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
    dispatcher
        .register(
            "agent.deregister",
            move |params: Option<serde_json::Value>| {
                let registry = s.agent_registry.clone();
                let db = s.db.clone();
                async move {
                    let p = match params {
                        Some(v) => v,
                        None => return Ok(serde_json::json!({"error": "missing params: id"})),
                    };
                    let id = p["id"].as_str().unwrap_or("").to_string();
                    if id.is_empty() {
                        return Ok(serde_json::json!({"error": "id is required"}));
                    }
                    registry.deregister_agent(&id).await;
                    let _ = sqlx::query("DELETE FROM agents WHERE id = ?")
                        .bind(&id)
                        .execute(&db)
                        .await;
                    Ok(serde_json::json!({"id": id, "status": "deregistered"}))
                }
            },
        )
        .await;

    let s = state.clone();
    dispatcher
        .register("task.create", move |params: Option<serde_json::Value>| {
            let task_queue = s.task_queue.clone();
            async move {
                let p = match params {
                    Some(v) => v,
                    None => return Ok(serde_json::json!({"error": "missing params"})),
                };
                let task_type = p["task_type"].as_str().unwrap_or("generic").to_string();
                let priority = p["priority"].as_i64().unwrap_or(0) as i32;

                let (agent_id, prompt) =
                    if let Some(payload) = p.get("payload").and_then(|v| v.as_object()) {
                        let aid = payload
                            .get("agent_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let pr = payload
                            .get("prompt")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        (aid, pr)
                    } else {
                        let aid = p["agent_id"].as_str().unwrap_or("").to_string();
                        let pr = p["prompt"].as_str().unwrap_or("").to_string();
                        (aid, pr)
                    };

                let payload = serde_json::json!({
                    "agent_id": agent_id,
                    "prompt": prompt,
                });

                match task_queue
                    .enqueue(&task_type, priority, payload, 3, 0, Some(&agent_id))
                    .await
                {
                    Ok(id) => Ok(serde_json::json!({
                        "id": id,
                        "task_type": task_type,
                        "agent_id": agent_id,
                        "status": "pending",
                    })),
                    Err(e) => Ok(serde_json::json!({"error": e.to_string()})),
                }
            }
        })
        .await;

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
    dispatcher
        .register("config.update", move |params: Option<serde_json::Value>| {
            let db = s.db.clone();
            async move {
                let p = match params {
                    Some(v) => v,
                    None => return Ok(serde_json::json!({"error": "missing params"})),
                };
                let now = chrono::Utc::now().timestamp();
                let fields = [
                    ("ollama_url", p.get("ollama_url").and_then(|v| v.as_str())),
                    (
                        "openai_api_key",
                        p.get("openai_api_key").and_then(|v| v.as_str()),
                    ),
                    (
                        "default_model",
                        p.get("default_model").and_then(|v| v.as_str()),
                    ),
                    ("webdav_url", p.get("webdav_url").and_then(|v| v.as_str())),
                    (
                        "webdav_username",
                        p.get("webdav_username").and_then(|v| v.as_str()),
                    ),
                    (
                        "webdav_password",
                        p.get("webdav_password").and_then(|v| v.as_str()),
                    ),
                    ("qdrant_url", p.get("qdrant_url").and_then(|v| v.as_str())),
                    (
                        "gateway_mode",
                        p.get("gateway_mode").and_then(|v| v.as_str()),
                    ),
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
                        "INSERT OR REPLACE INTO kv_store (key, value, updated_at) VALUES (?, ?, ?)",
                    )
                    .bind(config_key)
                    .bind(val_str)
                    .bind(now)
                    .execute(&db)
                    .await;
                }

                Ok(serde_json::json!({"status": "updated"}))
            }
        })
        .await;

    let s = state.clone();
    struct McpSkillProxy {
        id: String,
        description: String,
        server_name: String,
        tool_name: String,
        mcp_host: Arc<McpHostManager>,
    }
    impl Skill for McpSkillProxy {
        fn id(&self) -> &str {
            &self.id
        }
        fn description(&self) -> &str {
            &self.description
        }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            let rt = tokio::runtime::Handle::current();
            let _guard = rt.enter();
            tokio::task::block_in_place(|| {
                rt.block_on(async {
                    self.mcp_host
                        .call_tool(&self.server_name, &self.tool_name, config.clone())
                        .await
                })
            })
        }
    }

    dispatcher.register("skill.install", move |params: Option<serde_json::Value>| {
        let registry = s.skill_registry.clone();
        let mcp_host_install = s.mcp_host.clone();
        async move {
            let p = match params {
                Some(v) => v,
                None => return Ok(serde_json::json!({"error": "missing params"})),
            };
            let skill_id = p["skill_id"].as_str().unwrap_or("").to_string();
            if skill_id.is_empty() {
                return Ok(serde_json::json!({"error": "skill_id is required"}));
            }

            if let Some(mcp_config) = p.get("mcp_config") {
                let mcp_host = mcp_host_install.clone();
                let config: maple_gateway::mcp_host::McpServerConfig = serde_json::from_value(mcp_config.clone())
                    .unwrap_or_else(|_| maple_gateway::mcp_host::McpServerConfig {
                        name: skill_id.clone(),
                        transport: maple_gateway::mcp_host::McpTransportConfig::Stdio {
                            command: vec!["node".to_string(), p["command"].as_str().unwrap_or("").to_string()],
                        },
                        description: None,
                        env: None,
                    });

                if let Err(e) = mcp_host.start_server(&config).await {
                    return Ok(serde_json::json!({"skill_id": skill_id, "status": "error", "error": e.to_string()}));
                }
                let all_tools = mcp_host.list_tools();
                let server_tools: Vec<(String, String)> = all_tools.into_iter().filter(|(server, _)| server == &skill_id).collect();
                for (server, tool_name) in &server_tools {
                    let proxy_id = format!("{}:{}", server, tool_name);
                    registry.register(Box::new(McpSkillProxy {
                        id: proxy_id,
                        description: format!("MCP tool: {}", tool_name),
                        server_name: skill_id.clone(),
                        tool_name: tool_name.clone(),
                        mcp_host: mcp_host.clone(),
                    })).await;
                }

                return Ok(serde_json::json!({
                    "skill_id": skill_id,
                    "status": "installed",
                    "tools_count": server_tools.len(),
                    "tools": server_tools.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>(),
                }));
            }

            struct PlaceholderSkill { id: String }
            impl maple_engine::skill_registry::Skill for PlaceholderSkill {
                fn id(&self) -> &str { &self.id }
                fn description(&self) -> &str { "Placeholder skill" }
                fn execute(&self, config: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
                    Ok(serde_json::json!({"skill_id": self.id, "status": "placeholder", "input": config}))
                }
            }

            registry.register(Box::new(PlaceholderSkill { id: skill_id.clone() })).await;
            Ok(serde_json::json!({"skill_id": skill_id, "status": "installed_placeholder"}))
        }
    }).await;

    let s = state.clone();
    dispatcher
        .register(
            "skill.uninstall",
            move |params: Option<serde_json::Value>| {
                let registry = s.skill_registry.clone();
                let mcp_host = s.mcp_host.clone();
                async move {
                    let p = match params {
                        Some(v) => v,
                        None => return Ok(serde_json::json!({"error": "missing params"})),
                    };
                    let skill_id = p["skill_id"].as_str().unwrap_or("").to_string();
                    mcp_host.stop_server(&skill_id).await.ok();
                    registry.unregister(&skill_id).await;
                    let all_skills = registry.list().await;
                    for (skill_id_proxy, _) in &all_skills {
                        if skill_id_proxy.starts_with(&format!("{}:", skill_id)) {
                            registry.unregister(skill_id_proxy).await;
                        }
                    }
                    Ok(serde_json::json!({"skill_id": skill_id, "status": "uninstalled"}))
                }
            },
        )
        .await;
}
