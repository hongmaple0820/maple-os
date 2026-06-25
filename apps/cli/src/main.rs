#![allow(clippy::all)]
//! MapleOS CLI client (#25)
//!
//! Usage:
//!   maple login --url http://localhost:7788 --user admin --pass admin
//!   maple chat send "hello" --agent default
//!   maple workflow run wf-1
//!   maple trace <execution_id>
//!   maple status

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "maple", version, about = "MapleOS CLI client")]
struct Cli {
    /// Server URL (default: http://localhost:7788)
    #[arg(long, env = "MAPLE_URL", default_value = "http://localhost:7788")]
    url: String,

    /// Auth token (loaded from ~/.mapleos/token by default)
    #[arg(long, env = "MAPLE_TOKEN")]
    token: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Login and save token
    Login {
        #[arg(long)]
        user: String,
        #[arg(long)]
        pass: String,
    },
    /// Check server status
    Status,
    /// Chat commands
    Chat {
        #[command(subcommand)]
        action: ChatCommands,
    },
    /// Workflow commands
    Workflow {
        #[command(subcommand)]
        action: WorkflowCommands,
    },
    /// View execution trace
    Trace {
        /// Execution ID
        id: String,
    },
    /// Agent commands
    Agents {
        #[command(subcommand)]
        action: Option<AgentCommands>,
    },
    /// List models
    Models,
}

#[derive(Subcommand)]
enum ChatCommands {
    /// Send a message
    Send {
        /// Message content
        message: String,
        /// Agent ID (default: "default")
        #[arg(long, default_value = "default")]
        agent: String,
        /// Model (default: "auto")
        #[arg(long, default_value = "auto")]
        model: String,
    },
    /// List chat sessions
    Sessions,
}

#[derive(Subcommand)]
enum WorkflowCommands {
    /// List workflows
    List,
    /// Run a workflow
    Run {
        /// Workflow ID
        id: String,
        /// Input JSON (default: {})
        #[arg(long, default_value = "{}")]
        input: String,
    },
    /// List runs
    Runs {
        #[arg(long)]
        workflow_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    /// List registered agents
    List,
    /// Register a new agent
    Register {
        /// Agent name
        #[arg(long)]
        name: String,
        /// Optional agent description
        #[arg(long)]
        description: Option<String>,
        /// Optional model name, e.g. gpt-4
        #[arg(long)]
        model: Option<String>,
        /// Transport type for the agent
        #[arg(long, default_value = "websocket")]
        transport_type: String,
        /// Comma-separated capability names
        #[arg(long, value_delimiter = ',')]
        capability: Vec<String>,
        /// Comma-separated tag names
        #[arg(long, value_delimiter = ',')]
        tag: Vec<String>,
        /// Maximum concurrent tasks
        #[arg(long, default_value_t = 3)]
        max_concurrent_tasks: u32,
    },
}

#[derive(Serialize)]
#[allow(dead_code)]
struct LoginReq {
    username: String,
    password: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct LoginResp {
    token: Option<String>,
    user: Option<serde_json::Value>,
}

fn token_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".mapleos").join("token")
}

fn load_token() -> Option<String> {
    std::fs::read_to_string(token_path())
        .ok()
        .filter(|s| !s.trim().is_empty())
}

fn save_token(token: &str) -> anyhow::Result<()> {
    let path = token_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, token)?;
    Ok(())
}

async fn api_get(url: &str, token: &Option<String>) -> anyhow::Result<serde_json::Value> {
    let client = reqwest::Client::new();
    let mut req = client.get(url);
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {}", t));
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "HTTP {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }
    Ok(resp.json().await?)
}

