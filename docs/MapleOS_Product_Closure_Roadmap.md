# MapleOS Product Closure Roadmap

版本：2026-06-15  
范围：未来待实现功能、缺陷问题、产品未闭环情况、GitHub issue 维护口径

## 1. 目的

这份文档是 MapleOS v3 后续落地的产品闭环单一事实源。它不替代 `MapleOS_v3_Design_Blueprint.md` 和 `MapleOS_v3_Engineering_Handbook.md`，而是把当前实现、缺陷、未闭环业务流和 GitHub issues 对齐到可验收的交付切片。

每个后续切片必须同时回答四个问题：

1. 用户业务动作是否能从入口走到结果，而不是只完成页面或局部接口。
2. 执行事实是否能被 `execution_events`、`tool_invocations`、`tasks`、`audit`、`activity` 追踪。
3. 失败、审批、重试、取消、恢复、沉淀是否有可见路径。
4. 是否有自动化或人工可复现验收证据。

## 2. 状态口径

| 状态 | 含义 | 关闭要求 |
| --- | --- | --- |
| Closed | 本地实现、UI 交互、数据链路和测试/截图证据已经闭合 | 可关闭对应 issue |
| Partially closed | 核心链路可跑，但状态、异常、E2E 或跨模块串联不足 | 保持 issue 打开，补验收项 |
| Open | 未实现或仍是 mock/静态展示 | 必须进入 backlog |
| Needs verification | 代码中已有实现迹象，但需要按真实用户路径复验 | 先验证，再关闭或拆分 |

## 3. 已闭合或接近闭合的链路

| 链路 | 当前判断 | 剩余风险 | GitHub 映射 |
| --- | --- | --- | --- |
| Rig tool approval 自检 | Partially closed。已跑通 `agent run -> Rig tool_call -> tool_approval task -> approve -> platform_publish -> agent resume -> chat final reply` | 仍缺 Playwright 回归和 CI 门禁 | #89，关联 #66/#67 |
| Chat runtime context preview | Partially closed。已有上下文来源、学习候选和流式面板实现 | 需要确认所有 Agent/Session 入口都走同一解析器 | #52、#54、#55、#56 |
| Workflow artifact 写入 KB | Partially closed。已有 artifact -> KB 去重与沉淀路径 | 需要把审批、失败重试、trace 展示纳入 E2E | #89、#92 |
| Learning governance | Partially closed。已有候选、审批、写入 KB/Memory/Prompt 的基础能力 | 缺质量门禁、回滚、污染防护和下一轮生效验证 | #91 |
| LLM provider settings | Needs verification。已有统一设置入口和供应商配置迹象 | 本地模型展示仍有公开 bug，需要按真实配置复验 | #86 |
| Desktop/Tauri first run | Needs verification。已有 desktop/Tauri 方向 issue | 版本结构、首次运行、安装体验未验收 | #85、#87、#63 |

## 4. P0 未闭环问题

| 编号 | 问题 | 用户影响 | 目标闭环 | 验收标准 |
| --- | --- | --- | --- | --- |
| P0-1 | 缺产品级 E2E 门禁 | 每次补功能后容易再次断链，用户只能靠手点发现问题 | 建立 chat、workflow、tool approval、learning、LLM settings 的 Playwright + CI 回归 | PR 上自动跑通核心路径，失败阻断合并 |
| P0-2 | Workflow Canvas 仍不像真实工作流编辑器 | 用户看得到节点，但不能稳定完成创建、编辑、校验、运行、回溯 | 节点 CRUD、连线、参数校验、版本保存、运行 trace、失败恢复串起来 | 新建流程后可运行，审批节点可恢复，历史版本可对比 |
| P0-3 | 执行事实链仍未成为所有模块的单源 | 页面状态可能互相割裂，审计、任务、trace、通知不一致 | 所有运行入口写入统一 execution fact chain，UI 从同一事实链解释状态 | Chat/Workflow/Task/Agent 面板展示同一个 execution id 的完整事件 |
| P0-4 | 统一 LLM 配置仍有可用性缺陷 | 用户配置模型后不确定是否真正生效，生图/对话模型割裂 | Provider、API key、chat model、image model、测试连接、脱敏展示、继承关系统一 | #86 复现路径关闭；Agent 创建、聊天、生图都能看到并使用同一配置 |
| P0-5 | 首次运行/桌面版本体验不稳定 | 下载项目或运行桌面端时直接失败，破坏产品信任 | 明确 Web/Tauri/CLI 启动路径，补 env 检查、错误提示和安装文档 | #85、#87 可按干净环境复验通过 |

