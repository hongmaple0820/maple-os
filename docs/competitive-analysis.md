# MapleOS Competitive Analysis Report

> Date: 2026-05-27 | Projects analyzed: hermes-agent, golutra, cc-haha, rig, claw-code/rust

---

## 1. hermes-agent (Python, CLI-first AI Agent — "Self-Improving General Agent")

### Architecture
- **Scale**: Production-grade, ~17k tests across ~900 files. `gateway/run.py` 894KB, `cli.py` 700KB, `conversation_loop.py` 245KB, `auxiliary_client.py` 250KB
- **Core**: `AIAgent` class (~60 init params) with conversation loop, tool dispatch, provider abstraction
- **Provider System**: Declarative `ProviderProfile` dataclass — auth, endpoints, quirks all in one place. 20+ providers (OpenAI, Anthropic, OpenRouter, Kimi, GMI Cloud, Bedrock, Azure, Google, etc.)
- **Tool System**: Self-registering registry with AST-based discovery (avoids importing 80+ modules). 80+ tool implementations. Check function TTL cache (30s). Override protection
- **Toolset System**: Named groups composing from other groups. Core toolset (~50 tools always available). Webhook-safe subset for untrusted sources
- **Subagent Delegation**: `delegate_task` spawns child AIAgent in ThreadPoolExecutor. Fresh conversation, restricted toolsets, focused prompt. Blocked tools: delegate_task, clarify, memory, send_message, execute_code. Depth cap 1-3
- **Mixture-of-Agents**: Multiple frontier models (Claude, Gemini, GPT, DeepSeek) generate responses in parallel, aggregator synthesizes (arXiv:2406.04692v1)
- **Skills**: YAML-defined, auto-discovered, slash-command invocable. Lifecycle: active → stale → archived. Curator background agent reviews/maintains
- **Context Management**: `ContextCompressor` — token-budget compression protecting head/tail, structured summary with Resolved/Pending tracking, iterative updates, tool output pruning, scaled summary budget
- **Streaming Context Scrubber**: State machine stripping memory-context fences from streaming output chunk-by-chunk, handling partial tag boundaries
- **Error Classifier**: 22 `FailoverReason` categories with priority-ordered classification. Drives retry/rotate/fallback/compress/abort decisions
- **IterationBudget**: Token spend tracking with one-turn grace call so agent can complete current thought
- **MCP Integration**: Full MCP client (stdio, HTTP, SSE). Auto-reconnection, sampling support, parallel tool calls, credential stripping
- **State**: SQLite with FTS5, WAL mode, schema v13, compression-triggered session splitting. Cross-session recall via FTS5 search
- **Memory Providers**: Abstract `MemoryProvider` with lifecycle hooks (initialize/prefetch/sync/shutdown). Honcho dialectic memory with multi-pass reasoning (1-3 depth levels)
- **Platform Adapters**: 20+ messaging platforms (Telegram, Discord, Slack, WhatsApp, Signal, Matrix, DingTalk, WeCom, WeChat, Email, SMS, Home Assistant, etc.)
- **Terminal Backends**: 7 backends — Local, Docker, SSH, Singularity, Modal, Daytona, Vercel Sandbox. Cloud sandboxes hibernate when idle
- **Cron Scheduler**: Natural-language task definitions, delivery to any platform. Prompt injection scanning on assembled prompts
- **ACP Adapter**: IDE integration (VS Code, Zed, JetBrains) via Agent Client Protocol
- **Trajectory Compression**: Post-processes agent trajectories for training data, preserving signal quality within token budgets
- **Supply-chain Security**: Exact-pinned deps, lazy-installed optional deps, response to "Mini Shai-Hulud" worm incident

