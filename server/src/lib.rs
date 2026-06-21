pub mod cache;
pub mod config;
pub mod db;
pub mod execution_handlers;
pub mod learning_handlers;
pub mod metrics;
pub mod middleware;
pub mod sandbox;
pub mod skills;
pub mod state;
pub mod v3_auth;

use axum::Router;
use axum::routing::{delete, get, post, put};
use state::AppState;
use std::sync::Arc;

/// Build a minimal AppState backed by an in-memory SQLite pool.
/// Runs all migrations and initialises only the services needed by v3 routes.
pub async fn build_test_app_state(pool: sqlx::SqlitePool) -> Arc<AppState> {
    use maple_collab::dm_service::DmService;
    use maple_collab::group::GroupManager;
    use maple_collab::group_cron::GroupCronService;
    use maple_collab::group_message::GroupMessageManager;
    use maple_collab::group_rules::{GroupRulesEngine, GroupRulesService};
    use maple_collab::workspace::WorkspaceManager;
    use maple_engine::approval::ApprovalService;
    use maple_engine::event_bus::EventBus;
    use maple_engine::executor::WorkflowExecutor;
    use maple_engine::memory_service::MemoryService;
    use maple_engine::scheduler::Scheduler;
    use maple_engine::skill_registry::SkillRegistry;
    use maple_engine::task_queue::TaskQueueService;
    use maple_engine::task_service::TaskService;
    use maple_gateway::auth::AuthService;
    use maple_gateway::mcp_host::McpHostManager;
    use maple_kb::bm25::BM25Searcher;
    use maple_kb::evolver::Evolver;
    use maple_kb::indexer::Indexer;
    use maple_kb::memory::MemoryStore;
    use maple_kb::prompt_version::PromptVersionManager;
    use maple_kb::retriever::HybridRetriever;
    use maple_kb::vector_store::InMemoryVectorStore;
    use maple_llm::embedding::FallbackEmbedder;
    use maple_llm::router::LlmRouter;
    use maple_llm::usage::UsageTracker;
    use maple_agent::registry::AgentRegistry;
    use maple_agent::session_store::SessionStore;

    // Run all migrations
    db::run_migrations(&pool).await.expect("migrations");

    let event_bus = Arc::new(EventBus::new());
    let scheduler = Arc::new(Scheduler::new());
    let usage_tracker = Arc::new(UsageTracker::new(50.0));
    let llm_router = Arc::new(LlmRouter::new(usage_tracker));
    let vector_store = Arc::new(InMemoryVectorStore::new(pool.clone()));
    let embedder: Arc<dyn maple_llm::embedding::Embedder> = Arc::new(FallbackEmbedder::new(128));
    let memory_store = Arc::new(tokio::sync::Mutex::new(MemoryStore::new(pool.clone())));
    let evolver = Arc::new(Evolver::new(llm_router.clone()).with_memory_store(memory_store.clone()));

    let task_queue = Arc::new(TaskQueueService::new(pool.clone()));
    task_queue.init_schema().await.unwrap();

    let group_manager = Arc::new(GroupManager::new(pool.clone()));
    let group_cron_service = Arc::new(GroupCronService::new(
        pool.clone(),
        scheduler.clone(),
        event_bus.clone(),
    ));
    group_cron_service.init().await.unwrap();

    let group_rules_engine = Arc::new(tokio::sync::RwLock::new(GroupRulesEngine::new()));
    let group_rules_service = Arc::new(GroupRulesService::new(pool.clone(), group_rules_engine.clone()));

    Arc::new(AppState {
        config: Arc::new(tokio::sync::RwLock::new(state::ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            database_url: "sqlite::memory:".to_string(),
            jwt_secret: "test-secret".to_string(),
            require_auth: false,
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            usage_limit_usd: 50.0,
            log_level: "info".to_string(),
        })),
        db: pool.clone(),
        event_bus: event_bus.clone(),
        llm_router: llm_router.clone(),
        workflow_executor: Arc::new({
            let hook_runner = Arc::new(maple_engine::hooks::HookRunner::new());
            let node_executor = maple_engine::executor::NodeExecutor::new(
                llm_router.clone(),
                Arc::new(SkillRegistry::new()),
                hook_runner.clone(),
            );
            WorkflowExecutor::new(
                event_bus.clone(),
                node_executor,
                Arc::new(maple_engine::checkpoint::CheckpointManager::new(pool.clone())),
                hook_runner,
            )
        }),
        agent_registry: Arc::new(AgentRegistry::new()),
        auth_service: Arc::new(AuthService::new("test-secret".to_string())),
        workspace_manager: Arc::new(tokio::sync::Mutex::new(WorkspaceManager::new(pool.clone()))),
        sync_engine: Arc::new(maple_sync::sync_engine::SyncEngine::new(None, 300)),
        skill_registry: Arc::new(SkillRegistry::new()),
        session_store: Arc::new(SessionStore::new(pool.clone())),
        bm25_searcher: Arc::new(BM25Searcher::new()),
        vector_store,
        hybrid_retriever: Arc::new(HybridRetriever::new()),
        indexer: Arc::new(Indexer::new(512, 64)),
        embedder,
        memory_store,
        evolver,
        prompt_version_mgr: Arc::new(PromptVersionManager::new(pool.clone())),
        task_queue,
        scheduler,
        group_rules: group_rules_engine,
        group_rules_service,
        group_manager,
        group_message_manager: Arc::new(GroupMessageManager::new(pool.clone())),
        task_service: Arc::new(TaskService::new(pool.clone())),
        approval_service: Arc::new(ApprovalService::new(pool.clone()).with_recorder(
            maple_engine::ExecutionRecorder::new(pool.clone()),
        )),
        memory_service: Arc::new(MemoryService::new(pool.clone())),
        dm_service: Arc::new(DmService::new(pool.clone(), GroupManager::new(pool.clone()))),
        group_cron_service,
        hook_service: Arc::new(maple_engine::agent_hooks::AgentHookService::new(pool.clone())),
        workflow_service: Arc::new(maple_engine::WorkflowService::new(pool.clone())),
        mcp_host: Arc::new(McpHostManager::new()),
        rate_limiter: state::RateLimiter::new(1000, 60),
        cache: cache::AppCache::new(),
        metrics: metrics::AppMetrics::new(),
        execution_recorder: maple_engine::ExecutionRecorder::new(pool.clone()),
        learning_governance: Arc::new(maple_kb::LearningGovernanceService::new(pool.clone())),
    })
}

