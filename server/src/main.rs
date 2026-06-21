// All shared modules live in the lib crate (server/src/lib.rs) so that
// bin and integration tests share the same AppState / handler types.
// T0-4 in docs/MapleOS_Implementation_Plan_2026Q3.md tracks the
// incremental cleanup; previously each module was `mod foo;` here which
// produced a duplicate (structurally identical but distinct) copy in the
// bin crate, preventing lib handlers from being mounted into the bin's
// Router without adapter shims.
use mapleos_server::{cache, config, db, metrics, middleware, sandbox, skills, state, v3_auth};

use axum::Json;
use axum::Router;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::IntoResponse;
use axum::response::sse::{Event, Sse};
use axum::routing::{delete, get, post, put};
use std::convert::Infallible;

use async_trait::async_trait;
use maple_agent::performance::PerformanceMonitor;
use maple_agent::react_loop::{ReactLoop, Session, ToolExecutor, ToolResult, ToolUse};
use maple_agent::registry::AgentRegistry;
use maple_agent::security::SecurityManager;
use maple_agent::session_store::SessionStore;
use maple_agent::tool_use_context::ToolUseContext;
use maple_collab::group::GroupManager;
use maple_collab::group_message::GroupMessageManager;
use maple_collab::group_rules::{GroupRulesEngine, RuleContext};
use maple_collab::workspace::WorkspaceManager;
use maple_engine::approval::ApprovalService;
use maple_engine::checkpoint::CheckpointManager;
use maple_engine::memory_service::MemoryService;
use maple_engine::task_service::TaskService;
use maple_engine::event_bus::EventBus;
use maple_engine::executor::{NodeExecutor, WorkflowExecutor};
use maple_engine::hooks::HookRunner;
use maple_engine::scheduler::{ScheduledJob, Scheduler};
use maple_engine::skill_registry::SkillRegistry;
use maple_engine::task_queue::TaskQueueService;
use maple_engine::agent_hooks::CreateHookRequest;
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
use maple_rpc::dispatch::RpcDispatcher;
use maple_rpc::server::RpcServer;
use maple_sync::sync_engine::SyncEngine;
use serde::{Deserialize, Serialize};
use state::{AppState, ServerConfig};
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
    let evolver = state.evolver.clone();
    let user_msg = req.message.clone();

    let _ = state
        .session_store
        .save_message(&session_id, "user", &req.message, None, None)
        .await;

    // Evaluate group rules for auto-assign
    let mut route_key = if req.model != "auto" {
        req.model.to_string()
    } else {
        req.agent_id.clone().unwrap_or_else(|| "default".to_string())
    };
    {
        let rules = state.group_rules.read().await;
        let ctx = RuleContext {
            message: req.message.clone(),
            sender_id: session_id.clone(),
            sender_type: "user".to_string(),
            sender_role: "user".to_string(),
        };
        for m in rules.evaluate(&ctx) {
            if let maple_collab::group_rules::RuleAction::AssignToAgent { agent_id } = m.action {
                tracing::info!(agent_id = %agent_id, rule = %m.rule.name, "Group rule auto-assigned agent");
                route_key = agent_id;
                break;
            }
        }
    }

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

    let llm_request = maple_llm::request::LlmRequest::new(enhanced_message, &route_key);

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
                // Fire-and-forget: extract knowledge from valuable conversations
                let evolver_bg = evolver.clone();
                let sid_bg = session_id.clone();
                let user_bg = user_msg.clone();
                let assistant_bg = response.content.clone();
                tokio::spawn(async move {
                    if let Err(e) = evolver_bg.on_chat_complete(&sid_bg, &user_bg, &assistant_bg).await {
                        tracing::warn!(error = %e, "Chat knowledge precipitation failed");
                    }
                });
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

        let mut react_loop = ReactLoop::new(10)
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

                // Fire-and-forget: extract knowledge via evolver
                let evolver_bg = evolver.clone();
                let sid_bg = session_id.clone();
                let user_bg = user_msg.clone();
                let assistant_bg = summary.content.clone();
                tokio::spawn(async move {
                    if let Err(e) = evolver_bg.on_chat_complete(&sid_bg, &user_bg, &assistant_bg).await {
                        tracing::warn!(error = %e, "Chat knowledge precipitation failed");
                    }
                });

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

