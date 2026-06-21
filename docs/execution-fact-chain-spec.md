# Execution Fact Chain Specification

版本：2026-06-20  
状态：Track 0 / T0-5 产出，Track 1 实施  
对齐 issue：#92  
对齐表结构：`migrations/014_execution_events.sql`、`migrations/015_tool_invocations.sql`

---

## 1. 目的

定义 MapleOS v3 的**统一执行事实链**契约：所有运行入口（Chat、Workflow、Task、Agent、Approval）必须把执行过程写入同一张 `execution_events` 表，所有 UI 面板必须从同一事实链解释状态。

不再允许各模块维护私有 status 字段并自行解释。

## 2. 核心概念

### 2.1 execution_id

- **定义**：每次"用户/系统触发的一次完整执行"分配一个 UUIDv4 作为 `execution_id`
- **生成时机**：
  - Chat：用户发送一条消息 → 一个 execution_id（覆盖该消息触发的所有 LLM 调用、工具调用、审批、最终回复）
  - Workflow：用户/Cron/事件触发一次 workflow run → 一个 execution_id
  - Agent run：用户在 Agent Center 点 Run → 一个 execution_id
  - Task：scheduler 取出一个 task → 一个 execution_id
- **不可变性**：execution_id 一旦分配不可变；所有相关事件必须用同一个 id
- **嵌套**：sub-agent 委派、workflow 子流程 → 生成新的 execution_id，但 `parent_execution_id` 指向父

### 2.2 事件（execution_events）

- **append-only**：事件只能新增，不能修改或删除
- **顺序**：按 `created_at` ASC 排序，同一毫秒内按插入顺序
- **payload**：JSON，schema 由 `event_type` 决定（见 §3）

### 2.3 聚合视图（executions）

- 每个 execution_id 在 `executions` 表有一行，记录当前 status、actor、timing、error
- `event_count` 由触发器或 recorder 维护，等于该 execution_id 的事件总数
- `status` 由最后一个 `done`/`error`/`cancelled` 事件决定

## 3. 事件类型枚举

| event_type | 写入者 | payload schema | 语义 |
| --- | --- | --- | --- |
| `started` | 任意入口 | `{entry: "chat"\|"workflow"\|..., trigger: "manual"\|"cron"\|..., input_summary: string}` | 执行开始 |
| `delta` | chat / agent | `{token: string, cumulative_tokens: int, message_id: string}` | LLM 流式 token |
| `tool_call` | agent / workflow_node | `{tool_name: string, input: object, permission_level: string, invocation_id: string}` | 请求调用工具 |
| `tool_result` | tool | `{invocation_id: string, output: object\|null, error: string\|null, duration_ms: int}` | 工具返回 |
| `node_started` | workflow | `{node_id: string, node_type: string, step_index: int}` | workflow 节点开始 |
| `node_finished` | workflow | `{node_id: string, status: "success"\|"failed"\|"skipped", output: object\|null, error: string\|null, duration_ms: int}` | 节点结束 |
| `artifact` | 任意 | `{artifact_type: "kb_doc"\|"memory"\|"prompt"\|"file", target_id: string, summary: string}` | 产出沉淀物 |
| `usage` | llm | `{prompt_tokens: int, completion_tokens: int, total_tokens: int, cost_usd: float, model: string}` | token 计费 |
| `approval_requested` | approval | `{approval_id: string, action_type: string, description: string, urgency: string, expires_at: int}` | 请求审批 |
| `approval_decided` | approval | `{approval_id: string, decision: "approved"\|"rejected"\|"modified", voter_id: string, comment: string\|null}` | 审批结果 |
| `retry` | 任意 | `{target_event_id: string, reason: string, attempt: int}` | 重试某步 |
| `paused` | 任意 | `{reason: "waiting_approval"\|"manual_pause", resume_token: string\|null}` | 暂停 |
| `resumed` | 任意 | `{reason: "approval_granted"\|"manual_resume", actor: string}` | 恢复 |
| `cancelled` | 任意 | `{reason: string, actor: string}` | 取消 |
| `done` | 任意入口 | `{output_summary: string, total_duration_ms: int, total_cost_usd: float\|null}` | 成功完成 |
| `error` | 任意入口 | `{error_type: string, message: string, stack: string\|null, recoverable: bool}` | 失败 |

## 4. source 字段语义

| source | 含义 | 典型事件 |
| --- | --- | --- |
| `chat` | 聊天入口 | started, delta, done, error |
| `workflow` | 工作流引擎 | node_started, node_finished, paused, resumed |
| `task` | 任务队列 | started, done, error |
| `approval` | 审批服务 | approval_requested, approval_decided |
| `agent` | ReAct 循环 | tool_call, artifact |
| `tool` | 工具执行器 | tool_result |
| `scheduler` | 调度器 | started（cron/event/message 触发） |
| `system` | 其他系统事件 | cancelled, error |

## 5. ExecutionRecorder API 契约

