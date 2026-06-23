use async_trait::async_trait;
use serde_json::Value;
use reqwest::Client;
use anyhow::Result;
use uuid::Uuid;

use serde_json::json;
#[derive(Debug, Clone)]
pub struct RawMessage {
    pub channel: String,
    pub sender_id: String,
    pub content: String,
    pub raw_data: Value,
}

pub struct FmpMessage {
    pub id: String,
    pub msg_type: String,
    pub channel: String,
    pub sender_id: String,
    pub sender_type: String,
    pub content: String,
    pub metadata: Value,
}

#[async_trait]
pub trait ChannelAdapter: Send + Sync {
    fn name(&self) -> &str;
    async fn normalize_message(&self, raw: &RawMessage) -> Result<FmpMessage>;
    async fn denormalize_message(&self, msg: &FmpMessage) -> Result<Value>;
    async fn send_to_group(&self, group_id: &str, msg: &FmpMessage) -> Result<()>;
    async fn send_to_user(&self, user_id: &str, msg: &FmpMessage) -> Result<()>;
}

#[allow(dead_code)]
struct FeishuAdapter {
    app_id: String,
    app_secret: String,
    client: Client,
}
#[allow(dead_code)]
struct TelegramAdapter {
    bot_token: String,
    client: Client,
}

#[allow(dead_code)]
struct DingtalkAdapter {
    app_key: String,
    app_secret: String,
    client: Client,
}
#[allow(dead_code)]
impl FeishuAdapter {
    pub fn new(app_id: &str, app_secret: &str) -> Self {
        Self {
            app_id: app_id.to_string(),
            app_secret: app_secret.to_string(),
            client: Client::new(),
        }
    }

    async fn get_tenant_token(&self) -> Result<String> {
        let resp = self.client
            .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
            .json(&json!({
                "app_id": &self.app_id,
                "app_secret": &self.app_secret,
            }))
            .send()
            .await?;
        let body: Value = resp.json().await?;
        body.get("tenant_access_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Failed to get Feishu tenant token"))
    }
}
#[allow(dead_code)]
impl TelegramAdapter {
    pub fn new(bot_token: &str) -> Self {
        Self {
            bot_token: bot_token.to_string(),
            client: Client::new(),
        }
    }
}
#[allow(dead_code)]
impl DingtalkAdapter {
    pub fn new(app_key: &str, app_secret: &str) -> Self {
        Self {
            app_key: app_key.to_string(),
            app_secret: app_secret.to_string(),
            client: Client::new(),
        }
    }

    async fn get_access_token(&self) -> Result<String> {
        let resp = self.client
            .post("https://oapi.dingtalk.com/gettoken")
            .json(&json!({
                "appkey": self.app_key,
                "appsecret": self.app_secret,
            }))
            .send()
            .await?;
        let body: Value = resp.json().await?;
        body.get("access_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Failed to get DingTalk access token"))
    }
}

#[async_trait]
impl ChannelAdapter for FeishuAdapter {
    fn name(&self) -> &str { "feishu" }

    async fn normalize_message(&self, raw: &RawMessage) -> Result<FmpMessage> {
        Ok(FmpMessage {
            id: Uuid::new_v4().to_string(),
            msg_type: "message".to_string(),
            channel: "feishu".to_string(),
            sender_id: raw.sender_id.clone(),
            sender_type: "human".to_string(),
            content: raw.content.clone(),
            metadata: raw.raw_data.clone(),
        })
    }

    async fn denormalize_message(&self, msg: &FmpMessage) -> Result<Value> {
        Ok(json!({
            "msg_type": "text",
            "content": { "text": msg.content },
        }))
    }

    async fn send_to_group(&self, group_id: &str, msg: &FmpMessage) -> Result<()> {
        let token = self.get_tenant_token().await?;
        let payload = self.denormalize_message(msg).await?;
        let _resp = self.client
            .post("https://open.feishu.cn/open-apis/im/v2/messages?receive_id_type=chat_id")
            .header("Authorization", format!("Bearer {}", token))
            .json(&json!({
                "receive_id": group_id,
                "msg_type": "text",
                "content": payload,
            }))
            .send()
            .await?;
        tracing::info!(group_id = group_id, "Sent message to Feishu group");
        Ok(())
    }