async fn ws_group_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .or_else(|| params.get("token").map(|s| s.as_str()))
        .unwrap_or("");

    let user_id = match state.auth_service.verify_agent_token(token).await {
        Ok(id) => id,
        Err(_) => {
            if state.config.read().await.require_auth {
                return ws.on_upgrade(move |_socket| async move {});
            }
            "anonymous".to_string()
        }
    };

    ws.on_upgrade(move |socket| {
        ws_gateway::handle_group_ws(socket, state.event_bus.clone(), user_id)
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

    // ── Track 1 / T1-3: open a unified execution fact chain ──
    // The execution_id is emitted to the client as the first SSE event
    // (event: execution) so the frontend can subscribe to
    // /api/v3/executions/:id/events/stream for a unified trace view
    // (see docs/execution-fact-chain-spec.md §7.1).
    let execution_recorder = state.execution_recorder.clone();
    let exec_id = execution_recorder
        .start(
            "chat",
            None, // user_id — not available without auth context; T1-3.1
                  // will wire this once auth middleware exposes user_id.
            Some("human"),
            "manual",
            serde_json::json!({
                "session_id": session_id,
                "message_preview": if req.message.len() > 200 {
                    req.message[..200].to_string()
                } else {
                    req.message.clone()
                },
                "agent_id": req.agent_id,
                "model": req.model,
            }),
            None,
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to start execution recorder");
            String::new()
        });

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
        // ── T1-3: emit execution_id first so client can subscribe ──
        if !exec_id.is_empty() {
            yield Ok(Event::default().event("execution").data(serde_json::json!({
                "execution_id": exec_id,
            }).to_string()));
        }
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
                                        if chunk.reasoning {
                                            yield Ok(Event::default().event("thinking").data(serde_json::json!({"token": chunk.delta}).to_string()));
                                        } else {
                                            full_content.push_str(&chunk.delta);
                                            // ── T1-3: append delta event to fact chain ──
                                            // (best-effort; failure is logged, not surfaced — the SSE
                                            // token stream is the primary delivery channel)
                                            if !exec_id.is_empty() {
                                                let _ = execution_recorder.append(
                                                    &exec_id,
                                                    "chat",
                                                    "delta",
                                                    serde_json::json!({
                                                        "token": chunk.delta,
                                                        "message_id": sid,
                                                    }),
                                                    None,
                                                    Some("human"),
                                                ).await;
                                            }
                                            yield Ok(Event::default().event("token").data(serde_json::json!({"token": chunk.delta}).to_string()));
                                        }
                                    }
                                    if chunk.finish_reason.is_some() {
                                        break;
                                    }
                                }
                                Ok(None) => break,
                                Err(e) => {
                                    yield Ok(Event::default().event("error").data(format!("Stream error: {}", e)));
                                    if !exec_id.is_empty() {
                                        let _ = execution_recorder.fail(
                                            &exec_id,
                                            &format!("LLM stream error: {e}"),
                                            true,
                                        ).await;
                                    }
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
                        // ── T1-3: mark execution done ──
                        if !exec_id.is_empty() {
                            let summary = if full_content.len() > 200 {
                                full_content[..200].to_string()
                            } else {
                                full_content.clone()
                            };
                            let _ = execution_recorder.done(&exec_id, &summary).await;
                        }
                        yield Ok(Event::default().event("done").data(serde_json::json!({"done": true, "execution_id": exec_id}).to_string()));
                    }
                    Err(e) => {
                        yield Ok(Event::default().event("error").data(format!("Stream init error: {}", e)));
                        if !exec_id.is_empty() {
                            let _ = execution_recorder.fail(
                                &exec_id,
                                &format!("LLM stream init error: {e}"),
                                true,
                            ).await;
                        }
                        yield Ok(Event::default().event("done").data("{\"done\":true}"));
                    }
                }
            }
            Err(e) => {
                yield Ok(Event::default().event("error").data(format!("No LLM available: {}", e)));
                if !exec_id.is_empty() {
                    let _ = execution_recorder.fail(
                        &exec_id,
                        &format!("No LLM available: {e}"),
                        false,
                    ).await;
                }
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

async fn feishu_webhook_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    // Feishu challenge/response verification
    if body.get("type").and_then(|v| v.as_str()) == Some("url_verification") {
        let challenge = body.get("challenge")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        return Ok(axum::Json(serde_json::json!({ "challenge": challenge })));
    }

    // Parse event callback (schema 2.0)
    let header = match body.get("header") {
        Some(h) => h,
        None => return Ok(axum::Json(serde_json::json!({ "status": "ignored" }))),
    };

    let event_type = header.get("event_type").and_then(|v| v.as_str()).unwrap_or("");

    match event_type {
        "im.message.receive_v1" => {
            let event = match body.get("event") {
                Some(e) => e,
                None => return Ok(axum::Json(serde_json::json!({ "status": "no_event" }))),
            };

            let message = event.get("message");
            let sender = event.get("sender");

            let chat_id = message
                .and_then(|m| m.get("chat_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let msg_type = message
                .and_then(|m| m.get("message_type"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let content_str = message
                .and_then(|m| m.get("content"))
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let sender_id = sender
                .and_then(|s| s.get("sender_id"))
                .and_then(|s| s.get("open_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Parse text content from Feishu's {"text":"..."} format
            let text = if msg_type == "text" {
                serde_json::from_str::<serde_json::Value>(content_str)
                    .ok()
                    .and_then(|c| c.get("text").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .unwrap_or_else(|| content_str.to_string())
            } else {
                format!("[{}]", msg_type)
            };

            tracing::info!(
                chat_id = chat_id,
                sender_id = sender_id,
                msg_type = msg_type,
                text = text,
                "Feishu message received"
            );

            // Route message through LLM pipeline
            let session_id = format!("feishu:{}:{}", chat_id, sender_id);
            let _ = state.session_store
                .save_message(&session_id, "user", &text, None, None)
                .await;

            // RAG enrichment
            let mut enhanced_message = text.clone();
            let query_embedding = match state.embedder.embed(&text).await {
                Ok(emb) => emb,
                Err(_) => maple_llm::embedding::simple_embedding(&text, 128),
            };
            let vector_results = state.vector_store.search(&query_embedding, 3).await;
            let bm25_results = state.bm25_searcher.search(&text, 3);
            let kb_results = state.hybrid_retriever
                .search(&text, 3, vector_results, bm25_results)
                .await
                .unwrap_or_default();

            if !kb_results.is_empty() {
                let context: Vec<String> = kb_results.iter().map(|r| r.content.clone()).collect();
                enhanced_message = format!(
                    "[Knowledge Base Context]\n{}\n---\n[User Question]\n{}",
                    context.join("\n---\n"),
                    text
                );
            }

            // Synchronous LLM completion
            let llm_request = maple_llm::request::LlmRequest::new(enhanced_message, "default");
            let response_text = match state.llm_router.route(&llm_request).await {
                Ok(adapter) => {
                    match adapter.complete(llm_request).await {
                        Ok(resp) => resp.text(),
                        Err(e) => {
                            tracing::error!(error = %e, "LLM completion failed for Feishu message");
                            "Sorry, I encountered an error processing your message.".to_string()
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "LLM routing failed for Feishu message");
                    "Sorry, no LLM provider available.".to_string()
                }
            };

            let _ = state.session_store
                .save_message(&session_id, "assistant", &response_text, None, None)
                .await;

            tracing::info!(
                session_id = session_id,
                response_len = response_text.len(),
                "Feishu message processed"
            );

            Ok(axum::Json(serde_json::json!({
                "status": "received",
                "chat_id": chat_id,
                "sender_id": sender_id,
                "session_id": session_id,
            })))
        }
        _ => {
            tracing::debug!(event_type = event_type, "Ignoring Feishu event");
            Ok(axum::Json(serde_json::json!({ "status": "ignored" })))
        }
    }
}

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
        let parser = maple_kb::DocumentParserRegistry::new();
        let content = parser.parse_by_extension(&filename, &bytes)
            .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

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

async fn kb_delete_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(doc_id): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    let result = sqlx::query("DELETE FROM kb_documents WHERE id = ?")
        .bind(&doc_id)
        .execute(&state.db)
        .await;
    match result {
        Ok(r) => axum::Json(serde_json::json!({ "deleted": r.rows_affected() > 0, "id": doc_id })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
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

async fn sse_group_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let group_ids: Vec<String> = params
        .get("group_id")
        .map(|s| s.split(',').map(|g| g.trim().to_string()).filter(|g| !g.is_empty()).collect())
        .unwrap_or_default();
    sse_gateway::handle_group_sse(state.event_bus.clone(), group_ids).await
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
struct DeviceLoginRequest {
    device_id: String,
}

async fn device_login_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<DeviceLoginRequest>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let device_id = req.device_id.trim().to_string();
    if device_id.is_empty() {
        return Err(axum::http::StatusCode::BAD_REQUEST);
    }

    let username = format!("device-{}", device_id);
    let now = chrono::Utc::now().timestamp();

    // Try to find existing device user
    let existing: Option<(String, String)> =
        sqlx::query_as("SELECT id, role FROM users WHERE username = ?")
            .bind(&username)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let (user_id, role) = if let Some((id, role)) = existing {
        (id, role)
    } else {
        // Create a new device user with a random password hash (not used for login)
        let user_id = uuid::Uuid::new_v4().to_string();
        let password_hash = bcrypt::hash(&uuid::Uuid::new_v4().to_string(), bcrypt::DEFAULT_COST)
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

        sqlx::query(
            "INSERT INTO users (id, username, password_hash, email, role, created_at) VALUES (?, ?, ?, ?, 'user', ?)"
        )
        .bind(&user_id)
        .bind(&username)
        .bind(&password_hash)
        .bind(format!("{}@device.local", username))
        .bind(now)
        .execute(&state.db)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

        (user_id, "user".to_string())
    };

    let token = state
        .auth_service
        .create_token_for_user(&user_id, &role, 3600)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let refresh_token = uuid::Uuid::new_v4().to_string();
    let refresh_hash = bcrypt::hash(&refresh_token, bcrypt::DEFAULT_COST)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
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

    Ok(axum::Json(serde_json::json!({
        "token": token,
        "refresh_token": refresh_token,
        "user_id": user_id,
        "username": username,
        "role": role,
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

// ── Workspace CRUD ────────────────────────────────────────────────

async fn list_workspaces_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    let rows: Vec<(String, String, Option<String>, String, i64)> =
        sqlx::query_as("SELECT id, name, description, owner_id, created_at FROM workspaces ORDER BY created_at DESC")
            .fetch_all(&state.db).await.unwrap_or_default();
    let workspaces: Vec<serde_json::Value> = rows.into_iter()
        .map(|(id, name, desc, owner, created)| {
            serde_json::json!({ "id": id, "name": name, "description": desc, "owner_id": owner, "created_at": created })
        }).collect();
    axum::Json(serde_json::json!({ "workspaces": workspaces }))
}

#[derive(Debug, Deserialize)]
struct CreateWorkspaceRequest {
    name: String,
    description: Option<String>,
    owner_id: Option<String>,
}

async fn create_workspace_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> axum::Json<serde_json::Value> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let owner = req.owner_id.unwrap_or_else(|| "default-user".to_string());
    let _ = sqlx::query(
        "INSERT INTO workspaces (id, name, description, owner_id, created_at) VALUES (?, ?, ?, ?, ?)"
    ).bind(&id).bind(&req.name).bind(&req.description).bind(&owner).bind(now)
    .execute(&state.db).await;
    axum::Json(serde_json::json!({ "id": id, "name": req.name }))
}

// ── Workspace Members ─────────────────────────────────────────────

async fn list_workspace_members_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(workspace_id): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    let rows: Vec<(String, String, String, String)> =
        sqlx::query_as("SELECT member_id, name, member_type, role FROM workspace_members WHERE workspace_id = ?")
            .bind(&workspace_id)
            .fetch_all(&state.db).await.unwrap_or_default();
    let members: Vec<serde_json::Value> = rows.into_iter()
        .map(|(id, name, mtype, role)| {
            serde_json::json!({ "id": id, "name": name, "member_type": mtype, "role": role })
        }).collect();
    axum::Json(serde_json::json!({ "members": members }))
}

#[derive(Debug, Deserialize)]
struct AddWorkspaceMemberRequest {
    member_id: String,
    name: String,
    member_type: Option<String>,
    role: Option<String>,
}

async fn add_workspace_member_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(workspace_id): axum::extract::Path<String>,
    Json(req): Json<AddWorkspaceMemberRequest>,
) -> axum::Json<serde_json::Value> {
    let mtype = req.member_type.unwrap_or_else(|| "human".to_string());
    let role = req.role.unwrap_or_else(|| "member".to_string());
    let _ = sqlx::query(
        "INSERT OR REPLACE INTO workspace_members (workspace_id, member_id, name, member_type, role) VALUES (?, ?, ?, ?, ?)"
    ).bind(&workspace_id).bind(&req.member_id).bind(&req.name).bind(&mtype).bind(&role)
    .execute(&state.db).await;
    axum::Json(serde_json::json!({ "workspace_id": workspace_id, "member_id": req.member_id, "status": "added" }))
}

async fn remove_workspace_member_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((workspace_id, member_id)): axum::extract::Path<(String, String)>,
) -> axum::Json<serde_json::Value> {
    let _ = sqlx::query(
        "DELETE FROM workspace_members WHERE workspace_id = ? AND member_id = ? AND role != 'owner'"
    ).bind(&workspace_id).bind(&member_id)
    .execute(&state.db).await;
    axum::Json(serde_json::json!({ "workspace_id": workspace_id, "member_id": member_id, "status": "removed" }))
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
    #[allow(clippy::type_complexity)]
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
                    "assignee": assignee_name.map(|name| serde_json::json!({"name": name, "avatar": assignee_avatar})),
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

    state.event_bus.publish(maple_engine::event_bus::Event::TaskCreated {
        task_id: id.clone(),
        title: req.title.clone(),
        status: status.clone(),
    }).await;

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
        state.event_bus.publish(maple_engine::event_bus::Event::TaskUpdated {
            task_id: task_id.clone(),
            title: req.title.unwrap_or_default(),
            status: req.status.unwrap_or_default(),
        }).await;
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
        state.event_bus.publish(maple_engine::event_bus::Event::TaskDeleted {
            task_id: task_id.clone(),
        }).await;
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
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, Option<String>, String, Option<String>, Option<String>, String, i64, i64)> =
        sqlx::query_as("SELECT id, parent_id, author_name, author_avatar, author_role, content, likes, created_at FROM board_comments WHERE task_id = ? ORDER BY created_at DESC")
            .bind(&task_id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

    // Separate top-level comments and replies
    let mut top_level: Vec<serde_json::Value> = Vec::new();
    let mut replies: std::collections::HashMap<String, Vec<serde_json::Value>> = std::collections::HashMap::new();

    for (id, parent_id, author_name, author_avatar, author_role, content, likes, created_at) in rows {
        let comment = serde_json::json!({
            "id": id,
            "author": { "name": author_name, "avatar": author_avatar, "role": author_role },
            "content": content,
            "likes": likes,
            "created_at": created_at,
        });
        if let Some(pid) = &parent_id {
            replies.entry(pid.clone()).or_default().push(comment);
        } else {
            top_level.push(comment);
        }
    }

    // Attach replies to their parent comments
    let comments: Vec<serde_json::Value> = top_level
        .into_iter()
        .map(|mut c| {
            let cid = c["id"].as_str().unwrap_or_default().to_string();
            let r = replies.remove(&cid).unwrap_or_default();
            c["replies"] = serde_json::json!(r);
            c
        })
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

    state.event_bus.publish(maple_engine::event_bus::Event::CommentCreated {
        comment_id: id.clone(),
        task_id: req.task_id.clone(),
        author: req.author_name.clone(),
    }).await;

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

#[derive(Debug, Deserialize)]
struct UpdateCommentRequest {
    content: String,
}

async fn update_comment_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(comment_id): axum::extract::Path<String>,
    Json(req): Json<UpdateCommentRequest>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let result = sqlx::query("UPDATE board_comments SET content = ? WHERE id = ?")
        .bind(&req.content).bind(&comment_id)
        .execute(&state.db).await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() > 0 {
        Ok(axum::Json(serde_json::json!({ "id": comment_id, "updated": true })))
    } else {
        Err(axum::http::StatusCode::NOT_FOUND)
    }
}

// ── Activity Feed ─────────────────────────────────────────────────

async fn list_activity_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    let rows: Vec<(i64, String, String, Option<String>, Option<String>, i64)> =
        sqlx::query_as("SELECT id, actor_name, action, target, details, created_at FROM activity_log ORDER BY created_at DESC LIMIT 50")
            .fetch_all(&state.db).await.unwrap_or_default();
    let activities: Vec<serde_json::Value> = rows.into_iter()
        .map(|(id, actor, action, target, details, created_at)| {
            serde_json::json!({
                "id": id,
                "actor": actor,
                "action": action,
                "target": target,
                "details": details.and_then(|d| serde_json::from_str::<serde_json::Value>(&d).ok()),
                "created_at": created_at,
            })
        }).collect();
    axum::Json(serde_json::json!({ "activities": activities }))
}

async fn create_activity_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    let now = chrono::Utc::now().timestamp();
    let actor = req["actor"].as_str().unwrap_or("system").to_string();
    let action = req["action"].as_str().unwrap_or("unknown").to_string();
    let target = req["target"].as_str().map(|s| s.to_string());
    let details = req["details"].as_object().map(|_| req["details"].to_string());
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO activity_log (workspace_id, actor_name, action, target, details, created_at) VALUES ('default', ?, ?, ?, ?, ?) RETURNING id"
    ).bind(&actor).bind(&action).bind(&target).bind(&details).bind(now)
    .fetch_one(&state.db).await.unwrap_or(0);

    state.event_bus.publish(maple_engine::event_bus::Event::ActivityLogged {
        actor: actor.clone(),
        action: action.clone(),
        target: target.clone(),
    }).await;

    axum::Json(serde_json::json!({ "id": id }))
}

// ── Board Attachments ──────────────────────────────────────────────

async fn upload_attachment_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
    multipart: axum::extract::Multipart,
) -> axum::Json<serde_json::Value> {
    let now = chrono::Utc::now().timestamp();
    let mut uploaded = Vec::new();
    let mut mp = multipart;

    while let Some(mut field) = mp.next_field().await.unwrap_or(None) {
        let filename = field.file_name().unwrap_or("unknown").to_string();
        let content_type = field.content_type().unwrap_or("application/octet-stream").to_string();
        let mut data = Vec::new();
        while let Some(chunk) = field.chunk().await.unwrap_or(None) {
            data.extend_from_slice(&chunk);
        }
        let size = data.len() as i64;
        let id = format!("att-{}", uuid::Uuid::new_v4());
        let _ = sqlx::query(
            "INSERT INTO board_attachments (id, task_id, filename, content_type, size, data, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)"
        ).bind(&id).bind(&task_id).bind(&filename).bind(&content_type).bind(size).bind(&data).bind(now)
        .execute(&state.db).await;
        uploaded.push(serde_json::json!({ "id": id, "filename": filename, "size": size, "content_type": content_type }));
    }

    axum::Json(serde_json::json!({ "uploaded": uploaded }))
}

async fn list_attachments_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    let rows: Vec<(String, String, String, i64, i64)> = sqlx::query_as(
        "SELECT id, filename, content_type, size, created_at FROM board_attachments WHERE task_id = ? ORDER BY created_at DESC"
    ).bind(&task_id).fetch_all(&state.db).await.unwrap_or_default();

    let attachments: Vec<serde_json::Value> = rows.into_iter()
        .map(|(id, filename, ct, size, created)| serde_json::json!({
            "id": id, "filename": filename, "content_type": ct, "size": size, "created_at": created
        })).collect();

    axum::Json(serde_json::json!({ "attachments": attachments }))
}

async fn download_attachment_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(att_id): axum::extract::Path<String>,
) -> Result<(axum::http::HeaderMap, Vec<u8>), axum::http::StatusCode> {
    let row: Option<(String, String, Vec<u8>)> = sqlx::query_as(
        "SELECT filename, content_type, data FROM board_attachments WHERE id = ?"
    ).bind(&att_id).fetch_optional(&state.db).await.map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    match row {
        Some((filename, ct, data)) => {
            let mut headers = axum::http::HeaderMap::new();
            headers.insert("content-type", ct.parse().unwrap_or_else(|_| "application/octet-stream".parse().unwrap()));
            headers.insert("content-disposition", format!("attachment; filename=\"{}\"", filename).parse().unwrap());
            Ok((headers, data))
        }
        None => Err(axum::http::StatusCode::NOT_FOUND),
    }
}

async fn delete_attachment_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(att_id): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    let result = sqlx::query("DELETE FROM board_attachments WHERE id = ?")
        .bind(&att_id).execute(&state.db).await;
    match result {
        Ok(r) => axum::Json(serde_json::json!({ "deleted": r.rows_affected() > 0 })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

// ── Message Attachments ─────────────────────────────────────────

async fn v3_upload_message_attachment_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    auth_user: Option<axum::extract::Extension<v3_auth::AuthenticatedUser>>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
    multipart: axum::extract::Multipart,
) -> axum::Json<serde_json::Value> {
    let now = chrono::Utc::now().timestamp();
    let mut uploaded = Vec::new();
    let mut mp = multipart;
    let uploader_id = auth_user
        .as_ref()
        .map(|u| u.user_id.clone())
        .unwrap_or_else(|| "anonymous".to_string());

    while let Some(mut field) = mp.next_field().await.unwrap_or(None) {
        let filename = field.file_name().unwrap_or("unknown").to_string();
        let content_type = field.content_type().unwrap_or("application/octet-stream").to_string();
        let mut data = Vec::new();
        while let Some(chunk) = field.chunk().await.unwrap_or(None) {
            data.extend_from_slice(&chunk);
        }
        let size = data.len() as i64;
        let id = format!("msgatt-{}", uuid::Uuid::new_v4());
        let _ = sqlx::query(
            "INSERT INTO message_attachments (id, group_id, message_id, uploader_id, filename, content_type, size, data, created_at) VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?)"
        ).bind(&id).bind(&group_id).bind(&uploader_id).bind(&filename).bind(&content_type).bind(size).bind(&data).bind(now)
        .execute(&state.db).await;
        uploaded.push(serde_json::json!({ "id": id, "filename": filename, "size": size, "content_type": content_type }));
    }

    axum::Json(serde_json::json!({ "attachments": uploaded }))
}

async fn v3_list_message_attachments_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    let rows: Vec<(String, String, String, Option<String>, String, i64, i64)> = sqlx::query_as(
        "SELECT id, filename, content_type, message_id, uploader_id, size, created_at FROM message_attachments WHERE group_id = ? ORDER BY created_at DESC LIMIT 50"
    ).bind(&group_id).fetch_all(&state.db).await.unwrap_or_default();

    let attachments: Vec<serde_json::Value> = rows.into_iter()
        .map(|(id, filename, ct, msg_id, uploader, size, created)| serde_json::json!({
            "id": id, "filename": filename, "content_type": ct,
            "message_id": msg_id, "uploader_id": uploader, "size": size, "created_at": created
        })).collect();

    axum::Json(serde_json::json!({ "attachments": attachments }))
}

async fn v3_download_message_attachment_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(att_id): axum::extract::Path<String>,
) -> Result<(axum::http::HeaderMap, Vec<u8>), axum::http::StatusCode> {
    let row: Option<(String, String, Vec<u8>)> = sqlx::query_as(
        "SELECT filename, content_type, data FROM message_attachments WHERE id = ?"
    ).bind(&att_id).fetch_optional(&state.db).await.map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    match row {
        Some((filename, ct, data)) => {
            let mut headers = axum::http::HeaderMap::new();
            headers.insert("content-type", ct.parse().unwrap_or_else(|_| "application/octet-stream".parse().unwrap()));
            headers.insert("content-disposition", format!("attachment; filename=\"{}\"", filename).parse().unwrap());
            Ok((headers, data))
        }
        None => Err(axum::http::StatusCode::NOT_FOUND),
    }
}

async fn v3_delete_message_attachment_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(att_id): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    let result = sqlx::query("DELETE FROM message_attachments WHERE id = ?")
        .bind(&att_id).execute(&state.db).await;
    let deleted = result.map(|r| r.rows_affected() > 0).unwrap_or(false);
    axum::Json(serde_json::json!({ "deleted": deleted }))
}

#[derive(serde::Deserialize)]
struct LinkAttachmentRequest {
    message_id: String,
}

async fn v3_link_attachment_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(att_id): axum::extract::Path<String>,
    Json(req): Json<LinkAttachmentRequest>,
) -> axum::Json<serde_json::Value> {
    let result = sqlx::query("UPDATE message_attachments SET message_id = ? WHERE id = ?")
        .bind(&req.message_id).bind(&att_id).execute(&state.db).await;
    let updated = result.map(|r| r.rows_affected() > 0).unwrap_or(false);
    axum::Json(serde_json::json!({ "linked": updated }))
}

