# MapleOS 统一实施计划 — 基于 5 个竞品深度分析

> 综合 hermes-agent、golutra、rig、claw-code、cc-haha 的最佳实践
> 生成日期：2026-05-27

---

## 执行摘要

通过深度分析 5 个竞品项目，识别出 **42 个可采纳的创新点**，按优先级和依赖关系分为 **5 个阶段**，总工作量约 **16-20 周**。

### 核心收益

| 维度 | 当前状态 | 目标状态 |
|------|---------|---------|
| 工具执行 | 串行、无并发 | 并发安全、有序发射 |
| 错误处理 | 裸 anyhow::bail | 22 类错误分类 + 自动恢复 |
| 上下文管理 | 丢弃式压缩 | LLM 摘要 + 结构化记忆 |
| 工具系统 | 两个不共享 trait | 统一 Tool trait + 自注册 |
| Provider 管理 | 分散配置 | 统一 ProviderProfile |
| Hook 系统 | 仅 workflow 生效 | 全链路 hook |
| Agent 委派 | 无运行时生成 | 轻量 sub-agent + 工具子集 |
| 记忆系统 | 无 | 长期记忆 + 压缩感知 |

---

## Phase 1: 基础设施（3-4 周）

> 目标：建立核心抽象层，为后续功能提供基础

### 1.1 ProviderProfile（2-3 天）

**来源**：hermes-agent

**当前问题**：
- 每个 adapter 各自持有 `base_url`, `api_key`, `model` 等字段
- 无统一抽象，无法运行时管理

**改造方案**：
```rust
// core/maple-llm/src/provider_profile.rs
pub struct ProviderProfile {
    pub provider_id: String,
    pub adapter_type: AdapterType,
    pub base_url: String,
    pub api_key: Option<String>,
    pub default_model: String,
    pub pricing: (f64, f64),
    pub context_length: usize,
    pub rate_limit: Option<RateLimit>,
    pub priority: u8,
    pub health: ProviderHealth,
    pub quirks: ProviderQuirks,
}
```

**集成点**：
- 重构 `server/src/config.rs:38-46` 的 ProviderConfig
- 重构 `build_llm_router()` 从 ProviderProfile 构建 adapter
- DB 存储 profile，支持运行时 CRUD

**验收标准**：
- [ ] 所有 adapter 从 ProviderProfile 构建
- [ ] 运行时可查询/修改 provider 配置
- [ ] 测试覆盖 ProviderProfile CRUD

---

### 1.2 统一 Tool Trait（3-5 天）

**来源**：rig 3 层 trait + hermes-agent 自注册

**当前问题**：
- `skill_registry.rs:17-21` — 同步 `Skill` trait
- `react_loop.rs:94-97` — 异步 `ToolExecutor` trait
- `main.rs:169-181` — 桥接 hack

**改造方案**：
```rust
// core/maple-agent/src/tool.rs
pub trait Tool: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    fn required_permission(&self) -> PermissionLevel;
    async fn execute(&self, input: &serde_json::Value) -> Result<serde_json::Value>;
}

// 自动实现 ToolDyn（动态派发）
impl<T: Tool> ToolDyn for T { ... }

// 工具注册表
pub struct ToolRegistry {
    tools: DashMap<String, Arc<dyn ToolDyn>>,
}
```

**集成点**：
- 替换 `SkillRegistry` 和 `AppToolExecutor`
- 为 `ToolRegistry` 实现 `ToolExecutor` 以桥接 ReAct 循环
- 从 `Tool` trait 方法自动生成 `ToolDefinition`

**验收标准**：
- [ ] 所有内置工具实现统一 Tool trait
- [ ] ToolRegistry 替换旧的 SkillRegistry
- [ ] ReAct 循环通过 ToolRegistry 执行工具
- [ ] JSON Schema 参数验证工作

---

### 1.3 Hook System 扩展（1-2 天）

**来源**：rig 7 个 hook 点

**当前问题**：
- `hooks.rs:29-35` 已有 Hook trait，但只在 workflow 引擎中生效
- ReAct 循环完全绕过 hooks

