# MapleOS 产品状态全景 (2026-05-24)

> 本次审计基于代码库实际扫描，区分"真实实现"和"占位/存根"

## 一、已实现功能 (真实可用)

### Rust 核心引擎 — 8 个 crate 全部有实质实现

| Crate | 模块数 | 核心能力 | 状态 |
|---|---|---|---|
| maple-engine | 8 | Workflow DAG (8节点类型) / 执行器 / 任务队列 / 事件总线 / 检查点 / Hook / 技能注册 / Cron调度 | **真实** |
| maple-llm | 7 | LLM Router / Ollama / OpenAI / Anthropic 三适配器 / Embedding / Budget / SSE流解析 | **真实** |
| maple-agent | 7 | Agent 注册 / ReAct循环 / 会话存储 / 会话压缩 / 权限策略 / 多Agent编排 / 任务委派 | **真实** |
| maple-kb | 7 | BM25搜索 / 向量存储 / 混合检索(RRF) / 分块索引 / 三层记忆 / 自演化引擎 / Prompt版本管理 | **真实** |
| maple-sync | 3 | WebDAV同步 / CRDT合并 / 冲突解决(3策略) / 后台同步循环 | **真实** |
| maple-gateway | 5 | JWT认证 / WebSocket网关 / SSE流 / Webhook签名 / MCP Host (Stdio/HTTP/WS) | **真实** |
| maple-collab | 4 | Workspace CRUD / FMP协议 / 实时广播 / 群组规则引擎 | **真实** |
| maple-rpc | 3 | JSON-RPC 2.0 / 异步分发器 / HTTP handler | **真实** |

### REST API — 16 个端点有真实逻辑

- `/health`, `/api/chat`, `/api/models`, `/api/skills`
- `/api/kb/index`, `/api/kb/search` (BM25 + Vector + RRF)
- `/api/sessions`, `/api/memories`, `/api/memories/search`
- `/api/prompts`, `/api/tasks/enqueue`, `/api/tasks/stats`, `/api/tasks/dead-letter`
- `/api/tasks/:id/requeue`, `/api/events` (SSE), `/ws/agents` (WS)

### JSON-RPC — 11 个方法已注册

- `system.info`, `system.health`, `workflow.list`, `workflow.create`, `workflow.execute`
- `agent.list`, `workspace.create`, `llm.models`, `skill.list`, `scale.tools`, `scale.call`

### Web 前端 — 8 个页面全部有真实 API 调用

- Dashboard (10s轮询system.info + tasks/stats)
- Chat (POST /api/maple/api/chat + agent.list)
- Workflow Editor (workflow.list/create/execute + 拖拽画布)
- Agent Center (agent.list/llm.models/skill.list + tasks/stats + memories)
- Knowledge (kb/search + kb/index + 评分可视化)
- SCALE Engine (scale/tools/call + Artifact创建 + FSM可视化)
- Plugins (skill.list + 分类筛选)
- Settings (llm.models + config编辑)

### SCALE Engine — 真实 submodule + HTTP bridge

- bridge-http.mjs 端口7790, 提供 /health /tools /call /mcp
- v0.38.0, 包含 CLI/MCP Server/技能/规则/测试

---

## 二、存根/占位实现 (需要补实)

### 内置技能 — 4/5 是 mock

| 技能 | 状态 | 当前返回 |
|---|---|---|
| echo | **真实** | 返回输入 |
| web_search | **存根** | 空 results + "not yet configured" |
| code_execute | **存根** | stderr "not available" |
| file_ops | **存根** | "requires MCP server connection" |
| http_request | **存根** | status=0, body="not available" |

### 其他存根

| 项目 | 状态 |
|---|---|
| Desktop (Tauri) | 只有 package.json + src-tauri 空骨架, main() 打印一行, 无前端 |
| Mobile (React Native) | **目录不存在** |
| Plugins 目录 | **目录不存在** |
| packages/config | 空 package (无源文件) |
| Scheduler 后台循环 | 代码存在但 main.rs 中未启动 |
| Automerge CRDT | Cargo.toml 依赖存在, **从未 import/使用** |
| routing_rules.yaml | README 引用但**文件不存在** |

---

## 三、关键产品缺陷 (阻断端到端使用)

### 1. RPC 方法缺失 — 前端调用但后端未注册

| 前端调用 | 后端注册? | 影响 |
|---|---|---|
| `agent.chat` | **未注册** | Agent 协作对话完全失败 |
| `agent.register` | **未注册** | 注册 Agent 按钮404 |
| `task.create` | **未注册** | 任务派发按钮404 |
| `config.get` | **未注册** | Settings 加载配置失败 |
| `config.update` | **未注册** | Settings 保存配置失败 |
| `skill.install` | **未注册** | Plugin 安装按钮失败 |
| `skill.uninstall` | **未注册** | Plugin 卸载按钮失败 |

### 2. system.info 响应字段不匹配

- 后端返回: `{name, version}`
- 前端期望: `{version, uptime_secs, agents_count, workflows_count, tasks_count}`
- 结果: Dashboard 所有指标卡显示 `0` 或 `undefined`

### 3. 任务队列无 Worker

- `/api/tasks/enqueue` 写入 pending 任务
- **无后台循环消费** — 任务永远停在 pending
- TaskQueueService 有 dequeue/complete/fail 方法但无人调用

