# MapleOS 落地实施总方案（2026 Q3）

版本：2026-06-20  
分支：`master` → 在 `master` 上长出 `feat/closure-track-{A..F}` 系列工作分支，按 Track 顺序合并  
对齐文档：`docs/MapleOS_Product_Closure_Roadmap.md`、`docs/MapleOS_Open_Source_Cobuild_Backlog.md`、`docs/unified-implementation-plan.md`、`ISSUES_ORGANIZATION.md`  
GitHub Issues：本文每条任务都绑定 issue 编号，PR 必须在描述里链接对应 issue

---

## 0. 现状诊断（来自代码 + Issues + Roadmap 三方对账）

### 0.1 已经能跑通的链路（不要重复造轮子）

| 链路 | 现状 | 证据 |
| --- | --- | --- |
| Local Mode 启动 | Web + Rust 后端可启动 | `tests/e2e/product-gate.spec.ts` |
| Agent 注册 / KB 上传 / Workflow 新建 | 烟囱路径已覆盖 | 同上 |
| Chat SSE 流式 | 后端 + 前端已有实现 | `apps/web/src/lib/v3-ws.ts`、`server/src/main.rs` |
| Workflow SSE 节点状态 | 已有部分推送 | `core/maple-engine/src/event_bus.rs` |
| Tool Approval | `agent run → tool_call → approval → resume` 已跑通 | `core/maple-engine/src/approval.rs` |
| Learning 候选 + 写回 | 候选、审批、写 KB/Memory/Prompt 已有 | `core/maple-kb/src/evolver.rs` |
| LLM 多 Provider | OpenAI / Anthropic / Ollama 适配器齐全 | `core/maple-llm/src/adapters/` |

### 0.2 真正阻塞产品闭环的缺口（按影响排序）

| 缺口 | 影响 | 涉及 issue |
| --- | --- | --- |
| **G1 执行事实链未统一**：`workflow_runs` / `workflow_step_executions` 各写各的，没有 `execution_events` / `tool_invocations` 这张统一表，UI 三个面板拼不出同一个 trace | Chat/Workflow/Task/Agent 状态互相打架，#89 E2E 没法做断言 | #92、#89、#53、#61 |
| **G2 Workflow Canvas 不是真编辑器**：节点可显示可拖拽，但 CRUD/连线/参数 schema/版本/失败恢复都没串起来 | 用户做不出一个能复用的工作流 | #90、#17、#61 |
| **G3 LLM 模型列表类型不匹配**（#86）：`list_models()` 返回 `Vec<String>`，前端期望 `{id,name,provider}`，且本地 Ollama 没拉 `/v1/models` | 配置完模型实际跑不通，信任崩塌 | #86 |
| **G4 Learning 治理无门禁**：候选可生成可审批，但没有质量分阈值、回滚、污染防护、下一轮生效验证 | 自学习可能污染长期上下文 | #91、#55、#56 |
| **G5 E2E 门禁只是 smoke**：`product-gate.spec.ts` 只验证页面可见，不验证 chat→approval→workflow→artifact→learning→next-run 闭环 | 每次合并都可能默默断链 | #89、#66、#67 |
| **G6 前端工作台单体过大**：`workflow-manager.tsx` 856 行、`settings-page.tsx` 496 行，state 散落 | 加新功能容易破坏老功能 | #93 |
| **G7 桌面/首次运行未验收**：Tauri 版本结构、env 检查、错误提示都没完整复验 | 新用户下载后跑不起来 | #85、#87、#63 |
| **G8 真实工具仍是 mock**：`web_search` / `code_execute` / `file_ops` / `http_request` 没补实 | 用户以为能用，实际完不成任务 | #57、#58、#72、#22、#69 |
| **G9 跨端缺口**：Sync / CLI / Mobile / 桌面自动更新全部 Open | 离"多端"还远 | #65、#70、#25、#68 |
| **G10 Issue hygiene**：21 个 open issue 中部分已实现，但未标 `needs-verification` | 路线漂移 | #94 |

### 0.3 关键代码现状（用于估算工作量）

