//! HTTP handlers for the learning governance API (Track 3 / T3-6..T3-11).
//!
//! Routes:
//!   GET    /api/v3/learning/candidates           -> list_candidates_handler
//!   GET    /api/v3/learning/candidates/pending   -> list_pending_handler
//!   GET    /api/v3/learning/candidates/:id       -> get_candidate_handler
//!   POST   /api/v3/learning/candidates/:id/approve -> approve_handler
//!   POST   /api/v3/learning/candidates/:id/reject  -> reject_handler
//!   POST   /api/v3/learning/candidates/:id/revoke  -> revoke_handler
//!   GET    /api/v3/learning/blocked               -> is_blocked_handler
//!
//! See docs/MapleOS_Implementation_Plan_2026Q3.md Track 3 and Issue #91.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::state::{ApiError, AppState};

#[derive(Debug, Serialize)]
pub struct CandidateResponse {
    pub id: String,
    pub target_type: String,
    pub target_key: Option<String>,
    pub content: String,
    pub score: f64,
    pub evidence: Option<String>,
    pub source_execution_id: Option<String>,
    pub source_metadata: Option<serde_json::Value>,
    pub persisted_target_id: Option<String>,
    pub status: String,
    pub decided_by: Option<String>,
    pub decided_at: Option<i64>,
    pub rejection_reason: Option<String>,
    pub approval_threshold: f64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<maple_kb::LearningCandidate> for CandidateResponse {
    fn from(c: maple_kb::LearningCandidate) -> Self {
        Self {
            id: c.id,
            target_type: c.target_type,
            target_key: c.target_key,
            content: c.content,
            score: c.score,
            evidence: c.evidence,
            source_execution_id: c.source_execution_id,
            source_metadata: c.source_metadata,
            persisted_target_id: c.persisted_target_id,
            status: c.status,
            decided_by: c.decided_by,
            decided_at: c.decided_at,
            rejection_reason: c.rejection_reason,
            approval_threshold: c.approval_threshold,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DecisionRequest {
    pub decided_by: String,
    pub reason: Option<String>,
}

/// GET /api/v3/learning/candidates — list recent candidates.
/// Optional ?status=pending|approved|rejected|auto_approved|persisted|revoked
/// Optional ?limit=N (default 50)
pub async fn list_candidates_handler(
    State(state): State<std::sync::Arc<AppState>>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);

    let candidates = if let Some(status) = q.status.as_deref() {
        // Filter by status — currently only 'pending' is optimised with
        // a dedicated list_pending method; for other statuses we fetch
        // all recent and filter in-memory (small N for typical usage).
        let all = state
            .learning_governance
            .list_recent(limit)
            .await
            .map_err(|e| ApiError::new(format!("list failed: {e}"), "INTERNAL_ERROR"))?;
        all.into_iter()
            .filter(|c| c.status == status)
            .collect::<Vec<_>>()
    } else {
        state
            .learning_governance
            .list_recent(limit)
            .await
            .map_err(|e| ApiError::new(format!("list failed: {e}"), "INTERNAL_ERROR"))?
    };

    let resp: Vec<CandidateResponse> = candidates.into_iter().map(Into::into).collect();
    Ok(Json(serde_json::json!({ "candidates": resp })))
}

/// GET /api/v3/learning/candidates/pending — list only pending candidates.
pub async fn list_pending_handler(
    State(state): State<std::sync::Arc<AppState>>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let candidates = state
        .learning_governance
        .list_pending(limit)
        .await
        .map_err(|e| ApiError::new(format!("list failed: {e}"), "INTERNAL_ERROR"))?;
    let resp: Vec<CandidateResponse> = candidates.into_iter().map(Into::into).collect();
    Ok(Json(serde_json::json!({ "candidates": resp })))
}

/// GET /api/v3/learning/candidates/:id — fetch a single candidate.
pub async fn get_candidate_handler(
    State(state): State<std::sync::Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<CandidateResponse>, ApiError> {
    let c = state
        .learning_governance
        .get(&id)
        .await
        .map_err(|e| ApiError::new(format!("fetch failed: {e}"), "INTERNAL_ERROR"))?
        .ok_or_else(|| ApiError::new(format!("candidate {id} not found"), "NOT_FOUND"))?;
    Ok(Json(CandidateResponse::from(c)))
}

/// POST /api/v3/learning/candidates/:id/approve — approve a pending candidate.
///
/// Persistence is deferred — the candidate is marked 'persisted' with a
/// synthetic persisted_target_id (format: `<target_type>_<candidate_id>`).
/// T3-9.1 will wire the actual Memory / KB / Prompt write path once those
/// stores expose stable add/delete APIs. For now, the approval flow is
/// complete from the governance perspective: the candidate moves from
/// 'pending' → 'persisted' and is no longer in the pending queue.
pub async fn approve_handler(
    State(state): State<std::sync::Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<DecisionRequest>,
) -> Result<Json<CandidateResponse>, ApiError> {
    let approved = state
        .learning_governance
        .approve(&id, &req.decided_by, |c| {
            // Synthetic persisted id — T3-9.1 will replace with real write
            let persisted_id = format!("{}_{}", c.target_type, &c.id[..8.min(c.id.len())]);
            Box::pin(async move { Ok(persisted_id) })
        })
        .await
        .map_err(|e| ApiError::new(format!("approve failed: {e}"), "INTERNAL_ERROR"))?;

    Ok(Json(CandidateResponse::from(approved)))
}

/// POST /api/v3/learning/candidates/:id/reject — reject a pending candidate.
/// Adds content hash to blocklist so it's never re-proposed.
pub async fn reject_handler(
    State(state): State<std::sync::Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<DecisionRequest>,
) -> Result<Json<CandidateResponse>, ApiError> {
    let rejected = state
        .learning_governance
        .reject(&id, &req.decided_by, req.reason.as_deref().unwrap_or("rejected"))
        .await
        .map_err(|e| ApiError::new(format!("reject failed: {e}"), "INTERNAL_ERROR"))?;
    Ok(Json(CandidateResponse::from(rejected)))
}

/// POST /api/v3/learning/candidates/:id/revoke — revoke a previously-approved candidate.
/// Adds content to blocklist so it's never re-proposed.
///
/// Actual deletion from Memory/KB/Prompt stores is deferred (T3-9.1).
/// For now the candidate status moves to 'revoked' and the content is
/// blocked from future re-proposal.
pub async fn revoke_handler(
    State(state): State<std::sync::Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<DecisionRequest>,
) -> Result<Json<CandidateResponse>, ApiError> {
    let revoked = state
        .learning_governance
        .revoke(&id, &req.decided_by, req.reason.as_deref().unwrap_or("revoked"), |_c| {
            // T3-9.1 will wire actual deletion from memory/kb/prompt stores
            Box::pin(async move { Ok(()) })
        })
        .await
        .map_err(|e| ApiError::new(format!("revoke failed: {e}"), "INTERNAL_ERROR"))?;

    Ok(Json(CandidateResponse::from(revoked)))
}

/// GET /api/v3/learning/blocked?content=... — check if content is blocked.
pub async fn is_blocked_handler(
    State(state): State<std::sync::Arc<AppState>>,
    Query(q): Query<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let content = q["content"].as_str().unwrap_or("").to_string();
    let blocked = state
        .learning_governance
        .is_blocked(&content)
        .await
        .map_err(|e| ApiError::new(format!("check failed: {e}"), "INTERNAL_ERROR"))?;
    Ok(Json(serde_json::json!({ "blocked": blocked })))
}
