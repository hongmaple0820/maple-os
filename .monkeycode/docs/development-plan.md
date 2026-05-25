# MapleOS 开发计划 (2026-05-24)

> 基于 .monkeycode/docs/product-status-audit.md 审计结果制定

---

## Phase 1: 闭环修复 (1-2 周)

目标: 消灭所有阻断端到端使用的缺陷，让产品基本跑通。

| # | 任务 | 涉及模块 | 预估 | 优先级 |
|---|---|---|---|---|
| 1.1 | Chat SSE 流式输出 | Rust backend + Web chat-panel | 1d | P0 |
| 1.2 | Workflow SSE 实时节点状态 | Rust EventBus + Web workflow-manager | 1d | P0 |
| 1.3 | Chat→Knowledge 交叉引用 | Web chat-panel + knowledge-manager | 0.5d | P0 |
| 1.4 | memory_search_handler 接口对齐 | Rust /api/memories/search (前端传 query+limit, 后端期望 keyword+memory_type+limit) | 0.5d | P0 |
| 1.5 | kb_search_handler 补充 source_type | Rust /api/kb/search 结果缺少 source_type 字段 | 0.5d | P1 |

**1.1 Chat SSE 流式输出**

- 后端: `/api/chat` 改为 SSE stream, 逐 token 推送 `data: {"token": "...", "done": false}`
- 前端: chat-panel.tsx 用 EventSource 接收, 逐步拼接到 assistantMsg.content
- 兼容: 保留 JSON 响应模式作为 fallback (非 SSE 客户端)

**1.2 Workflow SSE 实时节点状态**

- 后端: WorkflowExecutor 执行节点时 EventBus 发布 NodeStarted / NodeCompleted / NodeFailed
- 前端: workflow-manager.tsx 订阅 `/api/events` SSE, 匹配 workflow_id, 更新 canvasNodes 状态 Badge
- 执行历史: 新增 workflow_executions 列表 UI + 详情查看

**1.3 Chat→Knowledge 交叉引用**

- Chat 输入框旁加"搜索知识库"按钮
- 发送 `/api/kb/search` 查询, 结果插入对话上下文作为 system message
- Agent 可引用知识库结果回答

**1.4 memory_search 接口对齐**

- 前端调用 `/api/memories/search?query=X&limit=10`
- 后端当前期望 `{keyword, memory_type, limit}` POST body
- 修改: 支持 GET query 参数 + POST body 双模式

**1.5 kb_search 补 source_type**

- 从 kb_documents 表关联 source_type
- 搜索结果每条加 source_type 字段

---

## Phase 2: 核心体验 (2-3 周)

目标: 补实存根，让核心功能可用而非 mock。

| # | 任务 | 涉及模块 | 预估 | 优先级 |
|---|---|---|---|---|
| 2.1 | web_search 技能补实 | Rust skill_registry → Searxng/DuckDuckGo API | 2d | P1 |
| 2.2 | code_execute 技能补实 | Rust → Docker sandbox / WASM runtime | 3d | P1 |
| 2.3 | Scheduler 后台启动 | Rust main.rs 启动 Cron 循环 | 0.5d | P1 |
| 2.4 | routing_rules.yaml 默认文件 | Rust LlmRouter 加载 YAML 路由规则 | 1d | P1 |
| 2.5 | Workflow 执行历史 UI | Web workflow-manager 新增历史列表 + 详情 | 1d | P1 |
| 2.6 | Session 管理 UI | Web chat-panel 新建/切换/删除 session | 1d | P1 |

**2.1 web_search 补实**

- 优先方案: 调用 Searxng 自建实例 (隐私友好)
- 备选: DuckDuckGo Lite HTML API (无 API key)
- 返回: { results: [{title, url, snippet}], query }
- 无外部服务时: 降级为当前 mock 提示

**2.2 code_execute 补实**

- 方案 A: Docker 容器执行 (需要 Docker runtime)
- 方案 B: WASM wasmtime runtime (纯 Rust, 无外部依赖)
- 推荐: WASM 方案, 与 local-first 理念一致
- 支持: Python / JavaScript / Rust 编译为 WASM

**2.3 Scheduler 启动**

- main.rs 中 tokio::spawn 启动 Scheduler::run_loop
- 从 scheduled_jobs 表读取 Cron 任务
- 到达 next_run_at 时触发 workflow.execute

**2.4 routing_rules.yaml**

- 创建 infra/routing_rules.yaml 默认文件
- LlmRouter 启动时从 YAML 加载规则
- 条件匹配: task_type, privacy_level, budget, latency

---

## Phase 3: 桌面端 (2-3 周)

目标: Tauri 2 桌面客户端可运行，提供原生体验。

| # | 任务 | 涉及模块 | 预估 | 优先级 |
|---|---|---|---|---|
| 3.1 | Tauri 2 项目结构完善 | apps/desktop/ src-tauri/Cargo.toml + tauri.conf.json + icons | 1d | P2 |
| 3.2 | 桌面端嵌入 Web 前端 | Tauri WebView 加载 localhost:3000 或本地打包 | 1d | P2 |
| 3.3 | Rust 后端随桌面启动 | Tauri sidecar 启动 mapleos-server | 1d | P2 |
| 3.4 | SCALE bridge 随桌面启动 | Tauri sidecar 启动 bridge-http.mjs | 0.5d | P2 |
| 3.5 | 原生菜单 + 通知 + 文件系统 | Tauri API: fs/notification/menu/dialog | 2d | P2 |
| 3.6 | 桌面端自动更新 | Tauri updater plugin | 1d | P3 |

