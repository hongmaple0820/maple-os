use axum::response::sse::{Event, Sse};
use std::convert::Infallible;
use std::sync::Arc;

use maple_engine::event_bus::EventBus;

pub async fn handle_user_sse(
    event_bus: Arc<EventBus>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let mut rx = event_bus.subscribe_all().await;

    let stream = async_stream::stream! {
        while let Some(event) = rx.recv().await {
            let data = serde_json::to_string(&event).unwrap_or_default();
            yield Ok(Event::default().event(event.event_type()).data(data));
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}

/// SSE endpoint for v3 group chat real-time events.
/// Accepts optional `group_id` query param to filter events for a specific group.
pub async fn handle_group_sse(
    event_bus: Arc<EventBus>,
    group_ids: Vec<String>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let mut rx = event_bus.subscribe_all().await;
    let filter = !group_ids.is_empty();

    let stream = async_stream::stream! {
        while let Some(event) = rx.recv().await {
            if filter {
                let event_group_id = match &event {
                    maple_engine::event_bus::Event::GroupMessageSent { group_id, .. }
                    | maple_engine::event_bus::Event::GroupMessageEdited { group_id, .. }
                    | maple_engine::event_bus::Event::GroupMessageDeleted { group_id, .. }
                    | maple_engine::event_bus::Event::GroupMemberJoined { group_id, .. }
                    | maple_engine::event_bus::Event::GroupMemberLeft { group_id, .. } => Some(group_id.as_str()),
                    _ => None,
                };
                if let Some(gid) = event_group_id {
                    if !group_ids.iter().any(|id| id == gid) {
                        continue;
                    }
                }
            }
            let data = serde_json::to_string(&event).unwrap_or_default();
            yield Ok(Event::default().event(event.event_type()).data(data));
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}
