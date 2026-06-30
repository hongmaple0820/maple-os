# MapleOS

<p align="center">
  <img src="image/mapleos-logo.png" width="120" height="120" alt="MapleOS Logo" />
</p>

<p align="center">
  <strong>AI Native Multi-Agent Collaborative Workstation Operating System</strong>
</p>

<p align="center">
  <em>Human + Agent + Workflow + Knowledge + Tools</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-2.1.0-orange?style=flat-square" alt="version" />
  <img src="https://img.shields.io/badge/rust-1.95-blue?style=flat-square" alt="rust" />
  <img src="https://img.shields.io/badge/node-24-blue?style=flat-square" alt="node" />
  <img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="license" />
  <a href="https://scale-os.hongmaple.top/"><img src="https://img.shields.io/badge/website-scale--os.hongmaple.top-blue?style=flat-square" alt="website" /></a>
</p>

<p align="center">
  <a href="https://repostars.dev/?repos=hongmaple0820%2Fmaple-os&theme=copper"><img src="https://repostars.dev/api/embed?repo=hongmaple0820%2Fmaple-os&theme=copper" alt="RepoStars" /></a>
</p>

<p align="center">
  <a href="https://scale-os.hongmaple.top/">Website</a> · <a href="./.monkeycode/docs/product-design-blueprint.md">Design Blueprint</a> · <a href="./README.md">中文</a> · <a href="#community">Community</a> · <a href="#contributing">Contributing</a>
</p>

---

## What is MapleOS

MapleOS is not yet another AI chatbot, IDE, or automation tool.

It is an **AI Operating System** — a next-generation intelligent workstation for individuals and teams:

- **Multi-Agent Collaboration Hub** — Not a single AI, but an AI Team
- **Workflow Orchestration** — Prompt + Tools + Logic fusion system
- **Local-first** — Privacy and offline capability, your data belongs to you
- **Self-Evolving** — The system gets smarter over time, accumulating long-term assets
- **Rust Runtime** — Performance-grade Agent OS

> Truly realizing Human + AI Team collaborative operating system.

---

## Core Features

| Feature | Description |
|---|---|
| Workflow Engine | DAG scheduling / concurrent execution / event-driven / state recovery / failure retry |
| Multi-Agent | Registration / dispatch / team orchestration / real-time status / messaging |
| LLM Router | Dynamic model switching based on cost / latency / privacy / reasoning complexity |
| Knowledge Base | Hybrid retrieval (BM25 + Embedding) / auto-indexing / memory / self-evolution |
| SCALE Engine | Spec/Plan/Task/Defect lifecycle / FSM state machine / governance gates |
| Local-first Sync | CRDT / WebDAV / offline-capable / auto conflict resolution |
| Plugin Ecosystem | Skills / MCP / CLI tools / browser control / code sandbox |
| Security Gateway | Tool call interception / sensitive operation detection / role permissions / brute-force detection |

---

## Architecture

```
L1 Interface:  Web (Next.js 15) / Desktop (Tauri 2) / Mobile (React Native) / CLI
L2 Collaboration: Workspace / Permissions / Task Dispatch / Realtime Sync
L3 Orchestration: Workflow Engine / Event Bus / Agent Orchestrator / Hooks / SCALE Engine
L4 Capabilities: Skills / MCP / Browser / Code Exec / Webhooks / Plugin SDK
L5 Intelligence: LLM Router / Prompt Mgmt / Vector KB (BM25+Embedding) / Self-Evolution
L6 Storage: SQLite / PostgreSQL / Qdrant / Automerge CRDT / WebDAV
```

## Project Structure

```
mapleos/
 ├── core/               # Rust core engine (Cargo workspace)
 │   ├── maple-engine/   # Workflow engine + task queue
 │   ├── maple-llm/      # LLM routing layer + embedding
 │   ├── maple-agent/    # Agent management + React Loop
 │   ├── maple-kb/       # Knowledge base (BM25 + Vector + Memory)
 │   ├── maple-sync/     # Sync engine (CRDT + WebDAV)
 │   ├── maple-gateway/  # Agent gateway (WS/SSE/RPC)
 │   ├── maple-collab/   # Collaboration layer
 │   ├── maple-rpc/      # JSON-RPC 2.0 service
 │   ├── maple-macro/    # Proc macros (#[tool] derive macro)
 │   └── scale-engine/   # SCALE governance engine (Node.js submodule)
 ├── server/             # Rust Axum backend service
 ├── apps/
 │   ├── web/            # Next.js 15 web application
 │   ├── desktop/        # Tauri 2 desktop client
 │   └── mobile/         # React Native mobile app
 ├── packages/
 │   ├── ui/             # shadcn/ui component library (React)
 │   ├── sdk/            # MapleOS TypeScript SDK
 │   └── config/         # Shared config
 ├── plugins/            # Built-in plugins
 ├── infra/              # Docker deployment configs
 └── .monkeycode/        # Project docs / blueprint / memory
```

---

## v2.1.0 — Product Closure