**改造方案**：
```rust
// 扩展 Hook trait
pub trait Hook: Send + Sync {
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

**集成点**：
- 将 `HookRunner` 注入 `ReactLoop`
- 在每次 LLM 调用和工具执行前后调用 hooks
- 实现具体 hooks：`AuditLogHook`, `MetricsHook`

**验收标准**：
- [ ] Hook 在 ReAct 循环中生效
- [ ] Hook 在 workflow 引擎中继续生效
- [ ] 至少实现 2 个具体 hook

---

### 1.4 并发工具执行（1 天）

**来源**：rig `buffer_unordered`

**当前问题**：
- `react_loop.rs:162-179` — 串行 for 循环

**改造方案**：
```rust
// 替换串行 for 循环
let results: Vec<ToolResult> = futures::stream::iter(&assistant_msg.tool_uses)
    .map(|tool_use| async { tool_executor.execute(tool_use).await })
    .buffer_unordered(max_concurrent) // 默认 4
    .collect()
    .await;
```

**集成点**：
- 修改 `react_loop.rs` 的工具执行逻辑
- 添加 `max_concurrent_tools` 参数
- 确保结果按索引顺序追加到 session

**验收标准**：
- [ ] 多个工具可并行执行
- [ ] 结果顺序与 tool_use 索引一致
- [ ] 单个工具失败不阻塞其他工具

---

## Phase 2: 错误处理与上下文（3-4 周）

> 目标：建立健壮的错误处理和上下文管理

### 2.1 Error Classifier（2-3 天）

**来源**：hermes-agent 22 类 FailoverReason

**当前问题**：
- 所有 adapter 用 `anyhow::bail!` — 无错误分类
- 重试逻辑对所有错误无差别处理

**改造方案**：
```rust
// core/maple-llm/src/error.rs
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

pub struct ClassifiedError {
    pub reason: FailoverReason,
    pub retryable: bool,
    pub should_compress: bool,
    pub should_rotate_credential: bool,
    pub should_fallback: bool,
}
```

**集成点**：
- 每个 adapter 的 `complete()`/`stream()` 解析 HTTP 状态码 + JSON 错误体
- `LlmRouter` 跟踪 per-adapter 健康状态
- `is_available()` 检查 circuit breaker 状态

**验收标准**：
- [ ] HTTP 状态码正确映射到 LlmError 变体
- [ ] 重试逻辑根据错误类型决策
- [ ] Provider 健康状态实时跟踪

---

### 2.2 Context Compressor（2-3 天基础版）

**来源**：hermes-agent 头尾保护 + 工具输出裁剪

**当前问题**：
- `react_loop.rs:188-208` — `auto_compact()` 保留最后 6 条，丢弃其余
- `conversation.rs:32-66` — LLM 摘要压缩，**从未被使用**
- Token 计数到处用 `content.len() / 4`

**改造方案**：
```rust
// core/maple-agent/src/context_compressor.rs
pub struct ContextCompressor {
    context_length: usize,
    threshold_tokens: usize,      // max(context_length * 0.50, MINIMUM)
    tail_token_budget: usize,     // threshold * 0.20
    max_summary_tokens: usize,    // min(context_length * 0.05, 12_000)
    previous_summary: Option<String>,
}