**3.1-3.4 桌面端基础**

- src-tauri/Cargo.toml: tauri 2 依赖 + mapleos-server 依赖
- tauri.conf.json: 窗口 1200x800, title "MapleOS", devUrl localhost:3000
- Sidecar: 启动 Rust 后端 (7788) + SCALE bridge (7790)
- 构建产物: macOS .dmg / Windows .msi / Linux .AppImage

**3.5 原生能力**

- 文件系统: 直接读写本地文件 (Tauri fs API)
- 通知: 任务完成/失败桌面通知
- 菜单: 自定义菜单栏 (文件/编辑/视图/帮助)
- 对话框: 确认/输入/文件选择

---

## Phase 4: 测试 & 质量 (1-2 周)

目标: 自动化测试覆盖核心流程，CI 集成。

| # | 任务 | 涉及模块 | 预估 | 优先级 |
|---|---|---|---|---|
| 4.1 | Playwright E2E 框架搭建 | tests/e2e/ + playwright.config.ts | 1d | P2 |
| 4.2 | Dashboard E2E | 页面加载 + 指标卡显示 + 状态 Badge | 0.5d | P2 |
| 4.3 | Chat E2E | 发送消息 + 接收回复 + Agent 选择 | 1d | P2 |
| 4.4 | Workflow E2E | 创建 + 执行 + 查看结果 | 1d | P2 |
| 4.5 | Knowledge E2E | 索引 + 搜索 + 结果评分 | 0.5d | P2 |
| 4.6 | Rust 单元测试补充 | core 各 crate 关键路径测试 | 2d | P3 |
| 4.7 | CI Pipeline | GitHub Actions: cargo test + pnpm build + playwright | 1d | P3 |

**4.1 Playwright 框架**

- pnpm add -D @playwright/test
- playwright.config.ts: baseUrl localhost:3000, webServer 启动命令
- 测试需要: Rust 后端 7788 运行 + SCALE bridge 7790 运行
- fixtures: 启动/停止后端服务

---

## Phase 5: 移动端 (3-4 周)

目标: React Native 骨架 + 共享 SDK + 核心页面。

| # | 任务 | 涉及模块 | 预估 | 优先级 |
|---|---|---|---|---|
| 5.1 | Expo 项目初始化 | apps/mobile/ + Expo Router + TypeScript | 1d | P3 |
| 5.2 | 共享 SDK 包 | packages/sdk RN 兼容 (去掉 isomorphic-ws) | 0.5d | P3 |
| 5.3 | 移动端 Chat 页面 | React Native Chat UI + SSE | 2d | P3 |
| 5.4 | 移动端 Dashboard | RN 卡片布局 + 指标 | 1d | P3 |
| 5.5 | 移动端 Knowledge | RN 搜索 + 索引 | 1d | P3 |
| 5.6 | 移动端 Agent Center | RN Agent 列表 + 协作 | 2d | P3 |

**5.1 Expo 初始化**

- npx create-expo-app apps/mobile --template tabs
- Expo Router (文件路由)
- 与 Web 共享 packages/sdk + packages/ui (RN 适配)

---

## Phase 6: 生态完善 (持续)

目标: 插件生态 + 真正的 CRDT + 共享配置。

| # | 任务 | 涉及模块 | 预估 | 优先级 |
|---|---|---|---|---|
| 6.1 | Plugins 目录 + 真实加载机制 | Rust MCP Host + Web 插件配置 | 2d | P3 |
| 6.2 | Automerge CRDT 替换自定义 merge | Rust maple-sync | 3d | P3 |
| 6.3 | packages/config 共享配置包 | 主题常量 / 默认值 / API 路径 | 1d | P3 |
| 6.4 | file_ops 技能补实 | Rust → Tauri fs API / MCP server | 2d | P3 |
| 6.5 | http_request 技能补实 | Rust reqwest 真实 HTTP 调用 | 1d | P3 |

---

## 时间线概览

```
Week 1-2:  Phase 1 — 闭环修复 (SSE streaming + 接口对齐)
Week 3-5:  Phase 2 — 核心体验 (技能补实 + Scheduler + UI 完善)
Week 6-8:  Phase 3 — 桌面端 (Tauri 2 + 原生能力)
Week 9-10: Phase 4 — 测试 & 质量 (Playwright + CI)
Week 11+:  Phase 5 — 移动端 (Expo + 共享 SDK)
持续:       Phase 6 — 生态完善 (插件 + CRDT + 配置)
```

## 验收标准

- Phase 1 完成: 用户可 Chat 流式对话 + Workflow 实时执行 + Knowledge 搜索引用
- Phase 2 完成: 内置技能至少 3 个真实可用 + Scheduler 运行 + Session 管理
- Phase 3 完成: macOS/Windows/Linux 安装包可下载运行
- Phase 4 完成: CI 全绿 + 5 个核心 E2E 测试通过
- Phase 5 完成: iOS/Android 可安装 Chat + Dashboard