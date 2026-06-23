# MapleOS v3 产品设计文档

> 融合 MapleClaw(枫琳) 产品设计 + 8 个参考项目 + rig 框架
> 版本: 3.0 | 日期: 2026-06-04

---

## 一、产品愿景

**MapleOS v3** — 人机共生的 Agent 群聊协作平台

> "飞书连接人与人，MapleOS 连接人与 AI"

核心公式: `群聊(Group) + Agent(rig) + Workflow + Knowledge + Tools + Skills + Hooks + Schedules`

产品哲学: `人机共生 = 人主导方向 × AI 执行细节 × 工作流保障质量`

- **人**负责创意、决策、审批
- **AI**负责分析、执行、验证
- **工作流**负责结构化、可追溯、可审计

### 1.1 对标产品

| 维度 | 对标 | 借鉴点 |
|------|------|--------|
| Agent 循环 | Claude Code | Tool trait、Subagent、Streaming、Permission modes |
| 群聊体验 | Discord | 频道、成员、权限、实时消息 |
| 任务管理 | Linear | 看板、状态机、智能分配 |
| 工作流编排 | n8n | DAG 引擎、YAML 定义、可视化编辑 |
| 人机协作 | 飞书 | 群聊+任务+审批+多渠道 |

### 1.2 核心差异

| 特性 | MapleOS v3 | 传统 Agent 框架 |
|------|-----------|----------------|
| 交互模式 | 群聊协作（多人+多 Agent） | 单人单 Agent 对话 |
| Agent 身份 | 统一用户模型（人/Agent 共享） | 独立的 Agent 实体 |
| 工作流 | 三模式（5阶段+可视化+YAML） | 单一模式 |
| 治理 | 证据驱动+质量门控+护栏 | 无 |
| 渠道 | Web+Desktop+CLI+IM 适配器 | 仅 API |

---

## 二、用户角色

### 2.1 角色定义

| 角色 | 描述 | 权限 |
|------|------|------|
| **Admin** | 平台管理员 | 全部权限，管理用户/Agent/系统配置 |
| **User** | 普通用户 | 创建群聊、任务、工作流；与 Agent 交互 |
| **Agent** | AI Agent | 响应消息、执行工具、运行工作流、生成报告 |
| **Viewer** | 只读用户 | 查看群聊/任务/工作流，不可操作 |

### 2.2 统一用户模型

人类和 Agent 共享 `users` 表，通过 `user_type` 区分:

```
users
├── id, name, email, avatar_url
├── user_type: human | agent
├── status: online | away | busy | offline | error
├── role: user | admin
├── Agent 专属: soul_config, memory_config, connection_type, llm_provider, llm_model
└── rig 配置: rig_provider, rig_model, tools_config, skills_config
```

设计决策: 人/Agent 共享 User 表使得好友、群组、消息等关系模型无需区分人/AI，极大简化系统复杂度。

---

## 三、信息架构

### 3.1 导航结构

```
侧边栏:
├── 群聊列表（核心入口）
│   ├── 开发协作群
│   ├── 代码审查群
│   └── 运维监控群
├── 任务中心
│   ├── 我的任务
│   ├── 看板视图
│   └── 日历视图
├── 工作流
│   ├── 工作流列表
│   ├── 可视化编辑器
│   └── 定时任务
├── Agent 管理
│   ├── Agent 列表
│   ├── Agent 创建/编辑
│   └── Agent 健康监控
├── 知识库
│   ├── 文档管理
│   ├── 搜索
│   └── 知识图谱
├── Skills & 插件
│   ├── 已安装 Skills
│   ├── 插件市场
│   └── MCP 服务器
└── 设置
    ├── 个人设置
    ├── 团队设置
    ├── API 配置
    └── 安全设置
```

### 3.2 群聊界面

群聊是核心交互界面，借鉴 Discord + Slack:

```
┌─────────────────────────────────────────────────────────────┐
│ 群聊名称  │  成员(5)  │  规则  │  设置  │  搜索            │
├───────────┼───────────┴───────┴────────┴────────────────────┤
│           │ ┌─────────────────────────────────────────────┐ │
│  消息列表  │ │ [Agent] 我已经分析了代码，发现 3 个问题:      │ │
│           │ │ 1. SQL 注入风险...                           │ │
│  2026/6/4 │ │ 2. XSS 漏洞...                              │ │
│           │ │ [工具调用] security_scan → 2 个高危           │ │
│           │ │ [Human] 请修复第一个问题                      │ │
│           │ │ [Agent] 正在修复... [工具调用] file_edit      │ │
│           │ │ [Agent] 已修复，请审查: [diff]                │ │
│           │ │ [审批请求] 请确认代码变更                      │ │
│           │ │ [Human] ✅ 批准                              │ │
│           │ └─────────────────────────────────────────────┘ │
│           │ ┌─────────────────────────────────────────────┐ │
│           │ │ 输入消息...  [@] [📎] [emoji] [发送]         │ │
│           │ └─────────────────────────────────────────────┘ │
├───────────┼─────────────────────────────────────────────────┤
│ 成员列表   │ Agent: coder (🟢), reviewer (🟢), security (🟡)│
│           │ Human: 张三 (🟢), 李四 (⚪)                    │
└───────────┴─────────────────────────────────────────────────┘
```

---

## 四、数据模型

### 4.1 核心实体关系

```
users ─┬── groups (owner)
       ├── group_members ── groups
       ├── group_messages ── groups
       ├── sessions ── groups
       ├── agent_hooks
       ├── agent_schedules
       ├── agent_memories
       └── agent_workflows

groups ─┬── group_members
        ├── group_messages
        ├── group_rules
        ├── sessions
        └── cron_jobs
```

### 4.2 完整表结构

#### users — 统一用户表

```sql
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    password_hash TEXT,
    avatar_url TEXT,
    user_type TEXT DEFAULT 'human',     -- human | agent
    status TEXT DEFAULT 'offline',      -- online | away | busy | offline | error
    role TEXT DEFAULT 'user',           -- user | admin

    -- Agent 专属字段
    soul_config TEXT,                   -- SOUL.md 人格定义
    memory_config TEXT,                 -- JSON: 记忆策略
    agent_config TEXT,                  -- JSON: 运行时配置
    connection_type TEXT,               -- llm-api | http-ws | sdk | a2a | rig
    connection_config TEXT,             -- JSON: 连接参数
    llm_provider TEXT,                  -- openai | anthropic | deepseek | ...
    llm_model TEXT,                     -- gpt-4 | claude-3 | ...
    llm_api_key TEXT,                   -- 加密存储
    llm_base_url TEXT,
    agent_api_key TEXT,
    agent_api_secret TEXT,              -- 加密
    a2a_endpoint TEXT,

    -- rig Agent 配置
    rig_provider TEXT,
    rig_model TEXT,
    tools_config TEXT,                  -- JSON
    skills_config TEXT,                 -- JSON

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

#### groups — 群聊表

```sql
CREATE TABLE groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    avatar_url TEXT,
    group_type TEXT DEFAULT 'collaboration',  -- collaboration | project | channel | dm
    owner_id TEXT NOT NULL,
    settings JSON NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

#### group_members — 群成员表

```sql
CREATE TABLE group_members (
    group_id TEXT NOT NULL,
    member_id TEXT NOT NULL,
    member_type TEXT NOT NULL,  -- human | agent
    role TEXT DEFAULT 'member',  -- owner | admin | member | viewer
    nickname TEXT,
    joined_at INTEGER NOT NULL,
    last_active_at INTEGER,
    PRIMARY KEY (group_id, member_id)
);
```

#### group_messages — 群消息表

```sql
CREATE TABLE group_messages (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    sender_type TEXT NOT NULL,  -- human | agent | system
    message_type TEXT NOT NULL,
    content TEXT NOT NULL,
    content_type TEXT DEFAULT 'text',  -- text | markdown | image | file | voice
    metadata TEXT,               -- JSON: replyToId, forwardedFrom, attachments
    source_channel TEXT,         -- web | api | sdk | webhook | a2a | im_feishu | im_wechat
    pinned INTEGER DEFAULT 0,
    edited_at INTEGER,
    deleted_at INTEGER,          -- 软删除
    created_at INTEGER NOT NULL
);
```