- `server/src/main.rs` 5654 行单文件，路由+handler 全塞一起 → 后端要先拆模块再做事实链
- `migrations/` 没有 `execution_events` / `tool_invocations` 表 → 必须先建表才能做 G1
- `core/maple-llm/src/router.rs:204` 的 `list_models()` 直接返回 adapter keys 字符串 → G3 修复点明确
- `apps/web/src/components/workflow-manager.tsx` 用了 ReactFlow，节点 CRUD 半成品 → G2 在此基础上补
- `tests/e2e/product-gate.spec.ts` 只有 57 行 → G5 扩充点明确

---

## 1. 实施策略：六条 Track 串行 + Track 内并行

Roadmap 里 Phase A~F 是按"主题"分，本方案按"工程依赖"重排为 6 条 Track，每条 Track 内部尽量并行，Track 之间强依赖串行。

```
Track 0 (准备) ──▶ Track 1 (统一事实链) ──▶ Track 2 (Canvas 真编辑器)
                       │                          │
                       ├──▶ Track 3 (LLM 配置硬化 + Learning 治理) ──┐
                       │                                              │
                       └──▶ Track 4 (E2E 门禁 + 前端 IA 模块化) ◀──────┘
                                       │
                                       ├──▶ Track 5 (真实工具 + 插件)
                                       └──▶ Track 6 (桌面/同步/CLI/移动)
```

**关键原则**（来自 `MapleOS_Open_Source_Cobuild_Backlog.md` §5）：
1. 不接受"只改页面"的 PR，每个 PR 必须说明 user entry / runtime path / persistence path / error path / 验证证据
2. mock 能力必须在 UI 上明确标记 `disabled`/`mock`，不能假装能用
3. 任何改动 chat/workflow/approval/memory/provider 的 PR 必须同时更新 Playwright 覆盖

---

## 2. Track 0：准备 + Issue Hygiene（0.5 周，必须最先做）

**目标**：让后续每条 Track 都能在干净仓库上开工，避免重复劳动

### 任务清单

| ID | 任务 | 产出 | 关联 issue |
| --- | --- | --- | --- |
| T0-1 | 仓库体检：本地 `cargo check` + `pnpm i && pnpm build` + `playwright test --grep product-gate` 全跑通，记录 baseline | `docs/baseline-2026-06-20.md` 含命令、版本、截图 | #94 |
| T0-2 | 21 个 open issue 逐个体检：有实现的打 `needs-verification`，已完成的附证据关闭，重复的合并 | PR 关闭/打标 ≥10 个 issue | #94 |
| T0-3 | 在 `migrations/` 新增 `012_execution_events.sql`、`013_tool_invocations.sql`（schema 见 Track 1） | 两个迁移文件 | #92 |
| T0-4 | 把 `server/src/main.rs` 5654 行按域拆成 `server/src/routes/{chat,workflow,agent,kb,settings,approval}.rs`，行为不变 | 拆分 PR，CI 绿 | #92、#93 |
| T0-5 | 建立 `docs/execution-fact-chain-spec.md`：定义 execution_id 生成规则、事件类型枚举、所有入口必须写入的契约 | 文档 | #92 |

**验收**：CI 在 `pull_request` 和 `push master` 上跑通 baseline，T0-2 提交 issue hygiene 报告到 #94。

---

## 3. Track 1：统一执行事实链（1.5 周，所有后续 Track 的地基）

**目标**：Chat / Workflow / Task / Approval / Audit / Activity 都从同一张 `execution_events` 表解释状态

### 3.1 数据层（T0-3 已建表）

```sql
-- 012_execution_events.sql
CREATE TABLE execution_events (
  id TEXT PRIMARY KEY,
  execution_id TEXT NOT NULL,           -- 统一执行 ID
  parent_execution_id TEXT,             -- 委派/子流程时关联
  source TEXT NOT NULL,                 -- chat|workflow|task|approval|agent|tool
  event_type TEXT NOT NULL,             -- started|delta|tool_call|tool_result|node_started|node_finished|artifact|usage|approval_requested|approval_decided|done|error
  payload TEXT NOT NULL,                -- JSON
  actor TEXT,                           -- user_id / agent_id / system
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_exec_events_id ON execution_events(execution_id, created_at);
CREATE INDEX idx_exec_events_source ON execution_events(source, created_at DESC);

-- 013_tool_invocations.sql
CREATE TABLE tool_invocations (
  id TEXT PRIMARY KEY,
  execution_id TEXT NOT NULL,
  tool_name TEXT NOT NULL,
  input TEXT, output TEXT, error TEXT,
  permission_level TEXT,                -- read_only|workspace_write|danger
  approval_id TEXT,
  started_at INTEGER, completed_at INTEGER, duration_ms INTEGER,
  status TEXT NOT NULL                  -- pending|running|approved|rejected|success|failed|cancelled
);
CREATE INDEX idx_tool_inv_exec ON tool_invocations(execution_id, started_at);
```

