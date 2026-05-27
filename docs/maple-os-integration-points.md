# MapleOS 集成点分析 — 每个创新的具体改造方案

> 基于源码深度分析，2026-05-27

---

## 1. ProviderProfile 重构

**现状问题：**
- 每个 adapter 各自持有 `base_url`, `api_key`, `model`, `pricing` 等字段，无统一抽象
- `OpenAiCompatAdapter` (openai_compat.rs:8-16) 有 6 个字段
- `AnthropicAdapter` (anthropic.rs:8-13) 有 4 个字段
- `OllamaAdapter` (ollama.rs:8-13) 有 4 个字段
- `server/src/config.rs:38-46` 的 `ProviderConfig` 是最接近 profile 的东西，但只在构建时使用

**改造方案：**
1. 在 `core/maple-llm/src/` 创建统一 `ProviderProfile`：
   ```rust
   pub struct ProviderProfile {
       pub provider_id: String,
       pub adapter_type: AdapterType, // OpenAiCompat, Anthropic, Ollama
       pub base_url: String,
       pub api_key: Option<String>,
       pub default_model: String,
       pub pricing: (f64, f64), // input, output per 1M tokens
       pub context_length: usize,
       pub rate_limit: Option<RateLimit>,
       pub priority: u8,
       pub health: ProviderHealth,
       pub retry_config: RetryConfig,
       pub quirks: ProviderQuirks, // temperature behavior, header overrides, etc.
   }
   ```
2. 重构 `build_llm_router()` (config.rs:89-382) 从 `ProviderProfile` 构建 adapter
3. 在 DB 中存储 profile，支持运行时 CRUD

**工作量：** M (2-3天) | **依赖：** 无

---

## 2. Error Classifier

**现状问题：**
- 所有 adapter 用 `anyhow::bail!("API error ({}): {}", status, text)` — 无错误分类
- `openai_compat.rs:171-173`, `anthropic.rs:82-83`, `ollama.rs:103-104` 都是裸错误
- `LlmRouter::is_available()` (router.rs:110-112) 只检查 key 是否存在，不跟踪错误状态
- `WorkflowExecutor::execute_with_retry()` (executor.rs:659-681) 对所有错误无差别重试

**改造方案：**
1. 定义 `LlmError` 枚举：
   ```rust
   pub enum LlmError {
       RateLimited { retry_after: Duration },
       AuthFailed,
       ContextTooLong { current: usize, max: usize },
       ModelOverloaded,
       NetworkError { source: anyhow::Error },
       InvalidRequest { message: String },
       QuotaExceeded,
       ServerError { status: u16 },
       Unknown { status: u16, body: String },
   }
   ```
2. 每个 adapter 的 `complete()`/`stream()` 解析 HTTP 状态码 + JSON 错误体为 `LlmError`
3. 添加 `ErrorClassifier` 映射 HTTP 状态码到 22 类失败原因
4. `LlmRouter` 跟踪 per-adapter 健康状态：`DashMap<String, ProviderHealth>`
5. `is_available()` 检查 circuit breaker 状态
6. 重试逻辑检查错误是否可重试

**工作量：** M (2-3天) | **依赖：** ProviderProfile

---

## 3. Context Compressor

**现状问题：**
- 两个粗糙的压缩机制，都不成熟：
  - `react_loop.rs:188-208` — `auto_compact()`: 保留系统提示 + 最后 6 条，丢弃其余。无 LLM 摘要
  - `conversation.rs:32-66` — `ConversationManager::compact()`: 保留系统提示 + 最后 4 条，LLM 摘要旧消息。**但从未被使用**
- Token 计数到处用 `content.len() / 4`（粗糙启发式）
- 每个 adapter 的 `max_context_length()` 方法存在但从不检查

**改造方案：**
1. 创建 `ContextCompressor` trait：
   ```rust
   trait ContextCompressor: Send + Sync {
       fn compress(&self, messages: &[Message], max_tokens: usize) -> Vec<Message>;
   }
   ```
   实现：`SlidingWindow`, `TokenBudget`, `LlmSummarize`, `HierarchicalSummary`
2. 在 `ReactLoop::run_turn()` 的 `adapter.complete()` 调用前接入压缩器
3. 在 `LlmAdapter`/`LlmRouter` 添加预发送检查：`estimated_tokens > max_context_length()` 时自动压缩
4. 用 `tiktoken-rs` 替换 `content.len() / 4`

