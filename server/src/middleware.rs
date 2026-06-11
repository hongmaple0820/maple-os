use crate::state::AppState;
use axum::extract::State;
use axum::middleware::Next;
use maple_gateway::auth::Permission;
use std::sync::Arc;

pub async fn audit_log_middleware(
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> axum::response::Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(|q| q.to_string()).unwrap_or_default();
    let user_agent = req
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let client_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let start = std::time::Instant::now();
    let response = next.run(req).await;
    let duration = start.elapsed();
    let status = response.status().as_u16();

    tracing::info!(
        method = %method,
        path = %path,
        query = %query,
        status = status,
        duration_ms = duration.as_millis() as u64,
        user_agent = %user_agent,
        client_ip = %client_ip,
        "API request"
    );

    response
}

pub async fn rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    let path = req.uri().path();

    if path == "/health" || path == "/health/deep" || path.starts_with("/ws/") || path.starts_with("/webhook/") {
        return Ok(next.run(req).await);
    }

    let client_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .split(',')
        .next()
        .unwrap_or("unknown")
        .trim()
        .to_string();

    if state.rate_limiter.check(&client_ip).await {
        Ok(next.run(req).await)
    } else {
        Err(axum::http::StatusCode::TOO_MANY_REQUESTS)
    }
}

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    let path = req.uri().path();
    let method = req.method().clone();

    if path == "/health"
        || path == "/health/deep"
        || path.starts_with("/ws/")
        || path.starts_with("/api/events")
        || path.starts_with("/api/auth/")
        || path.starts_with("/webhook/")
    {
        return Ok(next.run(req).await);
    }

    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let token = auth_header.strip_prefix("Bearer ").unwrap_or_default();

    if token.is_empty() {
        if state.config.read().await.require_auth {
            return Err(axum::http::StatusCode::UNAUTHORIZED);
        }
        return Ok(next.run(req).await);
    }

    match state.auth_service.verify_token(token) {
        Ok(claims) => {
            let required_permission = get_required_permission(path, &method);
            if let Some(permission) = required_permission
                && !claims.has_permission(&permission)
            {
                tracing::warn!(
                    user_id = ?claims.user_id,
                    role = %claims.role,
                    path = %path,
                    method = %method,
                    "Permission denied"
                );
                return Err(axum::http::StatusCode::FORBIDDEN);
            }

            // Inject AuthenticatedUser into request extensions for v3 handlers
            if path.starts_with("/api/v3/") || path.starts_with("/ws/") {
                let (user_id, user_type) = if let Some(aid) = claims.agent_id.clone() {
                    (aid, "agent".to_string())
                } else if let Some(uid) = claims.user_id.clone() {
                    (uid, "human".to_string())
                } else {
                    (claims.sub.clone(), "human".to_string())
                };
                let auth_user = crate::v3_auth::AuthenticatedUser {
                    user_id,
                    user_type,
                    role: claims.role.clone(),
                };
                let mut req = req;
                req.extensions_mut().insert(auth_user);
                return Ok(next.run(req).await);
            }

            Ok(next.run(req).await)
        }
        Err(_) => Err(axum::http::StatusCode::UNAUTHORIZED),
    }
}

pub(crate) fn get_required_permission(
    path: &str,
    method: &axum::http::Method,
) -> Option<Permission> {
    match (method.as_str(), path) {
        ("GET", p) if p.starts_with("/api/workflows") => Some(Permission::ReadWorkflows),
        ("POST", p) if p.starts_with("/api/workflows") => Some(Permission::WriteWorkflows),
        ("PUT", p) if p.starts_with("/api/workflows") => Some(Permission::WriteWorkflows),
        ("DELETE", p) if p.starts_with("/api/workflows") => Some(Permission::DeleteWorkflows),

        ("GET", p) if p.starts_with("/api/agents") => Some(Permission::ReadAgents),
        ("POST", p) if p.starts_with("/api/agents") => Some(Permission::WriteAgents),
        ("DELETE", p) if p.starts_with("/api/agents") => Some(Permission::ManageAgents),

        ("GET", p) if p.starts_with("/api/sessions") => Some(Permission::ReadSessions),
        ("DELETE", p) if p.starts_with("/api/sessions") => Some(Permission::DeleteSessions),

        ("GET", p) if p.starts_with("/api/memories") => Some(Permission::ReadMemories),
        ("POST", p) if p.starts_with("/api/memories") => Some(Permission::WriteMemories),
        ("DELETE", p) if p.starts_with("/api/memories") => Some(Permission::DeleteMemories),

        ("GET", p) if p.starts_with("/api/prompts") => Some(Permission::ReadPrompts),
        ("POST", p) if p.starts_with("/api/prompts") => Some(Permission::WritePrompts),

        ("GET", "/api/config") => Some(Permission::ManageConfig),
        ("PUT", "/api/config") => Some(Permission::ManageConfig),

        ("GET", "/health/deep") => Some(Permission::ViewMetrics),
        ("GET", "/api/agents/status") => Some(Permission::ViewMetrics),
        ("GET", "/api/tasks/stats") => Some(Permission::ViewMetrics),

        _ => None,
    }
}
