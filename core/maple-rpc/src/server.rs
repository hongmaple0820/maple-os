use axum::{Router, routing::post, Json, extract::State};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;


use crate::dispatch::RpcDispatcher;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn method_not_found() -> Self {
        Self { code: -32601, message: "Method not found".to_string(), data: None }
    }

    pub fn invalid_params(msg: &str) -> Self {
        Self { code: -32602, message: msg.to_string(), data: None }
    }

    pub fn internal_error(msg: &str) -> Self {
        Self { code: -32603, message: msg.to_string(), data: None }
    }
}

pub struct RpcServer {
    dispatcher: Arc<RpcDispatcher>,
}

impl RpcServer {
    pub fn new(dispatcher: Arc<RpcDispatcher>) -> Self {
        Self { dispatcher }
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/rpc", post(rpc_handler))
            .with_state(self.dispatcher.clone())
    }
}

async fn rpc_handler(
    State(dispatcher): State<Arc<RpcDispatcher>>,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    if request.jsonrpc != "2.0" {
        return Json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(JsonRpcError {
                code: -32600,
                message: "Invalid Request".to_string(),
                data: None,
            }),
        });
    }

    let result = dispatcher.dispatch(&request.method, request.params).await;

    match result {
        Ok(value) => Json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(value),
            error: None,
        }),
        Err(e) => Json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(JsonRpcError::internal_error(&e.to_string())),
        }),
    }
}