Track 1 将在 `core/maple-engine/src/execution_chain.rs` 实现：

```rust
pub struct ExecutionRecorder {
    pool: SqlitePool,
}

impl ExecutionRecorder {
    /// 在执行入口调用。生成 execution_id 并写入 executions 行 + started 事件。
    pub async fn start(
        &self,
        source: &str,
        actor: Option<&str>,
        actor_type: Option<&str>,
        trigger_type: &str,
        trigger_payload: serde_json::Value,
        parent_execution_id: Option<&str>,
    ) -> Result<String>;

    /// 追加事件。payload 必须符合 event_type 的 schema。
    pub async fn append(
        &self,
        execution_id: &str,
        source: &str,
        event_type: &str,
        payload: serde_json::Value,
        actor: Option<&str>,
        actor_type: Option<&str>,
    ) -> Result<String>;

    /// 标记完成。写入 done 事件并更新 executions.status = 'success'。
    pub async fn done(&self, execution_id: &str, output_summary: &str) -> Result<()>;

    /// 标记失败。写入 error 事件并更新 executions.status = 'failed'。
    pub async fn fail(&self, execution_id: &str, error: &str, recoverable: bool) -> Result<()>;

    /// 取消。写入 cancelled 事件并更新 executions.status = 'cancelled'。
    pub async fn cancel(&self, execution_id: &str, actor: &str, reason: &str) -> Result<()>;
}

/// 查询接口（供 API handler 使用）
impl ExecutionRecorder {
    /// GET /api/v3/executions/:id/events
    pub async fn list_events(&self, execution_id: &str) -> Result<Vec<ExecutionEvent>>;

    /// GET /api/v3/executions/:id
    pub async fn get_execution(&self, execution_id: &str) -> Result<Execution>;
}
```

## 6. API 契约

### 6.1 GET /api/v3/executions/:id

返回聚合视图：

```json
{
  "id": "exec_abc123",
  "parent_execution_id": null,
  "source": "chat",
  "status": "running",
  "actor": "user_xyz",
  "actor_type": "human",
  "trigger_type": "manual",
  "trigger_payload": {},
  "started_at": 1718880000,
  "completed_at": null,
  "error": null,
  "event_count": 7
}
```

### 6.2 GET /api/v3/executions/:id/events

返回事件列表（按 created_at ASC）：

```json
{
  "execution_id": "exec_abc123",
  "events": [
    {
      "id": "evt_001",
      "event_type": "started",
      "source": "chat",
      "payload": {"entry": "chat", "trigger": "manual", "input_summary": "hello"},
      "actor": "user_xyz",
      "actor_type": "human",
      "created_at": 1718880000
    },
    {
      "id": "evt_002",
      "event_type": "tool_call",
      "source": "agent",
      "payload": {"tool_name": "kb_search", "input": {"query": "maple"}, "permission_level": "read_only", "invocation_id": "inv_001"},
      "actor": "agent_default",
      "actor_type": "agent",
      "created_at": 1718880001
    },
    {
      "id": "evt_003",
      "event_type": "tool_result",
      "source": "tool",
      "payload": {"invocation_id": "inv_001", "output": {"hits": 3}, "error": null, "duration_ms": 42},
      "created_at": 1718880001
    }
  ]
}
```

### 6.3 GET /api/v3/executions/:id/events/stream (SSE)

服务端推送事件流，客户端用 EventSource 订阅：

```
event: started
data: {"id":"evt_001","event_type":"started",...}

event: delta
data: {"id":"evt_002","event_type":"delta","payload":{"token":"Hello",...},...}

event: tool_call
data: {...}

event: done
data: {"id":"evt_n","event_type":"done",...}
```

订阅时如果执行已结束，服务端先回放历史事件再关闭流；如果执行进行中，先回放历史再实时推送。

## 7. 接入契约（每个入口必须做）

### 7.1 Chat

`POST /api/v3/chat` handler 改造：

```rust
let exec_id = recorder.start("chat", Some(&user_id), Some("human"),
                              "manual", json!({"message": &msg}), None).await?;
// SSE 流式输出时：
recorder.append(&exec_id, "chat", "delta", json!({"token": tok, ...}), ...).await?;
// Agent 调用工具时：
recorder.append(&exec_id, "agent", "tool_call", json!({...}), ...).await?;
// 完成时：
recorder.done(&exec_id, &summary).await?;
```

### 7.2 Workflow

`POST /api/workflows/:id/runs` handler 改造：

```rust
let exec_id = recorder.start("workflow", Some(&user_id), Some("human"),
                              &trigger_type, json!({...}), None).await?;
// 节点执行时：
recorder.append(&exec_id, "workflow", "node_started", json!({...}), ...).await?;
// 节点完成时：
recorder.append(&exec_id, "workflow", "node_finished", json!({...}), ...).await?;
// 人工审批节点暂停时：
recorder.append(&exec_id, "workflow", "paused", json!({"reason":"waiting_approval"}), ...).await?;
// 审批通过恢复时：
recorder.append(&exec_id, "workflow", "resumed", json!({"reason":"approval_granted"}), ...).await?;
```