### Key Innovations to Borrow
1. **ProviderProfile pattern** — declarative provider config with quirks/hooks. MapleOS should adopt this for `maple-llm`
2. **Context Compressor** — token-budget compression with head/tail protection and structured summaries. Far superior to simple truncation
3. **Self-registering tool registry with AST discovery** — tools self-register at import, AST scan finds them without importing all modules. MapleOS tool system should adopt this
4. **Toolset composition** — group tools into named sets, compose from other sets. Webhook-safe subset for untrusted sources
5. **Error classifier** — 22 structured failover reasons driving recovery decisions. MapleOS needs this for robust LLM integration
6. **IterationBudget** — token spend tracking with grace call. Prevents runaway costs while allowing graceful completion
7. **Streaming Context Scrubber** — state machine for stripping injected context from streaming output. Non-trivial engineering
8. **Curator agent** — background skill maintenance with lifecycle states. MapleOS Evolver is similar but less mature
9. **Mixture-of-Agents** — parallel multi-model responses with aggregator synthesis. Unique reasoning approach
10. **Subagent delegation** — isolated context, restricted tools, approval callbacks, depth caps. MapleOS has Agent node but lacks runtime delegation
11. **Threat pattern scanning** — context files + cron prompts scanned for injection before execution
12. **Memory provider lifecycle** — initialize/prefetch/sync/shutdown hooks. Clean abstraction for pluggable memory
13. **Trajectory compression for training** — enables using agent interactions as training data for next-gen models
14. **Platform adapter pattern** — 20+ platforms via clean adapter interface with session persistence across platforms

### Gaps (MapleOS advantage)
- No web UI (CLI-only) — MapleOS has full web/mobile/desktop
- No workflow DAG — MapleOS has visual workflow engine with node types
- No knowledge base — MapleOS has maple-kb with vector search and Evolver
- No real-time collaboration — MapleOS has kanban, comments, team features
- Python (slower) — MapleOS is Rust (faster, lower memory)
- God objects (AIAgent ~60 params, 700KB+ files) — MapleOS has cleaner module boundaries
- Sync/async mismatch — complex bridging code. MapleOS is natively async (tokio)
- No type checking in CI — ruff only, ty rules minimal

---

## 2. golutra (Rust, Tauri Desktop App — "One Person. One AI Squad")

### Architecture
- **Core**: Tauri 2.x desktop app with hexagonal architecture (Rust backend + Vue 3 frontend)
- **Hexagonal Layers**: `ui_gateway` → `application` → `orchestration` → `terminal_engine` + `message_service`, with `ports/` for dependency inversion
- **Multi-Agent Orchestration**: Wraps external AI CLIs (Claude Code, Gemini CLI, Codex, etc.) as workspace "members" running in separate PTY sessions
- **Chat-Terminal Fusion**: Terminal output captured, filtered, and persisted as first-class chat messages via `SemanticWorker` thread per session
- **State**: `redb` embedded key-value store for chat persistence, `project_data` JSON for workspace config
- **Binaries**: Main app + `shim` (PTY wrapper with OSC signals) + `golutra-cli` (IPC client)

### Key Innovations to Borrow
1. **Terminal-as-Chat-Member** — terminal output transformed into searchable chat messages. Each session has a dedicated `SemanticWorker` with its own virtual terminal emulator. MapleOS could adopt this for agent output capture
2. **Semantic-Gated Dispatch Batching** — `ChatDispatchBatcher` + `TerminalDispatchGate` ensures next message only sent after current response fully processed. Prevents context corruption from overlapping requests. MapleOS agent dispatch needs this
3. **Shim-Based Ready Detection** — lightweight wrapper emits OSC `633;A` (ready) and `633;D;{exit_code}` (exit). Decouples ready detection from CLI implementation. MapleOS agent runtime should use this pattern
4. **Post-Ready Step Queue** — configurable startup sequence per CLI type: Input → ExtractSessionId → WaitForPattern → Introduction. With auto-restart on timeout. MapleOS agent initialization should adopt this
5. **Outbox Pattern** — lease-based claiming with exponential backoff (max 6 attempts, 280ms poll). Reliable message delivery without coupling. MapleOS task dispatch could use this
6. **ACK-Based Flow Control** — high/low watermarks (200KB/20KB) for PTY output backpressure. 16ms emission interval (~60fps). Production-grade terminal handling
7. **Trigger-Based Scheduling** — `TriggerBus` + `TriggerScheduler` with priority queue, deduplication by key, deferred stages (Stable, Silence, Debounce, PostReadyTick). Replaces polling with event-driven evaluation
8. **Filter Pipeline** — CLI-specific filter profiles with three decisions (Allow/Drop/Defer). `prompt_block.rs` parses CLI output formats to extract meaningful content
9. **Dispatch Queue** — max 32 items per terminal, batch merging for same-sender consecutive messages, duplicate detection across inflight/queue/recent (128 window)
10. **CLI Compatibility Layer** — uniform abstraction for any CLI tool with `TerminalDefaultMemberConfig`: id, type, command, unlimited_access_flag, resume_command_template, post_ready_plan