**关键常量（参考 hermes-agent）：**
- 最小摘要 token：2000
- 压缩内容的摘要比例：20%
- 摘要 token 上限：12000
- 保护头部（系统提示）+ 尾部（最近 N 条，基于 token 预算）
- 工具输出裁剪（便宜的预处理步骤）

**工作量：** M (2-3天基础版，L 完整 LLM 摘要) | **依赖：** 无基础版，ProviderProfile 完整版

---

## 4. Tool System 重构

**现状问题：**
- 两个不共享的 trait 抽象层：
  - `skill_registry.rs:17-21` — `Skill` trait: `fn id()`, `fn description()`, `fn execute(&self, config: &Value) -> Result<Value>`。同步
  - `react_loop.rs:94-97` — `ToolExecutor` trait: `async fn execute(&self, tool_use: &ToolUse) -> Result<ToolResult>`。异步
  - `request.rs:92-97` — `ToolDefinition` (发给 LLM 的)
  - `main.rs:169-181` — `AppToolExecutor` 桥接两者（hack）

**改造方案：**
1. 统一 `Tool` trait：
   ```rust
   trait Tool: Send + Sync {
       fn name(&self) -> &str;
       fn description(&self) -> &str;
       fn parameters_schema(&self) -> serde_json::Value; // JSON Schema
       fn required_permission(&self) -> PermissionLevel;
       async fn execute(&self, input: &Value) -> Result<Value>;
   }
   ```
2. 创建 `ToolRegistry` 替代 `SkillRegistry` 和 `AppToolExecutor`，工具自注册
3. 从 `Tool` trait 方法自动生成 `ToolDefinition`
4. 添加 JSON Schema 参数验证
5. 为 `ToolRegistry` 实现 `ToolExecutor` 以桥接 ReAct 循环

**工作量：** L (3-5天) | **依赖：** 无

---

## 5. Hook System 扩展

**现状：**
- 已有 hook 基础设施 (hooks.rs:29-35)：
  ```rust
  pub trait Hook: Send + Sync {
      fn name(&self) -> &str;
      fn on_pre_tool_use(&self, _tool_name: &str, _input: &Value) -> HookDecision { HookDecision::Allow }
      fn on_post_tool_use(&self, _tool_name: &str, _result: &Value) {}
  }
  ```
- `HookRunner` (line 37-63) 运行 pre-hooks，第一个非 Allow 决策停止
- **但只在 workflow 引擎中接入** (executor.rs:613)
- ReAct 循环 (react_loop.rs:162-179) 完全绕过 hooks
- 缺少 `on_pre_llm_call`, `on_post_llm_call`, `on_error` hooks

**改造方案：**
1. 扩展 `Hook` trait：
   ```rust
   trait Hook: Send + Sync {
       fn name(&self) -> &str;
       fn on_session_start(&self, _session_id: &str) {}
       fn on_pre_llm_call(&self, _request: &LlmRequest) -> HookDecision { HookDecision::Allow }
       fn on_post_llm_call(&self, _response: &LlmResponse) {}
       fn on_pre_tool_use(&self, _tool_name: &str, _input: &Value) -> HookDecision { HookDecision::Allow }
       fn on_post_tool_use(&self, _tool_name: &str, _result: &Value) {}
       fn on_error(&self, _error: &LlmError) {}
       fn on_session_end(&self, _session_id: &str) {}
   }
   ```
2. 将 `HookRunner` 注入 `ReactLoop`，在每次工具执行前后调用
3. 实现具体 hooks：`AuditLogHook`, `MetricsHook`, `RateLimitHook`, `ContentFilterHook`

**工作量：** S (1-2天) | **依赖：** Tool 重构更干净，但不阻塞

---

## 6. 并发工具执行

**现状问题：**
- ReAct 循环中工具**串行执行**：
  ```rust
  // react_loop.rs:162-179
  for tool_use in &assistant_msg.tool_uses { ... } // 逐个 await
  ```
- Workflow 引擎支持并行（executor.rs:198-300 的 `execute_parallel()` 用 `tokio::spawn` + `join_all`），但这是节点级别

**改造方案：**
```rust
// 替换串行 for 循环
let results: Vec<ToolResult> = futures::stream::iter(&assistant_msg.tool_uses)
    .map(|tool_use| async { tool_executor.execute(tool_use).await })
    .buffer_unordered(max_concurrent) // 默认 4
    .collect()
    .await;
```
- 添加 `max_concurrent_tools` 参数到 `ReactLoop`
- 确保结果按索引顺序（非完成顺序）追加到 session