impl ContextCompressor {
    pub fn compress(&self, messages: &[Message]) -> Vec<Message> {
        // 1. 保护头部（系统提示 + 前 3 条）
        // 2. 保护尾部（基于 token 预算，非固定消息数）
        // 3. 工具输出裁剪（去重 → 摘要 → 截断）
        // 4. LLM 摘要中间部分
    }
}
```

**集成点**：
- 替换 `auto_compact()` 和未使用的 `ConversationManager::compact()`
- 在 `ReactLoop::run_turn()` 的 `adapter.complete()` 调用前接入
- 用 `tiktoken-rs` 替换 `content.len() / 4`

**验收标准**：
- [ ] Token 计数准确（tiktoken-rs）
- [ ] 头部（系统提示）永不被压缩
- [ ] 尾部（最近 N 条）基于 token 预算保护
- [ ] 工具输出经过 3 遍裁剪
- [ ] LLM 摘要中间部分

---

### 2.3 Memory Policy（2-3 天）

**来源**：hermes-agent MemoryProvider 生命周期

**当前问题**：
- `Session` 是内存中 `Vec<Message>`，无持久化策略
- `SessionStore::load_session()` 加载所有消息，无窗口限制
- `maple-kb/src/memory.rs` 的 `MemoryStore` 未与聊天集成

**改造方案**：
```rust
// core/maple-agent/src/memory_policy.rs
pub trait MemoryPolicy: Send + Sync {
    fn manage(&self, messages: &[Message], max_tokens: usize) -> Vec<Message>;
}

// 实现
pub struct SlidingWindow { window_size: usize }
pub struct TokenBudgetWindow { max_tokens: usize }
pub struct LLMSummarize { compressor: ContextCompressor }
```

**集成点**：
- 注入 `ReactLoop`，替换 `auto_compact()`
- `SessionStore::load_session()` 接受 `max_messages` 或 `max_tokens` 参数
- 集成 `MemoryStore`（长期记忆）到对话上下文

**验收标准**：
- [ ] Session 加载有窗口限制
- [ ] 长期记忆注入到系统提示
- [ ] 至少实现 2 种 MemoryPolicy

---

## Phase 3: 可靠性与编排（3-4 周）

> 目标：建立生产级的可靠性和任务编排能力

### 3.1 Worker Boot 状态机（3-5 天）

**来源**：claw-code

**当前问题**：
- Agent 启动无状态跟踪
- 无启动失败检测和恢复

**改造方案**：
```rust
// core/maple-agent/src/worker_boot.rs
pub enum WorkerState {
    Spawning,
    TrustRequired,
    ToolPermissionRequired,
    ReadyForPrompt,
    Running,
    Finished,
    Failed(String),
}

pub struct WorkerBootMachine {
    state: WorkerState,
    evidence: StartupEvidenceBundle,
    recovery_ledger: RecoveryLedger,
}
```

**集成点**：
- 在 `DelegateEngine::delegate()` 中使用状态机
- 添加启动证据收集（transport 健康、MCP 状态、prompt 状态）
- 实现 7 种恢复策略

**验收标准**：
- [ ] Worker 启动状态可追踪
- [ ] 启动失败自动恢复（最多 3 次）
- [ ] 恢复策略可配置

---

### 3.2 ToolUseContext DI（2-3 天）

**来源**：cc-haha

**当前问题**：
- 工具执行上下文分散在多个结构体中
- 无法统一 mock 测试

**改造方案**：
```rust
// core/maple-agent/src/tool_use_context.rs
pub struct ToolUseContext {
    pub session_id: String,
    pub workspace_root: PathBuf,
    pub cwd: PathBuf,
    pub permission_level: PermissionLevel,
    pub cancellation_token: CancellationToken,
    pub progress_sender: Option<ProgressSender>,
    pub feature_flags: FeatureFlags,
    // ... ~20 个字段
}
```

**集成点**：
- 统一所有工具的执行上下文
- 从 Session + Config + Runtime 构建 context
- 传递到每个工具的 `execute()` 方法

**验收标准**：
- [ ] 所有工具通过 ToolUseContext 获取依赖
- [ ] 可通过 mock ToolUseContext 测试工具
- [ ] 取消信号正确传播

---

### 3.3 Agent Delegation（3-5 天）

**来源**：hermes-agent delegate_task + claw-code TaskPacket

**当前问题**：
- 无运行时 subagent 生成
- `AgentHandler` 是裸闭包 — 无协议、无工具过滤

**改造方案**：
```rust
// core/maple-agent/src/delegation.rs
pub trait AgentDelegator: Send + Sync {
    async fn delegate(
        &self,
        goal: &str,
        tools: &[String],
        context: &Value,
        opts: DelegateOpts,
    ) -> Result<String>;
}