/// Build an axum Router with all v3 API routes for integration testing.
pub fn build_v3_test_router(state: Arc<AppState>) -> Router {
    use v3_handlers::*;

    Router::new()
        // Groups
        .route("/api/v3/groups", get(v3_list_groups).post(v3_create_group))
        .route("/api/v3/groups/:id", get(v3_get_group))
        .route("/api/v3/groups/:id/members", get(v3_list_members).post(v3_add_member))
        // Messages
        .route("/api/v3/groups/:id/messages", get(v3_list_messages).post(v3_send_message))
        .route("/api/v3/groups/:id/messages/:mid", put(v3_edit_message).delete(v3_delete_message))
        .route("/api/v3/groups/:id/messages/:mid/reactions", post(v3_add_reaction))
        .route("/api/v3/groups/:id/messages/:mid/reactions/:emoji", delete(v3_remove_reaction))
        .route("/api/v3/groups/:id/messages/:mid/pin", post(v3_pin_message).delete(v3_unpin_message))
        .route("/api/v3/groups/:id/messages/:mid/thread", get(v3_get_thread))
        .route("/api/v3/groups/:id/messages/:mid/read", post(v3_mark_read))
        .route("/api/v3/groups/:id/messages/search", get(v3_search_messages))
        // Tasks
        .route("/api/v3/tasks", get(v3_list_tasks).post(v3_create_task))
        .route("/api/v3/tasks/:id", get(v3_get_task))
        .route("/api/v3/tasks/:id/transition", post(v3_transition_task))
        .route("/api/v3/tasks/:id/comments", post(v3_add_comment))
        .route("/api/v3/tasks/:id/history", get(v3_task_history))
        // Approvals
        .route("/api/v3/approvals", post(v3_create_approval))
        .route("/api/v3/approvals/pending", get(v3_list_pending_approvals))
        .route("/api/v3/approvals/:id", get(v3_get_approval))
        .route("/api/v3/approvals/:id/vote", post(v3_vote))
        .route("/api/v3/approvals/:id/votes", get(v3_list_votes))
        // Memory
        .route("/api/v3/memories", post(v3_memory_store))
        .route("/api/v3/memories/search", post(v3_memory_search))
        .route("/api/v3/memories/stats", get(v3_memory_stats))
        // DMs
        .route("/api/v3/dms", post(v3_create_dm).get(v3_list_dms))
        .route("/api/v3/dms/:id/grants", post(v3_grant_tool).get(v3_list_grants))
        .route("/api/v3/dms/:id/grants/:tool", delete(v3_revoke_tool))
        // A2A
        .route("/api/v3/dms/:id/delegations", post(v3_create_delegation))
        .route("/api/v3/a2a/delegations", get(v3_list_delegations))
        .route("/api/v3/a2a/:id/intervene", post(v3_intervene_delegation))
        // Rules
        .route("/api/v3/groups/:id/rules", get(v3_list_rules).post(v3_create_rule))
        .route("/api/v3/groups/:id/rules/:rid", put(v3_update_rule).delete(v3_delete_rule))
        // Cron
        .route("/api/v3/groups/:id/cron", get(v3_list_cron).post(v3_create_cron))
        .route("/api/v3/groups/:id/cron/:cid", put(v3_update_cron).delete(v3_delete_cron))
        // Attachments
        .route("/api/v3/groups/:id/attachments", get(v3_list_attachments).post(v3_upload_attachment))
        .route("/api/v3/attachments/:aid", get(v3_download_attachment).delete(v3_delete_attachment).put(v3_link_attachment))
        // Hooks
        .route("/api/v3/groups/:id/hooks", get(v3_list_hooks).post(v3_create_hook))
        .route("/api/v3/groups/:id/hooks/:hid", get(v3_get_hook).put(v3_update_hook).delete(v3_delete_hook))
        .route("/api/v3/groups/:id/hooks/:hid/logs", get(v3_list_hook_logs))
        // Workflow definitions
        .route("/api/v3/workflows", get(v3_list_workflows).post(v3_create_workflow))
        .route("/api/v3/workflows/:wid", get(v3_get_workflow).put(v3_update_workflow).delete(v3_delete_workflow))
        // T2-1: workflow validate endpoint
        .route("/api/v3/workflows/:wid/validate", post(v3_validate_workflow))
        // Workflow runs
        .route("/api/v3/workflow-runs", get(v3_list_workflow_runs).post(v3_create_workflow_run))
        .route("/api/v3/workflow-runs/:rid", get(v3_get_workflow_run))
        .route("/api/v3/workflow-runs/:rid/status", put(v3_update_workflow_run_status))
        .route("/api/v3/workflow-runs/:rid/checkpoints", get(v3_list_checkpoints).post(v3_record_checkpoint))
        // Unified execution fact chain (Track 1 / T1-2)
        // NOTE: legacy /api/executions/:id (workflow_executions) is mounted
        // in main.rs; the new unified chain lives under /api/v3/executions/*.
        .route("/api/v3/executions/:id", get(execution_handlers::get_execution_handler))
        .route("/api/v3/executions/:id/events", get(execution_handlers::list_events_handler))
        .route("/api/v3/executions/:id/tool-invocations", get(execution_handlers::list_tool_invocations_handler))
        .route("/api/v3/executions/:id/events/stream", get(execution_handlers::sse_events_handler))
        // Learning governance (Track 3 / T3-6..T3-11)
        .route("/api/v3/learning/candidates", get(learning_handlers::list_candidates_handler))
        .route("/api/v3/learning/candidates/pending", get(learning_handlers::list_pending_handler))
        .route("/api/v3/learning/candidates/:id", get(learning_handlers::get_candidate_handler))
        .route("/api/v3/learning/candidates/:id/approve", post(learning_handlers::approve_handler))
        .route("/api/v3/learning/candidates/:id/reject", post(learning_handlers::reject_handler))
        .route("/api/v3/learning/candidates/:id/revoke", post(learning_handlers::revoke_handler))
        .route("/api/v3/learning/blocked", get(learning_handlers::is_blocked_handler))
        .with_state(state)
}

