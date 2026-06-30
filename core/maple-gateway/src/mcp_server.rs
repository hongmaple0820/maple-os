use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef { pub name: String, pub description: String, pub input_schema: Value }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource { pub uri: String, pub name: String, pub mime_type: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt { pub name: String, pub description: String, pub arguments: Vec<McpPromptArg> }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArg { pub name: String, pub description: String, pub required: bool }

#[derive(Debug, Deserialize)]
struct JsonRpcRequest { #[allow(dead_code)] jsonrpc: String, id: Value, method: String, #[serde(default)] params: Value }
#[derive(Debug, Serialize)]
struct JsonRpcResponse { jsonrpc: String, id: Value, #[serde(skip_serializing_if = "Option::is_none")] result: Option<Value>, #[serde(skip_serializing_if = "Option::is_none")] error: Option<JsonRpcError> }
#[derive(Debug, Serialize)]
struct JsonRpcError { code: i32, message: String }

pub struct MapleMcpServer {
    tools: Arc<Mutex<Vec<McpToolDef>>>,
    tool_handler: Arc<dyn Fn(&str, &Value) -> Result<Value> + Send + Sync>,
}

impl MapleMcpServer {
    pub fn new<F>(handler: F) -> Self where F: Fn(&str, &Value) -> Result<Value> + Send + Sync + 'static {
        Self { tools: Arc::new(Mutex::new(Vec::new())), tool_handler: Arc::new(handler) }
    }
    pub async fn register_tool(&self, tool: McpToolDef) { self.tools.lock().await.push(tool); }
    pub async fn run_stdio(&self) -> Result<()> {
        let stdin = std::io::stdin(); let mut stdout = std::io::stdout();
        for line in stdin.lock().lines() {
            let line = line?; if line.trim().is_empty() { continue; }
            let req: JsonRpcRequest = match serde_json::from_str(&line) { Ok(r) => r, Err(_) => continue };
            let resp = self.handle(&req).await;
            writeln!(stdout, "{}", serde_json::to_string(&resp)?)?; stdout.flush()?;
        }
        Ok(())
    }
    async fn handle(&self, req: &JsonRpcRequest) -> JsonRpcResponse {
        let result = match req.method.as_str() {
            "initialize" => Ok(json!({"protocolVersion":"2025-06-18","capabilities":{"tools":{"listChanged":true},"resources":{"listChanged":true},"prompts":{"listChanged":true}},"serverInfo":{"name":"mapleos","version":env!("CARGO_PKG_VERSION")}})),
            "tools/list" => { let t = self.tools.lock().await; Ok(json!({"tools": t.clone()})) },
            "tools/call" => {
                let name = req.params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = req.params.get("arguments").cloned().unwrap_or(Value::Null);
                match (self.tool_handler)(name, &args) {
                    Ok(r) => Ok(json!({"content":[{"type":"text","text":r.to_string()}]})),
                    Err(e) => Err(JsonRpcError{code:-32603, message:e.to_string()}),
                }
            }
            _ => Err(JsonRpcError{code:-32601, message:format!("Method not found: {}", req.method)}),
        };
        match result {
            Ok(r) => JsonRpcResponse{jsonrpc:"2.0".into(), id:req.id.clone(), result:Some(r), error:None},
            Err(e) => JsonRpcResponse{jsonrpc:"2.0".into(), id:req.id.clone(), result:None, error:Some(e)},
        }
    }
}

pub fn builtin_tool_defs() -> Vec<McpToolDef> {
    vec![
        McpToolDef{name:"mapleos__kb_search".into(),description:"Search KB".into(),input_schema:json!({"type":"object","required":["query"],"properties":{"query":{"type":"string"},"limit":{"type":"number","default":5}}})},
        McpToolDef{name:"mapleos__workflow_run".into(),description:"Run workflow".into(),input_schema:json!({"type":"object","required":["workflow_id"],"properties":{"workflow_id":{"type":"string"},"input":{"type":"string","default":"{}"}}})},
        McpToolDef{name:"mapleos__memory_search".into(),description:"Search agent memory".into(),input_schema:json!({"type":"object","required":["agent_id","query"],"properties":{"agent_id":{"type":"string"},"query":{"type":"string"},"limit":{"type":"number","default":5}}})},
        McpToolDef{name:"mapleos__agent_delegate".into(),description:"Delegate task".into(),input_schema:json!({"type":"object","required":["agent_id","task"],"properties":{"agent_id":{"type":"string"},"task":{"type":"string"},"context":{"type":"string"}}})},
        McpToolDef{name:"mapleos__execution_trace".into(),description:"Get execution trace".into(),input_schema:json!({"type":"object","required":["execution_id"],"properties":{"execution_id":{"type":"string"}}})},
        McpToolDef{name:"mapleos__core_memory_get".into(),description:"Get core memory".into(),input_schema:json!({"type":"object","required":["agent_id"],"properties":{"agent_id":{"type":"string"}}})},
        McpToolDef{name:"mapleos__core_memory_set".into(),description:"Set core memory".into(),input_schema:json!({"type":"object","required":["agent_id","block_type","block_key","block_value"],"properties":{"agent_id":{"type":"string"},"block_type":{"type":"string","enum":["persona","goals","pinned_facts","custom"]},"block_key":{"type":"string"},"block_value":{"type":"string"}}})},
        McpToolDef{name:"mapleos__archival_search".into(),description:"Search archival memory".into(),input_schema:json!({"type":"object","required":["agent_id","query"],"properties":{"agent_id":{"type":"string"},"query":{"type":"string"},"limit":{"type":"number","default":5}}})},
        McpToolDef{name:"mapleos__archival_insert".into(),description:"Insert archival memory".into(),input_schema:json!({"type":"object","required":["agent_id","content"],"properties":{"agent_id":{"type":"string"},"content":{"type":"string"},"memory_type":{"type":"string","enum":["episodic","semantic"],"default":"episodic"},"importance_score":{"type":"number","default":0.5}}})},
        McpToolDef{name:"mapleos__reflect".into(),description:"Trigger reflection".into(),input_schema:json!({"type":"object","required":["agent_id"],"properties":{"agent_id":{"type":"string"}}})},
    ]
}