pub struct DelegateOpts {
    pub max_iterations: usize,
    pub timeout: Duration,
    pub permission_level: PermissionLevel,
    pub approval_callback: Option<ApprovalCallback>,
}
```

**集成点**：
- 替换 `DelegateEngine::delegate()` 的 mpsc channel 模式
- 添加轻量 agent 生成：工具子集 + 目标 + 独立 ReAct 循环
- 传递执行上下文到委派 agent

**验收标准**：
- [ ] 运行时可生成 sub-agent
- [ ] sub-agent 获取工具子集
- [ ] sub-agent 超时自动终止
- [ ] 结果正确返回父 agent

---

### 3.4 StreamingToolExecutor（2-3 天）

**来源**：cc-haha

**改造方案**：
```rust
// core/maple-agent/src/streaming_executor.rs
pub enum ToolConcurrency {
    ConcurrentSafe,  // read_file, search, web_fetch
    Exclusive,       // write_file, execute_bash
}

pub struct StreamingToolExecutor {
    max_concurrent: usize,
    tools: Arc<ToolRegistry>,
}

impl StreamingToolExecutor {
    pub async fn execute_all(
        &self,
        tool_uses: &[ToolUse],
    ) -> Vec<ToolResult> {
        // 1. 分类 concurrent-safe vs exclusive
        // 2. concurrent-safe 并行执行
        // 3. exclusive 串行执行
        // 4. 按原始顺序发射结果
    }
}
```

**验收标准**：
- [ ] 并发安全工具并行执行
- [ ] 独占工具串行执行
- [ ] 结果按索引顺序发射
- [ ] 错误级联取消剩余工具

---

## Phase 4: 高级功能（4-5 周）

> 目标：实现生产级的高级功能

### 4.1 Trigger System（1 周）

**来源**：golutra

**改造方案**：
```rust
// core/maple-agent/src/trigger.rs
pub struct TriggerBus {
    sender: mpsc::Sender<TriggerEvent>,
    receiver: mpsc::Receiver<TriggerEvent>,
}

pub struct TriggerScheduler {
    queue: BinaryHeap<TriggerEntry>, // 最早到期优先
    dedup: HashMap<TriggerKey, u64>, // 每个 key 的最新 due_at
}

pub enum TriggerStage {
    Stable,
    Silence,
    Debounce,
    PostReadyTick,
    ChatPendingForce,
}
```

**验收标准**：
- [ ] 事件驱动规则评估
- [ ] 延迟阶段正确实现
- [ ] 去重机制工作

---

### 4.2 Cron Tasks（3-5 天）

**来源**：cc-haha

**改造方案**：
```rust
// core/maple-agent/src/cron.rs
pub struct CronScheduler {
    jobs: Vec<CronJob>,
    max_jobs: usize, // 50
    storage: Box<dyn CronStorage>,
}

pub struct CronJob {
    pub id: String,
    pub schedule: CronExpression,
    pub task: CronTask,
    pub job_type: CronJobType, // Recurring, Durable, Permanent
    pub timezone: Option<String>,
}
```

**验收标准**：
- [ ] Cron 表达式解析正确
- [ ] 任务持久化（文件或 DB）
- [ ] 跨重启恢复
- [ ] 最大任务数限制

---

### 4.3 Coordinator Mode（3-5 天）

**来源**：cc-haha

**改造方案**：
```rust
// core/maple-agent/src/coordinator.rs
pub struct Coordinator {
    max_workers: usize,
    worker_pool: WorkerPool,
    task_queue: VecDeque<SubTask>,
}

