# Contributing to MapleOS

Thank you for your interest in contributing to MapleOS! This document covers everything you need to get started.

## Quick Start

```bash
# Clone
git clone https://github.com/hongmaple0820/maple-os.git
cd maple-os

# Install Rust
curl -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable

# Install Node.js + pnpm
npm install -g pnpm

# Install dependencies
pnpm install --no-frozen-lockfile

# Build
cargo check --workspace
pnpm --filter mapleos-web build

# Run tests
cargo test --workspace --lib
pnpm test:e2e:product

# Start dev server
pnpm dev  # Starts web frontend on :3000
cargo run -p mapleos-server  # Starts backend on :7788
```

## Development Workflow

### 1. Pick an Issue

Browse [open issues](https://github.com/hongmaple0820/maple-os/issues) or create a new one describing what you want to work on.

### 2. Create a Branch

```bash
git checkout -b feat/your-feature-name
```

### 3. Make Changes

Follow the coding standards below. Every PR must include:

- **User entry**: Where does the user start?
- **Runtime path**: Key files/functions touched
- **Persistence path**: What tables/files get written?
- **Error path**: What does the user see on failure?
- **Validation evidence**: Test command, screenshot, or trace id

### 4. Test Locally

```bash
# Rust
cargo check --workspace --all-targets
cargo test --workspace --lib
cargo clippy --all-targets -- -D warnings

# Frontend
pnpm --filter mapleos-web typecheck
pnpm --filter mapleos-web build

# E2E
pnpm test:e2e:product
```

### 5. Submit a PR

Use the [PR template](.github/PULL_REQUEST_TEMPLATE.md) and fill in all sections.

## CI Gates

All PRs must pass 4 CI jobs:

| Job | What it checks |
|-----|----------------|
| `rust-check` | cargo check + cargo test --lib + cargo clippy -D warnings |
| `frontend-check` | pnpm typecheck + pnpm build |
| `e2e-product-gate` | Playwright E2E (11 tests) |
| `docker-build` | Docker image builds successfully |

## Architecture Overview

```
┌─────────────────────────────────────────────────┐
│                   Web App (Next.js)              │
│  Dashboard / Chat / Workflow / Agents / KB /     │
│  Settings / Collaboration / Plugins               │
└──────────────────────┬──────────────────────────┘
                       │ HTTP / SSE / WebSocket
┌──────────────────────┴──────────────────────────┐
│                  Rust Server (Axum)               │
│  ┌─────────┐ ┌──────────┐ ┌──────────────────┐  │
│  │ Chat    │ │ Workflow │ │ Execution Fact    │  │
│  │ Handler │ │ Engine   │ │ Chain (Recorder)  │  │
│  └─────────┘ └──────────┘ └──────────────────┘  │
│  ┌─────────┐ ┌──────────┐ ┌──────────────────┐  │
│  │ Agent   │ │ Learning │ │ Trigger Manager   │  │
│  │ React   │ │ Govern.  │ │ (Event/Message)  │  │
│  │ Loop    │ │ Service  │ │                  │  │
│  └─────────┘ └──────────┘ └──────────────────┘  │
└──────────────────────┬──────────────────────────┘
                       │ SQLite
┌──────────────────────┴──────────────────────────┐
│              Database (20 migrations)             │
│  workflows / workflow_runs / execution_events /  │
│  tool_invocations / learning_candidates /        │
│  audit_logs / workflow_triggers / agents / ...   │
└─────────────────────────────────────────────────┘
```

### Key Concepts

- **Execution Fact Chain**: All Chat/Workflow/Agent/Approval events write to `execution_events` table. UI reads from one source via `<ExecutionTimeline />`.
- **Learning Governance**: Self-learning goes through candidate pipeline with quality gate (score ≥ 0.7 + evidence required). Rejected content is blocklisted.
- **Trigger Manager**: Workflows can be triggered by EventBus events or group messages (keyword/sender/group match).

### Crate Structure

| Crate | Purpose |
|-------|---------|
| `maple-engine` | Workflow engine, execution chain, triggers, scheduler |
| `maple-llm` | LLM router, adapters (OpenAI/Anthropic/Ollama), ModelDescriptor |
| `maple-agent` | React loop, agent registry, tool execution |
| `maple-kb` | Knowledge base, retriever (hybrid + reranker), learning governance |
| `maple-sync` | WebDAV sync, Automerge CRDT |
| `maple-gateway` | Auth, MCP host, channel adapters |
| `maple-collab` | Groups, DM, group rules, cron |
| `mapleos-server` | Axum HTTP server, handlers, middleware |
| `maple-cli` | CLI client (`maple` command) |

## Coding Standards

### Rust

- Zero compiler warnings (`cargo check --all-targets`)
- Zero clippy errors (`cargo clippy --all-targets -- -D warnings`)
- All public functions documented with `///`
- Tests for new logic (aim for >80% coverage on new code)

### TypeScript / React

- `pnpm --filter mapleos-web typecheck` must pass
- Use `StatePanel` for loading/empty/error/disabled states
- Use `useTranslation` for all user-facing strings
- Mock/disabled features must be clearly labeled

### Database

- New tables go in `migrations/NNN_description.sql` + `db.rs::run_v3_migration_NNN()`
- Use `CREATE TABLE IF NOT EXISTS` for idempotency
- Add indexes for all foreign-key-like columns

## Reporting Bugs

Use [GitHub Issues](https://github.com/hongmaple0820/maple-os/issues/new) with:

1. What happened (expected vs actual)
2. Steps to reproduce
3. Server logs (`RUST_LOG=debug`)
4. Execution ID (if applicable) — view via `maple trace <id>`

## License

MIT — see [LICENSE](LICENSE) file.