消息类型 (`message_type`):

| 类型 | 说明 |
|------|------|
| text / markdown | 基础消息 |
| tool_call / tool_result | Agent 工具调用 |
| thinking | Agent 思考过程 |
| approval_request / approval_response | 审批流 |
| workflow_run / workflow_step | 工作流 |
| skill_call / skill_result | Skill 调用 |
| system / member_join / member_leave | 系统消息 |
| task_update / cron_trigger | 任务/定时 |
| external_message | 来自外部 IM |

#### message_edit_history — 消息编辑历史

```sql
CREATE TABLE message_edit_history (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    old_content TEXT NOT NULL,
    new_content TEXT NOT NULL,
    edited_by TEXT NOT NULL,
    edited_at INTEGER NOT NULL
);
```

#### message_reads — 消息已读状态

```sql
CREATE TABLE message_reads (
    message_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    read_at INTEGER NOT NULL,
    PRIMARY KEY (message_id, user_id)
);
```

#### message_reactions — 消息表情反应

```sql
CREATE TABLE message_reactions (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    emoji TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(message_id, user_id, emoji)
);
```

#### message_bookmarks — 消息书签

```sql
CREATE TABLE message_bookmarks (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    note TEXT,
    created_at INTEGER NOT NULL
);
```

#### pinned_messages — 置顶消息

```sql
CREATE TABLE pinned_messages (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    group_id TEXT NOT NULL,
    pinned_by TEXT NOT NULL,
    pinned_at INTEGER NOT NULL
);
```

#### group_rules — 群规则

```sql
CREATE TABLE group_rules (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL,
    name TEXT NOT NULL,
    rule_type TEXT NOT NULL,
    config JSON NOT NULL,
    enabled INTEGER DEFAULT 1,
    priority INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL
);
```

#### sessions — 会话表

