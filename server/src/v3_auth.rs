use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use maple_gateway::auth::AuthService;

/// Authenticated user extracted from JWT token.
/// Injected into request extensions by auth_middleware, consumed by v3 handlers.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub user_type: String, // "human" | "agent"
    pub role: String,
}

/// Axum extractor that pulls AuthenticatedUser from request extensions.
/// Must be used AFTER the auth_middleware has run (i.e., on v3 routes).
#[async_trait::async_trait]
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, axum::Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthenticatedUser>()
            .cloned()
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({
                        "error": "unauthorized",
                        "message": "Authentication required"
                    })),
                )
            })
    }
}

/// Try to extract a JWT token from:
/// 1. Authorization: Bearer <token>
/// 2. ?token=<query_param> (for WebSocket/SSE)
#[allow(dead_code)]
pub fn extract_token(parts: &Parts) -> Option<String> {
    // Try Authorization header first
    if let Some(auth) = parts.headers.get("Authorization").and_then(|v| v.to_str().ok())
        && let Some(token) = auth.strip_prefix("Bearer ")
        && !token.is_empty()
    {
        return Some(token.to_string());
    }
    // Try query param
    if let Some(query) = parts.uri.query() {
        for pair in query.split('&') {
            if let Some(val) = pair.strip_prefix("token=")
                && !val.is_empty()
            {
                return Some(val.to_string());
            }
        }
    }
    None
}

/// Verify token and return AuthenticatedUser
#[allow(dead_code)]
pub fn verify_and_extract(
    auth_service: &AuthService,
    token: &str,
) -> Result<AuthenticatedUser, StatusCode> {
    match auth_service.verify_token(token) {
        Ok(claims) => {
            let (user_id, user_type) = if let Some(aid) = claims.agent_id {
                (aid, "agent".to_string())
            } else if let Some(uid) = claims.user_id {
                (uid, "human".to_string())
            } else {
                // Fallback: extract from sub field
                let sub = claims.sub.clone();
                if sub.starts_with("agent:") {
                    (sub.trim_start_matches("agent:").to_string(), "agent".to_string())
                } else {
                    (sub.trim_start_matches("user:").to_string(), "human".to_string())
                }
            };
            Ok(AuthenticatedUser {
                user_id,
                user_type,
                role: claims.role,
            })
        }
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use maple_gateway::auth::AuthService;

    /// Build a `Parts` for testing — `axum::http::request::Parts` does not
    /// implement `Default` in axum 0.7, so we construct a minimal Request
    /// and split it.
    fn make_test_parts() -> Parts {
        axum::http::Request::<()>::new(()).into_parts().0
    }

    #[test]
    fn test_extract_user_from_user_token() {
        let auth = AuthService::new("test-secret".to_string());
        let token = auth.create_token_for_user("u1", "user", 3600).unwrap();
        let user = verify_and_extract(&auth, &token).unwrap();
        assert_eq!(user.user_id, "u1");
        assert_eq!(user.user_type, "human");
        assert_eq!(user.role, "user");
    }

    #[test]
    fn test_extract_user_from_agent_token() {
        let auth = AuthService::new("test-secret".to_string());
        let token = auth.create_token_for_agent("a1", 3600).unwrap();
        let user = verify_and_extract(&auth, &token).unwrap();
        assert_eq!(user.user_id, "a1");
        assert_eq!(user.user_type, "agent");
        assert_eq!(user.role, "agent");
    }

    #[test]
    fn test_extract_token_from_bearer_header() {
        let mut parts = make_test_parts();
        parts.headers.insert(
            axum::http::HeaderName::from_static("authorization"),
            "Bearer mytoken123".parse().unwrap(),
        );
        assert_eq!(extract_token(&parts), Some("mytoken123".to_string()));
    }

    #[test]
    fn test_extract_token_from_query() {
        let mut parts = make_test_parts();
        parts.uri = "/ws/groups?token=abc123".parse().unwrap();
        assert_eq!(extract_token(&parts), Some("abc123".to_string()));
    }

    #[test]
    fn test_extract_token_none() {
        let parts = make_test_parts();
        assert_eq!(extract_token(&parts), None);
    }
}
