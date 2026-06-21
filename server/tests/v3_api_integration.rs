use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mapleos_server::build_test_app_state;
use mapleos_server::build_v3_test_router;
use mapleos_server::state::AppState;
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

async fn setup() -> axum::Router {
    let (_, router) = setup_with_state().await;
    router
}

/// Build a router + the underlying AppState so tests can drive the
/// ExecutionRecorder directly while still exercising the HTTP layer.
async fn setup_with_state() -> (Arc<AppState>, axum::Router) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let state = build_test_app_state(pool).await;
    let router = build_v3_test_router(state.clone());
    (state, router)
}

async fn send_json(app: &axum::Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let val: Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::json!({ "raw": String::from_utf8_lossy(&bytes) }));
    (status, val)
}

async fn get_json(app: &axum::Router, path: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let val: Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::json!({ "raw": String::from_utf8_lossy(&bytes) }));
    (status, val)
}

// ============================================================
// Test: Create and List Groups
// ============================================================

#[tokio::test]
async fn test_create_and_list_groups() {
    let app = setup().await;

    // Create a group
    let (status, body) = send_json(&app, "POST", "/api/v3/groups", serde_json::json!({
        "name": "test-group",
        "description": "A test group",
        "group_type": "collaboration",
    })).await;

    assert_eq!(status, StatusCode::CREATED, "create group failed: {:?}", body);
    let group = &body["group"];
    assert_eq!(group["name"], "test-group");
    assert_eq!(group["description"], "A test group");
    let group_id = group["id"].as_str().unwrap();

    // List groups
    let (status, body) = get_json(&app, "/api/v3/groups").await;
    assert_eq!(status, StatusCode::OK);
    let groups = body["groups"].as_array().unwrap();
    assert!(groups.len() >= 1, "expected at least 1 group, got {}", groups.len());

    // Get single group
    let (status, body) = get_json(&app, &format!("/api/v3/groups/{}", group_id)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["group"]["name"], "test-group");

    // 404 for non-existent
    let (status, _) = get_json(&app, "/api/v3/groups/nonexistent").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ============================================================
// Test: Send and List Messages
// ============================================================

#[tokio::test]
async fn test_send_and_list_messages() {
    let app = setup().await;

    // Create group
    let (_, body) = send_json(&app, "POST", "/api/v3/groups", serde_json::json!({
        "name": "msg-test",
    })).await;
    let group_id = body["group"]["id"].as_str().unwrap().to_string();

    // Send a message
    let (status, body) = send_json(&app, "POST", &format!("/api/v3/groups/{}/messages", group_id), serde_json::json!({
        "sender_id": "user-1",
        "sender_type": "human",
        "content": "Hello, world!",
    })).await;
    assert_eq!(status, StatusCode::CREATED, "send message failed: {:?}", body);
    let msg = &body["message"];
    assert_eq!(msg["content"], "Hello, world!");
    assert_eq!(msg["sender_id"], "user-1");
    let msg_id = msg["id"].as_str().unwrap().to_string();

    // Send another message
    let (status, _) = send_json(&app, "POST", &format!("/api/v3/groups/{}/messages", group_id), serde_json::json!({
        "sender_id": "agent-1",
        "sender_type": "agent",
        "content": "Hi there!",
    })).await;
    assert_eq!(status, StatusCode::CREATED);

    // List messages
    let (status, body) = get_json(&app, &format!("/api/v3/groups/{}/messages?limit=10", group_id)).await;
    assert_eq!(status, StatusCode::OK);
    let messages = body["messages"].as_array().unwrap();
    assert!(messages.len() >= 2, "expected >= 2 messages, got {}", messages.len());

    // Edit message
    let (status, body) = send_json(&app, "PUT", &format!("/api/v3/groups/{}/messages/{}", group_id, msg_id), serde_json::json!({
        "editor_id": "user-1",
        "content": "Hello, world! (edited)",
    })).await;
    assert_eq!(status, StatusCode::OK, "edit failed: {:?}", body);

    // Pin message
    let (status, body) = send_json(&app, "POST", &format!("/api/v3/groups/{}/messages/{}/pin", group_id, msg_id), serde_json::json!({
        "pinned_by": "user-1",
    })).await;
    assert_eq!(status, StatusCode::OK, "pin failed: {:?}", body);

    // Unpin message
    let (status, _) = send_json(&app, "DELETE", &format!("/api/v3/groups/{}/messages/{}/pin", group_id, msg_id), serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK);

    // Delete message
    let (status, body) = send_json(&app, "DELETE", &format!("/api/v3/groups/{}/messages/{}", group_id, msg_id), serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK, "delete failed: {:?}", body);
}

// ============================================================
// Test: Task Lifecycle
// ============================================================

#[tokio::test]
async fn test_task_lifecycle() {
    let app = setup().await;

    // Create group
    let (_, body) = send_json(&app, "POST", "/api/v3/groups", serde_json::json!({
        "name": "task-test",
    })).await;
    let group_id = body["group"]["id"].as_str().unwrap().to_string();

    // Create task
    let (status, body) = send_json(&app, "POST", "/api/v3/tasks", serde_json::json!({
        "title": "Implement feature X",
        "description": "Build the new feature",
        "creator_id": "user-1",
        "group_id": group_id,
        "priority": "high",
    })).await;
    assert_eq!(status, StatusCode::CREATED, "create task failed: {:?}", body);
    let task = &body["task"];
    assert_eq!(task["title"], "Implement feature X");
    assert_eq!(task["priority"], "high");
    let task_id = task["id"].as_str().unwrap().to_string();

    // Get task
    let (status, body) = get_json(&app, &format!("/api/v3/tasks/{}", task_id)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["task"]["title"], "Implement feature X");

    // List tasks
    let (status, body) = get_json(&app, &format!("/api/v3/tasks?group_id={}", group_id)).await;
    assert_eq!(status, StatusCode::OK);
    let tasks = body["tasks"].as_array().unwrap();
    assert!(tasks.len() >= 1);

    // Transition: backlog → todo
    let (status, body) = send_json(&app, "POST", &format!("/api/v3/tasks/{}/transition", task_id), serde_json::json!({
        "status": "todo",
        "changed_by": "user-1",
    })).await;
    assert_eq!(status, StatusCode::OK, "transition to todo failed: {:?}", body);

    // Transition: todo → in_progress
    let (status, body) = send_json(&app, "POST", &format!("/api/v3/tasks/{}/transition", task_id), serde_json::json!({
        "status": "in_progress",
        "changed_by": "agent-1",
    })).await;
    assert_eq!(status, StatusCode::OK, "transition to in_progress failed: {:?}", body);

    // Add comment
    let (status, body) = send_json(&app, "POST", &format!("/api/v3/tasks/{}/comments", task_id), serde_json::json!({
        "user_id": "agent-1",
        "content": "Working on this now",
    })).await;
    assert_eq!(status, StatusCode::OK, "add comment failed: {:?}", body);

    // Get history
    let (status, body) = get_json(&app, &format!("/api/v3/tasks/{}/history", task_id)).await;
    assert_eq!(status, StatusCode::OK);
    let history = body["history"].as_array().unwrap();
    assert!(history.len() >= 2, "expected >= 2 history entries, got {}", history.len());
}

// ============================================================
// Test: Approval Workflow
// ============================================================

#[tokio::test]
async fn test_approval_workflow() {
    let app = setup().await;

    // Create group
    let (_, body) = send_json(&app, "POST", "/api/v3/groups", serde_json::json!({
        "name": "approval-test",
    })).await;
    let group_id = body["group"]["id"].as_str().unwrap().to_string();

    // Create approval request
    let (status, body) = send_json(&app, "POST", "/api/v3/approvals", serde_json::json!({
        "group_id": group_id,
        "title": "Deploy to production",
        "description": "Deploy v2.0 to prod",
        "requester_id": "user-1",
        "approver_spec": "user-2,user-3",
        "quorum_type": "any",
    })).await;
    assert_eq!(status, StatusCode::CREATED, "create approval failed: {:?}", body);
    let approval = &body["approval"];
    assert_eq!(approval["title"], "Deploy to production");
    let approval_id = approval["id"].as_str().unwrap().to_string();

    // Get approval
    let (status, body) = get_json(&app, &format!("/api/v3/approvals/{}", approval_id)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["approval"]["title"], "Deploy to production");

    // Vote approve
    let (status, body) = send_json(&app, "POST", &format!("/api/v3/approvals/{}/vote", approval_id), serde_json::json!({
        "voter_id": "user-2",
        "decision": "approve",
        "comment": "LGTM",
    })).await;
    assert_eq!(status, StatusCode::OK, "vote failed: {:?}", body);

    // List votes
    let (status, body) = get_json(&app, &format!("/api/v3/approvals/{}/votes", approval_id)).await;
    assert_eq!(status, StatusCode::OK);
    let votes = body["votes"].as_array().unwrap();
    assert_eq!(votes.len(), 1);
    assert_eq!(votes[0]["decision"], "approve");

    // List pending approvals
    let (status, body) = get_json(&app, &format!("/api/v3/approvals/pending?user_id=user-3&group_id={}", group_id)).await;
    assert_eq!(status, StatusCode::OK);
}

// ============================================================
// Test: Memory Store and Search
// ============================================================

#[tokio::test]
async fn test_memory_store_and_search() {
    let app = setup().await;

    // Store a memory
    let (status, body) = send_json(&app, "POST", "/api/v3/memories", serde_json::json!({
        "agent_id": "agent-1",
        "memory_type": "episodic",
        "content": "The user asked about Rust async patterns",
        "summary": "Rust async discussion",
    })).await;
    assert_eq!(status, StatusCode::CREATED, "store memory failed: {:?}", body);
    assert!(body["memory"]["id"].as_str().is_some());

    // Store another memory
    let (status, _) = send_json(&app, "POST", "/api/v3/memories", serde_json::json!({
        "agent_id": "agent-1",
        "memory_type": "semantic",
        "content": "Tokio is the async runtime for Rust",
    })).await;
    assert_eq!(status, StatusCode::CREATED);

    // Get stats
    let (status, body) = get_json(&app, "/api/v3/memories/stats?agent_id=agent-1").await;
    assert_eq!(status, StatusCode::OK);
    let stats = &body["stats"];
    assert!(stats["total_count"].as_i64().unwrap() >= 2, "expected >= 2 memories");

    // Search memories
    let (status, body) = send_json(&app, "POST", "/api/v3/memories/search", serde_json::json!({
        "agent_id": "agent-1",
        "query_text": "Rust",
    })).await;
    assert_eq!(status, StatusCode::OK);
}

// ============================================================
// Test: DM (Private Chat)
// ============================================================

#[tokio::test]
async fn test_dm_workflow() {
    let app = setup().await;

    // Create DM
    let (status, body) = send_json(&app, "POST", "/api/v3/dms", serde_json::json!({
        "target_user_id": "user-2",
    })).await;
    assert_eq!(status, StatusCode::CREATED, "create dm failed: {:?}", body);

    // List DMs
    let (status, body) = get_json(&app, "/api/v3/dms").await;
    assert_eq!(status, StatusCode::OK);
}

// ============================================================
// Test: Cron Jobs
// ============================================================

#[tokio::test]
async fn test_cron_jobs() {
    let app = setup().await;

    // Create group
    let (_, body) = send_json(&app, "POST", "/api/v3/groups", serde_json::json!({
        "name": "cron-test",
    })).await;
    let group_id = body["group"]["id"].as_str().unwrap().to_string();

    // Create cron job
    let (status, body) = send_json(&app, "POST", &format!("/api/v3/groups/{}/cron", group_id), serde_json::json!({
        "name": "daily-standup",
        "cron_expr": "0 9 * * 1-5",
        "message_template": "Time for standup!",
    })).await;
    assert_eq!(status, StatusCode::CREATED, "create cron failed: {:?}", body);
    let job = &body["job"];
    assert_eq!(job["name"], "daily-standup");
    let job_id = job["id"].as_str().unwrap().to_string();

    // List cron jobs
    let (status, body) = get_json(&app, &format!("/api/v3/groups/{}/cron", group_id)).await;
    assert_eq!(status, StatusCode::OK);
    let jobs = body["jobs"].as_array().unwrap();
    assert!(jobs.len() >= 1);

    // Update cron job
    let (status, body) = send_json(&app, "PUT", &format!("/api/v3/groups/{}/cron/{}", group_id, job_id), serde_json::json!({
        "enabled": false,
    })).await;
    assert_eq!(status, StatusCode::OK, "update cron failed: {:?}", body);

    // Delete cron job
    let (status, body) = send_json(&app, "DELETE", &format!("/api/v3/groups/{}/cron/{}", group_id, job_id), serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK, "delete cron failed: {:?}", body);
}

// ============================================================
// Test: Members
// ============================================================

#[tokio::test]
async fn test_group_members() {
    let app = setup().await;

    // Create group
    let (_, body) = send_json(&app, "POST", "/api/v3/groups", serde_json::json!({
        "name": "member-test",
    })).await;
    let group_id = body["group"]["id"].as_str().unwrap().to_string();

    // Add member
    let (status, body) = send_json(&app, "POST", &format!("/api/v3/groups/{}/members", group_id), serde_json::json!({
        "member_id": "user-2",
        "member_type": "human",
        "role": "member",
    })).await;
    assert_eq!(status, StatusCode::OK, "add member failed: {:?}", body);
    assert_eq!(body["status"], "added");

    // List members
    let (status, body) = get_json(&app, &format!("/api/v3/groups/{}/members", group_id)).await;
    assert_eq!(status, StatusCode::OK);
    let members = body["members"].as_array().unwrap();
    assert!(members.len() >= 2, "expected >= 2 members (owner + added)");
}

// ============================================================
// Test: Group Rules CRUD
// ============================================================

#[tokio::test]
async fn test_group_rules_crud() {
    let app = setup().await;

    // Create group
    let (_, body) = send_json(&app, "POST", "/api/v3/groups", serde_json::json!({
        "name": "rules-test",
    })).await;
    let group_id = body["group"]["id"].as_str().unwrap().to_string();

    // Create rule
    let (status, body) = send_json(&app, "POST", &format!("/api/v3/groups/{}/rules", group_id), serde_json::json!({
        "rule_type": "auto_assign",
        "config": {
            "name": "assign-coder",
            "keyword": "code",
            "agent_id": "coder-agent"
        },
        "priority": 10,
    })).await;
    assert_eq!(status, StatusCode::CREATED, "create rule failed: {:?}", body);
    let rule = &body["rule"];
    assert_eq!(rule["rule_type"], "auto_assign");
    assert_eq!(rule["priority"], 10);
    let rule_id = rule["id"].as_str().unwrap().to_string();

    // Create another rule
    let (status, _) = send_json(&app, "POST", &format!("/api/v3/groups/{}/rules", group_id), serde_json::json!({
        "rule_type": "rate_limit",
        "config": {
            "name": "rate-limit-bot",
            "agent_id": "bot",
            "max_messages_per_minute": 5
        },
        "priority": 5,
    })).await;
    assert_eq!(status, StatusCode::CREATED);

    // List rules
    let (status, body) = get_json(&app, &format!("/api/v3/groups/{}/rules", group_id)).await;
    assert_eq!(status, StatusCode::OK);
    let rules = body["rules"].as_array().unwrap();
    assert!(rules.len() >= 2, "expected >= 2 rules, got {}", rules.len());

    // Update rule
    let (status, body) = send_json(&app, "PUT", &format!("/api/v3/groups/{}/rules/{}", group_id, rule_id), serde_json::json!({
        "enabled": false,
        "priority": 20,
    })).await;
    assert_eq!(status, StatusCode::OK, "update rule failed: {:?}", body);

    // Delete rule
    let (status, body) = send_json(&app, "DELETE", &format!("/api/v3/groups/{}/rules/{}", group_id, rule_id), serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK, "delete rule failed: {:?}", body);

    // Verify deletion
    let (status, body) = get_json(&app, &format!("/api/v3/groups/{}/rules", group_id)).await;
    assert_eq!(status, StatusCode::OK);
    let rules = body["rules"].as_array().unwrap();
    assert!(rules.iter().all(|r| r["id"] != rule_id), "deleted rule still in list");
}

// ============================================================
// Test: Message Attachments Lifecycle
// ============================================================

#[tokio::test]
async fn test_message_attachments_lifecycle() {
    let app = setup().await;

    // Create group
    let (_, body) = send_json(&app, "POST", "/api/v3/groups", serde_json::json!({
        "name": "attach-test",
    })).await;
    let group_id = body["group"]["id"].as_str().unwrap().to_string();

    // Upload attachment via multipart
    let boundary = "TestBoundary123";
    let file_content = b"Hello, this is a test file content.";
    let mut body_bytes = Vec::new();
    body_bytes.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body_bytes.extend_from_slice(b"Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n");
    body_bytes.extend_from_slice(b"Content-Type: text/plain\r\n\r\n");
    body_bytes.extend_from_slice(file_content);
    body_bytes.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

    let req = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/api/v3/groups/{}/attachments", group_id))
        .header("content-type", format!("multipart/form-data; boundary={}", boundary))
        .body(axum::body::Body::from(body_bytes))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let val: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let attachments = val["attachments"].as_array().unwrap();
    assert_eq!(attachments.len(), 1);
    let att_id = attachments[0]["id"].as_str().unwrap().to_string();
    assert_eq!(attachments[0]["filename"], "test.txt");
    assert_eq!(attachments[0]["size"], file_content.len() as i64);

    // List attachments
    let (status, body) = get_json(&app, &format!("/api/v3/groups/{}/attachments", group_id)).await;
    assert_eq!(status, StatusCode::OK);
    let atts = body["attachments"].as_array().unwrap();
    assert_eq!(atts.len(), 1);
    assert_eq!(atts[0]["id"], att_id);

    // Download attachment
    let (status, _) = get_json(&app, &format!("/api/v3/attachments/{}", att_id)).await;
    assert_eq!(status, StatusCode::OK);

    // Create a message to link to
    let (_, msg_body) = send_json(&app, "POST", &format!("/api/v3/groups/{}/messages", group_id), serde_json::json!({
        "sender_id": "user-1",
        "content": "Message with attachment",
    })).await;
    let msg_id = msg_body["message"]["id"].as_str().unwrap();

    // Link attachment to message
    let (status, body) = send_json(&app, "PUT", &format!("/api/v3/attachments/{}", att_id), serde_json::json!({
        "message_id": msg_id,
    })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["linked"], true);

    // Verify link
    let (_, body) = get_json(&app, &format!("/api/v3/groups/{}/attachments", group_id)).await;
    assert_eq!(body["attachments"][0]["message_id"], msg_id);

    // Delete attachment
    let req = axum::http::Request::builder()
        .method("DELETE")
        .uri(format!("/api/v3/attachments/{}", att_id))
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let val: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(val["deleted"], true);

    // Verify deletion
    let (status, _) = get_json(&app, &format!("/api/v3/attachments/{}", att_id)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ============================================================
// Test: Agent Hooks CRUD
// ============================================================

#[tokio::test]
async fn test_agent_hooks_crud() {
    let app = setup().await;

    // Create a group
    let (_, body) = send_json(&app, "POST", "/api/v3/groups", serde_json::json!({
        "name": "Hooks Test Group",
    })).await;
    let group_id = body["group"]["id"].as_str().unwrap();

    // Create a hook
    let (status, body) = send_json(&app, "POST", &format!("/api/v3/groups/{}/hooks", group_id), serde_json::json!({
        "agent_id": "agent-1",
        "event_types": ["message.created", "task.created"],
        "action_type": "notify",
        "action_config": { "channel": "webhook", "url": "https://example.com/hook" },
        "priority": 10,
    })).await;
    assert_eq!(status, StatusCode::CREATED);
    let hook1_id = body["hook"]["id"].as_str().unwrap().to_string();
    assert_eq!(body["hook"]["agent_id"], "agent-1");
    assert_eq!(body["hook"]["enabled"], true);
    assert_eq!(body["hook"]["priority"], 10);

    // Create a second hook
    let (status, body) = send_json(&app, "POST", &format!("/api/v3/groups/{}/hooks", group_id), serde_json::json!({
        "agent_id": "agent-2",
        "event_types": ["approval.requested"],
        "action_type": "auto_approve",
        "action_config": { "threshold": 0.9 },
        "priority": 5,
    })).await;
    assert_eq!(status, StatusCode::CREATED);
    let hook2_id = body["hook"]["id"].as_str().unwrap().to_string();

    // List hooks
    let (status, body) = get_json(&app, &format!("/api/v3/groups/{}/hooks", group_id)).await;
    assert_eq!(status, StatusCode::OK);
    let hooks = body["hooks"].as_array().unwrap();
    assert_eq!(hooks.len(), 2);

    // Get single hook
    let (status, body) = get_json(&app, &format!("/api/v3/groups/{}/hooks/{}", group_id, hook1_id)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["hook"]["id"], hook1_id);

    // Update hook — toggle enabled and change priority
    let (status, body) = send_json(&app, "PUT", &format!("/api/v3/groups/{}/hooks/{}", group_id, hook1_id), serde_json::json!({
        "enabled": false,
        "priority": 99,
    })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["updated"], true);

    // Verify update
    let (_, body) = get_json(&app, &format!("/api/v3/groups/{}/hooks/{}", group_id, hook1_id)).await;
    assert_eq!(body["hook"]["enabled"], false);
    assert_eq!(body["hook"]["priority"], 99);

    // List hook logs (should be empty initially)
    let (status, body) = get_json(&app, &format!("/api/v3/groups/{}/hooks/{}/logs", group_id, hook1_id)).await;
    assert_eq!(status, StatusCode::OK);
    let logs = body["logs"].as_array().unwrap();
    assert_eq!(logs.len(), 0);

    // Delete second hook
    let req = axum::http::Request::builder()
        .method("DELETE")
        .uri(format!("/api/v3/groups/{}/hooks/{}", group_id, hook2_id))
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let val: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(val["deleted"], true);

    // Verify deletion — only 1 hook remains
    let (_, body) = get_json(&app, &format!("/api/v3/groups/{}/hooks", group_id)).await;
    assert_eq!(body["hooks"].as_array().unwrap().len(), 1);
    assert_eq!(body["hooks"][0]["id"], hook1_id);
}

// ============================================================
// Test: Workflow Definitions & Runs
// ============================================================

#[tokio::test]
async fn test_workflow_definitions_and_runs() {
    let app = setup().await;

    // Create a workflow definition
    let (status, body) = send_json(&app, "POST", "/api/v3/workflows", serde_json::json!({
        "id": "wf-test",
        "name": "Test Workflow",
        "yaml_content": "nodes:\n  - id: step1\n    name: LLM Call\n  - id: step2\n    name: Tool Call",
    })).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["workflow"]["id"], "wf-test");
    assert_eq!(body["workflow"]["name"], "Test Workflow");
    assert_eq!(body["workflow"]["status"], "draft");

    // Create a second workflow
    let (status, _) = send_json(&app, "POST", "/api/v3/workflows", serde_json::json!({
        "id": "wf-other",
        "name": "Other Workflow",
        "yaml_content": "nodes: []",
    })).await;
    assert_eq!(status, StatusCode::CREATED);

    // List definitions
    let (status, body) = get_json(&app, "/api/v3/workflows").await;
    assert_eq!(status, StatusCode::OK);
    let defs = body["workflows"].as_array().unwrap();
    assert_eq!(defs.len(), 2);

    // Get single definition
    let (status, body) = get_json(&app, "/api/v3/workflows/wf-test").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["workflow"]["id"], "wf-test");

    // Update definition
    let (status, body) = send_json(&app, "PUT", "/api/v3/workflows/wf-test", serde_json::json!({
        "status": "active",
        "name": "Updated Workflow",
    })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["updated"], true);

    // Verify update
    let (_, body) = get_json(&app, "/api/v3/workflows/wf-test").await;
    assert_eq!(body["workflow"]["status"], "active");
    assert_eq!(body["workflow"]["name"], "Updated Workflow");

    // Create a workflow run
    let (status, body) = send_json(&app, "POST", "/api/v3/workflow-runs", serde_json::json!({
        "workflow_id": "wf-test",
        "workflow_version": 1,
        "input": "{\"prompt\": \"hello\"}",
    })).await;
    assert_eq!(status, StatusCode::CREATED);
    let run_id = body["run"]["id"].as_str().unwrap().to_string();
    assert_eq!(body["run"]["status"], "running");

    // Create a second run
    let (_, body2) = send_json(&app, "POST", "/api/v3/workflow-runs", serde_json::json!({
        "workflow_id": "wf-test",
        "workflow_version": 1,
        "input": "{}",
    })).await;
    let run2_id = body2["run"]["id"].as_str().unwrap().to_string();

    // List runs
    let (status, body) = get_json(&app, "/api/v3/workflow-runs").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["runs"].as_array().unwrap().len(), 2);

    // List runs filtered by workflow_id
    let (status, body) = get_json(&app, "/api/v3/workflow-runs?workflow_id=wf-test").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["runs"].as_array().unwrap().len(), 2);

    // Get single run
    let (status, body) = get_json(&app, &format!("/api/v3/workflow-runs/{}", run_id)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["run"]["id"], run_id);

    // Record checkpoints
    let (status, body) = send_json(&app, "POST", &format!("/api/v3/workflow-runs/{}/checkpoints", run_id), serde_json::json!({
        "node_id": "step1",
        "output": "LLM response here",
        "context_snapshot": "{\"step\": 1}",
    })).await;
    assert_eq!(status, StatusCode::CREATED);
    let cp1_id = body["id"].as_i64().unwrap();

    let (status, body) = send_json(&app, "POST", &format!("/api/v3/workflow-runs/{}/checkpoints", run_id), serde_json::json!({
        "node_id": "step2",
        "output": "Tool result here",
        "context_snapshot": "{\"step\": 2}",
    })).await;
    assert_eq!(status, StatusCode::CREATED);
    let cp2_id = body["id"].as_i64().unwrap();
    assert!(cp2_id > cp1_id);

    // List checkpoints
    let (status, body) = get_json(&app, &format!("/api/v3/workflow-runs/{}/checkpoints", run_id)).await;
    assert_eq!(status, StatusCode::OK);
    let cps = body["checkpoints"].as_array().unwrap();
    assert_eq!(cps.len(), 2);
    assert_eq!(cps[0]["node_id"], "step1");
    assert_eq!(cps[1]["node_id"], "step2");

    // Update run status to completed
    let (status, body) = send_json(&app, "PUT", &format!("/api/v3/workflow-runs/{}/status", run_id), serde_json::json!({
        "status": "completed",
        "output": "final result",
    })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["updated"], true);

    // Verify completion
    let (_, body) = get_json(&app, &format!("/api/v3/workflow-runs/{}", run_id)).await;
    assert_eq!(body["run"]["status"], "completed");
    assert!(body["run"]["completed_at"].as_i64().is_some());

    // Cancel the second run
    let (status, _) = send_json(&app, "PUT", &format!("/api/v3/workflow-runs/{}/status", run2_id), serde_json::json!({
        "status": "cancelled",
    })).await;
    assert_eq!(status, StatusCode::OK);

    // Delete workflow definition
    let req = axum::http::Request::builder()
        .method("DELETE")
        .uri("/api/v3/workflows/wf-other")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify deletion
    let (status, _) = get_json(&app, "/api/v3/workflows/wf-other").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ============================================================
