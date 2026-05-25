# Agent OS 落地探讨 -- MapleOS 实践报告

## 一、我们是谁

**MapleOS** -- AI Native 多 Agent 协作工作站操作系统

核心公式：`Human + Agent + Workflow + Knowledge + Tools`

- 开源：MIT License
- 仓库：github.com/hongmaple0820/maple-os
- 代码量：Rust 后端 12,652 行 + 前端 3,649 行 + SCALE Engine 75,247 行

---

## 二、为什么需要 Agent OS

### 现状：AI 应用孤岛

| 痛点 | 具体表现 |
|------|---------|
| Agent 不可控 | 单个 Agent 行为无法审计、无法回溯、无法约束 |
| Agent 不协作 | 多 Agent 各自为战，没有统一的任务编排和结果汇聚 |
| 知识不沉淀 | 对话即丢，没有长期记忆和知识库的闭环 |
| 工具不统一 | 每个 Agent 各自对接工具，重复造轮子 |
| 安全无保障 | 无权限体系、无治理门禁、无操作审计 |

### 我们的解法

Agent OS 不是另一个 Agent 框架，而是一个 **让多个 Agent 在受控环境下协作完成复杂任务的操作系统**：

```
传统模式：Human <-> 单个 Agent <-> 单个工具
Agent OS：Human <-> Orchestrator <-> [Agent A, Agent B, Agent C] <-> [Tool 1..N] <-> Knowledge Base
```

---

## 三、MapleOS 六层架构

```
┌─────────────────────────────────────────────────────┐
│  L1 Interface  │ Web / Desktop / Mobile / CLI       │  用户接入
├─────────────────┼───────────────────────────────────┤
│  L2 Collaboration│ Workspace / Permissions / Sync   │  协作层
├─────────────────┼───────────────────────────────────┤
│  L3 Orchestration│ Workflow / Event Bus / Agent编排  │  编排层
├─────────────────┼───────────────────────────────────┤
│  L4 Capabilities │ Skills / MCP / Browser / Code    │  能力层
├─────────────────┼───────────────────────────────────┤
│  L5 Intelligence │ LLM Router / KB / Memory / 进化  │  智能层
├─────────────────┼───────────────────────────────────┤
│  L6 Storage     │ SQLite / Qdrant / CRDT / WebDAV   │  存储层
└─────────────────────────────────────────────────────┘
```

### 关键技术选型

| 层级 | 选型 | 理由 |
|------|------|------|
| 运行时 | Rust + Axum + Tokio | Agent OS 需要高并发低延迟，Rust 保证性能和安全 |
| 工作流 | Petgraph DAG | 任务依赖关系的自然表达，支持并行和条件分支 |
| 知识库 | BM25 + Vector 混合检索 | 关键词精确匹配 + 语义理解，互补提升召回率 |
| 同步 | Automerge CRDT | 离线优先、冲突自动解决，适合多端协作 |
| 治理 | SCALE Engine | 将工程规范变成可执行的门禁和审计命令 |
| 桌面 | Tauri 2 | Rust 原生后端，跨平台，比 Electron 轻 10 倍 |

---

## 四、核心引擎详解

### 4.1 Agent 管理（maple-agent）

```
AgentRegistry (注册中心)
  ├── AgentSchema: id / name / transport / capabilities / triggers
  ├── AgentStatus: Online / Busy / Offline
  └── Transport: WebSocket / Webhook / MCP / Rest / SSE

ReactLoop (推理循环)
  ├── 多轮 ToolUse/ToolResult 交互
  ├── 最多 10 轮自动推理
  └── Session 消息持久化

Orchestrator (编排器)
  ├── 目标分解: goal -> [PlanStep]
  ├── Agent 匹配: required_tools -> best_agent
  ├── 并行执行: 无依赖步骤并发
  └── 结果汇聚: step_aggregate
```

### 4.2 工作流引擎（maple-engine）

9 种节点类型覆盖常见场景：

