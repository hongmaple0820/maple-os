# MapleOS 产品状态全景 (2026-05-26 更新)

> 本次审计基于代码库实际扫描，更新 2026-05-24 版本的审计结论
> 5月24日标记的 7 个 P0 阻断性问题已全部修复，5 个闭环断点已全部走通

---

## 一、已实现功能 (真实可用)

### Rust 核心引擎 — 8 个 crate 全部有实质实现，20,670 行，57 源文件

| Crate | 源文件 | 行数 | 核心能力 | 状态 |
|---|---|---|---|---|
| maple-engine | 9 | 4,190 | Workflow DAG (9节点类型) / 执行器 / 任务队列 / 事件总线 / 检查点 / Hook / 技能注册 / Cron调度 | **真实** |
| maple-llm | 11 | 2,466 | LLM Router / Ollama / OpenAI / Anthropic / DeepSeek / GLM 适配器 / Embedding / Budget / SSE真流式 | **真实** |
| maple-agent | 8 | 2,324 | Agent 注册/注销 / ReAct循环 / 会话存储 / 权限策略 / 多Agent编排 / 任务委派 | **真实** |
| maple-kb | 8 | 2,210 | BM25搜索 / 向量存储(Qdrant+InMemory) / 混合检索(RRF) / 分块索引 / 三层记忆 / 自演化引擎 / Prompt版本管理 | **真实** |
| maple-sync | 4 | 1,118 | WebDAV同步 / CRDT合并 / 冲突解决(3策略) / 后台同步循环 | **真实** |
| maple-gateway | 7 | 2,664 | JWT认证+RBAC / WebSocket网关 / SSE流 / Webhook签名 / MCP Host (Stdio/HTTP/WS) / 多平台消息适配 | **真实** |
| maple-collab | 5 | 1,626 | Workspace CRUD / FMP协议 / 实时广播 / 群组规则引擎 | **真实** |
| maple-rpc | 4 | 372 | JSON-RPC 2.0 / 异步分发器 / HTTP handler | **真实** |
| server/main.rs | 1 | 3,708 | 全部路由+handler+启动流程 | **真实** |

### REST API — 50+ 端点，29 个路径

- 认证: `/api/auth/login`, `/api/auth/register`, `/api/auth/refresh`, `/api/auth/token`
- Chat: `/api/chat`(非流式), `/api/chat/stream`(SSE真流式)
- Workflow: `/api/workflows/:id`(GET/PUT/DELETE), `/api/workflows/:id/executions`, `/api/workflows/:id/stats`, `/api/executions/:id`, `/api/executions/:id/checkpoints`
- Agent: `/api/agents`(GET/POST), `/api/agents/:id`(GET/DELETE), `/api/agents/:id/heartbeat`, `/api/agents/status`
- KB: `/api/kb/index`, `/api/kb/search`, `/api/kb/documents`, `/api/kb/upload`(multipart)
- Session: `/api/sessions`(GET), `/api/sessions/:id`(DELETE), `/api/sessions/:id/messages`(GET)
- Memory: `/api/memories`(POST), `/api/memories/search`(POST), `/api/memories/:id`(GET/DELETE)
- Task: `/api/tasks/enqueue`, `/api/tasks/stats`, `/api/tasks/dead-letter`, `/api/tasks/:id/requeue`
- Prompt: `/api/prompts`(POST), `/api/prompts/:ref`(GET), `/api/prompts/:ref/rollback`(POST)
- Sync: `/api/sync/trigger`, `/api/sync/status`
- Config: `/api/config`(GET/PUT)
- Events: `/api/events`(SSE), `/ws/agents`(WebSocket)
- Health: `/health`, `/health/deep`, `/metrics`

### JSON-RPC — 19 个方法已注册

- `system.info`(含完整字段), `system.health`, `workflow.list/create/execute`
- `agent.list/register/deregister/chat`, `workspace.create`
- `llm.models`, `skill.list/install/uninstall`, `task.create`
- `config.get/update`, `scale.tools/call`

### 内置技能 — 5/5 真实实现

