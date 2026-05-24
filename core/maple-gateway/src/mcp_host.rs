use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Stdio;
use dashmap::DashMap;
use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use futures::{SinkExt, StreamExt};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransportConfig,
    pub description: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportConfig {
    Stdio { command: Vec<String> },
    Http { url: String },
    WebSocket { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

struct ToolRoute {
    server_name: String,
    #[allow(dead_code)]
    raw_name: String,
}

type WsSink = futures::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>;

struct McpServerInstance {
    name: String,
    transport: McpTransportConfig,
    tools: Vec<McpTool>,
    status: McpServerStatus,
    child: Option<Child>,
    stdin: Option<tokio::process::ChildStdin>,
    stdout_reader: Option<Mutex<BufReader<tokio::process::ChildStdout>>>,
    ws_writer: Option<Mutex<WsSink>>,
    stderr_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for McpServerInstance {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.start_kill();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum McpServerStatus {
    Starting,
    Ready,
    Error,
    Stopped,
}

pub struct McpHostManager {
    servers: DashMap<String, Mutex<McpServerInstance>>,
    tool_index: DashMap<String, ToolRoute>,
    next_request_id: std::sync::atomic::AtomicU64,
}

impl McpHostManager {
    pub fn new() -> Self {
        Self {
            servers: DashMap::new(),
            tool_index: DashMap::new(),
            next_request_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    pub async fn start_server(&self, config: &McpServerConfig) -> Result<()> {
        tracing::info!(server = %config.name, "Starting MCP server");

        match &config.transport {
            McpTransportConfig::Stdio { command } => {
                if command.is_empty() {
                    anyhow::bail!("MCP stdio command is empty");
                }

                let mut cmd = Command::new(&command[0]);
                if command.len() > 1 {
                    cmd.args(&command[1..]);
                }
                if let Some(env) = &config.env {
                    cmd.envs(env);
                }
                cmd.stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());

                let mut child = cmd.spawn().map_err(|e| {
                    anyhow::anyhow!("Failed to spawn MCP server '{}': {}", config.name, e)
                })?;

                let stdin = child.stdin.take()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get stdin"))?;
                let stdout = child.stdout.take()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get stdout"))?;
                let stderr = child.stderr.take();

                let mut stdin_writer = stdin;
                let mut reader = BufReader::new(stdout);

                let stderr_handle = stderr.map(|stderr| {
                    let server_name = config.name.clone();
                    tokio::spawn(async move {
                        let mut stderr_reader = BufReader::new(stderr);
                        let mut line = String::new();
                        loop {
                            match stderr_reader.read_line(&mut line).await {
                                Ok(0) => break,
                                Ok(_) => {
                                    tracing::debug!(
                                        server = %server_name,
                                        stderr = %line.trim(),
                                        "MCP server stderr"
                                    );
                                    line.clear();
                                }
                                Err(_) => break,
                            }
                        }
                    })
                });

                let init_request = self.jsonrpc_request("initialize", serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": { "name": "mapleos", "version": env!("CARGO_PKG_VERSION") }
                }));

                self.send_jsonrpc_stdio(&mut stdin_writer, &init_request).await?;
                let _init_response = self.read_jsonrpc_response(&mut reader).await;

                let tools = match self.discover_tools_stdio(&mut stdin_writer, &mut reader).await {
                    Ok(t) => {
                        tracing::info!(server = %config.name, tool_count = t.len(), "Discovered MCP tools");
                        t
                    }
                    Err(e) => {
                        tracing::warn!(server = %config.name, error = %e, "Failed to discover tools");
                        Vec::new()
                    }
                };

                self.register_tools(&config.name, &tools);

                self.servers.insert(config.name.clone(), Mutex::new(McpServerInstance {
                    name: config.name.clone(),
                    transport: config.transport.clone(),
                    tools,
                    status: McpServerStatus::Ready,
                    child: Some(child),
                    stdin: Some(stdin_writer),
                    stdout_reader: Some(Mutex::new(reader)),
                    ws_writer: None,
                    stderr_handle,
                }));
            }
            McpTransportConfig::Http { url } => {
                let client = reqwest::Client::new();
                let tools = self.discover_tools_http_or_warn(&client, url, &config.name).await;

                self.register_tools(&config.name, &tools);

                self.servers.insert(config.name.clone(), Mutex::new(McpServerInstance {
                    name: config.name.clone(),
                    transport: config.transport.clone(),
                    tools,
                    status: McpServerStatus::Ready,
                    child: None,
                    stdin: None,
                    stdout_reader: None,
                    ws_writer: None,
                    stderr_handle: None,
                }));
            }
            McpTransportConfig::WebSocket { url } => {
                tracing::info!(server = %config.name, url = %url, "Connecting MCP WebSocket server");

                let (ws_stream, _) = tokio_tungstenite::connect_async(url).await
                    .map_err(|e| anyhow::anyhow!("Failed to connect MCP WS '{}': {}", config.name, e))?;

                let (mut write, mut read) = ws_stream.split();

                let init_request = self.jsonrpc_request("initialize", serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": { "name": "mapleos", "version": env!("CARGO_PKG_VERSION") }
                }));
                let init_json = serde_json::to_string(&init_request)?;
                write.send(Message::Text(init_json)).await
                    .map_err(|e| anyhow::anyhow!("WS send init failed: {}", e))?;
                let _ = Self::read_ws_json_response(&mut read).await;

                let notification = serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
                write.send(Message::Text(serde_json::to_string(&notification)?)).await
                    .map_err(|e| anyhow::anyhow!("WS send notification failed: {}", e))?;

                let tools_request = self.jsonrpc_request("tools/list", serde_json::json!({}));
                write.send(Message::Text(serde_json::to_string(&tools_request)?)).await
                    .map_err(|e| anyhow::anyhow!("WS tools/list failed: {}", e))?;

                let tools = match Self::read_ws_json_response(&mut read).await {
                    Some(json) => Self::parse_tools_from_response(&json),
                    None => Vec::new(),
                };

                tracing::info!(server = %config.name, tool_count = tools.len(), "Discovered MCP tools via WS");
                self.register_tools(&config.name, &tools);

                self.servers.insert(config.name.clone(), Mutex::new(McpServerInstance {
                    name: config.name.clone(),
                    transport: config.transport.clone(),
                    tools,
                    status: McpServerStatus::Ready,
                    child: None,
                    stdin: None,
                    stdout_reader: None,
                    ws_writer: Some(Mutex::new(write)),
                    stderr_handle: None,
                }));
            }
        }

        Ok(())
    }

    pub async fn stop_server(&self, name: &str) -> Result<()> {
        if let Some((_, mutex_inst)) = self.servers.remove(name) {
            let mut inst = mutex_inst.lock().await;
            if let Some(ref mut child) = inst.child {
                let _ = child.start_kill();
            }
            tracing::info!(server = name, "MCP server stopped");
        }
        Ok(())
    }

    pub async fn call_tool(&self, server_name: &str, tool_name: &str, args: Value) -> Result<Value> {
        let instance_ref = self.servers.get(server_name)
            .ok_or_else(|| anyhow::anyhow!("MCP server not found: {}", server_name))?;

        let transport = {
            let inst = instance_ref.lock().await;
            if inst.status != McpServerStatus::Ready {
                anyhow::bail!("MCP server {} not ready", server_name);
            }
            inst.transport.clone()
        };

        let request = self.jsonrpc_request("tools/call", serde_json::json!({
            "name": tool_name,
            "arguments": args,
        }));

        match &transport {
            McpTransportConfig::Stdio { .. } => {
                let mut inst = instance_ref.lock().await;
                let stdin = inst.stdin.as_mut()
                    .ok_or_else(|| anyhow::anyhow!("MCP server {} has no stdin", server_name))?;

                self.send_jsonrpc_stdio(stdin, &request).await?;

                if let Some(ref reader_mutex) = inst.stdout_reader {
                    let mut reader = reader_mutex.lock().await;
                    match self.read_jsonrpc_response(&mut reader).await {
                        Some(json) => {
                            if let Some(error) = json.get("error") {
                                anyhow::bail!("MCP tool error: {}", error["message"].as_str().unwrap_or("unknown"));
                            }
                            Ok(json["result"].clone())
                        }
                        None => Ok(serde_json::json!({"tool": tool_name, "status": "sent_no_response"})),
                    }
                } else {
                    Ok(serde_json::json!({"tool": tool_name, "status": "sent_via_stdio"}))
                }
            }
            McpTransportConfig::Http { url } => {
                let client = reqwest::Client::new();
                let resp = client
                    .post(format!("{}/mcp", url))
                    .json(&request)
                    .timeout(std::time::Duration::from_secs(30))
                    .send()
                    .await?;

                if !resp.status().is_success() {
                    anyhow::bail!("MCP HTTP call failed ({})", resp.status());
                }

                let json: Value = resp.json().await.unwrap_or_default();
                Ok(json["result"].clone())
            }
            McpTransportConfig::WebSocket { .. } => {
                let writer_mutex = instance_ref.lock().await.ws_writer.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("MCP server {} has no WebSocket connection", server_name))?
                    .clone();
                
                let mut writer = writer_mutex.lock().await;
                writer.send(Message::Text(serde_json::to_string(&request)?)).await
                    .map_err(|e| anyhow::anyhow!("WS send tool call failed: {}", e))?;
                drop(writer);

                Ok(serde_json::json!({"tool": tool_name, "status": "sent_via_ws"}))
            }
        }
    }

    pub fn list_servers(&self) -> Vec<(String, McpServerStatus, usize)> {
        self.servers.iter()
            .filter_map(|entry| {
                let inst = entry.value().try_lock().ok()?;
                Some((inst.name.clone(), inst.status.clone(), inst.tools.len()))
            })
            .collect()
    }

    pub fn list_tools(&self) -> Vec<(String, String)> {
        self.tool_index.iter()
            .map(|entry| (entry.key().clone(), entry.value().server_name.clone()))
            .collect()
    }

    fn next_id(&self) -> u64 {
        self.next_request_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    fn jsonrpc_request(&self, method: &str, params: Value) -> Value {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": method,
            "params": params,
        })
    }

    fn register_tools(&self, server_name: &str, tools: &[McpTool]) {
        for tool in tools {
            let qualified_name = format!("mcp__{}__{}", server_name, tool.name);
            self.tool_index.insert(qualified_name, ToolRoute {
                server_name: server_name.to_string(),
                raw_name: tool.name.clone(),
            });
        }
    }

    fn parse_tools_from_response(json: &Value) -> Vec<McpTool> {
        json["result"]["tools"].as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|t| {
                let name = t["name"].as_str()?.to_string();
                let description = t["description"].as_str().map(|s| s.to_string());
                let input_schema = t["inputSchema"].clone();
                Some(McpTool { name, description, input_schema })
            })
            .collect()
    }

    async fn send_jsonrpc_stdio(&self, stdin: &mut tokio::process::ChildStdin, request: &Value) -> Result<()> {
        let json = serde_json::to_string(request)?;
        stdin.write_all(format!("Content-Length: {}\r\n\r\n{}", json.len(), json).as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    async fn read_jsonrpc_response(&self, reader: &mut BufReader<tokio::process::ChildStdout>) -> Option<Value> {
        let mut header_line = String::new();
        let mut content_length: usize = 0;

        loop {
            header_line.clear();
            match reader.read_line(&mut header_line).await {
                Ok(0) | Err(_) => return None,
                Ok(_) => {
                    let line = header_line.trim();
                    if line.is_empty() { break; }
                    if let Some(length_str) = line.strip_prefix("Content-Length:") {
                        content_length = length_str.trim().parse().unwrap_or(0);
                    }
                }
            }
        }

        if content_length == 0 { return None; }

        let mut buffer = vec![0u8; content_length];
        match reader.read_exact(&mut buffer).await {
            Ok(_) => serde_json::from_str(&String::from_utf8_lossy(&buffer)).ok(),
            Err(_) => None,
        }
    }

    async fn discover_tools_stdio(
        &self,
        stdin: &mut tokio::process::ChildStdin,
        reader: &mut BufReader<tokio::process::ChildStdout>,
    ) -> Result<Vec<McpTool>> {
        let notification = serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        self.send_jsonrpc_stdio(stdin, &notification).await?;

        let tools_request = self.jsonrpc_request("tools/list", serde_json::json!({}));
        self.send_jsonrpc_stdio(stdin, &tools_request).await?;

        match self.read_jsonrpc_response(reader).await {
            Some(json) => Ok(Self::parse_tools_from_response(&json)),
            None => Ok(Vec::new()),
        }
    }

    async fn discover_tools_http_or_warn(&self, client: &reqwest::Client, base_url: &str, server_name: &str) -> Vec<McpTool> {
        let request = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}});
        match client
            .post(format!("{}/mcp", base_url))
            .json(&request)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let json: Value = resp.json().await.unwrap_or_default();
                let tools = Self::parse_tools_from_response(&json);
                tracing::info!(server = server_name, tool_count = tools.len(), "Discovered MCP tools via HTTP");
                tools
            }
            Ok(resp) => {
                tracing::warn!(status = %resp.status(), server = server_name, "MCP HTTP tools/list failed");
                Vec::new()
            }
            Err(e) => {
                tracing::warn!(error = %e, server = server_name, "MCP HTTP tools/list request failed");
                Vec::new()
            }
        }
    }

    async fn read_ws_json_response<S>(read: &mut S) -> Option<Value>
    where
        S: StreamExt<Item = Result<Message, tungstenite::Error>> + Unpin,
    {
        let timeout = tokio::time::Duration::from_secs(10);
        tokio::time::timeout(timeout, async {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(json) = serde_json::from_str::<Value>(&text) {
                            if json.get("id").is_some() {
                                return Some(json);
                            }
                        }
                    }
                    Ok(Message::Close(_)) => return None,
                    Err(_) => return None,
                    _ => continue,
                }
            }
            None
        }).await.unwrap_or(None)
    }
}