// ── Agent Hooks Handlers ─────────────────────────────────

async fn v3_list_hooks_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    match state.hook_service.list_hooks(&group_id).await {
        Ok(hooks) => axum::Json(serde_json::json!({ "hooks": hooks })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_create_hook_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
    axum::Json(req): axum::Json<CreateHookRequest>,
) -> impl axum::response::IntoResponse {
    match state.hook_service.create_hook(&group_id, &req).await {
        Ok(hook) => (axum::http::StatusCode::CREATED, axum::Json(serde_json::json!({ "hook": hook }))),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({ "error": e.to_string() }))),
    }
}

async fn v3_get_hook_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((_group_id, hook_id)): axum::extract::Path<(String, String)>,
) -> impl axum::response::IntoResponse {
    match state.hook_service.get_hook(&hook_id).await {
        Ok(Some(hook)) => axum::Json(serde_json::json!({ "hook": hook })).into_response(),
        Ok(None) => (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({ "error": "not found" }))).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn v3_update_hook_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((_group_id, hook_id)): axum::extract::Path<(String, String)>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    match state.hook_service.update_hook(&hook_id, &body).await {
        Ok(updated) => axum::Json(serde_json::json!({ "updated": updated })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_delete_hook_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((_group_id, hook_id)): axum::extract::Path<(String, String)>,
) -> axum::Json<serde_json::Value> {
    match state.hook_service.delete_hook(&hook_id).await {
        Ok(deleted) => axum::Json(serde_json::json!({ "deleted": deleted })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_list_hook_logs_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((_group_id, hook_id)): axum::extract::Path<(String, String)>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> axum::Json<serde_json::Value> {
    let limit = params.get("limit").and_then(|v| v.parse::<i64>().ok()).unwrap_or(50);
    match state.hook_service.list_logs(&hook_id, limit).await {
        Ok(logs) => axum::Json(serde_json::json!({ "logs": logs })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

// ── Workflow Definition Handlers ──

#[derive(Deserialize)]
struct V3CreateWorkflowRequest { id: String, name: String, yaml_content: String }
#[derive(Deserialize)]
struct V3UpdateWorkflowRequest { name: Option<String>, yaml_content: Option<String>, status: Option<String> }

async fn v3_list_workflows_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    match state.workflow_service.list_definitions().await {
        Ok(defs) => axum::Json(serde_json::json!({ "workflows": defs })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_create_workflow_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::Json(req): axum::Json<V3CreateWorkflowRequest>,
) -> impl axum::response::IntoResponse {
    match state.workflow_service.create_definition(&req.id, &req.name, &req.yaml_content).await {
        Ok(def) => (axum::http::StatusCode::CREATED, axum::Json(serde_json::json!({ "workflow": def }))),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({ "error": e.to_string() }))),
    }
}

async fn v3_get_workflow_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(wid): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    match state.workflow_service.get_definition(&wid).await {
        Ok(Some(def)) => (axum::http::StatusCode::OK, axum::Json(serde_json::json!({ "workflow": def }))),
        Ok(None) => (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({ "error": "not found" }))),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({ "error": e.to_string() }))),
    }
}

async fn v3_update_workflow_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(wid): axum::extract::Path<String>,
    axum::Json(req): axum::Json<V3UpdateWorkflowRequest>,
) -> axum::Json<serde_json::Value> {
    match state.workflow_service.update_definition(&wid, req.name.as_deref(), req.yaml_content.as_deref(), req.status.as_deref()).await {
        Ok(updated) => axum::Json(serde_json::json!({ "updated": updated })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_delete_workflow_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(wid): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    match state.workflow_service.delete_definition(&wid).await {
        Ok(deleted) => axum::Json(serde_json::json!({ "deleted": deleted })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

// ── Workflow Run Handlers ──

#[derive(Deserialize)]
struct CreateRunRequest { workflow_id: String, workflow_version: i64, input: String, group_id: Option<String>, agent_id: Option<String> }
#[derive(Deserialize)]
struct UpdateRunStatusRequest { status: String, output: Option<String>, error: Option<String> }
#[derive(Deserialize)]
struct RecordCheckpointRequest { node_id: String, output: String, context_snapshot: String }

async fn v3_list_workflow_runs_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> axum::Json<serde_json::Value> {
    let limit = params.get("limit").and_then(|v| v.parse::<i64>().ok());
    match state.workflow_service.list_runs(
        params.get("workflow_id").map(|s| s.as_str()),
        params.get("group_id").map(|s| s.as_str()),
        params.get("status").map(|s| s.as_str()),
        limit,
    ).await {
        Ok(runs) => axum::Json(serde_json::json!({ "runs": runs })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_create_workflow_run_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::Json(req): axum::Json<CreateRunRequest>,
) -> impl axum::response::IntoResponse {
    // ── Track 1 / T1-4: open a unified execution fact chain ──
    // The execution_id is returned to the client so the Workflow trace UI
    // can subscribe to /api/v3/executions/:id/events for a unified view
    // (see docs/execution-fact-chain-spec.md §7.2).
    let trigger_type = "manual"; // this handler is the manual trigger entry
                                 // point; cron/webhook/event/message triggers
                                 // go through scheduler and will be wired in T1-4.2
    let exec_id = state
        .execution_recorder
        .start(
            "workflow",
            None, // actor — auth context not wired yet (T1-3.1)
            Some("human"),
            trigger_type,
            serde_json::json!({
                "workflow_id": req.workflow_id,
                "workflow_version": req.workflow_version,
                "group_id": req.group_id,
                "agent_id": req.agent_id,
                "input_summary": serde_json::to_string(&req.input)
                    .unwrap_or_default()
                    .chars()
                    .take(200)
                    .collect::<String>(),
            }),
            None,
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to start workflow execution recorder");
            String::new()
        });

    match state.workflow_service.create_run(&req.workflow_id, req.workflow_version, &req.input, req.group_id.as_deref(), req.agent_id.as_deref()).await {
        Ok(run) => {
            // Append a 'started' artifact event linking the workflow run id
            // to the execution id, so the workflow UI can later look up
            // the unified trace by either id.
            if !exec_id.is_empty() {
                let _ = state.execution_recorder.append(
                    &exec_id,
                    "workflow",
                    "started",
                    serde_json::json!({
                        "workflow_run_id": run.id,
                        "workflow_id": req.workflow_id,
                        "status": run.status,
                    }),
                    None,
                    Some("human"),
                ).await;
            }
            (axum::http::StatusCode::CREATED, axum::Json(serde_json::json!({
                "run": run,
                "execution_id": exec_id,
            })))
        }
        Err(e) => {
            if !exec_id.is_empty() {
                let _ = state.execution_recorder.fail(
                    &exec_id,
                    &format!("workflow create_run failed: {e}"),
                    false,
                ).await;
            }
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({ "error": e.to_string() })))
        }
    }
}

async fn v3_get_workflow_run_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(rid): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    match state.workflow_service.get_run(&rid).await {
        Ok(Some(run)) => (axum::http::StatusCode::OK, axum::Json(serde_json::json!({ "run": run }))),
        Ok(None) => (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({ "error": "not found" }))),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({ "error": e.to_string() }))),
    }
}

async fn v3_update_workflow_run_status_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(rid): axum::extract::Path<String>,
    axum::Json(req): axum::Json<UpdateRunStatusRequest>,
) -> axum::Json<serde_json::Value> {
    // ── Track 1 / T1-4: workflow status changes project onto execution
    // events. We look up the most recent execution whose 'started' event
    // payload references this workflow_run_id and append a corresponding
    // event (paused / resumed / done / error / cancelled).
    // See docs/execution-fact-chain-spec.md §7.2.
    let exec_id: Option<String> = sqlx::query_scalar::<_, String>(
        "SELECT execution_id FROM execution_events
          WHERE event_type = 'started'
            AND source = 'workflow'
            AND json_extract(payload, '$.workflow_run_id') = ?
          ORDER BY created_at DESC LIMIT 1"
    )
    .bind(&rid)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    if let (Some(eid), Some(recorder_exec_id)) = (&exec_id, exec_id.as_ref()) {
        let event_type = match req.status.as_str() {
            "running" => Some("resumed"),
            "paused" | "waiting_approval" => Some("paused"),
            "success" | "completed" => Some("done"),
            "failed" => Some("error"),
            "cancelled" => Some("cancelled"),
            _ => None,
        };
        if let Some(et) = event_type {
            let payload = serde_json::json!({
                "workflow_run_id": rid,
                "status": req.status,
                "output": req.output,
                "error": req.error,
            });
            let _ = state.execution_recorder.append(
                recorder_exec_id,
                "workflow",
                et,
                payload,
                None,
                Some("human"),
            ).await;
        }
    }

    match state.workflow_service.update_run_status(&rid, &req.status, req.output.as_deref(), req.error.as_deref()).await {
        Ok(updated) => axum::Json(serde_json::json!({ "updated": updated, "execution_id": exec_id })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_list_checkpoints_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(rid): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    match state.workflow_service.list_checkpoints(&rid).await {
        Ok(cps) => axum::Json(serde_json::json!({ "checkpoints": cps })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_record_checkpoint_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(rid): axum::extract::Path<String>,
    axum::Json(req): axum::Json<RecordCheckpointRequest>,
) -> impl axum::response::IntoResponse {
    match state.workflow_service.record_checkpoint(&rid, &req.node_id, &req.output, &req.context_snapshot).await {
        Ok(id) => (axum::http::StatusCode::CREATED, axum::Json(serde_json::json!({ "id": id }))),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({ "error": e.to_string() }))),
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

    let mut agent_details: Vec<serde_json::Value> = Vec::new();
    for (id, name, status) in &agents {
        let last_hb: Option<i64> = sqlx::query_scalar(
            "SELECT last_heartbeat FROM agents WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
        agent_details.push(serde_json::json!({
            "id": id,
            "name": name,
            "status": format!("{:?}", status),
            "is_online": *status == maple_agent::registry::AgentStatus::Online,
            "last_heartbeat": last_hb,
        }));
    }

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
            // Persist heartbeat to DB
            let now = chrono::Utc::now().timestamp();
            let _ = sqlx::query("UPDATE agents SET last_heartbeat = ?, status = 'online' WHERE id = ?")
                .bind(now)
                .bind(&agent_id)
                .execute(&state.db)
                .await;
            Ok(axum::Json(serde_json::json!({
                "agent_id": agent_id,
                "status": "ok",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            })))
        }
        None => Err(axum::http::StatusCode::NOT_FOUND),
    }
}

// ── Scheduler CRUD handlers ──

async fn list_scheduler_jobs_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    let jobs = state.scheduler.list_jobs().await;
    axum::Json(serde_json::json!({
        "jobs": jobs,
    }))
}

#[derive(Debug, Deserialize)]
struct CreateSchedulerJobRequest {
    workflow_id: String,
    cron_expr: String,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
}

async fn create_scheduler_job_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<CreateSchedulerJobRequest>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let job_id = format!("job-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("x"));
    let now = chrono::Utc::now().timestamp();
    let enabled = req.enabled.unwrap_or(true);
    let timezone = req.timezone.unwrap_or_else(|| "UTC".to_string());

    // Compute next_run_at from cron
    let next_run = maple_engine::scheduler::next_timestamp_from_cron(&req.cron_expr, now)
        .unwrap_or(now + 3600);

    let _ = sqlx::query(
        "INSERT INTO scheduled_jobs (id, workflow_id, cron_expr, timezone, next_run_at, enabled) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(&job_id)
    .bind(&req.workflow_id)
    .bind(&req.cron_expr)
    .bind(&timezone)
    .bind(next_run)
    .bind(enabled)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create scheduled job: {}", e);
        e
    });

    let job = maple_engine::scheduler::ScheduledJob {
        id: job_id.clone(),
        workflow_id: req.workflow_id.clone(),
        cron_expr: req.cron_expr.clone(),
        timezone,
        last_run_at: None,
        next_run_at: next_run,
        enabled,
    };
    if enabled {
        let _ = state.scheduler.add_job(job).await;
    }

    Ok(axum::Json(serde_json::json!({
        "id": job_id,
        "workflow_id": req.workflow_id,
        "cron_expr": req.cron_expr,
        "enabled": enabled,
        "next_run_at": next_run,
    })))
}

#[derive(Debug, Deserialize)]
struct UpdateSchedulerJobRequest {
    cron_expr: Option<String>,
    enabled: Option<bool>,
}

async fn update_scheduler_job_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(job_id): axum::extract::Path<String>,
    Json(req): Json<UpdateSchedulerJobRequest>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let now = chrono::Utc::now().timestamp();

    if let Some(ref cron) = req.cron_expr {
        let next_run = maple_engine::scheduler::next_timestamp_from_cron(cron, now).unwrap_or(now + 3600);
        let _ = sqlx::query("UPDATE scheduled_jobs SET cron_expr = ?, next_run_at = ? WHERE id = ?")
            .bind(cron)
            .bind(next_run)
            .bind(&job_id)
            .execute(&state.db)
            .await;
        // Update in-memory scheduler
        state.scheduler.remove_job(&job_id).await.ok();
        // Re-add with updated cron
        if let Some(job) = sqlx::query_as::<_, (String, String, String, String, Option<i64>, i64, bool)>(
            "SELECT id, workflow_id, cron_expr, timezone, last_run_at, next_run_at, enabled FROM scheduled_jobs WHERE id = ?"
        )
        .bind(&job_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten() {
            let _ = state.scheduler.add_job(maple_engine::scheduler::ScheduledJob {
                id: job.0, workflow_id: job.1, cron_expr: job.2, timezone: job.3,
                last_run_at: job.4, next_run_at: job.5, enabled: job.6,
            }).await;
        }
    }

    if let Some(enabled) = req.enabled {
        let _ = sqlx::query("UPDATE scheduled_jobs SET enabled = ? WHERE id = ?")
            .bind(enabled)
            .bind(&job_id)
            .execute(&state.db)
            .await;
        if enabled {
            // Re-add to scheduler if enabling
            if let Some(job) = sqlx::query_as::<_, (String, String, String, String, Option<i64>, i64, bool)>(
                "SELECT id, workflow_id, cron_expr, timezone, last_run_at, next_run_at, enabled FROM scheduled_jobs WHERE id = ?"
            )
            .bind(&job_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten() {
                state.scheduler.remove_job(&job_id).await.ok();
                let _ = state.scheduler.add_job(maple_engine::scheduler::ScheduledJob {
                    id: job.0, workflow_id: job.1, cron_expr: job.2, timezone: job.3,
                    last_run_at: job.4, next_run_at: job.5, enabled: job.6,
                }).await;
            }
        } else {
            state.scheduler.remove_job(&job_id).await.ok();
        }
    }

    Ok(axum::Json(serde_json::json!({ "id": job_id, "status": "updated" })))
}

async fn delete_scheduler_job_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(job_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let _ = sqlx::query("DELETE FROM scheduled_jobs WHERE id = ?")
        .bind(&job_id)
        .execute(&state.db)
        .await;
    state.scheduler.remove_job(&job_id).await.ok();
    Ok(axum::Json(serde_json::json!({ "id": job_id, "status": "deleted" })))
}

// ── Group Rules CRUD ──────────────────────────────────────────────

async fn list_group_rules_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    let rules = state.group_rules.read().await;
    let rules_list = rules.list_rules();
    axum::Json(serde_json::json!({ "rules": rules_list }))
}

async fn create_group_rule_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(rule): Json<maple_collab::group_rules::GroupRule>,
) -> axum::Json<serde_json::Value> {
    let id = rule.id.clone();
    let now = chrono::Utc::now().timestamp();
    let rule_type_json = serde_json::to_string(&rule.rule_type).unwrap_or_default();
    let _ = sqlx::query(
        "INSERT OR REPLACE INTO group_rules (id, name, rule_type, enabled, created_at) VALUES (?, ?, ?, ?, ?)"
    ).bind(&id).bind(&rule.name).bind(&rule_type_json).bind(rule.enabled).bind(now)
    .execute(&state.db).await;
    let mut engine = state.group_rules.write().await;
    engine.add_rule(rule);
    axum::Json(serde_json::json!({ "id": id, "status": "created" }))
}