### Gaps (MapleOS advantage)
- No web version — MapleOS has Next.js web + Expo mobile
- No direct LLM integration — golutra wraps CLIs, MapleOS calls LLM APIs directly
- No knowledge base — MapleOS has maple-kb with vector search and Evolver
- No workflow DAG — MapleOS has visual workflow engine with node types
- No test coverage — significant risk for concurrent system
- Thread-per-session model (4+ threads each) — resource-intensive at scale
- Mutex-heavy concurrency (`Arc<Mutex<>>` on hot paths) — contention risk
- No persistent terminal state — crash loses all sessions (only remote_session_id survives)

---

## 3. cc-haha (TypeScript/Bun, Desktop Workbench for Claude Code)

### Architecture
- **Scale**: 100+ source directories, 50+ tools, 80+ slash commands. Runs on Bun (not Node.js)
- **Surfaces**: CLI (React+Ink TUI), Desktop (Tauri 2 + React+Vite), IM adapters (Telegram/Feishu/WeChat/DingTalk), local HTTP+WebSocket server (port 3456)
- **Protocol-Translating Proxy**: Intercepts Anthropic Messages API calls at server level, transparently converts to/from OpenAI Chat Completions or Responses API. The agent loop always thinks it's talking to Anthropic
- **Provider System**: `ProviderService` manages provider configs in `~/.claude/cc-haha/providers.json`. Supports Anthropic, OpenAI Chat, OpenAI Responses formats
- **Multi-Agent**: Coordinator mode (orchestrator + worker agents), Swarm/Teammate system (tmux/iTerm2/in-process backends), mailbox-based inter-agent communication
- **Background Tasks**: InProcessTeammate, LocalShell, LocalWorkflow, Dream, MonitorMcp task types
- **Skills**: Dynamic discovery with conditional activation based on file paths (lazy loading)
- **Session**: JSONL transcript files under `~/.claude/projects/`, resume support
- **Memory**: Cross-session persistent memory via memory files, team memory sync, nested CLAUDE.md attachments
- **Streaming**: `StreamingToolExecutor` with concurrency control — concurrent-safe tools run in parallel, exclusive tools get exclusive access. Results buffered and emitted in order

### Key Innovations to Borrow
1. **Protocol-translating proxy** — transparently converts Anthropic API to/from OpenAI format at server level. MapleOS could use this to support any Claude Code-compatible client
2. **StreamingToolExecutor with concurrency control** — concurrent-safe tools run in parallel while exclusive tools get exclusive access. MapleOS tool execution should adopt this
3. **AsyncGenerator-based query loop** — yields `StreamEvent | Message` objects incrementally. Clean pattern for SSE streaming
4. **ToolUseContext dependency injection** — rich context object (options, abort controller, app state, file state cache, permission context, MCP clients, agent definitions, callbacks) threaded through all tool executions. MapleOS should standardize this
5. **In-process multi-agent** — `InProcessBackend` using AsyncLocalStorage avoids process spawning overhead. MapleOS could use this for lightweight sub-agents
6. **Mailbox-based inter-agent communication** — team agents communicate via file-based mailbox pattern. MapleOS agent communication could adopt this
7. **Dynamic skill discovery** — conditional skills activated only when matching files are operated on. Novel lazy-loading pattern
8. **IM Adapter Architecture** — shared WebSocket bridge (`ws-bridge.ts`) with auto-reconnection, heartbeat, per-chat FIFO queuing. MapleOS platform adapters should study this
9. **H5 Remote Access** — one-time-token mobile access to desktop session with CORS origin allowlisting
10. **Stub pattern for internal features** — auto-generated Proxy-based no-ops for gated features, build-time dead code elimination via `bun:bundle`

### Gaps (MapleOS advantage)
- Based on leaked Anthropic source code — significant legal/ethical concerns
- Stub-heavy internals — core compaction, context collapse, history snipping are no-ops
- No knowledge base — MapleOS has maple-kb with vector search and Evolver
- No workflow DAG — MapleOS has visual workflow engine
- No real-time collaboration — MapleOS has kanban, comments, team features
- Complex circular dependencies — worked around with lazy require() and memoization
- Tightly coupled desktop+server — server must be running for desktop to function