### 3.2 后端契约

| ID | 任务 | 关联 issue |
| --- | --- | --- |
| T1-1 | `core/maple-engine/src/execution_chain.rs` 新模块：统一 `ExecutionRecorder`，所有入口（chat send / workflow run / agent run / tool call / approval create）调 `recorder.start(...)` / `recorder.append(...)` | #92 |
| T1-2 | `GET /api/executions/:id/events` 流式 SSE 接口，返回事件序列 | #92 |
| T1-3 | `GET /api/executions/:id` 返回聚合视图（status / source / actor / started_at / completed_at / event_count） | #92 |
| T1-4 | Chat handler / Workflow executor / Agent react loop / Approval service 全部接入 recorder，移除模块内私有 status 字段，改从 events 投影 | #92、#52、#53 |
| T1-5 | Approval approve/reject / retry / cancel / resume 都 append 事件，不直接改 `workflow_runs.status` | #92 |

### 3.3 前端契约

| ID | 任务 | 关联 issue |
| --- | --- | --- |
| T1-6 | `apps/web/src/components/execution-timeline.tsx` 共享组件：传 execution_id 即可渲染完整时间线（工具调用、节点状态、产物、审批、错误） | #92、#93 |
| T1-7 | Chat trace / Workflow trace / Task details / Agent run 面板全部复用 `<ExecutionTimeline executionId={...} />` | #92、#61 |
| T1-8 | SSE 解析器抽到 `apps/web/src/lib/execution-sse.ts`，统一处理 delta/tool_call/artifact/done/error | #93、#52 |

**验收**（对应 #92 acceptance criteria）：
- 任一 execution_id 通过 API 可拉到完整事件链
- 三个 UI 面板展示同一 execution_id 时事件一致
- approval/retry/cancel/resume 都能在事件链上看到

---

## 4. Track 2：Workflow Canvas 真编辑器（2 周）

**目标**：用户能在 Canvas 上完成 创建 → 编辑 → 校验 → 保存版本 → 运行 → 看trace → 失败恢复 全闭环

### 4.1 后端

| ID | 任务 | 关联 issue |
| --- | --- | --- |
| T2-1 | `POST /api/workflows/:id/validate`：校验节点 schema、必填参数、DAG 拓扑（无环、单出口、入口合法） | #90、#17 |
| T2-2 | `POST /api/workflows/:id/save` 每次保存生成新版本，`GET /api/workflows/:id/versions` 列版本，`GET /api/workflows/:id/versions/:v` 拉详情，`POST /api/workflows/:id/versions/:v/rollback` 回滚 | #17、#61 |
| T2-3 | 运行入口 `POST /api/workflows/:id/runs` 复用 Track 1 的 recorder，每个节点 start/finish 都 append 事件 | #90、#92 |
| T2-4 | 失败节点：`POST /api/workflow-runs/:rid/nodes/:nid/retry`、`POST /api/workflow-runs/:rid/nodes/:nid/skip`、死信列表 `GET /api/workflow-runs/:rid/deadletter` | #90 |

### 4.2 前端

| ID | 任务 | 关联 issue |
| --- | --- | --- |
| T2-5 | Canvas 节点 CRUD：右键菜单新建/删除/复制，拖拽连线，双击编辑参数（按 schema 自动生成表单） | #90、#93 |
| T2-6 | 保存按钮触发 validate → 失败高亮错误节点 + 错误原因 tooltip；成功后弹出"新版本 vN 已保存" | #90 |
| T2-7 | 版本侧边栏：版本列表、diff 视图（节点增删改 + 参数变化）、回滚按钮 | #17、#61 |
| T2-8 | 运行按钮：从 Canvas 直切 trace 视图，复用 `<ExecutionTimeline />` | #90、#92 |
| T2-9 | 审批节点：暂停态显示"等待审批"卡片，批准/拒绝后 Canvas 节点状态实时更新（走 SSE） | #90、#53 |
| T2-10 | 失败节点：红色高亮 + 错误原因 + 重试/跳过按钮，死信入口 | #90 |