### 7.3 Task / Agent / Approval

同理。所有入口的 `started` / `done` / `error` / `cancelled` 必须调 recorder。

## 8. 前端契约

### 8.1 共享组件

`apps/web/src/components/execution-timeline.tsx`：

```tsx
<ExecutionTimeline executionId="exec_abc123" />
```

- 内部用 SSE 订阅 `GET /api/v3/executions/:id/events/stream`
- 渲染时间线：started → delta（折叠）→ tool_call + tool_result（成对）→ artifact → done
- 不同 source 用不同颜色：chat 蓝 / workflow 紫 / agent 绿 / tool 黄 / approval 橙 / error 红

### 8.2 复用点

- Chat trace 面板 → `<ExecutionTimeline executionId={chatExecId} />`
- Workflow trace 面板 → `<ExecutionTimeline executionId={wfExecId} />`
- Task details 面板 → `<ExecutionTimeline executionId={taskExecId} />`
- Agent run 面板 → `<ExecutionTimeline executionId={agentExecId} />`

不再允许各面板自行渲染事件。

## 9. 投影规则（从事件链到派生视图）

| 派生视图 | 来源 | 投影规则 |
| --- | --- | --- |
| Chat trace | execution_events WHERE source IN ('chat','agent','tool') | 按 created_at 顺序 |
| Workflow trace | execution_events WHERE source IN ('workflow','approval','tool') | 按 created_at 顺序 |
| Task details | execution_events WHERE source IN ('task','agent','tool') | 按 created_at 顺序 |
| Audit log | execution_events WHERE event_type IN ('started','done','error','cancelled','approval_requested','approval_decided') | 按 created_at DESC |
| Activity feed | execution_events WHERE event_type IN ('started','done','artifact') | 按 created_at DESC，分页 |
| Usage stats | execution_events WHERE event_type = 'usage' | 聚合 sum(tokens) |

**重要**：派生视图是**只读投影**，不能反向修改事件链。

## 10. 验收标准（对应 #92）

- [ ] 所有运行入口（chat send / workflow run / agent run / tool call / approval create）调 `recorder.start(...)` / `recorder.append(...)` / `recorder.done(...)`
- [ ] `GET /api/v3/executions/:id/events` 返回完整事件链
- [ ] Chat/Workflow/Task/Agent 4 个 UI 面板都用 `<ExecutionTimeline />` 渲染，不再有模块私有 trace 组件
- [ ] approval approve/reject / retry / cancel / resume 都 append 事件到同一 execution_id
- [ ] E2E 测试：用一个 execution_id 拉事件，断言包含 started + tool_call + tool_result + done
- [ ] audit_log 和 activity_feed 是只读投影，不直接写入

## 11. 不变量（CI 必查）

1. **append-only**：任何代码不能 `UPDATE` 或 `DELETE` `execution_events` 行
2. **single source**：`workflow_runs.status`、`tasks_v3.status`、`approval_requests.status` 等模块私有 status 字段必须由事件链投影更新，不能由 handler 直接修改
3. **execution_id 必传**：所有 tool_call 必须带 execution_id，否则 recorder 拒绝写入
4. **payload 必须符合 schema**：recorder 在写入前校验 payload shape，不符合的拒绝并 panic（开发期）/ log error（生产期）

## 12. 迁移路径（Track 1 实施步骤）

1. **T1-1**：实现 `ExecutionRecorder` + 单元测试
2. **T1-2**：实现 `GET /api/v3/executions/:id/events` + `GET /api/v3/executions/:id`
3. **T1-3**：Chat handler 接入 recorder，移除 chat-panel 私有 trace 状态
4. **T1-4**：Workflow executor 接入 recorder，node_started/finished 走事件链
5. **T1-5**：Agent react loop 接入 recorder，tool_call/tool_result 走事件链
6. **T1-6**：Approval service 接入 recorder，approval_requested/decided 走事件链
7. **T1-7**：前端 `<ExecutionTimeline />` 组件 + SSE 订阅
8. **T1-8**：4 个 UI 面板替换为 `<ExecutionTimeline />`
9. **T1-9**：E2E 测试覆盖（含在 Track 4 product-gate 扩充）

每一步都必须不退化 baseline（cargo check + 440 lib tests + 12 v3 api tests + product-gate E2E）。

## 13. 反模式（禁止）

- ❌ handler 直接 `UPDATE executions SET status = 'success'` — 必须调 `recorder.done()`
- ❌ 模块自己建 trace 表（如 `chat_traces`、`workflow_traces`）— 用统一事件链
- ❌ 前端组件直接 fetch `/api/chat/:id/trace` — 用 `/api/v3/executions/:id/events`
- ❌ 事件 payload 里放二进制或大对象（> 64KB）— 用 artifact 引用
- ❌ 跨 execution_id 引用事件 — 用 parent_execution_id 表达嵌套