---

## 4. rig (Rust, Provider-Agnostic LLM Framework — 19 crates)

### Architecture
- **Scale**: 19 crates, 25+ LLM providers, 11+ vector store integrations. Rust edition 2024
- **Core (`rig-core`)**: Agent, completion loop, tool system, vector stores, embeddings, pipeline, memory, streaming, telemetry, WASM compat
- **Provider Architecture**: Generic `Client<Ext, H>` with typestate builder. `CompletionModel` trait per provider. Capability system with `Capable<M>` or `Nothing` markers
- **Tool System**: `Tool` trait (typed), `ToolDyn` (dynamic dispatch), `ToolEmbedding` (vector-store RAG-retrievable), `ToolServer`/`ToolServerHandle` (concurrent, runtime registration). `#[rig_tool]` derive macro
- **Memory**: `ConversationMemory` trait, `InMemoryConversationMemory`, history shaping policies (SlidingWindow, TokenWindow, DemotingPolicy, Compacting, TemplateCompactor)
- **Pipeline**: DAG-based operation chaining with `Op` trait (inspired by Airflow/Dagster)
- **Telemetry**: Full OpenTelemetry GenAI semantic conventions
- **WASM**: `WasmCompatSend`/`WasmCompatSync` marker traits for WASM targets
- **Derive Macros**: `#[derive(Embed)]`, `#[derive(ProviderClient)]`, `#[rig_tool]`

### Key Innovations to Borrow
1. **Typestate pattern throughout** — `AgentBuilder<M, P, ToolState>` uses compile-time state transitions. Adding `.tool()` changes the type, making incompatible methods unavailable. MapleOS should adopt this for builder APIs
2. **PromptRequest as IntoFuture** — `agent.prompt("hello").max_turns(3).await` triggers the full agent loop. Elegant API that makes multi-turn calls look like simple async calls
3. **Dynamic tools via vector stores** — `ToolEmbedding` tools stored in vector stores, RAG-retrieved at prompt time by semantic similarity. Agent with 1000+ tools only sends relevant ones to LLM
4. **Concurrent tool execution** — `with_tool_concurrency(N)` uses `buffer_unordered(N)`. `ToolServerHandle` releases read lock before executing, preventing deadlocks
5. **Hook system with Skip action** — 6 hook points with Continue/Skip/Terminate. Skip returns reason to LLM as tool result, enabling graceful rejection
6. **Compacting memory with rolling summaries** — `Compactor` takes evicted messages + `carry_over` from previous compactions, enabling recursive summarization
7. **MCP auto-sync** — `McpClientHandler` automatically re-fetches tools when MCP server sends `notifications/tools/list_changed`
8. **Tool trait design** — `Tool` (typed) → `ToolDyn` (dynamic) → `ToolEmbeddingDyn` (RAG-retrievable). Blanket impls provide ergonomic dynamic dispatch without sacrificing typed API
9. **Strict linting** — workspace forbids `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, `dbg_macro` project-wide
10. **25+ provider implementations** — Anthropic, OpenAI, Cohere, Gemini, Mistral, Ollama, DeepSeek, Groq, xAI, etc. Reference for MapleOS provider coverage

### Gaps (MapleOS advantage)
- No agent runtime — rig is a library, not an agent OS
- No built-in provider routing/fallback — users must implement themselves
- No agentic planning/reasoning primitives — just tool-calling loop
- Pipeline module underdeveloped — no error recovery, retry, branching
- Multi-agent is imperative (manual wiring) — no registry, message bus, or orchestration
- Default max_turns is 0 — surprising for users expecting agentic behavior
- Memory persistence limited — only in-memory backend built-in
- No rate limiting or token budgeting — Usage tracks but doesn't enforce
- Streaming/non-streaming are separate code paths (~800/~900 lines with duplication)
- No web UI, no knowledge base, no workflow engine, no collaboration

---

## 5. claw-code/rust (Rust, Claude Code Rewrite — ~105k lines, 11 crates)

### Architecture
- **Scale**: ~105,000 lines across 11 crates. `rusty-claude-cli` 16,437 lines, `tools` 10,595 lines
- **Workspace**: `api` (provider clients, SSE) → `runtime` (conversation loop, state, config, MCP, hooks, sandbox, compaction, policy) → `tools` (40+ built-in tools) → `rusty-claude-cli` (REPL, streaming display)
- **Providers**: Anthropic (native), xAI, OpenAI-compat (DashScope/Qwen). Model name → provider registry mapping with aliases
- **Auth**: API key, bearer token, both, or OAuth (PKCE) with auto-refresh
- **Streaming**: SSE with `IncrementalSseParser`, exponential backoff retry (8 retries, 1s-128s)
- **Provider Fallback**: Ordered chain of fallback models on retryable failures (429/500/503)
- **Session**: `.jsonl` files with rotation (256KB/file, max 3 rotated)
- **Config**: `.claw.json` + `.claw/settings.json` with user/project/local precedence
- **Safety**: `#![forbid(unsafe_code)]` in workspace lints

