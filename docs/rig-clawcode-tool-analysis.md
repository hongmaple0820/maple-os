# rig + claw-code 工具系统深度对比分析

---

## rig Tool 系统

### Tool Trait 层级（3 层）

**Layer 1: `Tool` (静态派发)**
```rust
trait Tool: Send + Sync + 'static {
    const NAME: &'static str;
    type Args: DeserializeOwned;
    type Output: Serialize;
    type Error: std::error::Error;
    fn definition(&self, prompt: String) -> ToolDefinition;
    fn call(&self, args: Self::Args) -> impl Future<Output = Result<Self::Output, Self::Error>>;
}
```

**Layer 2: `ToolDyn` (动态派发包装器)**
```rust
trait ToolDyn: Send + Sync {
    fn name(&self) -> String;
    fn definition(&self, prompt: String) -> ToolDefinition;
    fn call(&self, args: String) -> WasmBoxedFuture<Result<String, ToolError>>;
}
```

**Layer 3: `ToolEmbedding` (RAG 可检索工具)**
```rust
trait ToolEmbedding: Tool {
    type Context: Serialize + Send;
    type State: Serialize + Send;
    type InitError: Error;
    fn embedding_docs(&self, ctx: Self::Context) -> Vec<String>;
    fn context(&self) -> Self::Context;
    fn init(&self, state: Self::State) -> Result<(), Self::InitError>;
}
```

**关键：blanket impl `impl<T: Tool> ToolDyn for T`**
- 任何 `Tool` 自动成为 `ToolDyn`
- 处理 LLM 发送 `null` 参数的情况（回退到 `{}`）
- 输出序列化：字符串直通，对象转 JSON

### ToolServer 并发模式

```rust
// server.rs:143-163 — 释放锁后再执行
let tool = {
    let state = self.0.read().await;
    state.toolset.get(tool_name).cloned()  // 在短暂读锁下 clone Arc
};
// 读锁在此释放
match tool {
    Some(tool) => tool.call(args).await  // 无锁执行
}
```

### 并发工具执行

```rust
stream::iter(tool_calls)
    .map(|choice| async { /* hook check + tool execution */ })
    .buffer_unordered(self.concurrency)  // 默认 1，可配置
    .collect::<Vec<Result<UserContent, PromptError>>>()
    .await
```

### Hook 系统（7 个 hook 点）

| Hook | 触发时机 | 动作 |
|------|---------|------|
| `on_completion_call` | LLM 调用前 | Continue/Terminate |
| `on_completion_response` | LLM 响应后 | Continue/Terminate |
| `on_tool_call` | 工具执行前 | Continue/**Skip{reason}**/Terminate |
| `on_tool_result` | 工具执行后 | Continue/Terminate |
| `on_text_delta` | 流式文本 | Continue/Terminate |
| `on_tool_call_delta` | 流式工具调用 | Continue/Terminate |
| `on_stream_finish` | 流式完成 | Continue/Terminate |

**Skip 的巧妙之处**：返回 reason 字符串作为工具结果给 LLM，LLM 理解为什么工具被阻止并能适应。

### MCP 集成

- `McpTool`: 包装 `rmcp::model::Tool` + `rmcp::service::ServerSink`
- `McpClientHandler`: 收到 `notifications/tools/list_changed` 时自动重新获取和注册工具
- 传输：通用 `rmcp::transport::IntoTransport`

### derive 宏 `#[rig_tool]`

从函数自动生成：
1. `XxxParameters` 结构体（`#[derive(Deserialize)]`）
2. 零大小 `Xxx` 结构体（`#[derive(Default)]`）
3. 完整 `Tool` impl（自动生成 JSON Schema）
4. `static XXX: Xxx = Xxx;` 便于访问

---

## claw-code Tool 系统

### 三层注册

1. **内置工具** `mvp_tool_specs()`: 15+ 工具，静态 `ToolSpec` 结构体
2. **插件工具** `PluginTool`: 运行时加载，不与内置冲突
3. **运行时工具** `RuntimeToolDefinition`: 动态注册

### 权限系统（5 级）

```
ReadOnly < WorkspaceWrite < Prompt < Allow < DangerFullAccess
```

**动态分类**：
- `classify_bash_permission()`: 检查首 token 是否在 ~40 个只读命令白名单中，检查重定向 (`>`, `>>`) 和就地修改标志 (`-i`, `--in-place`)
- `classify_file_path_permission()`: 检查路径是否在 workspace root 内

### Worker Boot 状态机

```
Spawning → TrustRequired → ToolPermissionRequired → ReadyForPrompt → Running → Finished/Failed
```

