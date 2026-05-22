use serde::{Deserialize, Serialize};
use anyhow::Result;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub path: String,
    pub secret: String,
    pub agent_id: String,
}

pub struct WebhookHandler;

impl WebhookHandler {
    pub fn new() -> Self {
        Self
    }

    pub fn verify_signature(&self, body: &[u8], signature: &str, secret: &str) -> bool {
        let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
            Ok(m) => m,
            Err(_) => return false,
        };
        mac.update(body);
        let result = mac.finalize();
        let computed = hex::encode(result.into_bytes());
        computed.to_lowercase() == signature.to_lowercase()
    }

    pub async fn handle(&self, config: &WebhookConfig, body: &[u8], signature: &str) -> Result<serde_json::Value> {
        if !self.verify_signature(body, signature, &config.secret) {
            anyhow::bail!("Invalid webhook signature");
        }
        Ok(serde_json::json!({
            "status": "received",
            "agent_id": config.agent_id,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_signature_valid() {
        let handler = WebhookHandler::new();
        let secret = "my-webhook-secret";
        let body = b"{\"event\": \"push\"}";

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let sig = hex::encode(mac.finalize().into_bytes());

        assert!(handler.verify_signature(body, &sig, secret));
    }

    #[test]
    fn test_verify_signature_invalid() {
        let handler = WebhookHandler::new();
        assert!(!handler.verify_signature(b"body", "wrong-signature", "secret"));
    }

    #[test]
    fn test_verify_signature_tampered_body() {
        let handler = WebhookHandler::new();
        let secret = "secret";
        let body = b"original";

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let sig = hex::encode(mac.finalize().into_bytes());

        assert!(!handler.verify_signature(b"tampered", &sig, secret));
    }

    #[tokio::test]
    async fn test_handle_valid_signature() {
        let handler = WebhookHandler::new();
        let config = WebhookConfig {
            path: "/webhook".to_string(),
            secret: "secret".to_string(),
            agent_id: "agent-1".to_string(),
        };
        let body = b"payload";

        let mut mac = HmacSha256::new_from_slice(config.secret.as_bytes()).unwrap();
        mac.update(body);
        let sig = hex::encode(mac.finalize().into_bytes());

        let result = handler.handle(&config, body, &sig).await.unwrap();
        assert_eq!(result["status"], "received");
        assert_eq!(result["agent_id"], "agent-1");
    }

    #[tokio::test]
    async fn test_handle_invalid_signature() {
        let handler = WebhookHandler::new();
        let config = WebhookConfig {
            path: "/webhook".to_string(),
            secret: "secret".to_string(),
            agent_id: "agent-1".to_string(),
        };
        let result = handler.handle(&config, b"payload", "bad-sig").await;
        assert!(result.is_err());
    }
}
