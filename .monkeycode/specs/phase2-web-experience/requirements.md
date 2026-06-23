# Phase 2: Web 体验补全 — 需求规格说明书

> 日期: 2026-05-26
> 优先级: P1
> 目标: 补全 Web 前端的核心体验断点，提升可用性和可维护性

---

## 1. Session 历史消息加载 (MUST)

### 1.1 切换 Session 加载历史

**EARS 模式**: 当用户在 Chat 页面切换 Session 时，系统应加载该 Session 的历史消息并展示在聊天区域，用户可以继续在该 Session 中对话。

**需求明细**:
- 切换 Session 后调用 `GET /api/sessions/:id/messages` 加载历史
- 历史消息按时间顺序排列，最新的在底部
- 加载过程中显示 spinner
- 历史消息超过 50 条时，仅加载最近 50 条，顶部显示"加载更多"按钮
- 当前状态: 切换 Session 后消息清空，用户丢失上下文

### 1.2 新建 Session 默认行为

**EARS 模式**: 当用户点击"新建对话"时，系统应创建空白 Session，用户开始新对话后 Session 自动获取服务端分配的 ID。

**需求明细**:
- 新建 Session: 清空消息区域，显示空白对话
- 发送第一条消息后: SSE done 事件中返回 session_id，更新 Session 列表
- Session 列表排序: 按最近更新时间降序

---

## 2. Workflow 已有工作流编辑 (MUST)

### 2.1 加载已有工作流到画布

**EARS 模式**: 当用户在 Workflow 页面选择一个已有工作流时，系统应将其定义加载到 DAG 画布中，用户可以编辑节点和连线。

**需求明细**:
- 左侧列表点击工作流: 加载其 definition 到画布
- 画布节点: 根据 definition 的 nodes 数组渲染节点(位置、类型、配置)
- 画布连线: 根据 definition 的 edges 数组渲染连线
- 编辑节点: 右侧面板可修改节点配置，修改实时反映到画布
- 当前状态: 只能新建工作流，无法编辑已有工作流

### 2.2 保存工作流修改

**EARS 模式**: 当用户编辑已有工作流并点击保存时，系统应将修改后的定义更新到后端。

**需求明细**:
- 保存按钮: 点击后调用 `PUT /api/workflows/:id` 更新
- 保存成功: 显示 Toast 提示"已保存"
- 保存失败: 显示错误信息，不丢失本地修改
- 自动保存: 可选，每 30s 自动保存草稿

### 2.3 节点配置绑定 State (MUST)

**EARS 模式**: 当用户在右侧配置面板修改节点属性时，系统应实时更新画布上对应节点的数据，修改后的配置在执行工作流时生效。

**需求明细**:
- 配置面板字段修改: onChange 更新 canvasNodes 中对应节点的 config 字段
- 当前状态: Input 修改不更新 state，配置修改不生效

---

## 3. Workflow 导出与导入 (SHOULD)

### 3.1 导出为 JSON

**EARS 模式**: 当用户在 Workflow 页面点击"导出"时，系统应将当前工作流定义导出为 JSON 文件，用户可下载保存。

**需求明细**:
- 导出按钮: 在工作流列表项和画布工具栏中
- 导出格式: JSON 文件，包含 name, nodes, edges, variables
- 文件名: `{workflow_name}.json`

### 3.2 导入 JSON/YAML

**EARS 模式**: 当用户在 Workflow 页面点击"导入"时，系统应打开文件选择器，用户选择 JSON 或 YAML 文件后解析为工作流定义。

**需求明细**:
- 支持格式: JSON(优先) 和 YAML
- 导入后: 在列表中新增工作流(调用 workflow.create)
- 格式错误: 显示"文件格式无效"

---

## 4. SDK 完整封装 (MUST)

### 4.1 业务 RPC 方法封装

**EARS 模式**: 当 Web/Mobile 前端调用 MapleOS API 时，系统应通过 `@mapleos/sdk` 的统一方法而非内联 `rpcCall()` 调用，确保接口变更时只需修改 SDK 层。

**需求明细**:
- SDK 补全 12+ 个业务 RPC 方法封装: workflow.list/create/execute, agent.list/register/deregister/chat, llm.models, skill.list/install/uninstall, task.create, config.get/update, scale.tools/call
- SDK 补全 REST API 封装: kb.search/index/documents/upload, sessions.list/messages, memories.search/store, tasks.stats/enqueue
- SDK 补全 Auth 管理: login/register/refresh/token
- 前端逐步从内联调用迁移到 SDK 方法

### 4.2 SSE 通用订阅工具

**EARS 模式**: 当前端组件需要订阅实时事件时，系统应通过 SDK 的 `createEventSubscription()` 方法创建 SSE 连接，避免各组件重复实现 EventSource 逻辑。

**需求明细**:
- SDK 提供 `createEventSubscription(eventTypes)` 方法
- 返回 Observable/回调接口: onEvent, onError, onClose
- 自动重连: 连接断开后 5s 自动重连
- 多组件共享: 同一 EventSource 连接可被多个组件订阅

---

## 5. UI 状态管理与动效 (SHOULD)

### 5.1 Zustand 状态管理

**EARS 模式**: 当 Web 前端需要跨组件共享状态时，系统应通过 zustand store 管理，避免 prop drilling 和 useState 蔓延。

**需求明细**:
- 核心 store: useAuthStore(token/user), useChatStore(messages/sessions/agent), useWorkflowStore(canvas/workflows/executions)
- zustand 已安装(packages.json 有依赖)但当前未使用

### 5.2 Framer Motion 微动效

**EARS 模式**: 当 Web 前端页面切换和列表更新时，系统应使用 framer-motion 提供过渡动效，提升视觉流畅度。

**需求明细**:
- 页面切换: opacity + translateY 动画 (200ms)
- 列表项增删: layoutAnimation 自动排位
- 消息出现: fadeIn + slideUp (150ms)
- framer-motion 已安装但当前未使用

---

## 验收标准

1. Chat 切换 Session 后加载历史消息，用户可继续对话
2. Workflow 点击已有工作流加载到画布，可编辑节点和连线
3. Workflow 节点配置修改绑定 state，修改后保存生效
4. SDK 提供 12+ 业务 RPC 封装 + SSE 订阅工具
5. 前端至少 3 个核心模块迁移到 zustand store
6. 页面切换有动效过渡(非瞬间跳切)