**验收**（对应 #90 acceptance criteria）：
- 新建一个三节点工作流 → 保存 → 运行 → 看到完整 trace
- 加一个人工审批节点 → 运行 → 暂停 → 批准 → 恢复 → 完成
- 让某节点故意失败 → 看到错误 → 重试成功
- 切换到旧版本 → diff 显示差异 → 回滚

---

## 5. Track 3：LLM 配置硬化 + Learning 治理（1.5 周，可与 Track 2 并行）

### 5.1 LLM 配置（修 #86 + 硬化 P0-4）

| ID | 任务 | 关联 issue |
| --- | --- | --- |
| T3-1 | `core/maple-llm/src/router.rs` 重构 `list_models()` 返回 `Vec<ModelDescriptor>`：`{id, name, provider, adapter_type, context_length, is_local}` | #86 |
| T3-2 | Ollama adapter 新增 `list_remote_models()`：拉 `GET {ollama_url}/v1/models`，与本地注册合并去重 | #86 |
| T3-3 | `POST /api/llm/providers/:id/test` 测试连接：发一个最小 chat completion，返回 latency / model / error | #86、P0-4 |
| T3-4 | 前端 `settings-page.tsx`：模型列表按 provider 分组，API key 脱敏显示（前 4 后 4），测试连接按钮 + 结果展示 | #86、#93 |
| T3-5 | Agent 创建页：模型选择器继承全局配置，可覆盖；image_model 与 chat_model 分离 | P0-4 |

### 5.2 Learning 治理（#91）

| ID | 任务 | 关联 issue |
| --- | --- | --- |
| T3-6 | 候选生成时强制附 `score` / `evidence` / `source_execution_id` / `suggested_target`，缺一拒绝生成 | #91 |
| T3-7 | 质量门禁：score < threshold（默认 0.7）或 evidence 为空 → 自动拒绝并记录原因 | #91 |
| T3-8 | 污染防护：被拒绝候选不写入 KB/Memory/Prompt，且加入 `learning_blocklist` 表，下次相同内容不再生成候选 | #91 |
| T3-9 | 回滚：`POST /api/learning/items/:id/revoke` 撤销已批准项，下次 context preview 不再注入 | #91 |
| T3-10 | 下一轮生效验证：批准后立即在 Agent context preview 中显示"命中学习项 X（来自 execution Y）" | #91、#55、#56 |
| T3-11 | E2E 测试：低置信度候选不能进长期上下文（用 Playwright 跑完整 approve→next-run 流程） | #91、#89 |

**验收**：
- #86 截图中的错误不再出现，本地 Ollama 模型自动列出
- Agent 创建 / 聊天 / 生图共享同一份 provider 配置
- 批准一个学习项 → 下次 Agent run 的 context preview 能看到来源解释
- 拒绝的学习项永不进入上下文，可复现

---

## 6. Track 4：E2E 门禁 + 前端 IA 模块化（1 周，依赖 Track 1/2/3）

### 6.1 E2E 门禁（#89）

| ID | 任务 | 关联 issue |
| --- | --- | --- |
| T4-1 | `tests/e2e/product-gate.spec.ts` 扩成 5 个 describe block：chat / workflow / tool-approval / learning / llm-settings | #89 |
| T4-2 | Chat 用例：send → SSE delta → tool_call 渲染 → context source 展示 → learning candidate 出现 → done | #89、#52 |
| T4-3 | Workflow 用例：create → save → run → human approval → artifact 写 KB → trace 完整 | #89、#90 |
| T4-4 | Tool approval 用例：高风险工具触发 → approval task → approve → agent resume → final reply | #89 |
| T4-5 | Learning 用例：候选生成 → 审批 → next-run context preview 命中 | #89、#91 |
| T4-6 | LLM settings 用例：provider save → masked key → test connection → agent 继承 | #89、#86 |
| T4-7 | CI 工作流：PR 触发 `pnpm playwright test --project=product-gate`，失败阻断合并 | #89、#67 |