async fn api_post(
    url: &str,
    token: &Option<String>,
    body: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let client = reqwest::Client::new();
    let mut req = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(body);
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {}", t));
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "HTTP {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }
    Ok(resp.json().await?)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let token = cli.token.or_else(load_token);

    match cli.command {
        Commands::Login { user, pass } => {
            let url = format!("{}/api/auth/login", cli.url);
            let body = serde_json::json!({"username": user, "password": pass});
            let resp: serde_json::Value = api_post(&url, &None, &body).await?;
            if let Some(t) = resp.get("token").and_then(|v| v.as_str()) {
                save_token(t)?;
                println!("✓ Logged in. Token saved to ~/.mapleos/token");
            } else {
                println!("✗ Login failed: {}", resp);
            }
        }

        Commands::Status => {
            let url = format!("{}/health", cli.url);
            let resp = api_get(&url, &token).await?;
            println!("Server: {}", cli.url);
            println!(
                "Status: {}",
                resp.get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
            );
            println!(
                "Version: {}",
                resp.get("version").and_then(|v| v.as_str()).unwrap_or("?")
            );
        }

        Commands::Chat { action } => match action {
            ChatCommands::Send {
                message,
                agent,
                model,
            } => {
                let url = format!("{}/api/chat", cli.url);
                let body =
                    serde_json::json!({"message": message, "agent_id": agent, "model": model});
                let resp = api_post(&url, &token, &body).await?;
                if let Some(content) = resp.get("content").and_then(|v| v.as_str()) {
                    println!("{}", content);
                } else {
                    println!("{}", serde_json::to_string_pretty(&resp)?);
                }
            }
            ChatCommands::Sessions => {
                let url = format!("{}/api/sessions", cli.url);
                let resp = api_get(&url, &token).await?;
                println!("{}", serde_json::to_string_pretty(&resp)?);
            }
        },

        Commands::Workflow { action } => match action {
            WorkflowCommands::List => {
                let url = format!("{}/api/v3/workflows", cli.url);
                let resp = api_get(&url, &token).await?;
                if let Some(workflows) = resp.get("workflows").and_then(|v| v.as_array()) {
                    for wf in workflows {
                        let id = wf.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                        let name = wf.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let ver = wf.get("version").and_then(|v| v.as_i64()).unwrap_or(0);
                        println!("  {} v{} — {}", id, ver, name);
                    }
                } else {
                    println!("{}", serde_json::to_string_pretty(&resp)?);
                }
            }
            WorkflowCommands::Run { id, input } => {
                let url = format!("{}/api/v3/workflow-runs", cli.url);
                let input_val: serde_json::Value =
                    serde_json::from_str(&input).unwrap_or(serde_json::json!({}));
                let body = serde_json::json!({"workflow_id": id, "workflow_version": 1, "input": input_val.to_string()});
                let resp = api_post(&url, &token, &body).await?;
                let run_id = resp
                    .get("run")
                    .and_then(|r| r.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let exec_id = resp
                    .get("execution_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("none");
                println!("✓ Run started: {} (execution: {})", run_id, exec_id);
                if exec_id != "none" {
                    println!("  Trace: maple trace {}", exec_id);
                }
            }
            WorkflowCommands::Runs { workflow_id } => {
                let url = if let Some(wid) = workflow_id {
                    format!("{}/api/v3/workflow-runs?workflow_id={}", cli.url, wid)
                } else {
                    format!("{}/api/v3/workflow-runs", cli.url)
                };
                let resp = api_get(&url, &token).await?;
                if let Some(runs) = resp.get("runs").and_then(|v| v.as_array()) {
                    for run in runs {
                        let id = run.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                        let status = run.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                        let wf = run
                            .get("workflow_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?");
                        println!("  {} [{}] — {}", id, status, wf);
                    }
                } else {
                    println!("{}", serde_json::to_string_pretty(&resp)?);
                }
            }
        },

        Commands::Trace { id } => {
            let url = format!("{}/api/v3/executions/{}/events", cli.url, id);
            let resp = api_get(&url, &token).await?;
            if let Some(events) = resp.get("events").and_then(|v| v.as_array()) {
                println!("Execution: {} ({} events)", id, events.len());
                println!("{}", "─".repeat(60));
                for evt in events {
                    let ts = evt.get("created_at").and_then(|v| v.as_i64()).unwrap_or(0);
                    let time = chrono::DateTime::from_timestamp(ts, 0)
                        .map(|d| d.format("%H:%M:%S").to_string())
                        .unwrap_or_else(|| "??:??:??".to_string());
                    let source = evt.get("source").and_then(|v| v.as_str()).unwrap_or("?");
                    let event_type = evt
                        .get("event_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    println!(
                        "  {} [{}] {} — {}",
                        time,
                        source,
                        event_type,
                        summarize_payload(evt)
                    );
                }
            } else {
                println!("No events found for execution {}", id);
            }
        }

        Commands::Agents { action } => match action.unwrap_or(AgentCommands::List) {
            AgentCommands::List => list_agents(&cli.url, &token).await?,
            AgentCommands::Register {
                name,
                description,
                model,
                transport_type,
                capability,
                tag,
                max_concurrent_tasks,
            } => {
                let mut body = serde_json::json!({
                    "name": name,
                    "transport_type": transport_type,
                    "max_concurrent_tasks": max_concurrent_tasks,
                });
                if let Some(description) = description.filter(|s| !s.trim().is_empty()) {
                    body["description"] = serde_json::Value::String(description);
                }
                if let Some(model) = model.filter(|s| !s.trim().is_empty()) {
                    body["model"] = serde_json::Value::String(model);
                }
                if !capability.is_empty() {
                    body["capabilities"] = serde_json::Value::Array(
                        capability
                            .into_iter()
                            .map(serde_json::Value::String)
                            .collect(),
                    );
                }
                if !tag.is_empty() {
                    body["tags"] = serde_json::Value::Array(
                        tag.into_iter().map(serde_json::Value::String).collect(),
                    );
                }

                let url = format!("{}/api/agents", cli.url);
                let resp = api_post(&url, &token, &body).await?;
                let id = resp.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                let name = resp.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                println!("✓ Agent registered: {} — {}", id, name);
            }
        },

        Commands::Models => {
            let url = format!("{}/api/models", cli.url);
            let resp = api_get(&url, &token).await?;
            if let Some(models) = resp.get("models").and_then(|v| v.as_array()) {
                for m in models {
                    let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                    let provider = m.get("provider").and_then(|v| v.as_str()).unwrap_or("?");
                    let is_local = m.get("is_local").and_then(|v| v.as_bool()).unwrap_or(false);
                    let local_tag = if is_local { " (local)" } else { "" };
                    println!("  {} [{}]{}", id, provider, local_tag);
                }
            } else {
                println!("{}", serde_json::to_string_pretty(&resp)?);
            }
        }
    }

    Ok(())
}

async fn list_agents(url: &str, token: &Option<String>) -> anyhow::Result<()> {
    let url = format!("{}/api/agents", url);
    let resp = api_get(&url, token).await?;
    if let Some(agents) = resp.get("agents").and_then(|v| v.as_array()) {
        for agent in agents {
            let id = agent.get("id").and_then(|v| v.as_str()).unwrap_or("?");
            let name = agent.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let status = agent.get("status").and_then(|v| v.as_str()).unwrap_or("?");
            println!("  {} [{}] — {}", id, status, name);
        }
    } else {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    }
    Ok(())
}

fn summarize_payload(evt: &serde_json::Value) -> String {
    let p = evt.get("payload").unwrap_or(&serde_json::Value::Null);
    let event_type = evt.get("event_type").and_then(|v| v.as_str()).unwrap_or("");
    match event_type {
        "started" => format!(
            "entry={}",
            p.get("entry").and_then(|v| v.as_str()).unwrap_or("?")
        ),
        "delta" => p
            .get("token")
            .and_then(|v| v.as_str())
            .map(|t| format!("token=\"{}\"", t.chars().take(30).collect::<String>()))
            .unwrap_or_default(),
        "tool_call" => format!(
            "{}({})",
            p.get("tool_name").and_then(|v| v.as_str()).unwrap_or("?"),
            p.get("input")
                .map(|v| v.to_string())
                .unwrap_or_default()
                .chars()
                .take(40)
                .collect::<String>()
        ),
        "tool_result" => format!(
            "inv={}",
            p.get("invocation_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
        ),
        "done" => p
            .get("output_summary")
            .and_then(|v| v.as_str())
            .map(|s| s.chars().take(50).collect::<String>())
            .unwrap_or_default(),
        "error" => p
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string(),
        _ => p.to_string().chars().take(60).collect::<String>(),
    }
}