### Key Innovations to Borrow
1. **Worker Boot State Machine** — full lifecycle: Spawning → TrustRequired → ToolPermissionRequired → ReadyForPrompt → Running → Finished/Failed. With prompt misdelivery detection and `StartupEvidenceBundle` for diagnosing failures. MapleOS agent runtime should adopt this
2. **Recovery Recipes** — automatic recovery for 7 failure scenarios (trust prompt, misdelivery, stale branch, compile failure, MCP handshake, plugin startup, provider failure) with escalation policies. MapleOS needs this resilience
3. **Trident Compaction** — 3-stage session compression: (1) Supersede remove obsolete messages, (2) Collapse chain similar operations, (3) Cluster group by similarity. Far more sophisticated than simple summarization
4. **Lane Events + Policy Engine** — structured event system tracking parallel workstreams (started/ready/blocked/red/green/commit.created/pr.opened/merge.ready/finished/failed). Rule-based policy engine auto-merges, rebases, escalates, or closes lanes. MapleOS workflow engine could integrate this
5. **Task Packets** — structured handoff specs with objective, scope, repo, branch policy, acceptance tests, commit policy, reporting contract, escalation policy. MapleOS task system should adopt this format
6. **Permission-Enforced Tool Execution** — `classify_bash_permission()` dynamically classifies permission level based on actual command content (not just tool name). MapleOS needs this for security
7. **Mock Parity Harness** — deterministic mock Anthropic service for end-to-end parity testing. MapleOS should build this for CI
8. **MCP Lifecycle Hardening** — degraded startup reports, per-server failure tracking with lifecycle phase classification, recoverability assessment. MapleOS MCP integration needs this
9. **ToolSearch** — model can search for deferred/specialized tools by keyword at runtime. Enables tool discovery without loading all tools upfront
10. **Config Hierarchy** — user/project/local precedence with merged feature configs for hooks, plugins, MCP, OAuth, permissions, sandbox, provider fallbacks
11. **Summary Compression** — budget-based text compression (1200 chars, 24 lines) with deduplication and omission notices
12. **Provider Fallback Config** — ordered chain of fallback models tried when primary returns retryable failures

### Gaps (MapleOS advantage)
- No web UI (CLI-only) — MapleOS has full web/mobile/desktop
- No knowledge base — MapleOS has maple-kb with vector search and Evolver
- No real-time collaboration — MapleOS has kanban, comments, team features
- In-memory registries (task/team/cron/worker) — lost on restart. MapleOS has SQLite persistence
- No streaming tool execution — long-running commands block the turn
- Sandbox is Linux-only (`unshare` user namespaces) — MapleOS targets cross-platform
- No cost controls — `UsageTracker` exists but no automatic budget enforcement
- No intelligent routing — provider detection is prefix matching only
- RAG service early-stage — SQLite + linear scan cosine similarity
- Large monolithic files (main.rs 16k lines, tools/lib.rs 10k lines)

---

## Synthesis: MapleOS Core Moats to Build

### Tier 1 — Immediate (directly borrow)

