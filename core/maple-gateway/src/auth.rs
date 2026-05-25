use serde::{Deserialize, Serialize};
use jsonwebtoken::{encode, decode, Header, Algorithm, Validation, EncodingKey, DecodingKey};
use anyhow::Result;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashSet;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Editor,
    Viewer,
    Agent,
}

impl Role {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "admin" => Role::Admin,
            "editor" => Role::Editor,
            "viewer" => Role::Viewer,
            "agent" => Role::Agent,
            _ => Role::Viewer,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Role::Admin => "admin",
            Role::Editor => "editor",
            Role::Viewer => "viewer",
            Role::Agent => "agent",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ReadWorkflows,
    WriteWorkflows,
    ExecuteWorkflows,
    DeleteWorkflows,
    ReadAgents,
    WriteAgents,
    ManageAgents,
    ReadSessions,
    WriteSessions,
    DeleteSessions,
    ReadMemories,
    WriteMemories,
    DeleteMemories,
    ReadPrompts,
    WritePrompts,
    ManageConfig,
    ManageUsers,
    ViewMetrics,
}

impl Role {
    pub fn permissions(&self) -> HashSet<Permission> {
        match self {
            Role::Admin => {
                let mut perms = HashSet::new();
                perms.insert(Permission::ReadWorkflows);
                perms.insert(Permission::WriteWorkflows);
                perms.insert(Permission::ExecuteWorkflows);
                perms.insert(Permission::DeleteWorkflows);
                perms.insert(Permission::ReadAgents);
                perms.insert(Permission::WriteAgents);
                perms.insert(Permission::ManageAgents);
                perms.insert(Permission::ReadSessions);
                perms.insert(Permission::WriteSessions);
                perms.insert(Permission::DeleteSessions);
                perms.insert(Permission::ReadMemories);
                perms.insert(Permission::WriteMemories);
                perms.insert(Permission::DeleteMemories);
                perms.insert(Permission::ReadPrompts);
                perms.insert(Permission::WritePrompts);
                perms.insert(Permission::ManageConfig);
                perms.insert(Permission::ManageUsers);
                perms.insert(Permission::ViewMetrics);
                perms
            }
            Role::Editor => {
                let mut perms = HashSet::new();
                perms.insert(Permission::ReadWorkflows);
                perms.insert(Permission::WriteWorkflows);
                perms.insert(Permission::ExecuteWorkflows);
                perms.insert(Permission::ReadAgents);
                perms.insert(Permission::ReadSessions);
                perms.insert(Permission::WriteSessions);
                perms.insert(Permission::ReadMemories);
                perms.insert(Permission::WriteMemories);
                perms.insert(Permission::ReadPrompts);
                perms.insert(Permission::WritePrompts);
                perms.insert(Permission::ViewMetrics);
                perms
            }
            Role::Viewer => {
                let mut perms = HashSet::new();
                perms.insert(Permission::ReadWorkflows);
                perms.insert(Permission::ReadAgents);
                perms.insert(Permission::ReadSessions);
                perms.insert(Permission::ReadMemories);
                perms.insert(Permission::ReadPrompts);
                perms.insert(Permission::ViewMetrics);
                perms
            }
            Role::Agent => {
                let mut perms = HashSet::new();
                perms.insert(Permission::ReadWorkflows);
                perms.insert(Permission::ExecuteWorkflows);
                perms.insert(Permission::ReadSessions);
                perms.insert(Permission::WriteSessions);
                perms.insert(Permission::ReadMemories);
                perms.insert(Permission::WriteMemories);
                perms
            }
        }
    }

    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions().contains(permission)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthClaims {
    pub sub: String,
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub role: String,
    pub exp: u64,
    pub iat: u64,
}

impl AuthClaims {
    pub fn role(&self) -> Role {
        Role::from_str(&self.role)
    }

    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.role().has_permission(permission)
    }
}