## 5. P1/P2 未来功能分组

### 5.1 Agent 与 Chat

| 功能 | 当前状态 | 交付目标 | 关联 issue |
| --- | --- | --- | --- |
| Agent 中心三步创建 | Partially closed | 角色、模型、工具权限、KB/Memory 绑定、试运行在一个闭环内完成 | #92、#24 |
| 私聊流式消息 | Needs verification | SSE 统一事件解析，工具调用、上下文、产物、学习候选同屏可追踪 | #52 |
| 群聊协作规则 | Open/Needs verification | 四步创建：目标、成员、规则、触发器；规则触发能生成任务和 trace | #16、#24 |
| 工具权限与远程审批 | Partially closed | 高风险工具进入审批任务，批准后恢复 Agent 执行，拒绝后可解释失败 | #89 |

### 5.2 Workflow 与 Trigger

| 功能 | 当前状态 | 交付目标 | 关联 issue |
| --- | --- | --- | --- |
| Canvas 真编辑 | Open | 节点 CRUD、连线、参数 schema、版本、运行、trace 全闭环 | #90、#17、#61 |
| Manual/Webhook/Schedule/Message/Task Event 触发 | Partially closed | 五类触发统一进入 execution_events，并驱动 workflow_runs/tasks/audit/activity | #15、#16、#59 |
| 人工审批节点 | Partially closed | 审批卡片可查看上下文、工具、产物；批准后恢复运行，拒绝后进入可恢复失败 | #92 |
| 失败重试与死信 | Open/Needs verification | 节点失败有原因、重试策略、死信列表、恢复入口 | #92 |

### 5.3 Knowledge、Memory 与 Evolver

| 功能 | 当前状态 | 交付目标 | 关联 issue |
| --- | --- | --- | --- |
| KB 文档上传/切块/检索 | Partially closed | 文档、chunk、source、score、引用解释完整展示 | #56 |
| Memory owner 隔离与检索 | Partially closed | semantic/episodic/working memory 按 owner、source、status、confidence 隔离 | #55 |
| Learning candidate 审批 | Partially closed | 执行完成生成候选，人工批准后写入 KB/Memory/Prompt，拒绝不污染上下文 | #91 |
| Prompt Policy Pack | Partially closed | identity/tool/evidence/memory/risk/style 策略参与运行时 resolve | #91 |
| 下一轮生效验证 | Open | 批准沉淀后，下次 Agent context preview 能解释命中的 KB/Memory/Prompt 来源 | #91 |

### 5.4 Plugins、Skills、MCP 与工具

| 功能 | 当前状态 | 交付目标 | 关联 issue |
| --- | --- | --- | --- |
| `web_search` 真实能力 | Open/Partially mocked | 权限、调用、结果引用、失败路径、审计完整 | #57 |
| `code_execute` sandbox | Open | WASM/隔离执行、输入输出限制、审批和日志 | #58、#12 |
| `file_ops` 与 `http_request` | Open | 可授权、可测试、可追踪的 Rig tools | #72 |
| MCP 插件发现安装 | Open | 插件目录、安装、启停、授权、测试连接 | #22、#69 |
| Skills/模板市场 | Open | 技能包、工作流模板、版本、评分、收益/治理 | #23 |

### 5.5 Desktop、Sync、CLI、Mobile

| 功能 | 当前状态 | 交付目标 | 关联 issue |
| --- | --- | --- | --- |
| Tauri 2 桌面结构 | Needs verification | Web/Tauri 版本一致，首次运行可用 | #63、#87 |
| 桌面菜单/通知/文件系统 | Open | 原生能力接入审批、任务和同步状态 | #64 |
| 桌面自动更新 | Open | 发布、检查、回滚、失败提示完整 | #65 |
| WebDAV/CRDT 同步 | Open/Needs verification | 本地优先，冲突可解释，最终一致 | #70 |
| CLI 客户端 | Open | 登录、Agent run、workflow run、trace 查看 | #25 |
| 移动端 | Open | 先做审批、通知、聊天、任务轻工作台 | #68 |

### 5.6 Observability 与 Governance