    async fn send_to_user(&self, user_id: &str, msg: &FmpMessage) -> Result<()> {
        let token = self.get_tenant_token().await?;
        let payload = self.denormalize_message(msg).await?;
        let _resp = self.client
            .post("https://open.feishu.cn/open-apis/im/v2/messages?receive_id_type=open_id")
            .header("Authorization", format!("Bearer {}", token))
            .json(&json!({
                "receive_id": user_id,
                "msg_type": "text",
                "content": payload,
            }))
            .send()
            .await?;
        tracing::info!(user_id = user_id, "Sent message to Feishu user");
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for TelegramAdapter {
    fn name(&self) -> &str { "telegram" }
    async fn normalize_message(&self, raw: &RawMessage) -> Result<FmpMessage> {
        Ok(FmpMessage {
            id: Uuid::new_v4().to_string(),
            msg_type: "message".to_string(),
            channel: "telegram".to_string(),
            sender_id: raw.sender_id.clone(),
            sender_type: "human".to_string(),
            content: raw.content.clone(),
            metadata: raw.raw_data.clone(),
        })
    }

    async fn denormalize_message(&self, msg: &FmpMessage) -> Result<Value> {
        Ok(json!({ "text": msg.content }))
    }

    async fn send_to_group(&self, group_id: &str, msg: &FmpMessage) -> Result<()> {
        let payload = self.denormalize_message(msg).await?;
        let text = payload.get("text").and_then(|v| v.as_str()).unwrap_or(&msg.content);
        let _resp = self.client
            .post(format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token))
            .json(&json!({
                "chat_id": group_id,
                "text": text,
            }))
            .send()
            .await?;
        tracing::info!(group_id = group_id, "Sent message to Telegram group");
        Ok(())
    }

    async fn send_to_user(&self, user_id: &str, msg: &FmpMessage) -> Result<()> {
        let payload = self.denormalize_message(msg).await?;
        let text = payload.get("text").and_then(|v| v.as_str()).unwrap_or(&msg.content);
        let _resp = self.client
            .post(format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token))
            .json(&json!({
                "chat_id": user_id,
                "text": text,
            }))
            .send()
            .await?;
        tracing::info!(user_id = user_id, "Sent message to Telegram user");
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for DingtalkAdapter {
    fn name(&self) -> &str { "dingtalk" }
    async fn normalize_message(&self, raw: &RawMessage) -> Result<FmpMessage> {
        Ok(FmpMessage {
            id: Uuid::new_v4().to_string(),
            msg_type: "message".to_string(),
            channel: "dingtalk".to_string(),
            sender_id: raw.sender_id.clone(),
            sender_type: "human".to_string(),
            content: raw.content.clone(),
            metadata: raw.raw_data.clone(),
        })
    }

    async fn denormalize_message(&self, msg: &FmpMessage) -> Result<Value> {
        Ok(json!({
            "msgtype": "text",
            "text": { "content": msg.content },
        }))
    }

    async fn send_to_group(&self, group_id: &str, msg: &FmpMessage) -> Result<()> {
        let token = self.get_access_token().await?;
        let payload = self.denormalize_message(msg).await?;
        let _resp = self.client
            .post(format!("https://oapi.dingtalk.com/topapi/message/corpconversation/asyncsend_v2?access_token={}", token))
            .json(&json!({
                "agent_id": self.app_key,
                "userid_list": vec![group_id],
                "msg": payload,
            }))
            .send()
            .await?;
        tracing::info!(group_id = group_id, "Sent message to DingTalk group");
        Ok(())
    }

    async fn send_to_user(&self, user_id: &str, msg: &FmpMessage) -> Result<()> {
        let token = self.get_access_token().await?;
        let payload = self.denormalize_message(msg).await?;
        let _resp = self.client
            .post(format!("https://oapi.dingtalk.com/topapi/message/corpconversation/asyncsend_v2?access_token={}", token))
            .json(&json!({
                "agent_id": self.app_key,
                "userid_list": vec![user_id],
                "msg": payload,
            }))
            .send()
            .await?;
        tracing::info!(user_id = user_id, "Sent message to DingTalk user");
        Ok(())
    }
}