### 6.2 前端 IA 模块化（#93）

| ID | 任务 | 关联 issue |
| --- | --- | --- |
| T4-8 | `apps/web/src/components/workspace.tsx` 拆成 8 个独立 module 文件：Dashboard / Messages / Agents / Workflows / Tasks / Knowledge / Plugins / Settings | #93 |
| T4-9 | 每个 module 统一 5 状态：loading / empty / error / success / disabled，抽 `<StatePanel status={} />` 共享组件 | #93 |
| T4-10 | mock 工具在 UI 上挂 `<MockBadge />` 标签 + 跳转到对应 issue 链接 | #93 |
| T4-11 | 键盘可达性：tab 焦点链路完整，disabled 按钮有 tooltip 解释原因 | #93 |

**验收**：
- PR 不通过 E2E 不能合并 master
- 工作台首屏加载只渲染当前 module，其他 module 懒加载
- 所有 mock 能力都带视觉标记

---

## 7. Track 5：真实工具 + 插件系统（2 周，可与 Track 6 并行）

### 7.1 真实工具补实

| ID | 任务 | 关联 issue |
| --- | --- | --- |
| T5-1 | `web_search` skill：接 Tavily/Serper/Bing API，权限分级，结果带 source_url + 引用编号 | #57 |
| T5-2 | `code_execute` skill：WASM sandbox（wasmer/wasmtime），限制内存/CPU/网络，输入输出 size cap | #58、#12 |
| T5-3 | `file_ops` skill：受限工作目录读写，路径校验防穿越，高风险操作走 approval | #72 |
| T5-4 | `http_request` skill：域名白名单 + 超时 + size cap + approval | #72 |
| T5-5 | 每个工具接入 Track 1 的 `tool_invocations` 表，调用全留痕 | #92、#18 |

### 7.2 插件 / MCP

| ID | 任务 | 关联 issue |
| --- | --- | --- |
| T5-6 | MCP 插件目录页：列已安装、可安装、启停、配置、测试连接 | #22、#69 |
| T5-7 | 插件真实加载：从 `~/.mapleos/plugins/` 扫描 manifest，动态注册到 ToolRegistry | #69 |
| T5-8 | 插件沙箱：第三方插件默认 `read_only`，提权走 approval | #69 |
| T5-9 | Skills/Workflow 模板市场：先做静态目录（repo 内 JSON），版本+评分展示 | #23 |

**验收**：四个工具能跑通真实调用链，权限/审批/审计/失败路径都有 E2E。

---

## 8. Track 6：桌面 / 同步 / CLI / 移动（2 周，最后做）

| ID | 任务 | 关联 issue |
| --- | --- | --- |
| T6-1 | Tauri 2 项目结构对齐 `apps/desktop/src-tauri/`，Web/Tauri 共享前端代码 | #63、#87 |
| T6-2 | 干净 VM 复验：按 README 跑 `pnpm tauri dev`，env 检查脚本+错误提示完善 | #85、#87 |
| T6-3 | 原生菜单 / 系统通知 / 文件对话框接入 approval/task/notification | #64 |
| T6-4 | 自动更新：updater 配置 + 发布脚本 + 回滚提示 | #65 |
| T6-5 | WebDAV 同步硬化：冲突可解释（保留两份 + diff），CRDT 替换自定义 merge（用 `automerge`） | #70 |
| T6-6 | CLI：`maple login` / `maple agent run` / `maple workflow run` / `maple trace <id>` | #25 |
| T6-7 | 移动端 Expo RN：审批通知 + 聊天 + 任务轻工作台（不做编辑） | #68 |

**验收**：干净环境按文档能跑起 Web 和 Desktop；CLI 能跑 agent run 并看 trace；移动端能审批。

---

## 9. 时间表（含并行）