### Unified Execution Fact Chain (#92)
All Chat/Workflow/Agent/Approval events write to a single `execution_events` table. The `<ExecutionTimeline />` component renders the unified trace across all UI panels.

### Workflow Canvas Real Editor (#90)
Node CRUD + edge linking + 8-invariant validation + version management (list/get/rollback) + failed node retry/skip/deadletter + approval UI + trace view.

### LLM Config Fix (#86)
ModelDescriptor replaces bare String. Ollama auto-discovery via `/v1/models`. Test connection endpoint. API key masking.

### Learning Governance (#91)
Candidate pipeline + quality gate (score≥0.7 + evidence required) + blocklist (SHA-256 content hash) + revoke + context preview provenance badge.

### Event/Message Triggers (#15, #16)
TriggerManager: EventTrigger (EventBus event matching) + MessageTrigger (keyword/sender/group filter).

### Tool Hardening + Browser Automation (#10)
http_request SSRF guard + file_ops write approval gate + code_execute permission levels + browser skill (6 actions, puppeteer + HTTP fallback).

### Other Features
- Audit logs (#18) — DB persistence + API query
- Agent load balancing (#19) — by active task count
- Skill Schema validation (#11) — JSON Schema input/output
- Rerank (#14) — LLM-based reranker
- Automerge CRDT (#70) — replaces custom merge
- maple CLI (#25) — login/status/chat/workflow/trace/agents/models
- Frontend modularization (#93) — DashboardView + StatePanel
- Workflow/Skill templates (#23)
- Desktop auto-update (#65)

---

## Quick Start

### Prerequisites

- Rust 1.95+ (edition 2024)
- Node.js 24+ & pnpm 11+
- Ollama (optional, for local LLM)

### Build & Run

```bash
# Clone with submodules
git clone --recurse-submodules https://github.com/hongmaple0820/maple-os.git

# Rust backend
cargo run --release -p mapleos-server

# Install frontend dependencies
pnpm install

# Frontend web app
pnpm --filter=mapleos-web dev

# Tauri desktop app (optional)
pnpm desktop:build
```

### Docker Deployment

```bash
docker compose -f infra/docker/docker-compose.yml --profile allinone up -d
```

### LLM Routing Configuration

Edit `infra/routing_rules.yaml`:

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

---

## Developer Docs

- [`#[tool]` Macro](./docs/tool-macro.md) — Declarative tool definition with auto-generated JSON Schema
- [Competitive Analysis](./docs/competitive-analysis.md) — Deep competitor comparison & best practices
- [Unified Implementation Plan](./docs/unified-implementation-plan.md) — Architecture upgrade roadmap
 
- [Product Closure Roadmap](./docs/MapleOS_Product_Closure_Roadmap.md) — Current closure status, remaining gaps, and linked GitHub issues
- [Open-Source Co-build Backlog](./docs/MapleOS_Open_Source_Cobuild_Backlog.md) — Community implementation tracks for the remaining upgrades

---

## Design Blueprint

The complete Product + Design + Engineering unified blueprint is at [.monkeycode/docs/product-design-blueprint.md](./.monkeycode/docs/product-design-blueprint.md), covering:

- Figma Design System (colors / typography / components / tokens)
- Core product modules (Dashboard / Workflow Editor / Agent Center / Knowledge)
- Engineering architecture (Rust Runtime / LLM Router / CRDT Sync / Plugin SDK)
- Development roadmap (Phase 1-3)
- Developer Handoff specs

---

## Roadmap

### Phase 1 — Foundation (Current)

- Rust core engine: Workflow / Agent / LLM Router / Knowledge / Task Queue
- Web frontend: Dashboard / Chat / Workflow Editor / Agent Center
- SCALE Engine governance integration
- SQLite + BM25 + Vector hybrid retrieval

### Phase 2 — Collaboration & Evolution

- Multi-Agent panel collaboration / team orchestration
- Knowledge self-evolution / memory accumulation
- Workflow visual Canvas editor
- WebDAV / Automerge CRDT sync
- Tauri 2 desktop client

### Phase 3 — Ecosystem Platform

- Plugin marketplace / MCP open registration
- Enterprise features / team management / multi-tenant
- Agent marketplace / SaaS platform
- Private deployment solutions

---

## Why Open Source

MapleOS believes: **An AI workstation should be open, accumulative, and owned by every developer.**

We chose open source because:

1. **Transparent Trust** — Agent systems directly impact your workflow; you need to see what it does
2. **Local-first** — Data and privacy sovereignty belongs to users; open source is the only way
3. **Ecosystem** — Skills / MCP / plugins need community collaboration; one person can't build everything
4. **Long-term** — Knowledge and memory accumulation needs a stable, commercially-agnostic carrier

> If you believe AI should be a workstation operating system, not a chatbox appendage, join us.

---

## Contributing

We welcome all forms of contribution!

### Getting Started

1. Fork & Clone the project
2. Pick a [Good First Issue](https://github.com/hongmaple0820/maple-os/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
3. Submit a PR — we review within 24 hours

### Contribution Areas

| Area | Skills | Examples |
|---|---|---|
| Rust Core Engine | Rust / Tokio / Axum / SQLite | Workflow node types, Agent protocols, task queue |
| Web Frontend | React / Next.js / Tailwind | New pages, component optimization, interaction details |
| TypeScript SDK | TypeScript / JSON-RPC | SDK interfaces, type definitions, tests |
| SCALE Engine | Node.js / TypeScript / MCP | New detectors, workflow presets, knowledge modules |
| Plugin Development | Any language / MCP protocol | web_search, PDF processing, browser control |
| Docs & Design | Markdown / Figma | API docs, design specs, tutorials |
| Testing & CI | Vitest / Playwright / GitHub Actions | Unit tests, integration tests, E2E |

### Commit Convention

```
feat: new feature
fix: bug fix
refactor: refactoring
docs: documentation
chore: build/CI/dependencies
```

### Development Flow

```bash
# 1. Create branch
git checkout -b feat/your-feature

# 2. Develop & test
cargo test && pnpm build

# 3. Submit PR
git push origin feat/your-feature
# Create Pull Request on GitHub
```

---

## Community

<p align="center">
  <strong>Join the MapleOS open-source community, build the future of AI workstations together</strong>
</p>

| Channel | Address | Description |
|---|---|---|
| Website | [scale-os.hongmaple.top](https://scale-os.hongmaple.top/) | Product intro, online demo |
| QQ Group | **628043364** | Developer community, tech discussion, bug reports |
| WeChat Official Account | **鸿枫技术栈** | Tech sharing, release announcements, community events |
| WeChat ID | **mapleCx330** | Group entry — add as friend to join developer group |
| Feishu Group | Scan to join | Enterprise collaboration, in-depth tech exchange |
| Email | [2496155694@qq.com](mailto:2496155694@qq.com) | Bug reports, collaboration inquiries, security issues |
| GitHub Issues | [maple-os/issues](https://github.com/hongmaple0820/maple-os/issues) | Bug reports, feature requests, technical discussions |
| GitHub Discussions | [maple-os/discussions](https://github.com/hongmaple0820/maple-os/discussions) | Open discussions, Q&A, idea exchange |
| SCALE Engine | [scale-engine](https://github.com/hongmaple0820/scale-engine) | AI Agent governance engine (core dependency) |

> Follow the WeChat official account for latest release announcements and in-depth technical articles. Core authors provide real-time support in the developer group.

<p align="center">
  <img src="./image/wechat-public.jpg" alt="WeChat Official Account" width="200">
  <br>
  <strong>Follow WeChat Official Account</strong>
</p>

<p align="center">
  <img src="./image/wechat-id-qr.webp" alt="WeChat Group" width="200">
  <img src="./image/feishu-group-qr.webp" alt="Feishu Group" width="200">
  <br>
  <strong>WeChat Group</strong> &nbsp;&nbsp;&nbsp; <strong>Feishu Group</strong>
</p>

---

## Sponsor

If MapleOS is helpful to you, feel free to buy the author a coffee ☕

<p align="center">
  <img src="./image/wxPay.jpg" alt="WeChat Pay" width="200">
  <img src="./image/zfb.jpg" alt="Alipay" width="200">
  <br>
  <strong>WeChat Pay</strong> &nbsp;&nbsp;&nbsp; <strong>Alipay</strong>
</p>

---

## Tech Stack

| Layer | Tech | Why |
|---|---|---|
| Desktop | Tauri 2 | Cross-platform native desktop, Rust backend |
| Web | Next.js 15 | App Router + SSE + reverse proxy |
| Backend | Rust Axum | High performance, low latency, safe |
| Runtime | Tokio | Async runtime |
| Workflow | Petgraph | DAG scheduling |
| Database | SQLite | Local-first, zero dependencies |
| Vector DB | Qdrant (optional) | High-performance vector retrieval |
| Sync | Automerge CRDT | Offline sync, auto conflict resolution |
| AI Runtime | Ollama | Local LLM privacy inference |
| Governance | SCALE Engine | Agent governance + FSM + gates |

---

## Competitive Moat

1. **Self-Evolution** — System gets smarter over time: memory + rules + behavior tracking + lesson accumulation
2. **Multi-Agent** — Not a single AI, but an AI Team collaborative OS
3. **Local-first** — Privacy sovereignty, offline capability, data always belongs to users
4. **Rust Runtime** — Performance-grade Agent OS, not a toy
5. **Workflow + Knowledge** — Long-term asset accumulation, not temporary conversations

---

## Acknowledgments

- [SCALE Engine](https://github.com/hongmaple0820/scale-engine) — AI Agent governance engine
- [shadcn/ui](https://ui.shadcn.com/) — React component design system
- [Vercel AI Chatbot](https://github.com/vercel/ai-chatbot) — Chat UI reference architecture
- [Linear](https://linear.app/) / [Raycast](https://raycast.com/) — Product visual style reference
- [Automerge](https://automerge.org/) — CRDT sync engine
- [Qdrant](https://qdrant.tech/) — Vector database
- [Tauri](https://tauri.app/) — Desktop application framework

---

## License

MIT — Free to use, modify, and distribute. We believe an AI workstation should be open.