| 节点类型 | 用途 |
|---------|------|
| Llm | LLM 调用，支持模型路由 |
| Tool | 技能调用 |
| Condition | 条件分支 |
| Parallel | 并行执行 |
| Loop | 循环迭代 |
| HumanApproval | 人工审批 |
| SubWorkflow | 子工作流嵌套 |
| Webhook | 外部触发 |
| Delay | 延迟等待 |

配套能力：检查点恢复、事件总线、Hooks 钩子、Cron 调度

### 4.3 LLM 路由层（maple-llm）

```
LlmRouter
  ├── 路由规则: privacy_level / task_type / budget
  ├── Fallback 链: cloud -> local
  ├── 4 种适配器: Ollama / OpenAI-compat / Anthropic / GLM
  ├── 真流式输出: LiveSseStream (reqwest bytes_stream)
  ├── 用量追踪: daily_budget / per_request_cost
  └── Embedding: OllamaEmbedder + FallbackEmbedder
```

### 4.4 知识库（maple-kb）

```
Indexer (分块) -> BM25Searcher (关键词) + VectorStore (语义)
                -> HybridRetriever (混合排序) -> Top-K 结果

MemoryStore: 工作记忆 / 情景记忆 / 语义记忆
Evolver: 自进化引擎（规则沉淀 + 行为追踪 + Lesson 学习）
PromptVersionManager: Prompt 版本管理与 A/B 测试
```

### 4.5 SCALE 治理引擎

```
scale context    -- 理解项目上下文
scale plan       -- 生成执行计划
scale tdd        -- 测试驱动验证
scale status     -- 状态审计
scale diagnose   -- 问题诊断

Commit Discipline: git 状态监控 + 双阈值告警
Session Coordinator: 多会话冲突检测
Cross-Repo Orchestrator: 多仓库工作流协调
```

---

## 五、产品闭环：从注册到对话的完整路径

### 闭环路径

```
注册 -> 登录 -> 创建 Agent -> 上传知识 -> 对话 -> 管理工作流
```

### 我们修复了 5 个关键断点

| 断点 | 问题 | 修复方案 |
|------|------|---------|
| 无认证 | 硬编码 admin，无注册 | users 表 + bcrypt + JWT + refresh_token + 前端登录页 |
| API 不对齐 | REST 用 message/reply，RPC 用 prompt/response | 双向兼容，同时返回 reply + response |
| 伪流式 | 先等 LLM 完整返回再 8 字符切片推送 | LiveSseStream 真流式，逐 token 实时推送 |
| 工作流格式冲突 | 前端 JSON 后端 YAML 解析 | parse_definition 先 JSON 后 YAML 双格式兼容 |
| 知识不联动 | 对话和知识库割裂 | 对话前自动检索知识库注入 prompt + 文件上传 |

### 对话自动引用知识库

```
用户提问
  ↓
检索知识库 (BM25 + Vector Top-3)
  ↓
[Knowledge Base Context]
相关文档片段...
---
[User Question]
用户原始问题
  ↓
LLM 生成回答
  ↓
回复 + kb_sources (引用来源)
```

---

## 六、Agent OS 落地的三个核心挑战

### 挑战 1: 可控性

**问题**: Agent 行为不可预测，如何保证输出质量？

**MapleOS 方案**:
- SCALE Engine 治理门禁: 规范变可执行命令
- HumanApproval 节点: 关键操作需人工审批
- 审计日志: 所有 Agent 操作可追溯
- RBAC 权限: 不同角色不同能力边界

### 挑战 2: 协作效率

**问题**: 多 Agent 如何分工、如何避免冲突、如何汇聚结果？

**MapleOS 方案**:
- Orchestrator 编排: 目标分解 -> Agent 匹配 -> 并行执行 -> 结果汇聚
- Workflow DAG: 显式定义任务依赖，支持条件/并行/循环
- CRDT 同步: Automerge 离线优先，冲突自动解决
- Workspace: 隔离的工作空间，权限隔离

### 挑战 3: 知识沉淀

**问题**: 对话即丢，Agent 无法积累经验？

**MapleOS 方案**:
- 三层记忆: 工作记忆(短期) + 情景记忆(中期) + 语义记忆(长期)
- 知识库闭环: 对话自动引用 -> 新知识自动索引 -> 持续积累
- Prompt 版本管理: A/B 测试 + 版本回滚
- 自进化引擎: 规则沉淀 + Lesson 学习 + 行为追踪