| 周 | Track 0 | Track 1 | Track 2 | Track 3 | Track 4 | Track 5 | Track 6 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| W1 | ■■■■■ | | | | | | |
| W2 | | ■■■■■ | □ | □ | | | |
| W3 | | ■■■■■ | ■■■■■ | ■■■■■ | | | |
| W4 | | ■■■ | ■■■■■ | ■■■■■ | □ | | |
| W5 | | | ■■■■■ | ■■■■■ | ■■■■■ | □ | |
| W6 | | | ■■■ | | ■■■■■ | ■■■■■ | □ |
| W7 | | | | | ■■■ | ■■■■■ | ■■■■■ |
| W8 | | | | | | ■■■■■ | ■■■■■ |
| W9 | | | | | | ■■■ | ■■■■■ |
| W10 | | | | | | | ■■■■■ |

总工期约 10 周（2.5 个月）。■ 进行中，□ 准备/收尾。

---

## 10. PR / Commit 规范

### 10.1 分支命名

```
feat/closure-track1-execution-chain
feat/closure-track2-canvas-editor
feat/closure-track3-llm-config
feat/closure-track3-learning-governance
feat/closure-track4-e2e-gate
feat/closure-track4-frontend-ia
feat/closure-track5-real-tools
feat/closure-track6-desktop
```

### 10.2 PR 模板（强制）

```markdown
## Closes / Refs
Closes #<issue>

## User entry
<用户从哪个入口触发>

## Runtime path
<代码执行路径，关键文件:行号>

## Persistence path
<写入了哪些表/文件>

## Error path
<失败时用户看到什么，如何恢复>

## Validation evidence
- [ ] Playwright: <test name>
- [ ] Backend test: <test name>
- [ ] Screenshot: <link>
- [ ] Trace id: <id>
```

### 10.3 CI 门禁

- `cargo test --workspace`
- `pnpm build`
- `pnpm lint`
- `pnpm playwright test --project=product-gate`
- 任一失败阻断合并

---

## 11. 风险与缓解

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| Track 1 重构破坏现有 chat/workflow | HIGH | 保留旧 status 字段一个版本周期，新事件链稳定后再删；E2E 先跑通再合并 |
| Canvas 改动大，回归风险高 | HIGH | 拆 5 个小 PR（CRUD / validate / version / run-trace / failure-recovery）逐个合并 |
| Learning 治理过度限制导致正常候选被拒 | MEDIUM | 阈值做成可配置，默认 0.7 偏保守，跑两周看数据再调 |
| Tauri 升级破坏桌面端 | HIGH | 在干净 VM 复验，不依赖开发者本地环境 |
| 工具沙箱性能差 | MEDIUM | WASM 沙箱先支持 Python 子集，扩展按需迭代 |
| 社区贡献不符合闭环标准 | MEDIUM | PR 模板 + CI 门禁 + 维护者 review checklist |

---

## 12. 立即可启动的第一周工作清单

按依赖顺序，第一周（W1）必须完成 Track 0 全部 + Track 1 数据层：

1. **D1**：跑 baseline，记录到 `docs/baseline-2026-06-20.md`（T0-1）
2. **D2**：21 个 open issue 体检，给已实现的打 `needs-verification`，能关的关（T0-2）
3. **D2-D3**：写 `012_execution_events.sql` + `013_tool_invocations.sql` 迁移（T0-3）
4. **D3-D4**：拆 `server/src/main.rs` 成 routes 模块（T0-4）
5. **D4-D5**：写 `docs/execution-fact-chain-spec.md`（T0-5）
6. **D5**：起 `feat/closure-track1-execution-chain` 分支，开始 T1-1 `ExecutionRecorder` 模块

第二周（W2）开始 Track 1 后端契约 + Track 2 / Track 3 准备。

---

## 13. 成功指标（Track 4 完成时）

| 指标 | 当前 baseline | Track 4 完成后 |
| --- | --- | --- |
| E2E 通过率 | smoke 1 个 | 5 个 describe block 全绿 |
| Open P0 issue | 3 个（#89 #90 #86 关联） | 0 |
| 执行事实链覆盖率 | 0%（无统一表） | Chat/Workflow/Task/Agent 100% 走统一 recorder |
| LLM 配置可复现 | ❌ #86 bug | ✅ 本地 Ollama 自动列模型 + 测试连接 |
| Learning 治理 | 无门禁 | 质量分+回滚+污染防护+next-run 验证全有 |
| 前端 module 数 | 1 个超大工作台 | 8 个独立 module + 5 状态组件 |
| CI 阻断合并 | ❌ 无 | ✅ PR 必须过 E2E |