impl Coordinator {
    pub async fn coordinate(&self, goal: &str) -> Result<String> {
        // 1. Analyze: 分解目标为子任务
        // 2. Delegate: 创建 worker，分配子任务
        // 3. Monitor: 监控 worker 状态
        // 4. Synthesize: 汇总结果
    }
}
```

**验收标准**：
- [ ] 复杂任务自动分解
- [ ] Worker 并行执行
- [ ] 结果正确汇总
- [ ] 异常处理（worker 失败、超时）

---

### 4.4 Recovery Recipes（3-4 天）

**来源**：claw-code

**改造方案**：
```rust
// core/maple-agent/src/recovery.rs
pub enum RecoveryRecipe {
    TrustPromptUnresolved { action: AcceptTrustPrompt },
    PromptMisdelivery { action: RedirectPromptToAgent },
    StaleBranch { action: RebaseBranch },
    McpHandshakeFailure { action: RetryMcpHandshake },
    ProviderFailure { action: RestartWorker },
    // ...
}

pub struct RecoveryLedger {
    entries: Vec<RecoveryLedgerEntry>,
    max_retries: usize,
}
```

**验收标准**：
- [ ] 7 种故障场景覆盖
- [ ] 自动恢复尝试
- [ ] 升级策略（AlertHuman）

---

## Phase 5: 生产就绪（3-4 周）

> 目标：生产级的可靠性和可观测性

### 5.1 Health Monitoring（1 周）

**改造方案**：
- Provider 健康检查（circuit breaker）
- Agent 健康检查（心跳）
- 工具执行统计（成功率、延迟）
- 资源使用监控（token、内存、CPU）

**验收标准**：
- [ ] 健康状态 API
- [ ] 告警机制
- [ ] 监控仪表盘数据

---

### 5.2 Security Hardening（1 周）

**改造方案**：
- 5 级权限系统（ReadOnly < WorkspaceWrite < Prompt < Allow < DangerFullAccess）
- `classify_bash_permission()` 动态分类
- `classify_file_path_permission()` 路径验证
- 审计日志

**验收标准**：
- [ ] 权限系统工作
- [ ] 危险命令需要审批
- [ ] 审计日志完整

---

### 5.3 Performance Optimization（1 周）

**改造方案**：
- Token 计数缓存
- 工具结果缓存
- 并发限制调优
- 内存使用优化

**验收标准**：
- [ ] P95 延迟 < 2s
- [ ] 内存使用稳定
- [ ] 无内存泄漏

---

### 5.4 Testing & Documentation（1 周）

**改造方案**：
- 单元测试覆盖 80%+
- 集成测试覆盖核心流程
- API 文档
- 架构文档

**验收标准**：
- [ ] 测试覆盖率达标
- [ ] CI/CD 绿色
- [ ] 文档完整

---

## 依赖关系图

```
Phase 1:
  1.1 ProviderProfile ─────────────────────────────────┐
  1.2 统一 Tool Trait ─────────────────────────────────┤
  1.3 Hook System ─────────────────────────────────────┤
  1.4 并发工具执行 ────────────────────────────────────┤
                                                        │
Phase 2:                                                ▼
  2.1 Error Classifier ←── 依赖 1.1 ProviderProfile
  2.2 Context Compressor ──────────────────────────────┤
  2.3 Memory Policy ←── 依赖 2.2 Context Compressor    │
                                                        │
Phase 3:                                                ▼
  3.1 Worker Boot 状态机 ──────────────────────────────┤
  3.2 ToolUseContext DI ←── 依赖 1.2 Tool Trait        │
  3.3 Agent Delegation ←── 依赖 1.2 + 1.3             │
  3.4 StreamingToolExecutor ←── 依赖 1.2 + 1.4        │
                                                        │
Phase 4:                                                ▼
  4.1 Trigger System ──────────────────────────────────┤
  4.2 Cron Tasks ──────────────────────────────────────┤
  4.3 Coordinator Mode ←── 依赖 3.3 Agent Delegation   │
  4.4 Recovery Recipes ←── 依赖 3.1 Worker Boot        │
                                                        │
Phase 5:                                                ▼
  5.1 Health Monitoring ←── 依赖 2.1 Error Classifier  │
  5.2 Security Hardening ←── 依赖 3.2 ToolUseContext   │
  5.3 Performance Optimization ────────────────────────┤
  5.4 Testing & Documentation ─────────────────────────┘