async fn get_group_rule_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(rule_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let rules = state.group_rules.read().await;
    match rules.get_rule(&rule_id) {
        Some(rule) => Ok(axum::Json(serde_json::json!(rule))),
        None => Err(axum::http::StatusCode::NOT_FOUND),
    }
}

async fn update_group_rule_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(rule_id): axum::extract::Path<String>,
    Json(rule): Json<maple_collab::group_rules::GroupRule>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let rule_type_json = serde_json::to_string(&rule.rule_type).unwrap_or_default();
    let _ = sqlx::query("UPDATE group_rules SET name = ?, rule_type = ?, enabled = ? WHERE id = ?")
        .bind(&rule.name).bind(&rule_type_json).bind(rule.enabled).bind(&rule_id)
        .execute(&state.db).await;
    let mut engine = state.group_rules.write().await;
    if engine.update_rule(&rule_id, rule) {
        Ok(axum::Json(serde_json::json!({ "id": rule_id, "status": "updated" })))
    } else {
        Err(axum::http::StatusCode::NOT_FOUND)
    }
}

async fn delete_group_rule_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(rule_id): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    let _ = sqlx::query("DELETE FROM group_rules WHERE id = ?")
        .bind(&rule_id).execute(&state.db).await;
    let mut engine = state.group_rules.write().await;
    let removed = engine.remove_rule(&rule_id);
    axum::Json(serde_json::json!({ "id": rule_id, "status": if removed { "deleted" } else { "not_found" } }))
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
    skills: Option<Vec<String>>,
    #[serde(default)]
    supports_image: Option<bool>,
    #[serde(default)]
    supports_streaming: Option<bool>,
    #[serde(default)]
    max_context_length: Option<usize>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    triggers: Option<serde_json::Value>,
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
        "skills": req.skills.unwrap_or_default(),
        "max_context_length": req.max_context_length.unwrap_or(128_000),
        "supports_streaming": req.supports_streaming.unwrap_or(true),
        "supports_image": req.supports_image.unwrap_or(false),
        "supports_function_calling": true,
        "model": req.model,
    });

    let transport_type = req
        .transport_type
        .unwrap_or_else(|| "websocket".to_string());
    let transport_config = req
        .transport_config
        .unwrap_or_else(|| serde_json::json!({}));
    let triggers_json = req.triggers.map(|t| t.to_string());
    let tags_json = req.tags.map(|t| serde_json::to_string(&t).unwrap_or_default());

    let _ = sqlx::query(
        "INSERT INTO agents (id, name, description, transport_type, transport_config, capabilities, triggers, tags, status, max_concurrent_tasks, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'offline', ?, ?)"
    )
    .bind(&agent_id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&transport_type)
    .bind(transport_config.to_string())
    .bind(capabilities_json.to_string())
    .bind(&triggers_json)
    .bind(&tags_json)
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
        "capabilities": capabilities_json,
        "status": "registered",
    }))
}

