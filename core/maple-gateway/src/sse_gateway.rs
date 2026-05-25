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