```

---

## 工作量汇总

| Phase | 组件 | 工作量 | 来源 |
|-------|------|--------|------|
| **1** | ProviderProfile | 2-3 天 | hermes-agent |
| **1** | 统一 Tool Trait | 3-5 天 | rig |
| **1** | Hook System | 1-2 天 | rig |
| **1** | 并发工具执行 | 1 天 | rig |
| **2** | Error Classifier | 2-3 天 | hermes-agent |
| **2** | Context Compressor | 2-3 天 | hermes-agent |
| **2** | Memory Policy | 2-3 天 | hermes-agent |
| **3** | Worker Boot 状态机 | 3-5 天 | claw-code |
| **3** | ToolUseContext DI | 2-3 天 | cc-haha |
| **3** | Agent Delegation | 3-5 天 | hermes-agent |
| **3** | StreamingToolExecutor | 2-3 天 | cc-haha |
| **4** | Trigger System | 1 周 | golutra |
| **4** | Cron Tasks | 3-5 天 | cc-haha |
| **4** | Coordinator Mode | 3-5 天 | cc-haha |
| **4** | Recovery Recipes | 3-4 天 | claw-code |
| **5** | Health Monitoring | 1 周 | 综合 |
| **5** | Security Hardening | 1 周 | claw-code |
| **5** | Performance Optimization | 1 周 | 综合 |
| **5** | Testing & Documentation | 1 周 | 综合 |
| | **总计** | **~16-20 周** | |

---

## 快速收益（可立即实施）

以下改动工作量小，但收益明显：

1. **并发工具执行**（1 天）— 直接复制 rig 的 `buffer_unordered` 模式
2. **Hook System 接入 ReAct 循环**（1 天）— 已有代码，只需注入
3. **Token 计数替换**（0.5 天）— `content.len() / 4` → `tiktoken-rs`
4. **Session 加载窗口限制**（0.5 天）— 添加 `max_messages` 参数
5. **null-args 归一化**（1 小时）— 生产可靠性

**总计：3 天可完成 5 个快速收益**

---

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| Tool trait 重构破坏现有功能 | HIGH | 保持旧 trait 作为 wrapper，渐进迁移 |
| Context Compressor LLM 调用成本 | MEDIUM | 先实现无 LLM 的基础版，再添加 LLM 摘要 |
| 并发工具执行引入竞态条件 | MEDIUM | 充分测试，限制并发数 |
| Agent Delegation 资源泄漏 | HIGH | 添加超时和强制终止机制 |
| ProviderProfile 迁移中断服务 | HIGH | 灰度发布，保留回滚能力 |

---

## 成功指标

| 指标 | 当前 | Phase 1 后 | Phase 3 后 | Phase 5 后 |
|------|------|-----------|-----------|-----------|
| 工具执行延迟 | 串行 | 并发 4x | 并发 + 缓存 | 优化 |
| 错误恢复率 | 0% | 50% | 80% | 95% |
| 上下文利用率 | 丢弃式 | 基础压缩 | LLM 摘要 | 智能记忆 |
| 测试覆盖率 | ~30% | 50% | 70% | 80%+ |
| Provider 可用性 | 无跟踪 | 基础健康 | circuit breaker | 完整监控 |

---

## 下一步行动

1. **立即开始**：Phase 1.4 并发工具执行（1 天，无依赖）
2. **本周内**：Phase 1.1 ProviderProfile + 1.3 Hook System
3. **下周**：Phase 1.2 统一 Tool Trait
4. **持续**：按 Phase 顺序推进

---

## 参考文档

- `docs/competitive-analysis.md` — 竞品分析总览
- `docs/maple-os-integration-points.md` — MapleOS 集成点详细分析
- `docs/hermes-agent-deep-dive.md` — hermes-agent 深度分析
- `docs/rig-clawcode-tool-analysis.md` — rig + claw-code 工具系统分析
- `docs/golutra-product-design-analysis.md` — golutra 终端引擎分析
- `docs/cc-haha-deep-dive.md` — cc-haha 深度分析