---

## 七、与业界方案对比

| 维度 | AutoGPT / CrewAI | Dify / Coze | MapleOS |
|------|-----------------|-------------|---------|
| 定位 | Agent 框架 | LLM 应用平台 | Agent 操作系统 |
| 运行时 | Python | Python/Go | Rust |
| Agent 协作 | 简单链式 | 工作流编排 | DAG + 编排器 + CRDT |
| 知识管理 | 无内置 | RAG 管道 | BM25+Vector+Memory+自进化 |
| 治理能力 | 无 | 无 | SCALE Engine |
| 离线能力 | 无 | 无 | Local-first (SQLite+CRDT) |
| 桌面端 | 无 | 无 | Tauri 原生桌面 |
| 安全审计 | 无 | 无 | RBAC + 门禁 + 审计日志 |

**核心差异**: MapleOS 不是框架也不是平台，而是 Agent 的操作系统 -- 提供进程管理(Registry)、通信(IPC/RPC)、存储(KB/Memory)、调度(Workflow/Orchestrator)、安全(Auth/RBAC/SCALE) 等操作系统级能力。

---

## 八、Local-first 架构的价值

### 为什么 Agent OS 必须是 Local-first

| 场景 | 云端依赖的问题 | Local-first 的优势 |
|------|---------------|-------------------|
| 企业私有部署 | 数据不出域 | SQLite 零依赖，CRDT 离线同步 |
| 边缘计算 | 网络不稳定 | Ollama 本地推理，无需云端 API |
| 开发者体验 | 配置复杂 | 单二进制 + SQLite 文件即可运行 |
| 数据主权 | 隐私合规风险 | 所有数据本地，可选云同步 |

### 技术实现

```
核心存储: SQLite (零配置)
向量检索: InMemoryVectorStore (默认) / Qdrant (可选)
LLM 推理: Ollama (本地) / 云端 API (可选)
文件同步: WebDAV (可选)
状态同步: Automerge CRDT (可选)
```

最小部署：1 个二进制 + 1 个 SQLite 文件

---

## 九、开发路线图

```
Phase 1 -- 基础引擎 [当前]
  ├── Rust 核心 8 个 crate
  ├── Web + Desktop + Mobile 三端
  ├── SCALE 治理集成
  └── 产品闭环 (认证/流式/知识联动)

Phase 2 -- 协作进化 [规划中]
  ├── 多 Agent 实时协作面板
  ├── 团队编排与任务分配
  ├── 知识自我进化
  ├── WebDAV + CRDT 多端同步
  └── Tauri 桌面端完善

Phase 3 -- 生态平台 [规划中]
  ├── 插件市场 + MCP 开放注册
  ├── Agent 市场
  ├── 企业版 (SSO / 审计 / 合规)
  └── SaaS / 私有部署
```

---

## 十、讨论议题

1. **Agent OS 的边界在哪？** 操作系统 vs 框架 vs 平台，定位如何选择？
2. **可控性如何量化？** SLA？审计覆盖率？审批拦截率？
3. **多 Agent 协作的最优模型？** 主从 / 对等 / 拍卖 / 市场？
4. **知识沉淀的 ROI？** 投入建设知识库 vs 直接调 LLM，什么时候值得？
5. **Local-first 的商业化挑战？** 免费本地版如何与付费云端版共存？
6. **治理与创新的平衡？** 门禁太严影响效率，太松出事故，如何设计自适应治理？

---

## 附录：关键数据

| 指标 | 数值 |
|------|------|
| Rust 核心代码 | 12,652 行 |
| 前端代码 | 3,649 行 |
| SCALE Engine | 75,247 行 |
| 核心 crate 数 | 8 个 |
| 节点类型 | 9 种 |
| Agent 接入方式 | 5 种 |
| LLM 适配器 | 4 种 |
| 客户端平台 | 3 个 (Web/Desktop/Mobile) |
| API 端点 | 30+ |
| RPC 方法 | 15+ |
| 数据库表 | 15 张 |