| Feature | Source | Effort | Impact |
|---------|--------|--------|--------|
| ProviderProfile pattern | hermes-agent | M | Unifies LLM provider management |
| Context compressor | hermes-agent | L | Critical for long conversations |
| Error classifier (22 failover reasons) | hermes-agent | M | Robust LLM error recovery |
| IterationBudget with grace call | hermes-agent | S | Cost control + graceful completion |
| Self-registering tool registry | hermes-agent | M | Cleaner tool system, lazy loading |
| Typestate builder pattern | rig | M | Compile-time API safety |
| PromptRequest as IntoFuture | rig | S | Elegant multi-turn API |
| Tool trait hierarchy (Tool→ToolDyn→ToolEmbedding) | rig | M | Typed + dynamic + RAG tools |
| Concurrent tool execution | rig/cc-haha | M | Parallel tool execution |
| Hook system with Skip action | rig | M | Graceful tool rejection |
| Tool definition standardization | claw-code/rig | S | Cleaner tool system |
| Cache token tracking | rig/claw-code | S | Cost optimization |
| Thinking block support | claw-code | S | Extended thinking for complex tasks |
| Strict linting (no unwrap/panic/todo) | rig | S | Code quality enforcement |

### Tier 2 — Near-term (借鉴 + 融合)

| Feature | Source | Effort | Impact |
|---------|--------|--------|--------|
| Toolset composition | hermes-agent | M | Flexible agent capabilities |
| Runtime subagent delegation | hermes-agent | L | Dynamic agent spawning |
| Mixture-of-Agents (parallel multi-model) | hermes-agent | L | Superior reasoning quality |
| Streaming context scrubber | hermes-agent | M | Clean streaming output |
| Threat pattern scanning | hermes-agent | M | Security hardening |
| Memory provider lifecycle hooks | hermes-agent | M | Pluggable memory system |
| Trajectory compression for training | hermes-agent | L | Self-improvement loop |
| StreamingToolExecutor with concurrency | cc-haha | M | Parallel tool execution |
| ToolUseContext dependency injection | cc-haha | M | Clean tool execution context |
| In-process multi-agent (AsyncLocalStorage) | cc-haha | M | Lightweight sub-agents |
| Mailbox-based inter-agent communication | cc-haha | M | Agent-to-agent messaging |
| Dynamic skill discovery | cc-haha | M | Conditional skill activation |
| IM adapter WebSocket bridge | cc-haha | M | Platform adapter pattern |
| Dynamic tools via vector stores | rig | M | RAG-retrieved tool selection |
| Compacting memory with rolling summaries | rig | M | Recursive context summarization |
| MCP auto-sync (tools/list_changed) | rig | S | Live tool refresh |
| 25+ provider implementations | rig | L | Provider coverage reference |
| Worker boot state machine | claw-code | M | Reliable agent spawning |
| Recovery recipes (7 failure scenarios) | claw-code | M | Automatic failure recovery |
| Trident compaction (3-stage) | claw-code | L | Superior session compression |
| Lane events + policy engine | claw-code | L | Parallel workstream management |
| Task packets (structured handoff) | claw-code | M | Clean task delegation |
| Permission-enforced tool execution | claw-code | M | Dynamic security classification |
| Mock parity harness | claw-code | M | CI parity testing |
| MCP lifecycle hardening | claw-code | M | Robust MCP integration |
| Semantic-gated dispatch batching | golutra | M | Prevents agent context corruption |
| Shim-based ready detection | golutra | S | Reliable agent startup |
| Post-ready step queue | golutra | M | Configurable agent initialization |
| Outbox pattern for task dispatch | golutra | M | Reliable message delivery |
| ACK-based flow control | golutra | M | Production-grade backpressure |
| CLI filter pipeline | golutra | M | Clean agent output parsing |

### Tier 3 — Strategic (unique MapleOS moats)

| Feature | Why It's a Moat |
|---------|-----------------|
| **Visual Workflow DAG** | None of the competitors have this. MapleOS's workflow engine with Agent/LLM/Tool/Condition/Parallel/Loop nodes is unique |
| **Knowledge Base (maple-kb)** | Vector search + Evolver pattern for knowledge precipitation. hermes-agent has simple memory files, claw-code has basic RAG |
| **Rust Performance** | hermes-agent is Python, cc-haha is TypeScript. claw-code is Rust but CLI-only. MapleOS is Rust + full-stack |
| **Full Stack (Web + Mobile + Desktop)** | All competitors are CLI-only or desktop-only. MapleOS has Next.js + Expo + Tauri |
| **Real-time Collaboration** | Kanban, comments, team features. No competitor has this |
| **Agent Registry + Task Channels** | Runtime agent discovery and communication. Unique to MapleOS |
| **SQLite Persistence** | claw-code uses in-memory registries (lost on restart). MapleOS persists everything |
| **Cross-Platform Sandbox** | claw-code sandbox is Linux-only. MapleOS targets cross-platform |