| 功能 | 当前状态 | 交付目标 | 关联 issue |
| --- | --- | --- | --- |
| Audit logs | Needs verification | 用户动作、系统动作、工具动作、审批动作可检索 | #18 |
| Circuit breaker/load balancing/priority queue | Open | Agent 调度稳定性、限流、成本保护 | #19、#20、#21 |
| Rerank 与检索质量评估 | Open | KB/Memory context 质量可评分、可回归 | #14 |
| Issue hygiene | Open | 老 issue 验证、补标签、拆分/关闭 stale 条目 | 新增 issue hygiene issue |

## 6. 缺陷与技术债

| 问题 | 影响 | 处理方式 |
| --- | --- | --- |
| 前端工作台单体过大 | 交互状态难维护，新增功能容易割裂 | 拆成 Dashboard、Messages、Agents、Workflows、Tasks、Knowledge、Plugins、Settings 模块 |
| 旧 issue 与当前实现不同步 | 已实现项仍显示 open，真正缺口被淹没 | 每个 phase 前做 issue hygiene，给 `needs-verification` 标记 |
| Mock 工具与真实工具边界不清 | 用户以为功能可用，实际不能完成任务 | 所有 mock 能力在 UI 标记 disabled/mock，并建真实能力 issue |
| 文档和终端编码可能显示异常 | 中文文档审核与复制容易误判 | 统一 UTF-8，CI 增加文档编码检查 |
| 缺少截图/E2E 证据索引 | 质量状态不可追溯 | 每个 closed issue 附测试命令、截图或 trace id |

## 7. 后续六个执行阶段

| 阶段 | 目标 | 产出 | 关闭条件 |
| --- | --- | --- | --- |
| Phase A | Issue hygiene + E2E gate | 标签、闭环 issues、Playwright 核心路径 | P0 E2E issue 能在 CI 跑起来 |
| Phase B | Chat/Task/Workflow execution fact chain | 统一 trace UI、审批恢复、失败重试 | 任一 execution id 可解释完整事件 |
| Phase C | Learning governance hardening | 质量门禁、回滚、污染防护、下一轮生效验证 | 批准/拒绝学习都有可复现实验 |
| Phase D | Plugin real tools | web_search、code_execute、file_ops、http_request 真实能力 | 工具调用可授权、可测试、可审计 |
| Phase E | Desktop/first-run packaging | Tauri 版本一致、首次运行、安装文档 | 干净环境按文档可启动 |
| Phase F | Sync/CLI/Mobile | WebDAV/CRDT、CLI、移动审批工作台 | 跨端状态一致，冲突可解释 |

## 8. GitHub issue 维护规则

1. 每个产品闭环问题必须有一个 GitHub issue，并绑定 priority、phase、area、`product-closure` 或 `needs-verification` 标签。
2. issue 标题采用 `[P级-类型] 范围：可验收目标`，避免只写“优化”“完善”。
3. issue body 必须包含用户价值、当前缺口、验收标准、关联现有 issue、验证证据。
4. 只在有测试命令、浏览器截图、trace id 或可复现手工验收后关闭。
5. 已有实现但未复验的 issue 不关闭，先加 `needs-verification`。
6. 每次推进前先更新本文件和 GitHub issue 状态，避免产品路线和工程事实漂移。

## 9. 当前 GitHub issue 映射

| 路线项 | Issue |
| --- | --- |
| 产品闭环 E2E 回归门禁 | #89 |
| Workflow Canvas 真编辑闭环 | #90 |
| 自学习治理闭环硬化 | #91 |
| 统一执行事实链 | #92 |
| 前端信息架构模块化 | #93 |
| GitHub issue 盘点与旧 issue 验证关闭 | #94 |
| 本地 LLM 模型配置展示错误 | #86 |
| 项目下载后第一次运行不起来 | #85 |
| Tauri 版本与代码实现不一致 | #87 |
| Playwright E2E 框架 | #66 |
| CI Pipeline | #67 |
| Workflow SSE 实时节点状态 | #53 |
| Chat SSE 流式输出 | #52 |
| Memory/KB context 对齐 | #55、#56 |
| 工具真实能力 | #57、#58、#72 |
| 插件与生态 | #22、#23、#69 |
## 10. Open-source co-build handoff

剩余的迭代升级和产品闭环实现，统一以 `docs/MapleOS_Open_Source_Cobuild_Backlog.md` 作为开源共建入口。

后续社区 PR 至少要满足四点：

- 不只做页面，要说明真实用户路径
- 要写清 runtime path 和 data path
- 要覆盖失败、恢复或重试路径
- 要附带自动化或可复现的验证证据
