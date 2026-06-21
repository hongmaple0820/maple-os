//! HTTP handlers for the unified execution fact chain.
//!
//! See `docs/execution-fact-chain-spec.md` §6 for the API contract.
//!
//! Routes:
//!   GET  /api/executions/:id            -> get_execution_handler
//!   GET  /api/executions/:id/events     -> list_events_handler
//!   GET  /api/executions/:id/events/stream -> sse_events_handler (SSE)
//!
//! Lives in the lib crate so that:
//! - the bin crate can mount it via `mapleos_server::execution_handlers::*`
//! - the lib crate's integration tests can exercise it via `build_v3_test_router`
//! Both share the same `AppState` (from `crate::state`).

use axum::extract::{Path, State};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::Json;
use futures::stream::Stream;
use serde::Serialize;
use std::convert::Infallible;
use std::time::Duration;

use crate::state::{ApiError, AppState};

#[derive(Debug, Serialize)]
pub struct ExecutionResponse {
    pub id: String,
    pub parent_execution_id: Option<String>,
    pub source: String,
    pub status: String,
    pub actor: Option<String>,
    pub actor_type: Option<String>,
    pub trigger_type: Option<String>,
    pub trigger_payload: Option<serde_json::Value>,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub error: Option<String>,
    pub event_count: i64,
    pub updated_at: i64,
}

impl From<maple_engine::Execution> for ExecutionResponse {
    fn from(e: maple_engine::Execution) -> Self {
        Self {
            id: e.id,
            parent_execution_id: e.parent_execution_id,
            source: e.source,
            status: e.status,
            actor: e.actor,
            actor_type: e.actor_type,
            trigger_type: e.trigger_type,
            trigger_payload: e.trigger_payload,
            started_at: e.started_at,
            completed_at: e.completed_at,
            error: e.error,
            event_count: e.event_count,
            updated_at: e.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ExecutionEventResponse {
    pub id: String,
    pub execution_id: String,
    pub parent_execution_id: Option<String>,
    pub source: String,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub actor: Option<String>,
    pub actor_type: Option<String>,
    pub created_at: i64,
}

impl From<maple_engine::ExecutionEvent> for ExecutionEventResponse {
    fn from(e: maple_engine::ExecutionEvent) -> Self {
        Self {
            id: e.id,
            execution_id: e.execution_id,
            parent_execution_id: e.parent_execution_id,
            source: e.source,
            event_type: e.event_type,
            payload: e.payload,
            actor: e.actor,
            actor_type: e.actor_type,
            created_at: e.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EventsListResponse {
    pub execution_id: String,
    pub events: Vec<ExecutionEventResponse>,
}

/// GET /api/executions/:id — fetch the aggregate view of an execution.
pub async fn get_execution_handler(
    State(state): State<std::sync::Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ExecutionResponse>, ApiError> {
    let exec = state
        .execution_recorder
        .get_execution(&id)
        .await
        .map_err(|e| ApiError::new(format!("failed to fetch execution: {e}"), "INTERNAL_ERROR"))?
        .ok_or_else(|| ApiError::new(format!("execution {id} not found"), "NOT_FOUND"))?;

    Ok(Json(ExecutionResponse::from(exec)))
}

/// GET /api/executions/:id/events — list all events for an execution.
pub async fn list_events_handler(
    State(state): State<std::sync::Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<EventsListResponse>, ApiError> {
    let exec = state
        .execution_recorder
        .get_execution(&id)
        .await
        .map_err(|e| ApiError::new(format!("failed to fetch execution: {e}"), "INTERNAL_ERROR"))?
        .ok_or_else(|| ApiError::new(format!("execution {id} not found"), "NOT_FOUND"))?;

    let events = state
        .execution_recorder
        .list_events(&id)
        .await
        .map_err(|e| ApiError::new(format!("failed to fetch events: {e}"), "INTERNAL_ERROR"))?;

    Ok(Json(EventsListResponse {
        execution_id: exec.id,
        events: events.into_iter().map(Into::into).collect(),
    }))
}

/// GET /api/executions/:id/events/stream — Server-Sent Events stream.
///
/// Polls the recorder every 1s for new events until the execution reaches a
/// terminal state (success / failed / cancelled) or the client disconnects.
/// Historical events are replayed first.
pub async fn sse_events_handler(
    State(state): State<std::sync::Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, ApiError> {
    let _exec = state
        .execution_recorder
        .get_execution(&id)
        .await
        .map_err(|e| ApiError::new(format!("failed to fetch execution: {e}"), "INTERNAL_ERROR"))?
        .ok_or_else(|| ApiError::new(format!("execution {id} not found"), "NOT_FOUND"))?;

    let recorder = state.execution_recorder.clone();
    let exec_id = id.clone();

    let stream = async_stream::stream! {
        let mut last_seen_count: i64 = 0;

        loop {
            let exec = match recorder.get_execution(&exec_id).await {
                Ok(Some(e)) => e,
                Ok(None) => {
                    let _ = yield Ok(SseEvent::default()
                        .event("error")
                        .data(serde_json::json!({
                            "error": "execution_not_found",
                            "message": format!("execution {} no longer exists", exec_id),
                        }).to_string()));
                    return;
                }
                Err(e) => {
                    let _ = yield Ok(SseEvent::default()
                        .event("error")
                        .data(serde_json::json!({
                            "error": "internal_error",
                            "message": e.to_string(),
                        }).to_string()));
                    return;
                }
            };

            let events = match recorder.list_events(&exec_id).await {
                Ok(ev) => ev,
                Err(e) => {
                    let _ = yield Ok(SseEvent::default()
                        .event("error")
                        .data(serde_json::json!({
                            "error": "internal_error",
                            "message": e.to_string(),
                        }).to_string()));
                    return;
                }
            };

            for evt in events.iter().skip(last_seen_count as usize) {
                let payload = serde_json::json!({
                    "id": evt.id,
                    "execution_id": evt.execution_id,
                    "parent_execution_id": evt.parent_execution_id,
                    "source": evt.source,
                    "event_type": evt.event_type,
                    "payload": evt.payload,
                    "actor": evt.actor,
                    "actor_type": evt.actor_type,
                    "created_at": evt.created_at,
                });
                let sse_evt = SseEvent::default()
                    .event(&evt.event_type)
                    .data(payload.to_string());
                let _ = yield Ok(sse_evt);
            }
            last_seen_count = events.len() as i64;

            match exec.status.as_str() {
                "success" | "failed" | "cancelled" => {
                    let _ = yield Ok(SseEvent::default()
                        .event("stream_end")
                        .data(serde_json::json!({
                            "execution_id": exec.id,
                            "final_status": exec.status,
                            "event_count": exec.event_count,
                        }).to_string()));
                    return;
                }
                _ => {}
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