```sql
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    session_type TEXT DEFAULT 'chat',  -- chat | task | workflow | cron
    status TEXT DEFAULT 'active',      -- active | paused | completed | archived
    context JSON DEFAULT '{}',
    message_count INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

#### cron_jobs — 定时任务

```sql
CREATE TABLE cron_jobs (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    cron_expr TEXT NOT NULL,
    prompt TEXT NOT NULL,
    workflow_id TEXT,
    enabled INTEGER DEFAULT 1,
    last_run_at INTEGER,
    next_run_at INTEGER,
    run_count INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL
);
```

#### agent_hooks — Agent 事件钩子

```sql
CREATE TABLE agent_hooks (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    events TEXT NOT NULL,              -- JSON: ["message.created", ...]
    condition_expr TEXT,
    action TEXT NOT NULL,              -- JSON: { type, params }
    enabled INTEGER DEFAULT 1,
    priority INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL
);
```

#### agent_schedules — Agent 定时任务

```sql
CREATE TABLE agent_schedules (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    group_id TEXT,
    name TEXT NOT NULL,
    description TEXT,
    cron_expr TEXT NOT NULL,
    action TEXT NOT NULL,              -- JSON: { type, params }
    enabled INTEGER DEFAULT 1,
    last_run_at INTEGER,
    next_run_at INTEGER,
    run_count INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL
);
```

#### a2a_remote_agents — A2A 远程 Agent

```sql
CREATE TABLE a2a_remote_agents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    agent_card TEXT,                   -- JSON
    trust_level TEXT DEFAULT 'discovered',  -- discovered | verified | trusted | blocked
    capabilities TEXT,                 -- JSON
    last_seen_at INTEGER,
    created_at INTEGER NOT NULL
);
```

#### agent_workflows — Agent 工作流模板

```sql
CREATE TABLE agent_workflows (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    name TEXT NOT NULL,
    phases TEXT,                       -- JSON
    methodology TEXT,                  -- JSON
    constraints TEXT,                  -- JSON
    capabilities TEXT,                 -- JSON
    auto_plan INTEGER DEFAULT 0,
    auto_verify INTEGER DEFAULT 1,
    max_steps INTEGER DEFAULT 20,
    require_approval INTEGER DEFAULT 1,
    created_at INTEGER NOT NULL
);
```

#### agent_memories — Agent 记忆

```sql
CREATE TABLE agent_memories (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    memory_type TEXT NOT NULL,         -- working | episodic | semantic
    content TEXT NOT NULL,
    source_type TEXT,                  -- chat | skill | workflow | manual
    source_id TEXT,
    relevance_score REAL DEFAULT 0.7,
    group_id TEXT,
    access_count INTEGER DEFAULT 0,
    expires_at INTEGER,                -- 工作记忆过期
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

#### agent_hook_logs / agent_schedule_logs — 执行日志

```sql
CREATE TABLE agent_hook_logs (
    id TEXT PRIMARY KEY,
    hook_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    event_data TEXT,
    status TEXT NOT NULL,              -- success | failed | skipped
    result TEXT,
    error TEXT,
    executed_at INTEGER NOT NULL
);

CREATE TABLE agent_schedule_logs (
    id TEXT PRIMARY KEY,
    schedule_id TEXT NOT NULL,
    status TEXT NOT NULL,              -- success | failed | timeout
    result TEXT,
    error TEXT,
    executed_at INTEGER NOT NULL,
    duration_ms INTEGER
);
```

---

## 五、Agent 系统

### 5.1 Agent 架构

基于 rig 框架构建，MapleAgent 封装 rig Agent + 扩展元数据:

```
MapleAgent
├── rig::Agent (核心 LLM 交互)
├── AgentMeta (id, name, description, model, provider, capabilities)
├── SoulConfig (SOUL.md 人格定义)
├── MemorySystem (三层记忆)
└── AgentHealth (健康状态)
```

### 5.2 SOUL.md 人格系统

Agent 的人格通过 Markdown 文件定义:

```markdown
# Agent 名称

## 身份
我是 XXX，专注于 YYY 领域的 AI 助手。

## 行为准则
- 始终基于事实回答
- 不确定时明确说明
- 优先提供可执行建议

## 沟通风格
- 简洁专业
- 适当使用 emoji
- 代码示例优先
```

注入流程: Agent 收到消息 → 加载 SOUL.md → 注入记忆 → 注入技能清单 → 注入工作流上下文 → 组装 LLM 请求

### 5.3 三层记忆系统

```
┌───────────────┐  工作记忆 (Working)
│  当前对话上下文  │  · 最近 N 条消息
│  临时任务信息    │  · 进行中的任务状态
│  会话级缓存     │  · 自动创建 & 过期清理
└───────────────┘

┌───────────────┐  情景记忆 (Episodic)
│  历史交互记录   │  · 重要对话片段
│  事件序列      │  · 用户偏好记录
│  时间线索引     │  · 成功/失败案例
└───────────────┘

┌───────────────┐  语义记忆 (Semantic)
│  知识图谱      │  · 领域知识
│  概念关系      │  · 技能文档
│  持久化知识    │  · 用户画像
└───────────────┘
```

### 5.4 Agent 四种连接方式

| 连接方式 | 说明 | 适用场景 |
|----------|------|---------|
| `llm-api` | 平台直接调用 LLM API (rig) | 内置 Agent |
| `http-ws` | Webhook/HTTP 回调 | 外部 Agent 服务 |
| `sdk` | SDK 接入 (WebSocket/HTTP/Polling) | 自定义 Agent |
| `a2a` | A2A 协议远程 Agent | 跨平台 Agent |

### 5.5 Agent 生命周期

```
创建 → 配置(SOUL.md+模型+工具) → 注册 → 上线 → 接收消息 → 处理 → 回复
                                                              ↓
                                                         健康监控
                                                              ↓
                                                    离线/错误 → 重连/恢复
```

### 5.6 Agent Hook 事件系统

| 事件类别 | 事件名 | 触发时机 |
|----------|--------|----------|
| 消息事件 | message.created | 新消息创建 |
| 消息事件 | message.updated | 消息被编辑 |
| 消息事件 | message.deleted | 消息被删除 |
| 群组事件 | group.member_joined | 成员加入 |
| 群组事件 | group.member_left | 成员离开 |
| Agent 事件 | agent.online / offline / error | Agent 状态变化 |
| 工作流事件 | workflow.step_completed / completed | 工作流进度 |
| 任务事件 | task.created / completed | 任务生命周期 |

Hook 动作: execute_tool / send_message / trigger_skill / call_webhook / update_memory / a2a_delegate / chain

---

## 六、群聊系统

### 6.1 群聊类型

| 类型 | 说明 | 典型用途 |
|------|------|---------|
| collaboration | 协作群 | 开发团队+Agent 协作 |
| project | 项目群 | 特定项目的任务跟踪 |
| channel | 频道 | 公开讨论/公告 |
| dm | 私聊 | 1对1（人-人或人-Agent） |

### 6.2 群规则引擎

| 规则类型 | 说明 | 配置示例 |
|----------|------|---------|
| auto_assign | 关键词自动分配 Agent | `{"keywords": ["安全","漏洞"], "agent_id": "security-bot"}` |
| auto_approve | 自动审批 | `{"roles": ["admin"], "confidence_threshold": 0.9}` |
| rate_limit | 速率限制 | `{"max_per_minute": 30}` |
| time_window | 时间窗口 | `{"start_hour": 9, "end_hour": 18, "timezone": "Asia/Shanghai"}` |
| tool_restriction | 工具限制 | `{"denied_tools": ["bash", "file_delete"]}` |
| knowledge_scope | 知识库范围 | `{"allowed_kb_ids": ["kb-1", "kb-2"]}` |

### 6.3 消息功能

- 回复 (replyToId)
- 编辑 (带历史记录)
- 撤回 (软删除)
- 转发
- 表情反应 (reaction)
- 书签
- 置顶
- 已读状态
- 搜索 (全文+过滤)
- @ 提及
- 文件/图片/语音附件

---

## 七、工作流系统

### 7.1 三模式工作流

| 模式 | 说明 | 创建方式 |
|------|------|---------|
| 5 阶段结构化 | 分析→规划→执行→验证→报告 | Agent 自动 / 人工触发 |
| 可视化编辑 | React Flow DAG 编辑器 | 人工拖拽 |
| YAML 定义 | Agent 可创建的工作流 | Agent 编写 YAML |

### 7.2 5 阶段结构化工作流

```
┌────────┐  ┌────────┐  ┌────────┐  ┌────────┐  ┌────────┐
│ 分析    │→│ 规划    │→│ 执行    │→│ 验证    │→│ 报告    │
│Analyze │  │ Plan   │  │Execute │  │Verify  │  │Report  │
└────────┘  └────────┘  └────────┘  └────────┘  └────────┘
     ↑                                              │
     └──────────── 失败时重试/降级 ←─────────────────┘

Human-in-the-Loop: 每个阶段均可设置审批点
失败策略: retry / skip / abort / delegate
```

| 阶段 | 目标 | 输入 | 输出 |
|------|------|------|------|
| 分析 | 理解任务需求 | 原始消息/任务描述 | 需求分析报告 |
| 规划 | 制定执行方案 | 需求分析 | 执行计划 |
| 执行 | 按计划执行 | 执行计划 | 执行结果 |
| 验证 | 检验执行结果 | 执行结果+原始需求 | 验证报告 |
| 报告 | 生成最终报告 | 所有前序结果 | 最终报告+建议 |

### 7.3 YAML 工作流定义

```yaml
name: "代码审查工作流"
description: "自动审查代码变更并生成报告"
triggers:
  - type: webhook
    path: /webhook/pr
  - type: cron
    expr: "0 9 * * 1-5"

nodes:
  - id: fetch_pr
    type: tool
    tool: github.get_pr
    args:
      pr_id: "{{trigger.pr_id}}"

  - id: analyze_code
    type: agent
    agent: code-reviewer
    prompt: "审查以下代码变更: {{fetch_pr.output}}"

  - id: check_security
    type: agent
    agent: security-reviewer
    prompt: "检查安全问题: {{fetch_pr.output}}"

  - id: merge_reports
    type: llm
    prompt: "合并审查报告: 分析={{analyze_code.output}} 安全={{check_security.output}}"

edges:
  - from: fetch_pr
    to: [analyze_code, check_security]
  - from: [analyze_code, check_security]
    to: merge_reports
```

### 7.4 工作流节点类型

| 节点类型 | 说明 |
|----------|------|
| llm | LLM 调用 (使用 rig Agent) |
| tool | 工具调用 (使用 rig ToolServer) |
| agent | Agent 委托 |
| condition | 条件分支 |
| human | 人工审批 |
| parallel | 并行执行 |
| loop | 循环 |
| skill | Skill 调用 |
| http_request | HTTP 请求 |
| group_message | 群消息发送 |
| sub_workflow | 子工作流 |

---

## 八、Skills + MCP + CLI

### 8.1 Skills 系统

```rust
pub struct Skill {
    pub name: String,
    pub description: String,
    pub version: String,
    pub tools: Vec<ToolDefinition>,     // Skill 提供的工具
    pub prompts: Vec<PromptTemplate>,   // Skill 提供的 prompt 模板
}
```

Skill 来源: 内置 / 本地目录 / Git 仓库 / MCP 服务器 / 插件

### 8.2 MCP 中间件

```rust
pub trait McpMiddlewareHandler {
    async fn before_call(&self, ctx: &mut McpContext) -> Result<MiddlewareAction>;
    async fn after_call(&self, ctx: &mut McpContext, result: &ToolResult) -> Result<ToolResult>;
}

// MiddlewareAction: Continue | Skip | Modify { new_args } | Block { reason }
```

### 8.3 CLI 命令

```bash
# 群聊
maple group list / create / join / send

# Agent
maple agent list / create / chat / status

# 工作流
maple workflow list / run / create --yaml / edit

# 任务
maple task list / create / assign

# Skills
maple skill list / install / run

# 定时任务
maple cron list / create --expr "0 9 * * *" --prompt "生成日报"
```

---

## 九、多渠道接入

### 9.1 渠道管理器

```
┌─────────────────────────────────────────────┐
│             Channel Manager                  │
├─────────────────────────────────────────────┤
│  WebAdapter (Socket.io + REST + SSE)        │
│  FeishuAdapter                               │
│  WeChatAdapter                               │
│  DingTalkAdapter                             │
│  TelegramAdapter                             │
│  SlackAdapter                                │
│  DiscordAdapter                              │
│                                             │
│  统一消息格式:                                │
│  { channel_type, message_type, content,      │
│    sender_id, group_id, metadata }           │
└─────────────────────────────────────────────┘
```

### 9.2 消息流

```
外部渠道 → POST /api/channels/message → ChannelManager.route() →
  1. 根据 channel_type 选择 Adapter
  2. Adapter.normalize() 标准化消息
  3. 写入 group_messages (source_channel = channel_type)
  4. broadcast 到群组
  5. 触发 Agent Hooks (message.created)
```

### 9.3 Agent 认证

HMAC-SHA256 签名: `${timestamp}.${body}` 规范格式

---

## 十、治理与护栏

### 10.1 证据驱动治理

Agent 的所有声明必须有证据支撑 (Evidence: Command/Scan/File/Manual + 内容哈希防篡改)

### 10.2 质量门控

| 门控 | 检查内容 |
|------|---------|
| G0 Build | 构建/类型检查 |
| G1 Exploration | 探索充分性 (歧义评分 <= 20%) |
| G2 Planning | 计划完整性 |
| G3 TDD | TDD 证据 (RED→GREEN→REFACTOR) |
| G4 Lint | Lint 通过 |
| G5 Test | 测试通过 |
| G6 Coverage | 覆盖率 >= 80% |
| G7 Security | 安全扫描 |
| G8 ProductSmoke | 产品冒烟测试 |
| G9 Visual | UI 视觉审查 |

### 10.3 护栏检测器

| 检测器 | 说明 |
|--------|------|
| BruteRetryDetector | 暴力重试检测 |
| IdleToolDetector | 空闲工具检测 |
| PrematureDoneDetector | 过早完成检测 |
| DangerousCommandDetector | 危险命令检测 |
| SecretLeakDetector | 密钥泄露检测 |
| ScopeCreepDetector | 范围蔓延检测 |
| HallucinationDetector | 幻觉检测 |

### 10.4 FSM 工件状态机

```
Draft → InProgress → Review → Approved → Done → Archived
                          ↘ Rejected
```

---

## 十一、实时通信

### 11.1 Socket.io (双向)

- 群聊消息实时推送
- Agent 状态变化 (typing, thinking, online/offline)
- 审批请求/响应
- 工作流进度更新

### 11.2 SSE (单向)

- Agent 响应流式输出
- 工具调用进度
- 长任务进度

---

## 十二、前端页面

### 12.1 核心页面

| 页面 | 路由 | 功能 |
|------|------|------|
| 群聊列表 | / | 侧边栏群聊列表 + 主区域消息流 |
| 任务中心 | /tasks | 看板/列表/日历视图 |
| 工作流 | /workflows | 工作流列表 + 可视化编辑器 |
| Agent 管理 | /agents | Agent CRUD + 健康监控面板 |
| 知识库 | /knowledge | 文档管理 + 搜索 + 知识图谱 |
| Skills | /skills | Skill 列表 + 插件市场 |
| 设置 | /settings | 个人/团队/安全/API 配置 |

### 12.2 Agent 健康监控面板

```
┌─────────────────────────────────────────────────────────────┐
│ Agent 监控面板                                    [刷新]     │
├─────────────────────────────────────────────────────────────┤
│ 总计: 5  │  🟢 在线: 3  │  🟡 降级: 1  │  🔴 离线: 1      │
├─────────────────────────────────────────────────────────────┤
│ Agent      │ 状态  │ 最后心跳    │ 模型     │ 活跃任务 │ 操作│
│ coder      │ 🟢   │ 5s ago     │ gpt-4    │ 2       │ [💬]│
│ reviewer   │ 🟢   │ 3s ago     │ claude-3 │ 1       │ [💬]│
│ security   │ 🟡   │ 45s ago    │ gpt-4    │ 0       │ [💬]│
│ monitor    │ 🔴   │ 5min ago   │ llama-3  │ 0       │ [💬]│
└─────────────────────────────────────────────────────────────┘
```

---

## 十三、技术栈

| 层级 | 技术 |
|------|------|
| 前端 | Next.js + React + TailwindCSS + Socket.io-client |
| 桌面端 | Tauri v2 |
| 后端 | Rust (Axum) + SQLite |
| Agent 框架 | rig-core |
| 实时通信 | Socket.io + SSE |
| 向量存储 | 内置向量扩展 |
| 同步 | Automerge CRDT |
| CLI | clap |

---

## 十四、迁移计划

| 阶段 | 时间 | 内容 |
|------|------|------|
| Phase 1 | 2 周 | rig 集成 + maple-agent 重构 + 修复注册表 bug |
| Phase 2 | 2 周 | 群聊核心 (CRUD + 消息 + 规则 + 前端) |
| Phase 3 | 2 周 | 工作流 + 任务 + 定时任务 |
| Phase 4 | 2 周 | Skills + MCP + CLI |
| Phase 5 | 2 周 | IM 适配器 + 健康监控增强 |
| Phase 6 | 1 周 | 测试 + 优化 + 文档 |

---

## 十五、成功指标

| 指标 | 目标 |
|------|------|
| Agent 响应延迟 | < 2s (首 token) |
| 群聊消息吞吐 | > 100 msg/s |
| 工作流执行成功率 | > 95% |
| Agent 健康检查准确率 | 100% |
| E2E 测试覆盖率 | > 80% |
| CLI 命令响应时间 | < 500ms |
