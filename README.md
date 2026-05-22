# MapleOS

Multi-Agent Collaborative Workstation Operating System.

## Architecture

```
L1 Interface:  Web Chat UI / Group Collaboration / CLI / Bot / Mobile
L2 Collaboration: Workspace / Permissions / Task Dispatch / Realtime Sync
L3 Orchestration: Workflow Engine / Event Bus / Agent Orchestrator / Scheduler / Hooks
L4 Capabilities: Skills / MCP / CLI Tools / Webhook / Code Exec / Browser
L5 Intelligence: LLM Router / Prompt Mgmt / Vector KB / Self-Evolution
L6 Storage: SQLite / WebDAV / Qdrant / Object Store
```

## Project Structure

```
mapleos/
├── core/               # Rust core engine (Cargo workspace)
│   ├── maple-engine/   # Workflow engine
│   ├── maple-llm/      # LLM routing layer
│   ├── maple-agent/    # Agent management
│   ├── maple-kb/       # Knowledge base
│   ├── maple-sync/     # Sync engine (Local-first + CRDT)
│   ├── maple-gateway/  # Agent access gateway (WS/Webhook/MCP)
│   ├── maple-collab/   # Collaboration layer (FMP protocol)
│   └── maple-rpc/      # JSON-RPC 2.0 service
├── server/             # Cloud service (Rust Axum)
├── apps/
│   ├── desktop/        # Tauri desktop (macOS/Win/Linux)
│   ├── web/            # Next.js web app
│   └── mobile/         # React Native (iOS/Android)
├── packages/
│   ├── ui/             # Shared UI components (shadcn/ui)
│   ├── sdk/            # MapleOS JS/TS SDK
│   └── config/         # Shared config
├── plugins/            # Built-in plugins (skills, MCP servers, channels)
├── migrations/         # SQLite schema migrations
└── infra/              # Docker deployment configs
```

## Quick Start

### Prerequisites

- Rust 1.80+ (`rustup`)
- Node.js 20+ & pnpm 9+
- Ollama (optional, for local LLM)

### Build

```bash
# Rust core
cargo check

# Start server
cargo run -p mapleos-server

# Docker deployment
docker compose -f infra/docker/docker-compose.yml up
```

### Configuration

LLM routing rules in `routing_rules.yaml`:

```yaml
rules:
  - name: code_generation
    condition: "task.type == 'code_generation'"
    preferred: ["claude-3-5-sonnet", "deepseek-coder-v2"]
  - name: sensitive_data
    condition: "task.privacy_level == 'sensitive'"
    preferred: ["ollama/qwen2.5:7b"]
    fallback_to_cloud: false
```

## License

MIT