async fn list_agents_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    let agents = state.agent_registry.list_agents().await;
    let mut agent_list = Vec::new();
    for (id, name, status) in agents {
        // Fetch heartbeat and description from DB
        let db_row: Option<(Option<i64>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT last_heartbeat, description, tags FROM agents WHERE id = ?"
        )
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
        let (last_hb, description, tags) = db_row.unwrap_or((None, None, None));
        agent_list.push(serde_json::json!({
            "id": id,
            "name": name,
            "status": format!("{:?}", status),
            "is_online": status == maple_agent::registry::AgentStatus::Online,
            "last_heartbeat": last_hb,
            "description": description,
            "tags": tags.and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok()),
        }));
    }
    axum::Json(serde_json::json!({
        "agents": agent_list,
    }))
}

async fn get_agent_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(agent_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    match state.agent_registry.get_agent(&agent_id).await {
        Some(agent) => {
            // Fetch DB-only fields
            let db_row: Option<(Option<String>, Option<i64>, Option<String>)> = sqlx::query_as(
                "SELECT triggers, last_heartbeat, tags FROM agents WHERE id = ?"
            )
            .bind(&agent_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
            let (_triggers_str, last_hb, tags) = db_row.unwrap_or((None, None, None));
            Ok(axum::Json(serde_json::json!({
                "id": agent.id,
                "name": agent.name,
                "description": agent.description,
                "transport": agent.transport,
                "capabilities": agent.capabilities,
                "triggers": agent.triggers,
                "max_concurrent_tasks": agent.max_concurrent_tasks,
                "last_heartbeat": last_hb,
                "tags": tags.and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok()),
            })))
        }
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

// ============================================================
// v3 Group Handlers
// ============================================================

#[derive(Debug, Deserialize)]
struct CreateGroupRequest {
    name: String,
    description: Option<String>,
    group_type: Option<String>,
}

async fn v3_create_group_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<CreateGroupRequest>,
) -> impl IntoResponse {
    let group_type = match req.group_type.as_deref() {
        Some("project") => maple_collab::group::GroupType::Project,
        Some("channel") => maple_collab::group::GroupType::Channel,
        Some("dm") => maple_collab::group::GroupType::Dm,
        _ => maple_collab::group::GroupType::Collaboration,
    };
    let settings = maple_collab::group::GroupSettings::default();
    match state.group_manager.create_group(&req.name, req.description.as_deref(), group_type, "system", &settings).await {
        Ok(group) => axum::Json(serde_json::json!({ "group": group })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_list_groups_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl IntoResponse {
    match state.group_manager.list_groups("system").await {
        Ok(groups) => axum::Json(serde_json::json!({ "groups": groups })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_get_group_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    match state.group_manager.get_group(&group_id).await {
        Ok(Some(group)) => Ok(axum::Json(serde_json::json!({ "group": group }))),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn v3_list_members_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    match state.group_manager.list_members(&group_id).await {
        Ok(members) => axum::Json(serde_json::json!({ "members": members })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct AddMemberRequest {
    member_id: String,
    member_type: Option<String>,
    role: Option<String>,
}

async fn v3_add_member_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
    Json(req): Json<AddMemberRequest>,
) -> impl IntoResponse {
    let member_type = req.member_type.as_deref().unwrap_or("human");
    let role = req.role.as_deref().unwrap_or("member");
    match state.group_manager.add_member(&group_id, &req.member_id, member_type, role).await {
        Ok(true) => {
            state.event_bus.publish(maple_engine::event_bus::Event::GroupMemberJoined {
                group_id,
                member_id: req.member_id,
            }).await;
            axum::Json(serde_json::json!({ "status": "added" }))
        }
        Ok(false) => axum::Json(serde_json::json!({ "status": "already_member" })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

// ============================================================
// v3 Message Handlers
// ============================================================

#[derive(Debug, Deserialize)]
struct SendMessageRequest {
    sender_id: String,
    sender_type: Option<String>,
    message_type: Option<String>,
    content: String,
    reply_to_id: Option<String>,
    thread_root_id: Option<String>,
    source_channel: Option<String>,
}

async fn v3_send_message_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    auth_user: Option<axum::extract::Extension<v3_auth::AuthenticatedUser>>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
    Json(req): Json<SendMessageRequest>,
) -> impl IntoResponse {
    // Prefer authenticated user identity over request body
    let sender_id = auth_user
        .as_ref()
        .map(|u| u.user_id.clone())
        .unwrap_or_else(|| req.sender_id.clone());
    let sender_type = auth_user
        .as_ref()
        .map(|u| u.user_type.clone())
        .or(req.sender_type.clone())
        .unwrap_or_else(|| "human".to_string());

    let msg_type = match req.message_type.as_deref() {
        Some("markdown") => maple_collab::group_message::MessageType::Markdown,
        Some("tool_call") => maple_collab::group_message::MessageType::ToolCall,
        Some("tool_result") => maple_collab::group_message::MessageType::ToolResult,
        Some("system") => maple_collab::group_message::MessageType::System,
        _ => maple_collab::group_message::MessageType::Text,
    };
    let source_channel = req.source_channel.as_deref().unwrap_or("api");
    match state.group_message_manager.send_message(
        &group_id, &sender_id, &sender_type, msg_type, &req.content,
        req.reply_to_id.as_deref(), req.thread_root_id.as_deref(), source_channel,
    ).await {
        Ok(msg) => {
            state.event_bus.publish(maple_engine::event_bus::Event::GroupMessageSent {
                group_id: group_id.clone(),
                message_id: msg.id.clone(),
                sender_id: sender_id,
                content: req.content,
            }).await;
            axum::Json(serde_json::json!({ "message": msg }))
        }
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct ListMessagesQuery {
    limit: Option<i64>,
    before: Option<i64>,
}

async fn v3_list_messages_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
    axum::extract::Query(query): axum::extract::Query<ListMessagesQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(50).min(200);
    match state.group_message_manager.get_messages(&group_id, limit, query.before).await {
        Ok(page) => axum::Json(serde_json::json!({ "messages": page.messages, "has_more": page.has_more, "next_cursor": page.next_cursor })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct EditMessageRequest {
    editor_id: String,
    content: String,
}

async fn v3_edit_message_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((group_id, message_id)): axum::extract::Path<(String, String)>,
    Json(req): Json<EditMessageRequest>,
) -> impl IntoResponse {
    match state.group_message_manager.edit_message(&message_id, &req.editor_id, &req.content).await {
        Ok(true) => {
            state.event_bus.publish(maple_engine::event_bus::Event::GroupMessageEdited {
                group_id,
                message_id,
            }).await;
            axum::Json(serde_json::json!({ "status": "edited" }))
        }
        Ok(false) => axum::Json(serde_json::json!({ "error": "message not found" })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_delete_message_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((group_id, message_id)): axum::extract::Path<(String, String)>,
) -> impl IntoResponse {
    match state.group_message_manager.delete_message(&message_id).await {
        Ok(true) => {
            state.event_bus.publish(maple_engine::event_bus::Event::GroupMessageDeleted {
                group_id,
                message_id,
            }).await;
            axum::Json(serde_json::json!({ "status": "deleted" }))
        }
        Ok(false) => axum::Json(serde_json::json!({ "error": "message not found" })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct ReactionRequest {
    user_id: String,
    emoji: String,
}

async fn v3_add_reaction_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((_group_id, message_id)): axum::extract::Path<(String, String)>,
    Json(req): Json<ReactionRequest>,
) -> impl IntoResponse {
    match state.group_message_manager.add_reaction(&message_id, &req.user_id, &req.emoji).await {
        Ok(true) => axum::Json(serde_json::json!({ "status": "added" })),
        Ok(false) => axum::Json(serde_json::json!({ "status": "already_exists" })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_remove_reaction_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((_group_id, message_id, emoji)): axum::extract::Path<(String, String, String)>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let user_id = query.get("user_id").cloned().unwrap_or_default();
    match state.group_message_manager.remove_reaction(&message_id, &user_id, &emoji).await {
        Ok(true) => axum::Json(serde_json::json!({ "status": "removed" })),
        Ok(false) => axum::Json(serde_json::json!({ "error": "reaction not found" })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct PinRequest {
    pinned_by: String,
}

async fn v3_pin_message_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((group_id, message_id)): axum::extract::Path<(String, String)>,
    Json(req): Json<PinRequest>,
) -> impl IntoResponse {
    match state.group_message_manager.pin_message(&message_id, &group_id, &req.pinned_by).await {
        Ok(true) => axum::Json(serde_json::json!({ "status": "pinned" })),
        Ok(false) => axum::Json(serde_json::json!({ "status": "already_pinned" })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_unpin_message_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((_group_id, message_id)): axum::extract::Path<(String, String)>,
) -> impl IntoResponse {
    match state.group_message_manager.unpin_message(&message_id).await {
        Ok(true) => axum::Json(serde_json::json!({ "status": "unpinned" })),
        Ok(false) => axum::Json(serde_json::json!({ "error": "not pinned" })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct SearchMessagesQuery {
    q: String,
    limit: Option<i64>,
}

async fn v3_search_messages_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
    axum::extract::Query(query): axum::extract::Query<SearchMessagesQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(20).min(100);
    match state.group_message_manager.search_messages(&group_id, &query.q, limit).await {
        Ok(messages) => axum::Json(serde_json::json!({ "messages": messages })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_get_thread_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((_group_id, message_id)): axum::extract::Path<(String, String)>,
) -> impl IntoResponse {
    match state.group_message_manager.get_thread(&message_id, 100).await {
        Ok(messages) => axum::Json(serde_json::json!({ "messages": messages })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct MarkReadRequest {
    user_id: String,
}

async fn v3_mark_read_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((group_id, message_id)): axum::extract::Path<(String, String)>,
    Json(req): Json<MarkReadRequest>,
) -> impl IntoResponse {
    match state.group_message_manager.mark_as_read(&group_id, &req.user_id, &message_id).await {
        Ok(_) => axum::Json(serde_json::json!({ "status": "read" })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

// ============================================================
// v3 Task Handlers
// ============================================================

#[derive(Debug, Deserialize)]
struct CreateTaskV3Request {
    title: String,
    description: Option<String>,
    creator_id: String,
    project_id: Option<String>,
    group_id: Option<String>,
    priority: Option<String>,
    assignee_id: Option<String>,
    source_message_id: Option<String>,
}

async fn v3_create_task_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<CreateTaskV3Request>,
) -> impl IntoResponse {
    let priority = match req.priority.as_deref() {
        Some("critical") => maple_engine::task_service::TaskPriority::Critical,
        Some("urgent") => maple_engine::task_service::TaskPriority::Urgent,
        Some("high") => maple_engine::task_service::TaskPriority::High,
        Some("low") => maple_engine::task_service::TaskPriority::Low,
        _ => maple_engine::task_service::TaskPriority::Medium,
    };
    match state.task_service.create_task(
        &req.title, req.description.as_deref(), &req.creator_id,
        req.project_id.as_deref(), req.group_id.as_deref(), priority,
        req.assignee_id.as_deref(), req.source_message_id.as_deref(), None,
    ).await {
        Ok(task) => axum::Json(serde_json::json!({ "task": task })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct ListTasksV3Query {
    group_id: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
}

async fn v3_list_tasks_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Query(query): axum::extract::Query<ListTasksV3Query>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(50).min(200);
    match state.task_service.list_tasks(query.group_id.as_deref(), query.status.as_deref(), limit).await {
        Ok(tasks) => axum::Json(serde_json::json!({ "tasks": tasks })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_get_task_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    match state.task_service.get_task(&task_id).await {
        Ok(Some(task)) => Ok(axum::Json(serde_json::json!({ "task": task }))),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Debug, Deserialize)]
struct TransitionTaskRequest {
    status: String,
    changed_by: String,
    reason: Option<String>,
}

async fn v3_transition_task_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
    Json(req): Json<TransitionTaskRequest>,
) -> impl IntoResponse {
    let new_status = maple_engine::task_service::TaskV3Status::from_str(&req.status);
    match state.task_service.transition_task(&task_id, new_status, &req.changed_by, req.reason.as_deref()).await {
        Ok(task) => {
            state.event_bus.publish(maple_engine::event_bus::Event::TaskTransitioned {
                task_id: task_id.clone(),
                old_status: "unknown".to_string(),
                new_status: task.status.as_str().to_string(),
            }).await;
            axum::Json(serde_json::json!({ "task": task }))
        }
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct AddCommentRequest {
    user_id: String,
    content: String,
    source_message_id: Option<String>,
}

async fn v3_add_comment_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
    Json(req): Json<AddCommentRequest>,
) -> impl IntoResponse {
    match state.task_service.add_comment(&task_id, &req.user_id, &req.content, req.source_message_id.as_deref()).await {
        Ok(id) => axum::Json(serde_json::json!({ "id": id, "status": "created" })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_task_history_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    match state.task_service.get_status_history(&task_id).await {
        Ok(history) => axum::Json(serde_json::json!({ "history": history })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

// ============================================================
// v3 Approval Handlers
// ============================================================

#[derive(Debug, Deserialize)]
struct CreateApprovalRequest {
    group_id: String,
    title: String,
    description: Option<String>,
    request_type: Option<String>,
    requester_id: String,
    urgency: Option<String>,
    quorum_type: Option<String>,
    approver_spec: String,
    context: Option<String>,
}

async fn v3_create_approval_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<CreateApprovalRequest>,
) -> impl IntoResponse {
    let urgency = match req.urgency.as_deref() {
        Some("critical") => maple_engine::approval::ApprovalUrgency::Critical,
        Some("high") => maple_engine::approval::ApprovalUrgency::High,
        Some("low") => maple_engine::approval::ApprovalUrgency::Low,
        _ => maple_engine::approval::ApprovalUrgency::Normal,
    };
    let quorum = maple_engine::approval::QuorumType::from_str(&req.quorum_type.unwrap_or_else(|| "any".to_string()));
    let request_type = req.request_type.as_deref().unwrap_or("general");
    match state.approval_service.create_request(
        &req.group_id, &req.title, req.description.as_deref(), request_type,
        &req.requester_id, urgency, quorum, &req.approver_spec, req.context.as_deref(),
    ).await {
        Ok(approval) => axum::Json(serde_json::json!({ "approval": approval })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_get_approval_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(approval_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    match state.approval_service.get_request(&approval_id).await {
        Ok(Some(approval)) => Ok(axum::Json(serde_json::json!({ "approval": approval }))),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Debug, Deserialize)]
struct VoteRequest {
    voter_id: String,
    decision: String,
    comment: Option<String>,
}

async fn v3_vote_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(approval_id): axum::extract::Path<String>,
    Json(req): Json<VoteRequest>,
) -> impl IntoResponse {
    let decision = match req.decision.as_str() {
        "reject" => maple_engine::approval::VoteDecision::Reject,
        "abstain" => maple_engine::approval::VoteDecision::Abstain,
        _ => maple_engine::approval::VoteDecision::Approve,
    };
    match state.approval_service.vote(&approval_id, &req.voter_id, decision, req.comment.as_deref()).await {
        Ok(outcome) => {
            state.event_bus.publish(maple_engine::event_bus::Event::ApprovalVoteCast {
                approval_id: approval_id.clone(),
                voter_id: req.voter_id,
                decision: req.decision,
            }).await;
            if outcome.quorum_met {
                state.event_bus.publish(maple_engine::event_bus::Event::ApprovalResolved {
                    approval_id: approval_id.clone(),
                    approved: outcome.approved,
                }).await;
                // Resume any workflow waiting on this approval
                state.workflow_executor.resolve_approval(&approval_id, outcome.approved).await;
            }
            axum::Json(serde_json::json!({ "outcome": outcome }))
        }
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_list_votes_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(approval_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    match state.approval_service.list_votes(&approval_id).await {
        Ok(votes) => axum::Json(serde_json::json!({ "votes": votes })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct ListPendingApprovalsQuery {
    user_id: String,
    group_id: Option<String>,
}

async fn v3_list_pending_approvals_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Query(query): axum::extract::Query<ListPendingApprovalsQuery>,
) -> impl IntoResponse {
    match state.approval_service.list_pending_for_user(&query.user_id, query.group_id.as_deref()).await {
        Ok(approvals) => axum::Json(serde_json::json!({ "approvals": approvals })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

// ============================================================
// v3 Memory Handlers
// ============================================================

#[derive(Debug, Deserialize)]
struct V3MemoryStoreRequest {
    agent_id: String,
    memory_type: Option<String>,
    content: String,
    summary: Option<String>,
    source_type: Option<String>,
    source_id: Option<String>,
    group_id: Option<String>,
    ttl_hours: Option<i64>,
}

async fn v3_memory_store_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<V3MemoryStoreRequest>,
) -> impl IntoResponse {
    let layer = match req.memory_type.as_deref() {
        Some("episodic") => maple_engine::memory_service::MemoryLayer::Episodic,
        Some("semantic") => maple_engine::memory_service::MemoryLayer::Semantic,
        _ => maple_engine::memory_service::MemoryLayer::Working,
    };
    match state.memory_service.store(
        &req.agent_id, layer, &req.content, req.summary.as_deref(),
        req.source_type.as_deref(), req.source_id.as_deref(), req.group_id.as_deref(), req.ttl_hours,
    ).await {
        Ok(memory) => axum::Json(serde_json::json!({ "memory": memory })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct V3MemorySearchRequest {
    agent_id: String,
    query_text: Option<String>,
    memory_type: Option<String>,
    group_id: Option<String>,
    limit: Option<i64>,
}

async fn v3_memory_search_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<V3MemorySearchRequest>,
) -> impl IntoResponse {
    let layer = req.memory_type.as_deref().map(|t| match t {
        "episodic" => maple_engine::memory_service::MemoryLayer::Episodic,
        "semantic" => maple_engine::memory_service::MemoryLayer::Semantic,
        _ => maple_engine::memory_service::MemoryLayer::Working,
    });
    let query = maple_engine::memory_service::MemoryQuery {
        agent_id: req.agent_id,
        query_text: req.query_text,
        memory_type: layer,
        group_id: req.group_id,
        limit: req.limit.unwrap_or(10).min(100),
    };
    match state.memory_service.search(&query).await {
        Ok(results) => axum::Json(serde_json::json!({ "results": results })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_memory_stats_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let agent_id = query.get("agent_id").cloned().unwrap_or_default();
    match state.memory_service.stats(&agent_id).await {
        Ok(stats) => axum::Json(serde_json::json!({ "stats": stats })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

// ============================================================
// v3 DM Handlers
// ============================================================

#[derive(Debug, Deserialize)]
struct CreateDmRequest {
    other_user_id: String,
}

async fn v3_create_dm_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    v3_auth::AuthenticatedUser { user_id, .. }: v3_auth::AuthenticatedUser,
    Json(req): Json<CreateDmRequest>,
) -> impl IntoResponse {
    match state.dm_service.find_or_create(&user_id, &req.other_user_id).await {
        Ok(group_id) => axum::Json(serde_json::json!({ "group_id": group_id })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_list_dms_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    v3_auth::AuthenticatedUser { user_id, .. }: v3_auth::AuthenticatedUser,
) -> impl IntoResponse {
    match state.dm_service.list_user_dms(&user_id).await {
        Ok(dms) => axum::Json(serde_json::json!({ "dms": dms })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct GrantToolRequest {
    tool_name: String,
    expires_at: Option<i64>,
    scope: Option<String>,
}

async fn v3_grant_tool_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
    v3_auth::AuthenticatedUser { user_id, .. }: v3_auth::AuthenticatedUser,
    Json(req): Json<GrantToolRequest>,
) -> impl IntoResponse {
    match state.dm_service.grant_tool(&group_id, &req.tool_name, &user_id, req.expires_at, req.scope.as_deref()).await {
        Ok(grant) => axum::Json(serde_json::json!({ "grant": grant })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_revoke_tool_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((group_id, tool_name)): axum::extract::Path<(String, String)>,
    v3_auth::AuthenticatedUser { user_id, .. }: v3_auth::AuthenticatedUser,
) -> impl IntoResponse {
    match state.dm_service.revoke_tool(&group_id, &tool_name, &user_id).await {
        Ok(true) => axum::Json(serde_json::json!({ "status": "revoked" })),
        Ok(false) => axum::Json(serde_json::json!({ "status": "not_found" })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_list_grants_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    match state.dm_service.list_grants(&group_id).await {
        Ok(grants) => axum::Json(serde_json::json!({ "grants": grants })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct CreateDelegationRequest {
    executor_id: String,
    prompt: String,
    task_id: Option<String>,
    visible_to: Option<Vec<String>>,
}

async fn v3_create_delegation_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
    v3_auth::AuthenticatedUser { user_id, .. }: v3_auth::AuthenticatedUser,
    Json(req): Json<CreateDelegationRequest>,
) -> impl IntoResponse {
    let visible = req.visible_to.unwrap_or_default();
    match state.dm_service.create_delegation(&group_id, &user_id, &req.executor_id, &req.prompt, req.task_id.as_deref(), &visible).await {
        Ok(delegation) => axum::Json(serde_json::json!({ "delegation": delegation })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_list_delegations_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    v3_auth::AuthenticatedUser { user_id, .. }: v3_auth::AuthenticatedUser,
) -> impl IntoResponse {
    match state.dm_service.list_visible_delegations(&user_id).await {
        Ok(delegations) => axum::Json(serde_json::json!({ "delegations": delegations })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
struct InterveneRequest {
    status: Option<String>,
    result: Option<String>,
}

async fn v3_intervene_delegation_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(delegation_id): axum::extract::Path<String>,
    Json(req): Json<InterveneRequest>,
) -> impl IntoResponse {
    let status = maple_collab::dm_service::DelegationStatus::from_str(
        req.status.as_deref().unwrap_or("failed")
    );
    match state.dm_service.update_delegation_status(&delegation_id, status, req.result.as_deref()).await {
        Ok(true) => axum::Json(serde_json::json!({ "status": "updated" })),
        Ok(false) => axum::Json(serde_json::json!({ "error": "delegation not found" })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

// ============================================================
// v3 Rules Handlers
// ============================================================

#[derive(Debug, Deserialize)]
struct CreateRuleRequest {
    name: String,
    rule_type: serde_json::Value,
    enabled: Option<bool>,
}

async fn v3_list_rules_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(_group_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let rules_engine = state.group_rules.read().await;
    let rules = rules_engine.list_rules();
    axum::Json(serde_json::json!({ "rules": rules }))
}

async fn v3_create_rule_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(_group_id): axum::extract::Path<String>,
    Json(req): Json<CreateRuleRequest>,
) -> impl IntoResponse {
    let rule_type: maple_collab::group_rules::GroupRuleType = match serde_json::from_value(req.rule_type) {
        Ok(rt) => rt,
        Err(e) => return axum::Json(serde_json::json!({ "error": format!("invalid rule_type: {}", e) })),
    };
    let rule = maple_collab::group_rules::GroupRule {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        rule_type,
        enabled: req.enabled.unwrap_or(true),
    };
    let mut rules_engine = state.group_rules.write().await;
    rules_engine.add_rule(rule.clone());
    axum::Json(serde_json::json!({ "rule": rule }))
}

async fn v3_update_rule_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((_group_id, rule_id)): axum::extract::Path<(String, String)>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let mut rules_engine = state.group_rules.write().await;
    if let Some(mut rule) = rules_engine.get_rule(&rule_id) {
        if let Some(enabled) = req.get("enabled").and_then(|v| v.as_bool()) {
            rule.enabled = enabled;
        }
        if let Some(name) = req.get("name").and_then(|v| v.as_str()) {
            rule.name = name.to_string();
        }
        if let Some(rt) = req.get("rule_type") {
            if let Ok(parsed) = serde_json::from_value::<maple_collab::group_rules::GroupRuleType>(rt.clone()) {
                rule.rule_type = parsed;
            }
        }
        rules_engine.update_rule(&rule_id, rule);
        axum::Json(serde_json::json!({ "status": "updated" }))
    } else {
        axum::Json(serde_json::json!({ "error": "rule not found" }))
    }
}

async fn v3_delete_rule_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((_group_id, rule_id)): axum::extract::Path<(String, String)>,
) -> impl IntoResponse {
    let mut rules_engine = state.group_rules.write().await;
    if rules_engine.remove_rule(&rule_id) {
        axum::Json(serde_json::json!({ "status": "deleted" }))
    } else {
        axum::Json(serde_json::json!({ "error": "rule not found" }))
    }
}

// ============================================================
// v3 Cron Handlers
// ============================================================

async fn v3_list_cron_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    match state.group_cron_service.list_jobs(&group_id).await {
        Ok(jobs) => axum::Json(serde_json::json!({ "jobs": jobs })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_create_cron_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(group_id): axum::extract::Path<String>,
    Json(req): Json<maple_collab::group_cron::CreateCronJobRequest>,
) -> impl IntoResponse {
    match state.group_cron_service.create_job(&group_id, "system", req).await {
        Ok(job) => axum::Json(serde_json::json!({ "job": job })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_update_cron_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((_group_id, cron_id)): axum::extract::Path<(String, String)>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let name = req.get("name").and_then(|v| v.as_str());
    let cron_expr = req.get("cron_expr").and_then(|v| v.as_str());
    let message_template = req.get("message_template").and_then(|v| v.as_str());
    let enabled = req.get("enabled").and_then(|v| v.as_bool());

    match state.group_cron_service.update_job(&cron_id, name, cron_expr, message_template, enabled).await {
        Ok(true) => axum::Json(serde_json::json!({ "status": "updated" })),
        Ok(false) => axum::Json(serde_json::json!({ "error": "job not found" })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn v3_delete_cron_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((_group_id, cron_id)): axum::extract::Path<(String, String)>,
) -> impl IntoResponse {
    match state.group_cron_service.delete_job(&cron_id).await {
        Ok(true) => axum::Json(serde_json::json!({ "status": "deleted" })),
        Ok(false) => axum::Json(serde_json::json!({ "error": "job not found" })),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
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

    // Background task: sweep stale agents (mark offline if no heartbeat for 5 minutes)
    let sweep_pool = pool.clone();
    let sweep_registry = agent_registry.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let threshold = chrono::Utc::now().timestamp() - 300;
            match sqlx::query("UPDATE agents SET status = 'offline' WHERE status != 'offline' AND last_heartbeat < ?")
                .bind(threshold)
                .execute(&sweep_pool)
                .await
            {
                Ok(result) => {
                    let count = result.rows_affected();
                    if count > 0 {
                        tracing::info!(count = count, "Swept stale agents to offline");
                        // Only mark stale agents offline in-memory (not all agents)
                        let agents = sweep_registry.list_agents().await;
                        for (agent_id, _, _status) in agents {
                            // Check if this agent's heartbeat is stale via DB
                            let stale: bool = sqlx::query_scalar(
                                "SELECT last_heartbeat < ? FROM agents WHERE id = ?"
                            )
                            .bind(threshold)
                            .bind(&agent_id)
                            .fetch_optional(&sweep_pool)
                            .await
                            .ok()
                            .flatten()
                            .unwrap_or(false);
                            if stale {
                                let _ = sweep_registry.set_offline(&agent_id).await;
                            }
                        }
                    }
                }
                Err(e) => tracing::warn!(error = %e, "Agent sweep failed"),
            }
        }
    });

    // Background task: periodic knowledge distillation (every 6 hours)
    let evolve_evolver = evolver.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
            tracing::info!("Starting periodic knowledge distillation");
            match evolve_evolver.batch_evolve().await {
                Ok(stats) => {
                    tracing::info!(
                        pruned = stats.pruned_count,
                        semantic_created = stats.semantic_memories_created,
                        consolidated = stats.episodic_memories_consolidated,
                        "Knowledge distillation completed"
                    );
                }
                Err(e) => tracing::warn!(error = %e, "Knowledge distillation failed"),
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
        scheduler: scheduler.clone(),
        group_rules: {
            let engine = GroupRulesEngine::new();
            let gr = Arc::new(tokio::sync::RwLock::new(engine));
            // Load persisted rules from DB
            let rows: Vec<(String, String, String, bool)> = sqlx::query_as(
                "SELECT id, name, rule_type, enabled FROM group_rules"
            ).fetch_all(&pool).await.unwrap_or_default();
            {
                let mut eng = gr.write().await;
                for (id, name, rule_type_json, enabled) in rows {
                    if let Ok(rule_type) = serde_json::from_str::<maple_collab::group_rules::GroupRuleType>(&rule_type_json) {
                        eng.add_rule(maple_collab::group_rules::GroupRule { id, name, rule_type, enabled });
                    }
                }
                tracing::info!(count = eng.list_rules().len(), "Loaded group rules from DB");
            }
            gr.clone()
        },
        group_rules_service: Arc::new(maple_collab::group_rules::GroupRulesService::new(pool.clone(), {
            let engine = GroupRulesEngine::new();
            Arc::new(tokio::sync::RwLock::new(engine))
        })),
        group_manager: Arc::new(GroupManager::new(pool.clone())),
        group_message_manager: Arc::new(GroupMessageManager::new(pool.clone())),
        task_service: Arc::new(TaskService::new(pool.clone())),
        approval_service: Arc::new(ApprovalService::new(pool.clone())),
        memory_service: Arc::new(MemoryService::new(pool.clone())),
        dm_service: Arc::new(maple_collab::dm_service::DmService::new(pool.clone(), GroupManager::new(pool.clone()))),
        group_cron_service: Arc::new(maple_collab::group_cron::GroupCronService::new(
            pool.clone(), scheduler.clone(), event_bus.clone(),
        )),
        hook_service: Arc::new(maple_engine::agent_hooks::AgentHookService::new(pool.clone())),
        workflow_service: Arc::new(maple_engine::WorkflowService::new(pool.clone())),
        mcp_host: Arc::new(McpHostManager::new()),
        rate_limiter,
        cache: cache::AppCache::new(),
        metrics: metrics::AppMetrics::new(),
        execution_recorder: maple_engine::ExecutionRecorder::new(pool.clone()),
    });

    // Initialize group cron service
    if let Err(e) = state.group_cron_service.init().await {
        tracing::warn!("Failed to init group cron service: {}", e);
    }

    let scheduler_wf = workflow_executor.clone();
    let scheduler_db = pool.clone();
    let scheduler_cron = state.group_cron_service.clone();
    scheduler.start_loop(60, move |job: ScheduledJob| {
        let wf = scheduler_wf.clone();
        let db = scheduler_db.clone();
        let cron_svc = scheduler_cron.clone();
        async move {
            tracing::info!(job_id = %job.id, workflow_id = %job.workflow_id, "Scheduled job triggered");
            // Check if this is a group cron job
            let is_cron_job: bool = sqlx::query_scalar(
                "SELECT COUNT(*) FROM group_cron_jobs WHERE id = ?"
            ).bind(&job.id).fetch_one(&db).await.unwrap_or(0) > 0;

            if is_cron_job {
                if let Err(e) = cron_svc.execute_job(&job).await {
                    tracing::error!(job_id = %job.id, error = %e, "Group cron job execution failed");
                }
            } else {
                // Original workflow scheduler
                let yaml_str: Option<String> = sqlx::query_scalar(
                    "SELECT yaml_content FROM workflows WHERE id = ?"
                ).bind(&job.workflow_id).fetch_optional(&db).await.ok().flatten();
                if let Some(yaml) = yaml_str
                    && let Ok(parsed) = Workflow::parse_definition(&yaml)
                {
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
        .route("/ws/groups", get(ws_group_handler))
        .route("/api/v3/events", get(sse_group_handler))
        .route("/api/chat", post(chat_handler))
        .route("/api/chat/stream", post(chat_stream_handler))
        .route("/api/models", get(models_handler))
        .route("/api/skills", get(skills_handler))
        .route(
            "/api/config",
            get(get_config_handler).put(update_config_handler),
        )
        .route("/api/kb/index", post(kb_index_handler))
        .route("/api/kb/upload", post(kb_upload_handler))
        .route("/api/kb/search", post(kb_search_handler))
        .route("/api/kb/documents/:id", delete(kb_delete_handler))
        .route(
            "/api/board/tasks/:id/attachments",
            get(list_attachments_handler).post(upload_attachment_handler),
        )
        .route(
            "/api/board/attachments/:id",
            get(download_attachment_handler).delete(delete_attachment_handler),
        )
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
        .route("/api/scheduler/jobs", get(list_scheduler_jobs_handler).post(create_scheduler_job_handler))
        .route("/api/scheduler/jobs/:id", put(update_scheduler_job_handler).delete(delete_scheduler_job_handler))
        .route("/api/group-rules", get(list_group_rules_handler).post(create_group_rule_handler))
        .route("/api/group-rules/:id", get(get_group_rule_handler).put(update_group_rule_handler).delete(delete_group_rule_handler))
        .route("/api/executions/:id", get(get_execution_handler))
        .route(
            "/api/executions/:id/checkpoints",
            get(get_checkpoints_handler),
        )
        .route(
            "/api/workspaces",
            get(list_workspaces_handler).post(create_workspace_handler),
        )
        .route(
            "/api/workspaces/:id",
            get(get_workspace_handler)
                .put(update_workspace_handler)
                .delete(delete_workspace_handler),
        )
        .route(
            "/api/workspaces/:id/members",
            get(list_workspace_members_handler).post(add_workspace_member_handler),
        )
        .route(
            "/api/workspaces/:id/members/:member_id",
            axum::routing::delete(remove_workspace_member_handler),
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
        .route("/api/board/comments/:id", delete(delete_comment_handler).put(update_comment_handler))
        .route("/api/board/comments/:id/like", post(like_comment_handler))
        .route("/api/activity", get(list_activity_handler).post(create_activity_handler))
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/token", post(token_handler))
        .route("/api/auth/register", post(register_handler))
        .route("/api/auth/device-login", post(device_login_handler))
        // Bot webhook endpoints
        .route("/webhook/feishu", post(feishu_webhook_handler))
        .route("/api/auth/refresh", post(refresh_handler))
        // v3 Group APIs
        .route("/api/v3/groups", get(v3_list_groups_handler).post(v3_create_group_handler))
        .route("/api/v3/groups/:id", get(v3_get_group_handler))
        .route("/api/v3/groups/:id/members", get(v3_list_members_handler).post(v3_add_member_handler))
        // v3 Message APIs
        .route("/api/v3/groups/:id/messages", get(v3_list_messages_handler).post(v3_send_message_handler))
        .route("/api/v3/groups/:id/messages/search", get(v3_search_messages_handler))
        .route("/api/v3/groups/:id/messages/:mid", put(v3_edit_message_handler).delete(v3_delete_message_handler))
        .route("/api/v3/groups/:id/messages/:mid/reactions", post(v3_add_reaction_handler))
        .route("/api/v3/groups/:id/messages/:mid/reactions/:emoji", delete(v3_remove_reaction_handler))
        .route("/api/v3/groups/:id/messages/:mid/pin", post(v3_pin_message_handler).delete(v3_unpin_message_handler))
        .route("/api/v3/groups/:id/messages/:mid/thread", get(v3_get_thread_handler))
        .route("/api/v3/groups/:id/messages/:mid/read", post(v3_mark_read_handler))
        // v3 Task APIs
        .route("/api/v3/tasks", get(v3_list_tasks_handler).post(v3_create_task_handler))
        .route("/api/v3/tasks/:id", get(v3_get_task_handler))
        .route("/api/v3/tasks/:id/transition", post(v3_transition_task_handler))
        .route("/api/v3/tasks/:id/comments", post(v3_add_comment_handler))
        .route("/api/v3/tasks/:id/history", get(v3_task_history_handler))
        // v3 Approval APIs
        .route("/api/v3/approvals", post(v3_create_approval_handler))
        .route("/api/v3/approvals/pending", get(v3_list_pending_approvals_handler))
        .route("/api/v3/approvals/:id", get(v3_get_approval_handler))
        .route("/api/v3/approvals/:id/vote", post(v3_vote_handler))
        .route("/api/v3/approvals/:id/votes", get(v3_list_votes_handler))
        // v3 Memory APIs
        .route("/api/v3/memories", post(v3_memory_store_handler))
        .route("/api/v3/memories/search", post(v3_memory_search_handler))
        .route("/api/v3/memories/stats", get(v3_memory_stats_handler))
        // v3 DM APIs
        .route("/api/v3/dms", post(v3_create_dm_handler).get(v3_list_dms_handler))
        .route("/api/v3/dms/:id/grants", post(v3_grant_tool_handler).get(v3_list_grants_handler))
        .route("/api/v3/dms/:id/grants/:tool", delete(v3_revoke_tool_handler))
        // v3 A2A Delegation APIs
        .route("/api/v3/dms/:id/delegations", post(v3_create_delegation_handler))
        .route("/api/v3/a2a/delegations", get(v3_list_delegations_handler))
        .route("/api/v3/a2a/:id/intervene", post(v3_intervene_delegation_handler))
        // v3 Rules APIs
        .route("/api/v3/groups/:id/rules", get(v3_list_rules_handler).post(v3_create_rule_handler))
        .route("/api/v3/groups/:id/rules/:rid", put(v3_update_rule_handler).delete(v3_delete_rule_handler))
        // v3 Cron APIs
        .route("/api/v3/groups/:id/cron", get(v3_list_cron_handler).post(v3_create_cron_handler))
        .route("/api/v3/groups/:id/cron/:cid", put(v3_update_cron_handler).delete(v3_delete_cron_handler))
        // v3 Message Attachment APIs
        .route("/api/v3/groups/:id/attachments", get(v3_list_message_attachments_handler).post(v3_upload_message_attachment_handler))
        .route("/api/v3/attachments/:aid", get(v3_download_message_attachment_handler).delete(v3_delete_message_attachment_handler).put(v3_link_attachment_handler))
        // Agent Hooks
        .route("/api/v3/groups/:id/hooks", get(v3_list_hooks_handler).post(v3_create_hook_handler))
        .route("/api/v3/groups/:id/hooks/:hid", get(v3_get_hook_handler).put(v3_update_hook_handler).delete(v3_delete_hook_handler))
        .route("/api/v3/groups/:id/hooks/:hid/logs", get(v3_list_hook_logs_handler))
        // Workflow definitions
        .route("/api/v3/workflows", get(v3_list_workflows_handler).post(v3_create_workflow_handler))
        .route("/api/v3/workflows/:wid", get(v3_get_workflow_handler).put(v3_update_workflow_handler).delete(v3_delete_workflow_handler))
        // Workflow runs
        .route("/api/v3/workflow-runs", get(v3_list_workflow_runs_handler).post(v3_create_workflow_run_handler))
        .route("/api/v3/workflow-runs/:rid", get(v3_get_workflow_run_handler))
        .route("/api/v3/workflow-runs/:rid/status", put(v3_update_workflow_run_status_handler))
        .route("/api/v3/workflow-runs/:rid/checkpoints", get(v3_list_checkpoints_handler).post(v3_record_checkpoint_handler))
        // Unified execution fact chain (Track 1 / T1-2)
        // Handlers live in lib crate (server/src/execution_handlers.rs) so
        // the same code is reused by integration tests in
        // server/tests/v3_api_integration.rs via build_v3_test_router.
        //
        // NOTE: legacy `/api/executions/:id` (workflow_executions) still
        // exists below for backward compat with the old workflow UI; the
        // new unified chain lives under /api/v3/executions/*.
        .route("/api/v3/executions/:id", get(mapleos_server::execution_handlers::get_execution_handler))
        .route("/api/v3/executions/:id/events", get(mapleos_server::execution_handlers::list_events_handler))
        .route("/api/v3/executions/:id/events/stream", get(mapleos_server::execution_handlers::sse_events_handler))
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