### 4. Chat 无流式输出

- 前端等待完整 JSON 响应, 无 SSE token streaming
- 用户看到空白直到整条回复完成

### 5. Workflow 执行无实时反馈

- 后端 EventBus 有 NodeCompleted/NodeFailed 等事件
- `/api/events` SSE 已实现
- **前端不订阅 SSE** — 节点状态 Badge 不更新

### 6. Workflow YAML 格式隐患

- 前端发送 JSON (`JSON.stringify`)
- 后端用 `serde_yaml::from_str()` 解析
- JSON 是 YAML 子集, 技术上可以解析, 但字段命名不一致会出问题

---

## 四、产品闭环分析

### Workflow 创建→执行→查看结果

```
创建: ✅ 可添加节点连线 → 调用 workflow.create
执行: ✅ 调用 workflow.execute → DAG拓扑排序执行
查看: ❌ 无执行历史UI, 无节点级进度, 无SSE实时更新
```

**闭环断点**: 用户执行后只看到 console.log 文字, 无法可视化追踪节点执行状态

### Chat 对话→Agent协作

```
对话: ⚠️ POST /api/chat 可用, 但无LLM时返回错误, 无流式输出
Agent选择: ✅ 下拉选择Agent ID
协作: ❌ agent.chat RPC 未注册, 协作对话失败
```

**闭环断点**: Chat 能发消息但体验差(无streaming); Agent协作面板核心功能失败

### Knowledge 索引→搜索→引用

```
索引: ✅ 上传文本 → BM25+Embedding索引
搜索: ✅ 混合检索 → 评分可视化
引用: ❌ 无从Chat引用搜索结果的入口
```

**闭环断点**: 知识库独立可用, 但与Chat/Workflow没有交叉引用通路

### Agent 注册→派发任务→执行→结果

```
注册: ❌ agent.register RPC 未注册
派发: ❌ task.create RPC 未注册
执行: ❌ 无task worker消费
结果: ⚠️ tasks/stats 可查看队列统计
```

**闭环断点**: 整条Agent→Task链路全部断开

### Settings 配置→生效

```
读取: ❌ config.get 未注册
保存: ❌ config.update 未注册
生效: ❌ 无后端存储, 配置改了不持久化
```

**闭环断点**: 配置完全无法生效

---

## 五、下一步优先级规划

### P0 — 修复阻断性问题 (产品闭环必做)

1. **注册缺失RPC方法** — agent.chat, agent.register, task.create, config.get/update, skill.install/uninstall
2. **修复system.info** — 补充 uptime_secs, agents_count, workflows_count, tasks_count 字段
3. **启动Task Worker** — 后台循环 dequeue → execute → complete/fail
4. **Chat SSE Streaming** — 前端订阅 /api/events 或 /api/chat stream

### P1 — 完善核心体验

5. **Workflow SSE订阅** — 前端 EventSource 监听 NodeCompleted/NodeFailed, 实时更新节点Badge
6. **Chat→Knowledge交叉引用** — 对话中可搜索知识库并引用结果
7. **技能补实** — web_search 至少支持基本搜索, code_execute 支持沙箱执行
8. **启动Scheduler** — main.rs 中启动 Cron 调度循环
9. **配置持久化** — YAML/SQLite存储, 重启后恢复

### P2 — 平台扩展

10. **Tauri 2 Desktop** — 完善src-tauri, 嵌入Web前端, 本地菜单/通知/文件系统
11. **Playwright E2E** — 核心页面自动化测试 (Dashboard/Chat/Workflow/Knowledge)
12. **Mobile骨架** — React Native + Expo, 共享SDK逻辑

### P3 — 生态完善

13. **Plugins目录** — 真实插件加载机制 (MCP Host已实现, 需UI+配置)
14. **Automerge CRDT** — 替换自定义JSON merge, 真正的离线协同
15. **routing_rules.yaml** — 默认模型路由规则文件
16. **packages/config** — 共享配置包 (主题/默认值/常量)

---

## 六、技术栈完整性检查

| 层 | 技术 | 状态 |
|---|---|---|
| 桌面端 | Tauri 2 | 骨架仅存, 缺前端+Rust端 |
| Web端 | Next.js 15 | **完整**, 构建通过 |
| 后端 | Rust Axum | **完整**, 16+端点 |
| 运行时 | Tokio | **完整** |
| Workflow | Petgraph | **完整** |
| 数据库 | SQLite | **完整**, 16+表 |
| 向量库 | Qdrant(可选) + InMemory | **完整** |
| 同步 | WebDAV + 自定义CRDT | **完整** (Automerge未用) |
| AI运行时 | Ollama + OpenAI + Anthropic | **完整** |
| 治理 | SCALE Engine | **完整** (需启动bridge) |
| 测试 | 无E2E | **缺失** |

---

## 七、代码规模统计

| 区域 | 文件数 | 估算行数 |
|---|---|---|
| Rust Core (8 crates) | ~40 | ~5,000 |
| Rust Server | 1 | ~1,280 |
| Web Frontend | ~15 | ~2,500 |
| SDK | 5 | ~500 |
| UI Components | 9 | ~800 |
| SCALE Engine (submodule) | ~100+ | ~5,000+ |
| 总计 | ~170+ | ~15,000+ |