- **Trust gate**: 屏幕文本匹配检测信任提示
- **Prompt misdelivery detection**: 检查 prompt 是否落入 shell（command-not-found）、错误目标（CWD 不匹配）、错误任务（receipt 不匹配）
- **StartupEvidenceBundle**: 生命周期状态、transport/MCP 健康、prompt 状态

### Recovery Recipes（7 种故障场景）

| 场景 | 恢复步骤 | 升级策略 |
|------|---------|---------|
| TrustPromptUnresolved | AcceptTrustPrompt | AlertHuman |
| PromptMisdelivery | RedirectPromptToAgent | AlertHuman |
| StaleBranch | RebaseBranch + CleanBuild | AlertHuman |
| CompileRedCrossCrate | CleanBuild | AlertHuman |
| McpHandshakeFailure | RetryMcpHandshake(5000ms) | Abort |
| PartialPluginStartup | RestartPlugin + RetryMcpHandshake | LogAndContinue |
| ProviderFailure | RestartWorker | AlertHuman |

### Lane Events（21 种事件类型）

`started`, `ready`, `prompt_misdelivery`, `blocked`, `red`, `green`, `commit.created`, `pr.opened`, `merge.ready`, `finished`, `failed`, `reconciled`, `merged`, `superseded`, `closed`, `branch.stale_against_main`, `branch.workspace_mismatch`, `ship.prepared`, `ship.commits_selected`, `ship.merged`, `ship.pushed_main`

每个事件携带：`LaneEventMetadata`（seq, provenance, session_identity, ownership, nudge_id, event_fingerprint, timestamp_ms, environment_label, emitter_identity, confidence_level）

### Policy Engine

规则 = `PolicyCondition -> PolicyAction`（带优先级）

**条件**：`And`, `Or`, `GreenAt{level}`, `StaleBranch`, `StartupBlocked`, `LaneCompleted`, `LaneReconciled`, `ReviewPassed`, `ScopedDiff`, `TimedOut`, `RetryAvailable`, `RebaseRequired`, `StaleCleanupRequired`, `ApprovalTokenPresent`, `ApprovalTokenMissing`

**动作**：`MergeToDev`, `MergeForward`, `RecoverOnce`, `Retry`, `Rebase`, `Escalate`, `CloseoutLane`, `CleanupSession`, `CleanupStale`, `Reconcile`, `Notify`, `RequireApprovalToken`, `Block`, `Chain`

### Task Packets

```rust
struct TaskPacket {
    objective: String,
    scope: TaskScope, // Workspace/Module/SingleFile/Custom
    scope_path: Option<String>,
    repo: Option<String>,
    worktree: Option<String>,
    branch_policy: Option<BranchPolicy>,
    acceptance_tests: Option<Vec<String>>,
    acceptance_criteria: Option<String>,
    resources: Vec<TaskResource>,
    model: Option<String>,
    provider: Option<String>,
    permission_profile: Option<String>,
    commit_policy: Option<CommitPolicy>,
    reporting_contract: Option<String>,
    reporting_targets: Option<Vec<String>>,
    escalation_policy: Option<EscalationPolicy>,
    recovery_policy: Option<RecoveryPolicy>,
    verification_plan: Option<String>,
}
```

---

## MapleOS 应采纳的关键模式

### 从 rig（高优先级）

| 模式 | 价值 | Rust 代码 | 工作量 |
|------|------|----------|--------|
| Tool trait + blanket impl → ToolDyn | 编译时类型安全 + 运行时动态派发 | 直接适配 | 2-3 天 |
| Lock-release-before-call | 防止并发死锁 | 直接复制 | 0.5 天 |
| buffer_unordered(concurrency) | 并行工具执行 | 直接复制 | 0.5 天 |
| null-args 归一化 | 生产可靠性 | 复制粘贴 | 1 小时 |
| Skip{reason} hook action | 优雅的工具拒绝 | 直接适配 | 0.5 天 |
| #[rig_tool] derive 宏 | 减少样板代码 | 2-3 天 |

### 从 claw-code（高优先级）

| 模式 | 价值 | 工作量 |
|------|------|--------|
| 5 级权限系统 | 生产安全 | 3-4 天 |
| Worker Boot 状态机 | 可靠 agent 启动 | 5-7 天 |
| Recovery Recipes | 自动故障恢复 | 3-4 天 |
| Lane Events + 指纹去重 | 分布式执行追踪 | 3-4 天 |
| Policy Engine | 运营策略编码 | 2-3 天 |
| Task Packets | 结构化任务编排 | 2 天 |
| classify_bash_permission | 动态安全分类 | 1 天 |

**总计：~26-33 天**