### Competitive Positioning

```
                    Agent Sophistication
                           ^
                           |
            hermes-agent   |   MapleOS (target)
            (Python, rich  |   (Rust, full-stack,
             features,     |    workflow DAG, KB,
             30+ platforms)|    collaboration)
                           |
            golutra        |
            (Rust, Tauri,  |
             multi-CLI     |
             orchestration)|
    ───────────────────────┼─────────────────────>
    Simple                 |                 Complex
                           |
            claw-code      |   rig
            (protocol      |   (framework,
             impl)         |    library)
                           |
                    Integration Breadth
```

MapleOS's unique position: **the only Rust-native, full-stack agent OS with visual workflow orchestration, knowledge precipitation, and real-time collaboration**.

**hermes-agent** is the richest feature competitor (Python, CLI-first) — 20+ platform adapters, 7 terminal backends, self-improving skills loop, mixture-of-agents, trajectory compression for training. But it's Python (slow), CLI-only, has god objects (700KB+ files), and sync/async mismatch issues.

**claw-code/rust** is the closest Rust competitor — 105k lines, worker boot state machine, trident compaction, lane events + policy engine, recovery recipes, task packets. But it's CLI-only, has in-memory registries (lost on restart), no web UI, no knowledge base, and large monolithic files.

**cc-haha** is the richest desktop workbench — protocol-translating proxy, streaming tool executor with concurrency control, in-process multi-agent, IM adapters. But it's based on leaked code (legal risk), has stub-heavy internals, and no knowledge base or workflow engine.

**golutra** is the closest architectural competitor (Rust + Tauri + multi-agent) — but it wraps external CLIs rather than calling LLM APIs directly, and lacks web/mobile, knowledge base, and workflow engine.

### Priority Roadmap

> **版本说明**: 此路线图使用 v0.x 版本号，与 README 中的 Phase 1-3 为不同维度。Phase 对应产品阶段，v0.x 对应功能里程碑。

**v0.4.0 — Foundation (2-3 weeks)**
- [x] Token 计数精确化 — tiktoken-rs (cl100k_base) 已集成 ✅
- [x] `#[tool]` 派生宏 — 声明式工具定义 ✅
- [ ] ProviderProfile refactor for `maple-llm` (declarative provider config)
- [ ] Error classifier with structured failover reasons (22 categories)
- [ ] IterationBudget with grace call for cost control
- [ ] Cache token tracking in LLM usage metrics
- [ ] Thinking block support in message types

**v0.5.0 — Agent Runtime (3-4 weeks)**
- [x] Context Compressor (token-budget, head/tail protection) ✅
- [x] ToolRegistry with semantic search (cosine similarity) ✅
- [ ] Toolset composition (named groups, composable, webhook-safe subset)
- [ ] StreamingToolExecutor with concurrency control (parallel safe, exclusive for unsafe)
- [ ] ToolUseContext dependency injection (rich context for all tool executions)
- [ ] Streaming context scrubber for clean output

**v0.6.0 — Multi-Agent & Security (3-4 weeks)**
- Runtime subagent delegation (isolated context, restricted tools, depth caps)
- Worker boot state machine (spawn → trust → permission → ready → running)
- Recovery recipes for automatic failure recovery with escalation
- Permission-enforced tool execution (dynamic classification by command content)
- Threat pattern scanning (context files + runtime prompts)
- Outbox pattern for reliable task dispatch

**v0.7.0 — Intelligence (4-5 weeks)**
- Mixture-of-Agents (parallel multi-model reasoning)
- Trident compaction (3-stage: supersede, collapse, cluster)
- Lane events + policy engine for parallel workstream management
- Task packets (structured handoff with acceptance tests)
- In-process multi-agent (AsyncLocalStorage for lightweight sub-agents)
- Mailbox-based inter-agent communication
- Dynamic skill discovery (conditional activation by file paths)
- Memory provider lifecycle hooks (pluggable memory system)
- Trajectory compression for training data
- Post-ready step queue for agent initialization

**v1.0.0 — Full Stack Agent OS**
- Full competitive feature parity with hermes-agent's agent capabilities
- Maintaining Rust performance advantage (10-100x over Python)
- Visual workflow DAG (unique moat)
- Knowledge base + Evolver (unique moat)
- Real-time collaboration (unique moat)