| 技能 | 状态 | 实现详情 |
|---|---|---|
| echo | **真实** | 返回输入 |
| web_search | **真实** | Google Custom Search API + DuckDuckGo Lite HTML fallback |
| code_execute | **真实** | JavaScript(node -e) + Python(python3) + 超时控制 |
| file_ops | **真实** | read/write/list/exists + 路径安全校验 |
| http_request | **真实** | reqwest GET/POST/PUT/DELETE/PATCH + 自定义headers |

### Web 前端 — 8 个页面全部有真实 API 调用 + SSE 流式

- Dashboard (system.info含完整字段 + tasks/stats)
- Chat (SSE真流式 + KB自动搜索引用 + KnowledgeRefCard)
- Workflow Editor (SSE实时节点状态 + 执行历史UI + 拖拽画布)
- Agent Center (全部RPC: list/register/deregister/chat + task.create + memories)
- Knowledge (search/index/documents/upload)
- SCALE Engine (双路径: REST+RPC)
- Plugins (skill.list/install/uninstall)
- Settings (config.get/update + 模型列表)
- Auth (登录+注册+Token刷新)
- 命令面板 (Cmd+K)

### Desktop (Tauri v2) — 有实质内容

- sidecar 启动 mapleos-server(7788)
- 复用 Web 前端 (beforeDevCommand: pnpm dev)
- 2 个 Tauri command: greet + get_system_info
- 窗口 1280x800, 插件 shell/notification/dialog/fs

### Mobile (Expo 52) — 基础框架 + 4 Tab

- Tabs: Dashboard / Chat / Knowledge / Agents
- 暗色主题, 使用 SDK RpcClient + mobileRestCall
- 4 页面调用真实 API

---

## 二、已修复的阻断性问题 (对比5月24日)

| 原问题 | 5月24日状态 | 当前状态 |
|--------|-----------|----------|
| agent.chat RPC 未注册 | 断 | **已注册** (含LLM routing+session) |
| agent.register RPC 未注册 | 断 | **已注册** (DB INSERT+registry) |
| task.create RPC 未注册 | 断 | **已注册** (含payload构建) |
| config.get/update 未注册 | 断 | **已注册** (kv_store SQLite) |
| skill.install/uninstall 未注册 | 断 | **已注册** (含MCP server启动/停止) |
| system.info 字段缺失 | 断 | **已修复** (含uptime_secs/agents_count/workflows_count/tasks_count) |
| Task Worker 未启动 | 断 | **已启动** (tokio::spawn, 2s轮询) |
| Chat 无流式输出 | 断 | **已实现** (Sse逐token流式推送) |
| Workflow SSE未订阅 | 断 | **已实现** (EventSource 5种事件) |
| Scheduler 未启动 | 断 | **已启动** (start_loop 60s) |
| 4/5 技能是 mock | 断 | **已修复** (5/5 真实实现) |
| routing_rules.yaml 不存在 | 断 | **已创建** (5规则+fallback链) |

---

## 三、当前存留问题

### 需优化

| 项目 | 当前状态 | 优化方向 |
|------|---------|---------|
| Session 切换不加载历史消息 | 切换后清空 | 加载 `/api/sessions/:id/messages` |
| Workflow 画布不加载已有工作流 | 只能新建 | 选择已有工作流加载到画布 |
| Workflow 节点配置不绑定 state | Input修改不生效 | onChange更新canvasNodes |
| code_execute 无沙箱隔离 | 直接执行node/python3 | Docker/WASM沙箱 |
| file_ops 路径校验前缀匹配 | 前缀匹配可绕过 | canonicalize规范校验 |
| Skill 安装不持久化 | 重启丢失MCP skill | DB installed_skills表 |
| SDK 缺业务RPC封装 | 12+方法前端内联调用 | MapleClient统一封装 |
| SDK 无SSE订阅工具 | 各组件重复EventSource | EventSubscription通用类 |
| zustand/framer-motion 未使用 | 已安装未应用 | 渐进引入状态管理+动效 |
| packages/config 未被前端引用 | 两套配置并行 | 合一 |
| E2E 全基于 Mock | 无真实后端测试 | 真实后端+LLM Mock |
| CI 仅 tag push触发 | 日常分支无CI | PR/push触发 |