**工作量：** S (1天) | **依赖：** 无

---

## 7. Memory Policies

**现状问题：**
- `Session` (react_loop.rs:67-92) — 内存中 `Vec<Message>`，无持久化策略
- `SessionStore` (session_store.rs) — SQLite 持久化，`load_session()` 加载所有消息，无窗口限制
- `ConversationManager` (conversation.rs) — LLM 摘要压缩，**从未被使用**
- `auto_compact()` (react_loop.rs:188-208) — 保留最后 6 条，丢弃其余。无摘要
- `maple-kb/src/memory.rs` 的 `MemoryStore` — 长期记忆，未与聊天集成

**改造方案：**
1. 创建 `MemoryPolicy` trait：
   ```rust
   trait MemoryPolicy: Send + Sync {
       fn manage(&self, messages: &[Message], max_tokens: usize) -> Vec<Message>;
   }
   ```
   实现：`SlidingWindow`, `TokenBudgetWindow`, `LLMSummarize`, `ImportanceBased`
2. 注入 `ReactLoop`，替换 `auto_compact()`
3. `SessionStore::load_session()` 接受 `max_messages` 或 `max_tokens` 参数
4. 集成 `MemoryStore`（长期记忆）到对话上下文 — 注入相关记忆到系统提示
5. 添加 `on_session_end` hook 持久化重要事实到 `MemoryStore`

**工作量：** M (2-3天) | **依赖：** ContextCompressor, Hook System

---

## 8. Agent Delegation

**现状：**
- Workflow 级别 (executor.rs:161-181)：`NodeType::Agent` 调用 `AgentHandler` 回调，fire-and-forget + timeout
- Agent 级别 (delegate.rs:1-68)：`DelegateEngine::delegate()` 通过 mpsc channel 发送任务，oneshot 接收结果
- Orchestrator (orchestrator.rs)：`plan_and_execute()` 分解目标为子任务，按能力匹配分配 agent

**差距：**
- 无运行时 subagent 生成（hermes-agent 的 `delegate_task` 创建临时 sub-agent）
- `AgentHandler` 是裸闭包 — 无协议、无工具过滤、无上下文传递
- 无 agent 间通信（只有 task-result 模式）
- `find_available()` 简单工具列表匹配，无负载均衡

**改造方案：**
1. 统一 `AgentDelegator` trait：
   ```rust
   trait AgentDelegator: Send + Sync {
       async fn delegate(&self, goal: &str, tools: &[String], context: &Value, opts: DelegateOpts) -> Result<String>;
   }
   ```
2. 添加临时 sub-agent 生成：创建轻量 agent（工具子集 + 目标），运行 ReAct 循环，返回结果
3. 传递执行上下文（前序节点输出、workflow 变量）到委派 agent
4. `find_available()` 添加负载均衡（round-robin 或 least-busy）

**工作量：** L (3-5天) | **依赖：** Tool 重构, Hook System

---

## 推荐执行顺序

```
Phase 1 (立即):
  1. ProviderProfile (#1) — 基础设施
  2. 并发工具执行 (#6) — 快速收益，无依赖
  3. Hook System 扩展 (#5) — 现有代码，只需接入

Phase 2 (短期):
  4. Error Classifier (#2) — 依赖 #1
  5. Tool System 重构 (#4) — 基础设施
  6. Context Compressor (#3) — 独立

Phase 3 (中期):
  7. Memory Policies (#7) — 依赖 #3, #5
  8. Agent Delegation (#8) — 依赖 #4, #5
```

---

## 当前代码关键问题总结

| 问题 | 位置 | 严重度 | 修复难度 |
|------|------|--------|---------|
| Token 计数用 `len()/4` | 全局 | HIGH | S |
| Context 从不检查 max_length | router.rs | HIGH | S |
| 裸 `anyhow::bail!` 无分类 | 所有 adapter | HIGH | M |
| 工具串行执行 | react_loop.rs:162 | MEDIUM | S |
| ConversationManager 从未使用 | conversation.rs | MEDIUM | S |
| Hook 只在 workflow 中生效 | executor.rs:613 | MEDIUM | S |
| auto_compact 丢弃而非摘要 | react_loop.rs:188 | HIGH | M |
| Session 加载无窗口限制 | session_store.rs | HIGH | S |
| 两个不共享的 Tool trait | skill_registry + react_loop | HIGH | L |
| Agent 无运行时生成 | delegate.rs | MEDIUM | L |