// Execution fact chain (Track 1 / T1-2)
// ============================================================

#[tokio::test]
async fn test_execution_routes_get_unknown_returns_404() {
    let app = setup().await;
    let (status, body) = get_json(&app, "/api/v3/executions/exec_does_not_exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "execution exec_does_not_exist not found");
}

#[tokio::test]
async fn test_execution_routes_list_events_unknown_returns_404() {
    let app = setup().await;
    let (status, _) = get_json(&app, "/api/v3/executions/exec_unknown/events").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_execution_routes_happy_path() {
    // Drive the recorder directly to seed an execution + events, then
    // verify the HTTP routes surface them correctly.
    let (state, app) = setup_with_state().await;

    let exec_id = state
        .execution_recorder
        .start(
            "chat",
            Some("u1"),
            Some("human"),
            "manual",
            serde_json::json!({"message": "hello"}),
            None,
        )
        .await
        .unwrap();

    state
        .execution_recorder
        .append(
            &exec_id,
            "agent",
            "tool_call",
            serde_json::json!({
                "tool_name": "kb_search",
                "input": {"query": "maple"},
                "permission_level": "read_only",
                "invocation_id": "inv_1"
            }),
            Some("agent_default"),
            Some("agent"),
        )
        .await
        .unwrap();

    state
        .execution_recorder
        .append(
            &exec_id,
            "tool",
            "tool_result",
            serde_json::json!({
                "invocation_id": "inv_1",
                "output": {"hits": 3},
                "error": null,
                "duration_ms": 42
            }),
            None,
            None,
        )
        .await
        .unwrap();

    state
        .execution_recorder
        .done(&exec_id, "answered: maple is an AI OS")
        .await
        .unwrap();

    // GET /api/v3/executions/:id
    let (status, body) = get_json(&app, &format!("/api/v3/executions/{exec_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], exec_id);
    assert_eq!(body["source"], "chat");
    assert_eq!(body["status"], "success");
    assert_eq!(body["actor"], "u1");
    assert_eq!(body["actor_type"], "human");
    assert_eq!(body["event_count"], 4); // started + tool_call + tool_result + done
    assert!(body["completed_at"].as_i64().is_some());

    // GET /api/v3/executions/:id/events
    let (status, body) = get_json(&app, &format!("/api/v3/executions/{exec_id}/events")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["execution_id"], exec_id);
    let events = body["events"].as_array().unwrap();
    assert_eq!(events.len(), 4);
    assert_eq!(events[0]["event_type"], "started");
    assert_eq!(events[0]["source"], "chat");
    assert_eq!(events[1]["event_type"], "tool_call");
    assert_eq!(events[1]["source"], "agent");
    assert_eq!(events[1]["payload"]["tool_name"], "kb_search");
    assert_eq!(events[2]["event_type"], "tool_result");
    assert_eq!(events[2]["source"], "tool");
    assert_eq!(events[2]["payload"]["invocation_id"], "inv_1");
    assert_eq!(events[3]["event_type"], "done");
    assert_eq!(events[3]["source"], "system");
}

#[tokio::test]
async fn test_execution_routes_failed_status_carries_error() {
    let (state, app) = setup_with_state().await;

    let exec_id = state
        .execution_recorder
        .start("task", None, None, "cron", serde_json::json!({}), None)
        .await
        .unwrap();

    state
        .execution_recorder
        .fail(&exec_id, "tool timeout", true)
        .await
        .unwrap();

    let (status, body) = get_json(&app, &format!("/api/v3/executions/{exec_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "failed");
    assert_eq!(body["error"], "tool timeout");

    let (_, events_body) = get_json(&app, &format!("/api/v3/executions/{exec_id}/events")).await;
    let events = events_body["events"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1]["event_type"], "error");
    assert_eq!(events[1]["payload"]["recoverable"], true);
    assert_eq!(events[1]["payload"]["message"], "tool timeout");
}