pub struct AuthService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl AuthService {
    pub fn new(jwt_secret: String) -> Self {
        let secret_bytes = jwt_secret.as_bytes();
        Self {
            encoding_key: EncodingKey::from_secret(secret_bytes),
            decoding_key: DecodingKey::from_secret(secret_bytes),
        }
    }

    pub fn generate_token(&self, claims: &AuthClaims) -> Result<String> {
        let token = encode(&Header::default(), claims, &self.encoding_key)?;
        Ok(token)
    }

    pub fn create_token_for_user(&self, user_id: &str, role: &str, ttl_secs: u64) -> Result<String> {
        let now = chrono::Utc::now().timestamp() as u64;
        let claims = AuthClaims {
            sub: format!("user:{}", user_id),
            agent_id: None,
            user_id: Some(user_id.to_string()),
            role: role.to_string(),
            exp: now + ttl_secs,
            iat: now,
        };
        self.generate_token(&claims)
    }

    pub fn create_token_for_agent(&self, agent_id: &str, ttl_secs: u64) -> Result<String> {
        let now = chrono::Utc::now().timestamp() as u64;
        let claims = AuthClaims {
            sub: format!("agent:{}", agent_id),
            agent_id: Some(agent_id.to_string()),
            user_id: None,
            role: "agent".to_string(),
            exp: now + ttl_secs,
            iat: now,
        };
        self.generate_token(&claims)
    }

    pub fn verify_token(&self, token: &str) -> Result<AuthClaims> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 30;
        let data = decode::<AuthClaims>(token, &self.decoding_key, &validation)?;
        Ok(data.claims)
    }

    pub async fn verify_agent_token(&self, token: &str) -> Result<String> {
        let claims = self.verify_token(token)?;
        claims.agent_id.ok_or_else(|| anyhow::anyhow!("Not an agent token"))
    }

    pub fn verify_hmac(&self, body: &[u8], signature: &str, secret: &str) -> bool {
        let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
            Ok(m) => m,
            Err(_) => return false,
        };
        mac.update(body);
        let result = mac.finalize();
        let computed = hex::encode(result.into_bytes());
        let computed_lower = computed.to_lowercase();
        let signature_lower = signature.to_lowercase();
        computed_lower == signature_lower
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_roundtrip_user() {
        let auth = AuthService::new("test-secret".to_string());
        let token = auth.create_token_for_user("user1", "admin", 3600).unwrap();
        let claims = auth.verify_token(&token).unwrap();
        assert_eq!(claims.user_id.as_deref(), Some("user1"));
        assert_eq!(claims.role, "admin");
        assert!(claims.agent_id.is_none());
    }

    #[test]
    fn test_jwt_roundtrip_agent() {
        let auth = AuthService::new("test-secret".to_string());
        let token = auth.create_token_for_agent("agent-001", 3600).unwrap();
        let claims = auth.verify_token(&token).unwrap();
        assert_eq!(claims.agent_id.as_deref(), Some("agent-001"));
        assert_eq!(claims.role, "agent");
        assert!(claims.user_id.is_none());
    }

    #[test]
    fn test_hmac_verification() {
        let auth = AuthService::new("irrelevant".to_string());
        let secret = "webhook-secret";
        let body = b"hello world";

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let sig = hex::encode(mac.finalize().into_bytes());

        assert!(auth.verify_hmac(body, &sig, secret));
        assert!(!auth.verify_hmac(body, "wrong-signature", secret));
        assert!(!auth.verify_hmac(b"tampered body", &sig, secret));
    }

    #[test]
    fn test_expired_token() {
        let auth = AuthService::new("test-secret".to_string());
        let now = chrono::Utc::now().timestamp() as u64;
        let claims = AuthClaims {
            sub: "user:user1".to_string(),
            agent_id: None,
            user_id: Some("user1".to_string()),
            role: "admin".to_string(),
            exp: now - 3600,
            iat: now - 7200,
        };
        let token = auth.generate_token(&claims).unwrap();
        let result = auth.verify_token(&token);
        assert!(result.is_err());
    }
}