/// Thin v3 handler wrappers used by `build_v3_test_router`.
/// Unified execution fact chain handlers live in `crate::execution_handlers`
/// (declared at the top of this file). See `docs/execution-fact-chain-spec.md`
/// §6 for the API contract. Routes are mounted in `build_v3_test_router`
/// (lib) and in `main.rs::build_app` (bin) via
/// `mapleos_server::execution_handlers::*`.

mod v3_handlers {
    #![allow(unused_variables)]
    use axum::extract::{Path, Query, State};
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::Json;
    use serde::Deserialize;
    use std::sync::Arc;

    use crate::state::AppState;

    // ── Request types ──

    #[derive(Debug, Deserialize)]
    pub struct CreateGroupReq {
        pub name: String,
        pub description: Option<String>,
        pub group_type: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct AddMemberReq {
        pub member_id: String,
        pub member_type: Option<String>,
        pub role: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct SendMessageReq {
        pub sender_id: String,
        pub sender_type: Option<String>,
        pub message_type: Option<String>,
        pub content: String,
        pub reply_to_id: Option<String>,
        pub thread_root_id: Option<String>,
        pub source_channel: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct EditMessageReq {
        pub editor_id: String,
        pub content: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct ReactionReq {
        pub user_id: String,
        pub emoji: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct PinReq {
        pub pinned_by: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct MarkReadReq {
        pub user_id: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct CreateTaskReq {
        pub title: String,
        pub description: Option<String>,
        pub creator_id: String,
        pub project_id: Option<String>,
        pub group_id: Option<String>,
        pub priority: Option<String>,
        pub assignee_id: Option<String>,
        pub source_message_id: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct TransitionReq {
        pub status: String,
        pub changed_by: String,
        pub reason: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct CommentReq {
        pub user_id: String,
        pub content: String,
        pub source_message_id: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct CreateApprovalReq {
        pub group_id: String,
        pub title: String,
        pub description: Option<String>,
        pub request_type: Option<String>,
        pub requester_id: String,
        pub urgency: Option<String>,
        pub quorum_type: Option<String>,
        pub approver_spec: String,
        pub context: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct VoteReq {
        pub voter_id: String,
        pub decision: String,
        pub comment: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct MemoryStoreReq {
        pub agent_id: String,
        pub memory_type: Option<String>,
        pub content: String,
        pub summary: Option<String>,
        pub source_type: Option<String>,
        pub source_id: Option<String>,
        pub group_id: Option<String>,
        pub ttl_hours: Option<i64>,
    }

    #[derive(Debug, Deserialize)]
    pub struct MemorySearchReq {
        pub agent_id: String,
        pub query_text: Option<String>,
        pub memory_type: Option<String>,
        pub group_id: Option<String>,
        pub limit: Option<i64>,
    }

    #[derive(Debug, Deserialize)]
    pub struct CreateDmReq {
        pub target_user_id: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct GrantToolReq {
        pub tool_name: String,
        pub expires_at: Option<i64>,
        pub scope: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct CreateDelegationReq {
        pub executor_id: String,
        pub prompt: String,
        pub task_id: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    pub struct InterveneReq {
        pub action: String,
        pub reason: Option<String>,
        pub new_executor_id: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct CreateRuleReq {
        pub rule_type: String,
        pub config: serde_json::Value,
        pub priority: Option<i64>,
    }

    #[derive(Debug, Deserialize)]
    pub struct UpdateRuleReq {
        pub config: Option<serde_json::Value>,
        pub priority: Option<i64>,
        pub enabled: Option<bool>,
    }

    #[derive(Debug, Deserialize)]
    pub struct CreateCronReq {
        pub name: String,
        pub cron_expr: String,
        pub message_template: String,
        pub job_type: Option<String>,
        pub target_agent_id: Option<String>,
        pub enabled: Option<bool>,
    }

    #[derive(Debug, Deserialize)]
    pub struct UpdateCronReq {
        pub name: Option<String>,
        pub cron_expr: Option<String>,
        pub message_template: Option<String>,
        pub enabled: Option<bool>,
    }

    #[derive(Debug, Deserialize)]
    pub struct ListMessagesParams {
        pub limit: Option<i64>,
        pub before: Option<i64>,
    }

    #[derive(Debug, Deserialize)]
    pub struct ListTasksParams {
        pub group_id: Option<String>,
        pub status: Option<String>,
        pub limit: Option<i64>,
    }

    #[derive(Debug, Deserialize)]
    pub struct SearchParams {
        pub q: String,
        pub limit: Option<i64>,
    }

    #[derive(Debug, Deserialize)]
    pub struct PendingParams {
        pub user_id: String,
        pub group_id: Option<String>,
    }

    // ── Group handlers ──

    pub async fn v3_create_group(
        State(state): State<Arc<AppState>>,
        Json(req): Json<CreateGroupReq>,
    ) -> impl IntoResponse {
        let gt = match req.group_type.as_deref() {
            Some("project") => maple_collab::group::GroupType::Project,
            Some("channel") => maple_collab::group::GroupType::Channel,
            Some("dm") => maple_collab::group::GroupType::Dm,
            _ => maple_collab::group::GroupType::Collaboration,
        };
        let settings = maple_collab::group::GroupSettings::default();
        match state.group_manager.create_group(&req.name, req.description.as_deref(), gt, "system", &settings).await {
            Ok(group) => (StatusCode::CREATED, Json(serde_json::json!({ "group": group }))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
        }
    }

    pub async fn v3_list_groups(
        State(state): State<Arc<AppState>>,
    ) -> impl IntoResponse {
        match state.group_manager.list_groups("system").await {
            Ok(groups) => Json(serde_json::json!({ "groups": groups })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_get_group(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
    ) -> Result<Json<serde_json::Value>, StatusCode> {
        match state.group_manager.get_group(&id).await {
            Ok(Some(g)) => Ok(Json(serde_json::json!({ "group": g }))),
            Ok(None) => Err(StatusCode::NOT_FOUND),
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    }

    pub async fn v3_list_members(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
    ) -> impl IntoResponse {
        match state.group_manager.list_members(&id).await {
            Ok(m) => Json(serde_json::json!({ "members": m })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_add_member(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
        Json(req): Json<AddMemberReq>,
    ) -> impl IntoResponse {
        let mt = req.member_type.as_deref().unwrap_or("human");
        let role = req.role.as_deref().unwrap_or("member");
        match state.group_manager.add_member(&id, &req.member_id, mt, role).await {
            Ok(true) => Json(serde_json::json!({ "status": "added" })),
            Ok(false) => Json(serde_json::json!({ "status": "already_member" })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    // ── Message handlers ──

    pub async fn v3_send_message(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
        Json(req): Json<SendMessageReq>,
    ) -> impl IntoResponse {
        let msg_type = maple_collab::group_message::MessageType::from_str(
            req.message_type.as_deref().unwrap_or("text"),
        );
        match state.group_message_manager.send_message(
            &id, &req.sender_id,
            req.sender_type.as_deref().unwrap_or("human"),
            msg_type,
            &req.content,
            req.reply_to_id.as_deref(),
            req.thread_root_id.as_deref(),
            req.source_channel.as_deref().unwrap_or("api"),
        ).await {
            Ok(msg) => (StatusCode::CREATED, Json(serde_json::json!({ "message": msg }))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
        }
    }

    pub async fn v3_list_messages(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
        Query(params): Query<ListMessagesParams>,
    ) -> impl IntoResponse {
        match state.group_message_manager.get_messages(&id, params.limit.unwrap_or(50), params.before).await {
            Ok(page) => Json(serde_json::json!(page)),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_edit_message(
        State(state): State<Arc<AppState>>,
        Path((id, mid)): Path<(String, String)>,
        Json(req): Json<EditMessageReq>,
    ) -> impl IntoResponse {
        match state.group_message_manager.edit_message(&mid, &req.editor_id, &req.content).await {
            Ok(_) => Json(serde_json::json!({ "status": "edited" })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_delete_message(
        State(state): State<Arc<AppState>>,
        Path((id, mid)): Path<(String, String)>,
    ) -> impl IntoResponse {
        match state.group_message_manager.delete_message(&mid).await {
            Ok(_) => Json(serde_json::json!({ "status": "deleted" })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_add_reaction(
        State(state): State<Arc<AppState>>,
        Path((id, mid)): Path<(String, String)>,
        Json(req): Json<ReactionReq>,
    ) -> impl IntoResponse {
        match state.group_message_manager.add_reaction(&mid, &req.user_id, &req.emoji).await {
            Ok(_) => Json(serde_json::json!({ "status": "ok" })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_remove_reaction(
        State(state): State<Arc<AppState>>,
        Path((id, mid, emoji)): Path<(String, String, String)>,
        axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    ) -> impl IntoResponse {
        let user_id = params.get("user_id").cloned().unwrap_or_default();
        match state.group_message_manager.remove_reaction(&mid, &user_id, &emoji).await {
            Ok(_) => Json(serde_json::json!({ "status": "ok" })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_pin_message(
        State(state): State<Arc<AppState>>,
        Path((id, mid)): Path<(String, String)>,
        Json(req): Json<PinReq>,
    ) -> impl IntoResponse {
        match state.group_message_manager.pin_message(&mid, &id, &req.pinned_by).await {
            Ok(_) => Json(serde_json::json!({ "status": "pinned" })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_unpin_message(
        State(state): State<Arc<AppState>>,
        Path((id, mid)): Path<(String, String)>,
    ) -> impl IntoResponse {
        match state.group_message_manager.unpin_message(&mid).await {
            Ok(_) => Json(serde_json::json!({ "status": "unpinned" })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_get_thread(
        State(state): State<Arc<AppState>>,
        Path((id, mid)): Path<(String, String)>,
    ) -> impl IntoResponse {
        match state.group_message_manager.get_thread(&mid, 50).await {
            Ok(msgs) => Json(serde_json::json!({ "messages": msgs })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_mark_read(
        State(state): State<Arc<AppState>>,
        Path((id, mid)): Path<(String, String)>,
        Json(req): Json<MarkReadReq>,
    ) -> impl IntoResponse {
        match state.group_message_manager.mark_as_read(&id, &req.user_id, &mid).await {
            Ok(_) => Json(serde_json::json!({ "status": "ok" })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_search_messages(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
        Query(params): Query<SearchParams>,
    ) -> impl IntoResponse {
        match state.group_message_manager.search_messages(&id, &params.q, params.limit.unwrap_or(20)).await {
            Ok(msgs) => Json(serde_json::json!({ "messages": msgs })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    // ── Task handlers ──

    pub async fn v3_create_task(
        State(state): State<Arc<AppState>>,
        Json(req): Json<CreateTaskReq>,
    ) -> impl IntoResponse {
        let priority = maple_engine::task_service::TaskPriority::from_str(
            req.priority.as_deref().unwrap_or("medium"),
        );
        match state.task_service.create_task(
            &req.title, req.description.as_deref(), &req.creator_id,
            req.project_id.as_deref(), req.group_id.as_deref(),
            priority, req.assignee_id.as_deref(), req.source_message_id.as_deref(),
            None,
        ).await {
            Ok(task) => (StatusCode::CREATED, Json(serde_json::json!({ "task": task }))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
        }
    }

    pub async fn v3_list_tasks(
        State(state): State<Arc<AppState>>,
        Query(params): Query<ListTasksParams>,
    ) -> impl IntoResponse {
        match state.task_service.list_tasks(params.group_id.as_deref(), params.status.as_deref(), params.limit.unwrap_or(50)).await {
            Ok(tasks) => Json(serde_json::json!({ "tasks": tasks })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_get_task(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
    ) -> Result<Json<serde_json::Value>, StatusCode> {
        match state.task_service.get_task(&id).await {
            Ok(Some(t)) => Ok(Json(serde_json::json!({ "task": t }))),
            Ok(None) => Err(StatusCode::NOT_FOUND),
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    }

    pub async fn v3_transition_task(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
        Json(req): Json<TransitionReq>,
    ) -> impl IntoResponse {
        let new_status = maple_engine::task_service::TaskV3Status::from_str(&req.status);
        match state.task_service.transition_task(&id, new_status, &req.changed_by, req.reason.as_deref()).await {
            Ok(task) => Json(serde_json::json!({ "task": task })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_add_comment(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
        Json(req): Json<CommentReq>,
    ) -> impl IntoResponse {
        match state.task_service.add_comment(&id, &req.user_id, &req.content, req.source_message_id.as_deref()).await {

            Ok(cid) => Json(serde_json::json!({ "id": cid })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_task_history(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
    ) -> impl IntoResponse {
        match state.task_service.get_status_history(&id).await {
            Ok(h) => Json(serde_json::json!({ "history": h })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    // ── Approval handlers ──

    pub async fn v3_create_approval(
        State(state): State<Arc<AppState>>,
        Json(req): Json<CreateApprovalReq>,
    ) -> impl IntoResponse {
        let urgency = match req.urgency.as_deref() {
            Some("low") => maple_engine::approval::ApprovalUrgency::Low,
            Some("high") => maple_engine::approval::ApprovalUrgency::High,
            Some("critical") => maple_engine::approval::ApprovalUrgency::Critical,
            _ => maple_engine::approval::ApprovalUrgency::Normal,
        };
        let quorum = match req.quorum_type.as_deref() {
            Some("all") => maple_engine::approval::QuorumType::All,
            Some("majority") => maple_engine::approval::QuorumType::Majority,
            _ => maple_engine::approval::QuorumType::Any,
        };
        match state.approval_service.create_request(
            &req.group_id, &req.title, req.description.as_deref(),
            req.request_type.as_deref().unwrap_or("general"), &req.requester_id,
            urgency, quorum, &req.approver_spec, req.context.as_deref(),
        ).await {
            Ok(a) => (StatusCode::CREATED, Json(serde_json::json!({ "approval": a }))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
        }
    }

    pub async fn v3_get_approval(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
    ) -> Result<Json<serde_json::Value>, StatusCode> {
        match state.approval_service.get_request(&id).await {
            Ok(Some(a)) => Ok(Json(serde_json::json!({ "approval": a }))),
            Ok(None) => Err(StatusCode::NOT_FOUND),
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    }

    pub async fn v3_vote(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
        Json(req): Json<VoteReq>,
    ) -> impl IntoResponse {
        let decision = match req.decision.as_str() {
            "approve" => maple_engine::approval::VoteDecision::Approve,
            "reject" => maple_engine::approval::VoteDecision::Reject,
            _ => maple_engine::approval::VoteDecision::Abstain,
        };
        match state.approval_service.vote(&id, &req.voter_id, decision, req.comment.as_deref()).await {
            Ok(outcome) => Json(serde_json::json!({ "outcome": outcome })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_list_votes(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
    ) -> impl IntoResponse {
        match state.approval_service.list_votes(&id).await {
            Ok(votes) => Json(serde_json::json!({ "votes": votes })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_list_pending_approvals(
        State(state): State<Arc<AppState>>,
        Query(params): Query<PendingParams>,
    ) -> impl IntoResponse {
        match state.approval_service.list_pending_for_user(&params.user_id, params.group_id.as_deref()).await {
            Ok(approvals) => Json(serde_json::json!({ "approvals": approvals })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    // ── Memory handlers ──

    pub async fn v3_memory_store(
        State(state): State<Arc<AppState>>,
        Json(req): Json<MemoryStoreReq>,
    ) -> impl IntoResponse {
        let memory_type = maple_engine::memory_service::MemoryLayer::from_str(
            req.memory_type.as_deref().unwrap_or("episodic"),
        );
        match state.memory_service.store(
            &req.agent_id, memory_type,
            &req.content, req.summary.as_deref(),
            req.source_type.as_deref(), req.source_id.as_deref(),
            req.group_id.as_deref(), req.ttl_hours,
        ).await {
            Ok(m) => (StatusCode::CREATED, Json(serde_json::json!({ "memory": m }))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
        }
    }

    pub async fn v3_memory_search(
        State(state): State<Arc<AppState>>,
        Json(req): Json<MemorySearchReq>,
    ) -> impl IntoResponse {
        let memory_type = req.memory_type.as_deref().map(|s| maple_engine::memory_service::MemoryLayer::from_str(s));
        let query = maple_engine::memory_service::MemoryQuery {
            agent_id: req.agent_id,
            query_text: req.query_text,
            memory_type,
            group_id: req.group_id,
            limit: req.limit.unwrap_or(20),
        };
        match state.memory_service.search(&query).await {
            Ok(results) => Json(serde_json::json!({ "results": results })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_memory_stats(
        State(state): State<Arc<AppState>>,
        axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    ) -> impl IntoResponse {
        let agent_id = params.get("agent_id").cloned().unwrap_or_default();
        match state.memory_service.stats(&agent_id).await {
            Ok(stats) => Json(serde_json::json!({ "stats": stats })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    // ── DM handlers ──

    pub async fn v3_create_dm(
        State(state): State<Arc<AppState>>,
        Json(req): Json<CreateDmReq>,
    ) -> impl IntoResponse {
        match state.dm_service.find_or_create("system", &req.target_user_id).await {
            Ok(group_id) => match state.group_manager.get_group(&group_id).await {
                Ok(Some(group)) => (StatusCode::CREATED, Json(serde_json::json!({ "group": group }))),
                _ => (StatusCode::CREATED, Json(serde_json::json!({ "group": { "id": group_id } }))),
            },
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
        }
    }

    pub async fn v3_list_dms(
        State(state): State<Arc<AppState>>,
    ) -> impl IntoResponse {
        match state.dm_service.list_user_dms("system").await {
            Ok(dms) => Json(serde_json::json!({ "dms": dms })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_grant_tool(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
        Json(req): Json<GrantToolReq>,
    ) -> impl IntoResponse {
        match state.dm_service.grant_tool(&id, &req.tool_name, "system", req.expires_at, req.scope.as_deref()).await {
            Ok(gid) => Json(serde_json::json!({ "id": gid })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_list_grants(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
    ) -> impl IntoResponse {
        match state.dm_service.list_grants(&id).await {
            Ok(grants) => Json(serde_json::json!({ "grants": grants })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_revoke_tool(
        State(state): State<Arc<AppState>>,
        Path((id, tool)): Path<(String, String)>,
    ) -> impl IntoResponse {
        match state.dm_service.revoke_tool(&id, &tool, "system").await {
            Ok(_) => Json(serde_json::json!({ "status": "revoked" })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_create_delegation(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
        Json(req): Json<CreateDelegationReq>,
    ) -> impl IntoResponse {
        match state.dm_service.create_delegation(&id, "system", &req.executor_id, &req.prompt, req.task_id.as_deref(), &[]).await {
            Ok(d) => (StatusCode::CREATED, Json(serde_json::json!({ "delegation": d }))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
        }
    }

    pub async fn v3_list_delegations(
        State(state): State<Arc<AppState>>,
    ) -> impl IntoResponse {
        match state.dm_service.list_visible_delegations("system").await {
            Ok(delegations) => Json(serde_json::json!({ "delegations": delegations })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_intervene_delegation(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
        Json(req): Json<InterveneReq>,
    ) -> impl IntoResponse {
        // Revoke = update status to Revoked; Reroute = update executor
        let new_status = maple_collab::dm_service::DelegationStatus::Failed;
        match state.dm_service.update_delegation_status(&id, new_status, req.reason.as_deref()).await {
            Ok(_) => Json(serde_json::json!({ "status": "ok" })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    // ── Rules handlers ──

    pub async fn v3_list_rules(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
    ) -> impl IntoResponse {
        match state.group_rules_service.list_rules(&id).await {
            Ok(rules) => Json(serde_json::json!({ "rules": rules })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_create_rule(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
        Json(req): Json<CreateRuleReq>,
    ) -> impl IntoResponse {
        let create_req = maple_collab::group_rules::CreateGroupRuleRequest {
            name: req.config["name"].as_str().unwrap_or("rule").to_string(),
            description: req.config["description"].as_str().map(String::from),
            rule_type: req.rule_type,
            config: req.config,
            priority: req.priority,
            condition_expr: None,
        };
        match state.group_rules_service.create_rule(&id, "system", create_req).await {
            Ok(rule) => (StatusCode::CREATED, Json(serde_json::json!({ "rule": rule }))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
        }
    }

    pub async fn v3_update_rule(
        State(state): State<Arc<AppState>>,
        Path((_id, rid)): Path<(String, String)>,
        Json(req): Json<UpdateRuleReq>,
    ) -> impl IntoResponse {
        match state.group_rules_service.update_rule(&rid, req.config, req.priority, req.enabled, None).await {
            Ok(true) => Json(serde_json::json!({ "status": "updated" })),
            Ok(false) => Json(serde_json::json!({ "error": "rule not found or no changes" })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_delete_rule(
        State(state): State<Arc<AppState>>,
        Path((_id, rid)): Path<(String, String)>,
    ) -> impl IntoResponse {
        match state.group_rules_service.delete_rule(&rid).await {
            Ok(true) => Json(serde_json::json!({ "status": "deleted" })),
            Ok(false) => Json(serde_json::json!({ "error": "rule not found" })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    // ── Cron handlers ──

    pub async fn v3_list_cron(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
    ) -> impl IntoResponse {
        match state.group_cron_service.list_jobs(&id).await {
            Ok(jobs) => Json(serde_json::json!({ "jobs": jobs })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_create_cron(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
        Json(req): Json<CreateCronReq>,
    ) -> impl IntoResponse {
        let cron_req = maple_collab::group_cron::CreateCronJobRequest {
            name: req.name,
            cron_expr: req.cron_expr,
            message_template: req.message_template,
            job_type: req.job_type,
            target_agent_id: req.target_agent_id,
            enabled: req.enabled,
        };
        match state.group_cron_service.create_job(&id, "system", cron_req).await {
            Ok(job) => (StatusCode::CREATED, Json(serde_json::json!({ "job": job }))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
        }
    }

    pub async fn v3_update_cron(
        State(state): State<Arc<AppState>>,
        Path((_id, cid)): Path<(String, String)>,
        Json(req): Json<UpdateCronReq>,
    ) -> impl IntoResponse {
        match state.group_cron_service.update_job(&cid, req.name.as_deref(), req.cron_expr.as_deref(), req.message_template.as_deref(), req.enabled).await {
            Ok(_) => Json(serde_json::json!({ "status": "updated" })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_delete_cron(
        State(state): State<Arc<AppState>>,
        Path((_id, cid)): Path<(String, String)>,
    ) -> impl IntoResponse {
        match state.group_cron_service.delete_job(&cid).await {
            Ok(_) => Json(serde_json::json!({ "status": "deleted" })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    // ── Attachment handlers ──

    pub async fn v3_upload_attachment(
        State(state): State<Arc<AppState>>,
        Path(group_id): Path<String>,
        multipart: axum::extract::Multipart,
    ) -> impl IntoResponse {
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
            let id = format!("msgatt-{}", uuid::Uuid::new_v4());
            let _ = sqlx::query(
                "INSERT INTO message_attachments (id, group_id, message_id, uploader_id, filename, content_type, size, data, created_at) VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?)"
            ).bind(&id).bind(&group_id).bind("test-user").bind(&filename).bind(&content_type).bind(size).bind(&data).bind(now)
            .execute(&state.db).await;
            uploaded.push(serde_json::json!({ "id": id, "filename": filename, "size": size, "content_type": content_type }));
        }

        Json(serde_json::json!({ "attachments": uploaded }))
    }

    pub async fn v3_list_attachments(
        State(state): State<Arc<AppState>>,
        Path(id): Path<String>,
    ) -> impl IntoResponse {
        match sqlx::query_as::<_, (String, String, String, Option<String>, String, i64, i64)>(
            "SELECT id, filename, content_type, message_id, uploader_id, size, created_at FROM message_attachments WHERE group_id = ? ORDER BY created_at DESC"
        ).bind(&id).fetch_all(&state.db).await {
            Ok(rows) => {
                let attachments: Vec<serde_json::Value> = rows.iter().map(|r| {
                    serde_json::json!({
                        "id": r.0, "filename": r.1, "content_type": r.2,
                        "message_id": r.3, "uploader_id": r.4, "size": r.5, "created_at": r.6,
                    })
                }).collect();
                Json(serde_json::json!({ "attachments": attachments }))
            }
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_download_attachment(
        State(state): State<Arc<AppState>>,
        Path(aid): Path<String>,
    ) -> Result<impl IntoResponse, StatusCode> {
        match sqlx::query_as::<_, (String, String, Vec<u8>)>(
            "SELECT filename, content_type, data FROM message_attachments WHERE id = ?"
        ).bind(&aid).fetch_optional(&state.db).await {
            Ok(Some((filename, ct, data))) => {
                Ok((
                    [
                        ("content-type".to_string(), ct),
                        ("content-disposition".to_string(), format!("attachment; filename=\"{}\"", filename)),
                    ],
                    data,
                ))
            }
            Ok(None) => Err(StatusCode::NOT_FOUND),
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    }

    pub async fn v3_delete_attachment(
        State(state): State<Arc<AppState>>,
        Path(aid): Path<String>,
    ) -> Json<serde_json::Value> {
        let deleted = sqlx::query("DELETE FROM message_attachments WHERE id = ?")
            .bind(&aid).execute(&state.db).await
            .map(|r| r.rows_affected() > 0).unwrap_or(false);
        Json(serde_json::json!({ "deleted": deleted }))
    }

    #[derive(Deserialize)]
    pub struct LinkAttachmentReq {
        pub message_id: String,
    }

    pub async fn v3_link_attachment(
        State(state): State<Arc<AppState>>,
        Path(aid): Path<String>,
        Json(req): Json<LinkAttachmentReq>,
    ) -> Json<serde_json::Value> {
        let updated = sqlx::query("UPDATE message_attachments SET message_id = ? WHERE id = ?")
            .bind(&req.message_id).bind(&aid).execute(&state.db).await
            .map(|r| r.rows_affected() > 0).unwrap_or(false);
        Json(serde_json::json!({ "linked": updated }))
    }

    // ── Hook handlers ──

    #[derive(Deserialize)]
    pub struct CreateHookReq {
        pub agent_id: String,
        pub event_types: Vec<String>,
        pub condition_expr: Option<String>,
        pub action_type: String,
        pub action_config: serde_json::Value,
        pub priority: Option<i64>,
    }

    pub async fn v3_list_hooks(
        State(state): State<Arc<AppState>>,
        Path(group_id): Path<String>,
    ) -> Json<serde_json::Value> {
        match state.hook_service.list_hooks(&group_id).await {
            Ok(hooks) => Json(serde_json::json!({ "hooks": hooks })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_create_hook(
        State(state): State<Arc<AppState>>,
        Path(group_id): Path<String>,
        Json(req): Json<CreateHookReq>,
    ) -> impl IntoResponse {
        match state.hook_service.create_hook(&group_id, &maple_engine::agent_hooks::CreateHookRequest {
            agent_id: req.agent_id, event_types: req.event_types,
            condition_expr: req.condition_expr, action_type: req.action_type,
            action_config: req.action_config, priority: req.priority,
        }).await {
            Ok(hook) => (StatusCode::CREATED, Json(serde_json::json!({ "hook": hook }))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
        }
    }

    pub async fn v3_get_hook(
        State(state): State<Arc<AppState>>,
        Path((_group_id, hook_id)): Path<(String, String)>,
    ) -> impl IntoResponse {
        match state.hook_service.get_hook(&hook_id).await {
            Ok(Some(hook)) => Json(serde_json::json!({ "hook": hook })).into_response(),
            Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not found" }))).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
        }
    }

    pub async fn v3_update_hook(
        State(state): State<Arc<AppState>>,
        Path((_group_id, hook_id)): Path<(String, String)>,
        Json(body): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        match state.hook_service.update_hook(&hook_id, &body).await {
            Ok(updated) => Json(serde_json::json!({ "updated": updated })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_delete_hook(
        State(state): State<Arc<AppState>>,
        Path((_group_id, hook_id)): Path<(String, String)>,
    ) -> Json<serde_json::Value> {
        match state.hook_service.delete_hook(&hook_id).await {
            Ok(deleted) => Json(serde_json::json!({ "deleted": deleted })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_list_hook_logs(
        State(state): State<Arc<AppState>>,
        Path((_group_id, hook_id)): Path<(String, String)>,
        Query(params): Query<std::collections::HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        let limit = params.get("limit").and_then(|v| v.parse::<i64>().ok()).unwrap_or(50);
        match state.hook_service.list_logs(&hook_id, limit).await {
            Ok(logs) => Json(serde_json::json!({ "logs": logs })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    // ── Workflow Definitions ──

    #[derive(Debug, Deserialize)]
    pub struct CreateWorkflowReq {
        pub id: String,
        pub name: String,
        pub yaml_content: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct UpdateWorkflowReq {
        pub name: Option<String>,
        pub yaml_content: Option<String>,
        pub status: Option<String>,
    }

    pub async fn v3_list_workflows(
        State(state): State<Arc<AppState>>,
    ) -> Json<serde_json::Value> {
        match state.workflow_service.list_definitions().await {
            Ok(defs) => Json(serde_json::json!({ "workflows": defs })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_create_workflow(
        State(state): State<Arc<AppState>>,
        Json(req): Json<CreateWorkflowReq>,
    ) -> impl IntoResponse {
        match state.workflow_service.create_definition(&req.id, &req.name, &req.yaml_content).await {
            Ok(def) => (StatusCode::CREATED, Json(serde_json::json!({ "workflow": def }))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
        }
    }

    pub async fn v3_get_workflow(
        State(state): State<Arc<AppState>>,
        Path(wid): Path<String>,
    ) -> impl IntoResponse {
        match state.workflow_service.get_definition(&wid).await {
            Ok(Some(def)) => (StatusCode::OK, Json(serde_json::json!({ "workflow": def }))),
            Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not found" }))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
        }
    }

    pub async fn v3_update_workflow(
        State(state): State<Arc<AppState>>,
        Path(wid): Path<String>,
        Json(req): Json<UpdateWorkflowReq>,
    ) -> Json<serde_json::Value> {
        match state.workflow_service.update_definition(&wid, req.name.as_deref(), req.yaml_content.as_deref(), req.status.as_deref()).await {
            Ok(updated) => Json(serde_json::json!({ "updated": updated })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_delete_workflow(
        State(state): State<Arc<AppState>>,
        Path(wid): Path<String>,
    ) -> Json<serde_json::Value> {
        match state.workflow_service.delete_definition(&wid).await {
            Ok(deleted) => Json(serde_json::json!({ "deleted": deleted })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    /// T2-1: Validate a workflow definition.
    /// POST /api/v3/workflows/:wid/validate
    /// Returns { valid: bool, errors: [...] } with all validation violations.
    pub async fn v3_validate_workflow(
        State(state): State<Arc<AppState>>,
        Path(wid): Path<String>,
    ) -> Json<serde_json::Value> {
        match state.workflow_service.get_definition(&wid).await {
            Ok(Some(def)) => {
                match maple_engine::Workflow::parse_definition(&def.yaml_content) {
                    Ok(wf) => match wf.validate() {
                        Ok(()) => Json(serde_json::json!({
                            "valid": true,
                            "errors": [],
                            "workflow_id": wid,
                            "version": def.version,
                        })),
                        Err(errors) => Json(serde_json::json!({
                            "valid": false,
                            "errors": errors,
                            "workflow_id": wid,
                            "version": def.version,
                        })),
                    },
                    Err(e) => Json(serde_json::json!({
                        "valid": false,
                        "errors": [format!("parse error: {e}")],
                        "workflow_id": wid,
                        "version": def.version,
                    })),
                }
            }
            Ok(None) => Json(serde_json::json!({
                "valid": false,
                "errors": ["workflow not found"],
                "workflow_id": wid,
            })),
            Err(e) => Json(serde_json::json!({
                "valid": false,
                "errors": [format!("fetch error: {e}")],
                "workflow_id": wid,
            })),
        }
    }

    // ── Workflow Runs ──

    #[derive(Debug, Deserialize)]
    pub struct CreateRunReq {
        pub workflow_id: String,
        pub workflow_version: i64,
        pub input: String,
        pub group_id: Option<String>,
        pub agent_id: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct UpdateRunStatusReq {
        pub status: String,
        pub output: Option<String>,
        pub error: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct RecordCheckpointReq {
        pub node_id: String,
        pub output: String,
        pub context_snapshot: String,
    }

    pub async fn v3_list_workflow_runs(
        State(state): State<Arc<AppState>>,
        Query(params): Query<std::collections::HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        let limit = params.get("limit").and_then(|v| v.parse::<i64>().ok());
        match state.workflow_service.list_runs(
            params.get("workflow_id").map(|s| s.as_str()),
            params.get("group_id").map(|s| s.as_str()),
            params.get("status").map(|s| s.as_str()),
            limit,
        ).await {
            Ok(runs) => Json(serde_json::json!({ "runs": runs })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_create_workflow_run(
        State(state): State<Arc<AppState>>,
        Json(req): Json<CreateRunReq>,
    ) -> impl IntoResponse {
        match state.workflow_service.create_run(&req.workflow_id, req.workflow_version, &req.input, req.group_id.as_deref(), req.agent_id.as_deref()).await {
            Ok(run) => (StatusCode::CREATED, Json(serde_json::json!({ "run": run }))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
        }
    }

    pub async fn v3_get_workflow_run(
        State(state): State<Arc<AppState>>,
        Path(rid): Path<String>,
    ) -> impl IntoResponse {
        match state.workflow_service.get_run(&rid).await {
            Ok(Some(run)) => (StatusCode::OK, Json(serde_json::json!({ "run": run }))),
            Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not found" }))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
        }
    }

    pub async fn v3_update_workflow_run_status(
        State(state): State<Arc<AppState>>,
        Path(rid): Path<String>,
        Json(req): Json<UpdateRunStatusReq>,
    ) -> Json<serde_json::Value> {
        match state.workflow_service.update_run_status(&rid, &req.status, req.output.as_deref(), req.error.as_deref()).await {
            Ok(updated) => Json(serde_json::json!({ "updated": updated })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_list_checkpoints(
        State(state): State<Arc<AppState>>,
        Path(rid): Path<String>,
    ) -> Json<serde_json::Value> {
        match state.workflow_service.list_checkpoints(&rid).await {
            Ok(cps) => Json(serde_json::json!({ "checkpoints": cps })),
            Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
        }
    }

    pub async fn v3_record_checkpoint(
        State(state): State<Arc<AppState>>,
        Path(rid): Path<String>,
        Json(req): Json<RecordCheckpointReq>,
    ) -> impl IntoResponse {
        match state.workflow_service.record_checkpoint(&rid, &req.node_id, &req.output, &req.context_snapshot).await {
            Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
        }
    }
}