### 缺失功能

| 项目 | 严重程度 |
|------|---------|
| Mobile Auth 登录/注册页面 | **高** |
| Mobile Chat SSE streaming | **高** |
| Mobile 4缺失页面(Workflow/SCALE/Plugins/Settings) | 中 |
| Mobile Knowledge索引("coming soon") | 中 |
| @react-native-async-storage 依赖未声明 | **高** |
| Automerge CRDT 替换自定义merge | 低 |
| Workflow 导出/导入 | 低 |

---

## 四、产品闭环状态 (全部已走通)

| 闭环 | 状态 | 关键实现 |
|------|------|---------|
| Workflow 创建→执行→查看结果 | **已通** | SSE实时更新 + 执行历史UI + 后端端点 |
| Chat 对话→Agent协作 | **已通** | SSE真流式 + agent.chat RPC + 双字段兼容 |
| Knowledge 索引→搜索→引用 | **已通** | Chat自动KB搜索注入 + source_type字段 + 引用卡片 |
| Agent 注册→派发任务→执行→结果 | **已通** | agent.register/task.create RPC + Task Worker |
| Settings 配置→生效 | **已通** | config.get/update + kv_store SQLite持久化 |

---

## 五、技术栈完整性检查

| 层 | 技术 | 状态 |
|---|---|---|
| 桌面端 | Tauri 2 | **有实质内容** (sidecar+复用Web+原生插件) |
| Web端 | Next.js 15 | **完整**, 8页面全真实API |
| 移动端 | Expo 52 | **基础框架** (4 Tab+真实API, 缺Auth+SSE) |
| 后端 | Rust Axum | **完整**, 50+端点, 3,708行 |
| 运行时 | Tokio | **完整** |
| Workflow | Petgraph | **完整** (9节点类型) |
| 数据库 | SQLite | **完整**, 18表+9索引 |
| 向量库 | Qdrant(可选) + InMemory | **完整** |
| 同步 | WebDAV + 自定义CRDT | **完整** (Automerge未用) |
| AI运行时 | Ollama + OpenAI + Anthropic + DeepSeek + GLM | **完整** (5适配器) |
| 治理 | SCALE Engine | **完整** (bridge-http.mjs) |
| 认证 | JWT + RBAC + bcrypt | **完整** (4 Role + 17 Permission) |
| 测试 | Playwright 12用例(全Mock) + Rust 13模块单元测试 | **部分** |

---

## 六、代码规模统计

| 区域 | 文件数 | 行数 |
|---|---|---|
| Rust Core (8 crates) | 57 | 20,670 |
| Rust Server (main.rs) | 1 | 3,708 |
| Web Frontend | ~15 | ~2,500 |
| Mobile Frontend | ~8 | ~600 |
| Desktop (Tauri) | ~5 | ~150 |
| SDK | 5 | ~500 |
| UI Components | 9 | ~800 |
| Config Package | 2 | ~120 |
| SCALE Engine (submodule) | ~100+ | ~75,000+ |
| E2E Tests | 5 | ~300 |
| Migrations | 1 | ~220 |
| Docker/Infra | 3 | ~110 |
| **总计** | ~200+ | **~105,000+** |

---

## 七、后续计划 (详见 specs 目录)

| Phase | 目录 | 核心任务 |
|-------|------|---------|
| Phase 1 | `.monkeycode/specs/phase1-mobile-closed-loop/` | Mobile Auth + SSE Chat + Knowledge索引 |
| Phase 2 | `.monkeycode/specs/phase2-web-experience/` | Session历史 + Workflow编辑 + SDK封装 + zustand |
| Phase 3 | `.monkeycode/specs/phase3-security-robustness/` | 沙箱执行 + 路径校验 + Skill持久化 + CI |
| Phase 4 | `.monkeycode/specs/phase4-test-coverage/` | Rust单元测试 + E2E真实后端 |
| Phase 5 | `.monkeycode/specs/phase5-production-deployment/` | Docker+Nginx + 文档 + Automerge CRDT |