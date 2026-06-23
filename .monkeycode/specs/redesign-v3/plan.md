# MapleOS v3 重新设计计划 — Agent OS 群聊协作平台

> 基于 8 个参考项目 + MapleClaw 产品设计 + rig 框架为核心 + 当前代码现状

---

## 一、产品定位

**MapleOS v3**: 人机共生协作平台 — "飞书连接人与人，枫琳连接人与AI"

核心公式: `群聊(Group) + Agent(rig) + Workflow + Knowledge + Tools + Skills + Hooks + Schedules`

产品哲学: `人机共生 = 人主导方向 × AI 执行细节 × 工作流保障质量`

- **人**负责创意、决策、审批
- **AI**负责分析、执行、验证
- **工作流**负责结构化、可追溯、可审计

对标: Claude Code 的 Agent 循环 + Discord 的群聊体验 + Linear 的任务管理 + n8n 的工作流编排 + 飞书的人机协作

---

## 二、参考项目关键借鉴

| 项目 | 借鉴要点 |
|------|---------|
| **Claude Code** | Agent 循环、Tool trait、Subagent/Team 系统、Deferred tool loading、Streaming tool execution、Permission modes、Skills 系统 |
| **cc-haha** | IM 适配器侧车架构（Telegram/飞书/微信/钉钉）、Skills 插件系统、多模态 Agent 通信、权限模式传播 |
| **golutra** | Rust Agent 框架、Session 管理、Semantic filters |
| **hermes-agent** | 自注册工具发现（AST 解析）、多环境执行（Local/Docker/SSH）、Gateway Agent 缓存（LRU+TTL）、Cron 调度器、威胁模式扫描、轨迹压缩 |
| **oh-my-pi** | 对话管理、插件系统 |
| **OpenVibeCoding** | ACP 协议（JSON-RPC 2.0 + SSE）、双运行时注册表、沙箱优先架构、MCP 中间件框架、Plan Mode 作为权限模式、凭证隔离 |
| **scale-engine** | 证据驱动治理（Gate G0-G22）、FSM 工件状态机、Guardrails Gateway（12+ 检测器）、Skills 触发引擎、认知工作流（歧义评分/共识规划/苏格拉底提问）、多 Agent 协调（Registry/Pool/Dispatcher）、Shield 策略编译、Cortex 本能提取 |
| **rig** | Agent Builder（类型状态模式）、Tool trait + ToolEmbedding、ToolServer 并发执行、PromptHook 生命周期、ConversationMemory、25+ Provider |
| **MapleClaw (枫琳)** | 统一用户模型（人/Agent 共享 User 表）、SOUL.md 人格系统、三层记忆架构（工作/情景/语义）、5 阶段工作流（分析→规划→执行→验证→报告）、Agent Hooks 事件系统、Schedule 定时任务、SDK 设计（Transport 抽象）、A2A 协议（信任级别）、多渠道适配器模式、HMAC 签名认证、28 模型 Prisma 数据模型、群聊完整功能（Pin/Reaction/已读/书签/搜索/编辑历史/撤回/转发） |

---

## 三、新架构设计

### 3.1 整体架构

```
┌─────────────────────────────────────────────────────────────────┐
│                    L1 界面层                                      │
│  Web (Next.js) │ Desktop (Tauri) │ CLI │ IM 适配器 (飞书/微信/钉钉) │
├─────────────────────────────────────────────────────────────────┤
│                    L2 协作层 — 群聊核心                            │
│  GroupChat │ GroupRules │ Presence │ TaskSystem │ SessionSystem  │
├─────────────────────────────────────────────────────────────────┤
│                    L3 编排层                                      │
│  WorkflowEngine(rig) │ AgentOrchestrator │ EventBus │ Scheduler  │
├─────────────────────────────────────────────────────────────────┤
│                    L4 能力层                                      │
│  ToolServer(rig) │ Skills │ MCP │ KnowledgeBase │ VectorStore    │
├─────────────────────────────────────────────────────────────────┤
│                    L5 智能层                                      │
│  rig Agent │ LLM Router │ 25+ Providers │ Streaming │ Memory     │
├─────────────────────────────────────────────────────────────────┤
│                    L6 存储层                                      │
│  SQLite │ Automerge CRDT │ Vector DB │ File System                │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 核心模块划分

```
core/
├── maple-agent/          # Agent 核心（基于 rig 重构）
│   ├── src/
│   │   ├── lib.rs
│   │   ├── agent.rs      # rig Agent 封装 + MapleAgent 扩展
│   │   ├── registry.rs   # Agent 注册表（带心跳、健康检查）
│   │   ├── schema.rs     # Agent 能力描述（完整版）
│   │   ├── health.rs     # 健康监控
│   │   └── hooks.rs      # PromptHook 实现（日志、审计、权限）
│   └── Cargo.toml        # 依赖 rig-core
│
├── maple-group/          # 群聊协作（新模块，替代 maple-collab）
│   ├── src/
│   │   ├── lib.rs
│   │   ├── group.rs      # 群聊管理（创建、设置、成员）
│   │   ├── message.rs    # 消息系统（文本、工具调用、审批、系统）
│   │   ├── rules.rs      # 群规则引擎（基于现有 group_rules.rs 扩展）
│   │   ├── presence.rs   # 在线状态（Agent + Human）
│   │   ├── session.rs    # 会话管理（Agent 为主体）
│   │   └── types.rs      # 共享类型
│   └── Cargo.toml
│
├── maple-task/           # 任务系统（新模块）
│   ├── src/
│   │   ├── lib.rs
│   │   ├── task.rs       # 任务 CRUD + 状态机
│   │   ├── assign.rs     # 智能分配（Agent 能力匹配）
│   │   ├── kanban.rs     # 看板视图逻辑
│   │   └── types.rs
│   └── Cargo.toml
│
├── maple-workflow/       # 工作流引擎（基于 rig 重构）
│   ├── src/
│   │   ├── lib.rs
│   │   ├── engine.rs     # DAG 执行引擎
│   │   ├── nodes/        # 节点类型
│   │   │   ├── mod.rs
│   │   │   ├── llm.rs    # LLM 调用节点（使用 rig Agent）
│   │   │   ├── tool.rs   # 工具调用节点（使用 rig ToolServer）
│   │   │   ├── condition.rs
│   │   │   ├── human.rs  # 人工审批节点
│   │   │   └── agent.rs  # Agent 委托节点
│   │   ├── scheduler.rs  # 定时调度
│   │   ├── yaml.rs       # YAML 工作流定义（Agent 可创建）
│   │   └── visual.rs     # 可视化编辑器数据模型
│   └── Cargo.toml
│
├── maple-tools/          # 工具系统（基于 rig ToolServer）
│   ├── src/
│   │   ├── lib.rs
│   │   ├── server.rs     # rig ToolServer 封装
│   │   ├── builtin/      # 内置工具
│   │   │   ├── mod.rs
│   │   │   ├── bash.rs
│   │   │   ├── file_ops.rs
│   │   │   ├── web_search.rs
│   │   │   └── think.rs  # rig ThinkTool
│   │   ├── mcp/          # MCP 集成
│   │   │   ├── mod.rs
│   │   │   ├── client.rs
│   │   │   └── middleware.rs  # MCP 中间件（借鉴 OpenVibeCoding）
│   │   └── skills/       # Skills 系统
│   │       ├── mod.rs
│   │       ├── loader.rs
│   │       └── registry.rs
│   └── Cargo.toml
│
├── maple-llm/            # LLM 路由（保留，对接 rig providers）
│   └── src/
│       ├── lib.rs
│       ├── adapters/     # 现有适配器
│       └── router.rs     # 能力路由
│
├── maple-kb/             # 知识库（保留，增强）
│   └── src/
│       ├── lib.rs
│       ├── indexer.rs
│       ├── search.rs
│       └── loaders/      # 文档加载器（借鉴 rig loaders）
│
├── maple-cron/           # 定时任务（新模块）
│   ├── src/
│   │   ├── lib.rs
│   │   ├── scheduler.rs  # Cron 表达式调度
│   │   ├── executor.rs   # 任务执行（Agent 为主体）
│   │   └── types.rs
│   └── Cargo.toml
│
├── maple-im/             # 外部 IM 渠道（新模块）
│   ├── src/
│   │   ├── lib.rs
│   │   ├── adapter.rs    # 通用适配器 trait
│   │   ├── feishu.rs     # 飞书
│   │   ├── wechat.rs     # 微信
│   │   ├── dingtalk.rs   # 钉钉
│   │   └── telegram.rs   # Telegram
│   └── Cargo.toml
│
└── maple-sync/           # 同步（保留）
```

---

## 四、群聊系统设计（核心）

### 4.1 统一用户模型（借鉴 MapleClaw）

人类用户和 AI Agent 共享同一 User 表，通过 `user_type` 字段区分：

```sql
-- 统一用户表（人类 + Agent 共享）
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
    llm_base_url TEXT,                  -- 自定义 API 端点
    agent_api_key TEXT,                 -- Agent API Key
    agent_api_secret TEXT,              -- Agent API Secret (加密)
    a2a_endpoint TEXT,                  -- A2A 协议端点

    -- rig Agent 配置
    rig_provider TEXT,                  -- rig provider (openai/anthropic/...)
    rig_model TEXT,                     -- rig model ID
    tools_config TEXT,                  -- JSON: 工具配置
    skills_config TEXT,                 -- JSON: 技能配置

    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

**设计决策**: 人/Agent 共享 User 表使得好友、群组、消息等关系模型无需区分人/AI，极大简化系统复杂度。

### 4.2 群聊数据模型

```sql
-- 群聊
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

-- 群成员
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

-- 群消息（借鉴 MapleClaw 完整消息模型）
CREATE TABLE group_messages (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    sender_type TEXT NOT NULL,  -- human | agent | system
    message_type TEXT NOT NULL,  -- text | markdown | image | file | voice | system | tool_call | tool_result | approval | approval_response | skill_call | workflow_run | agent_thinking | workflow_step
    content TEXT NOT NULL,       -- 消息内容
    content_type TEXT DEFAULT 'text',  -- text | markdown | image | file | voice
    metadata TEXT,               -- JSON: replyToId, forwardedFrom, attachments, etc.
    source_channel TEXT,         -- web | api | sdk | webhook | a2a | im_feishu | im_wechat
    pinned INTEGER DEFAULT 0,
    edited_at INTEGER,
    deleted_at INTEGER,          -- 软删除
    created_at INTEGER NOT NULL,
    FOREIGN KEY (group_id) REFERENCES groups(id)
);

-- 消息编辑历史
CREATE TABLE message_edit_history (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    old_content TEXT NOT NULL,
    new_content TEXT NOT NULL,
    edited_by TEXT NOT NULL,
    edited_at INTEGER NOT NULL,
    FOREIGN KEY (message_id) REFERENCES group_messages(id)
);

-- 消息已读状态
CREATE TABLE message_reads (
    message_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    read_at INTEGER NOT NULL,
    PRIMARY KEY (message_id, user_id)
);

-- 群组已读水位（优化查询）
CREATE TABLE group_read_status (
    group_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    last_read_message_id TEXT,
    last_read_at INTEGER,
    PRIMARY KEY (group_id, user_id)
);

-- 消息表情反应
CREATE TABLE message_reactions (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    emoji TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (message_id) REFERENCES group_messages(id),
    UNIQUE(message_id, user_id, emoji)
);

-- 消息书签
CREATE TABLE message_bookmarks (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    note TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (message_id) REFERENCES group_messages(id)
);

-- 置顶消息
CREATE TABLE pinned_messages (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    group_id TEXT NOT NULL,
    pinned_by TEXT NOT NULL,
    pinned_at INTEGER NOT NULL,
    FOREIGN KEY (message_id) REFERENCES group_messages(id),
    FOREIGN KEY (group_id) REFERENCES groups(id)
);

-- 群规则
CREATE TABLE group_rules (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL,
    name TEXT NOT NULL,
    rule_type TEXT NOT NULL,  -- auto_assign | auto_approve | rate_limit | time_window | tool_restriction | knowledge_scope
    config JSON NOT NULL,
    enabled INTEGER DEFAULT 1,
    priority INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (group_id) REFERENCES groups(id)
);

-- 会话（Agent 为主体）
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    session_type TEXT DEFAULT 'chat',  -- chat | task | workflow | cron
    status TEXT DEFAULT 'active',      -- active | paused | completed | archived
    context JSON DEFAULT '{}',         -- 会话上下文
    message_count INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (group_id) REFERENCES groups(id)
);

-- 定时任务
CREATE TABLE cron_jobs (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    cron_expr TEXT NOT NULL,
    prompt TEXT NOT NULL,              -- 要执行的 prompt
    workflow_id TEXT,                  -- 可选：关联工作流
    enabled INTEGER DEFAULT 1,
    last_run_at INTEGER,
    next_run_at INTEGER,
    run_count INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (group_id) REFERENCES groups(id)
);

-- Agent 事件钩子（借鉴 MapleClaw）
CREATE TABLE agent_hooks (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    events TEXT NOT NULL,              -- JSON: ["message.created", "group.member_joined", ...]
    condition_expr TEXT,               -- 触发条件表达式
    action TEXT NOT NULL,              -- JSON: { type, params }
    enabled INTEGER DEFAULT 1,
    priority INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL
);

-- Agent Hook 执行日志
CREATE TABLE agent_hook_logs (
    id TEXT PRIMARY KEY,
    hook_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    event_data TEXT,                   -- JSON
    status TEXT NOT NULL,              -- success | failed | skipped
    result TEXT,                       -- JSON
    error TEXT,
    executed_at INTEGER NOT NULL,
    FOREIGN KEY (hook_id) REFERENCES agent_hooks(id)
);

-- Agent 定时任务（增强版）
CREATE TABLE agent_schedules (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    group_id TEXT,                     -- 目标群组
    name TEXT NOT NULL,
    description TEXT,
    cron_expr TEXT NOT NULL,
    action TEXT NOT NULL,              -- JSON: { type, params }  type: execute_tool | send_message | trigger_skill | execute_workflow
    enabled INTEGER DEFAULT 1,
    last_run_at INTEGER,
    next_run_at INTEGER,
    run_count INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL
);

-- Agent Schedule 执行日志
CREATE TABLE agent_schedule_logs (
    id TEXT PRIMARY KEY,
    schedule_id TEXT NOT NULL,
    status TEXT NOT NULL,              -- success | failed | timeout
    result TEXT,                       -- JSON
    error TEXT,
    executed_at INTEGER NOT NULL,
    duration_ms INTEGER,
    FOREIGN KEY (schedule_id) REFERENCES agent_schedules(id)
);

-- A2A 远程 Agent（借鉴 MapleClaw 信任级别）
CREATE TABLE a2a_remote_agents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    agent_card TEXT,                   -- JSON: 远程 Agent Card
    trust_level TEXT DEFAULT 'discovered',  -- discovered | verified | trusted | blocked
    capabilities TEXT,                 -- JSON: 远程 Agent 能力
    last_seen_at INTEGER,
    created_at INTEGER NOT NULL
);

-- Agent 工作流模板
CREATE TABLE agent_workflows (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    name TEXT NOT NULL,
    phases TEXT,                       -- JSON: 阶段定义
    methodology TEXT,                  -- JSON: 方法论
    constraints TEXT,                  -- JSON: 约束条件
    capabilities TEXT,                 -- JSON: 能力清单
    auto_plan INTEGER DEFAULT 0,
    auto_verify INTEGER DEFAULT 1,
    max_steps INTEGER DEFAULT 20,
    require_approval INTEGER DEFAULT 1,
    created_at INTEGER NOT NULL
);

-- Agent 记忆（三层记忆架构）
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
    expires_at INTEGER,                -- 工作记忆过期时间
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

### 4.3 群聊消息类型

借鉴 FMP 协议和 Claude Code 的消息系统：

```rust
pub enum MessageType {
    // 基础消息
    Text,                // 普通文本
    Markdown,            // Markdown 富文本

    // Agent 交互
    ToolCall,            // Agent 调用工具
    ToolResult,          // 工具执行结果
    Thinking,            // Agent 思考过程（ThinkTool）

    // 审批流
    ApprovalRequest,     // 请求审批
    ApprovalResponse,    // 审批响应（同意/拒绝/修改）

    // 工作流
    WorkflowRun,         // 工作流执行
    WorkflowStep,        // 工作流步骤更新

    // Skills
    SkillCall,           // Skill 调用
    SkillResult,         // Skill 执行结果

    // 系统
    System,              // 系统消息
    MemberJoin,          // 成员加入
    MemberLeave,         // 成员离开
    TaskUpdate,          // 任务状态更新
    CronTrigger,         // 定时任务触发

    // 外部 IM
    ExternalMessage,     // 来自外部 IM 的消息
}
```

### 4.4 群规则引擎

扩展现有 `GroupRulesEngine`，增加：

```rust
pub enum RuleType {
    // 已有
    AutoAssign { keywords: Vec<String>, agent_id: String },
    AutoApprove { roles: Vec<String>, confidence_threshold: f64 },
    RateLimit { max_per_minute: u32 },
    TimeWindow { start_hour: u32, end_hour: u32, timezone: String },

    // 新增
    ToolRestriction {
        allowed_tools: Option<Vec<String>>,  // 白名单
        denied_tools: Option<Vec<String>>,   // 黑名单
    },
    KnowledgeScope {
        allowed_kb_ids: Vec<String>,  // 限制可访问的知识库
    },
    WorkflowPermission {
        can_create: bool,
        can_execute: bool,
        max_concurrent: u32,
    },
    PromptTemplate {
        prefix: String,   // 群级 system prompt 前缀
        suffix: String,   // 群级 system prompt 后缀
    },
}
```

---

## 五、Agent 系统设计（基于 rig + 借鉴 MapleClaw）

### 5.1 统一用户模型（借鉴 MapleClaw）

Agent 和人类共享 `users` 表（见 4.1），通过 `user_type` 区分。Agent 拥有：
- **SOUL.md 人格定义** — Markdown 格式的身份、行为准则、沟通风格
- **三层记忆** — 工作记忆（会话级）、情景记忆（历史交互）、语义记忆（持久知识）
- **四种连接方式** — llm-api / http-ws / sdk / a2a
- **rig 集成** — 通过 rig Agent Builder 构建

### 5.2 MapleAgent 封装

```rust
use rig::agent::Agent;
use rig::providers::openai;

pub struct MapleAgent {
    /// rig Agent 核心
    inner: Agent<openai::CompletionModel>,
    /// 扩展元数据
    meta: AgentMeta,
    /// 健康状态
    health: AgentHealth,
    /// SOUL.md 人格
    soul: SoulConfig,
    /// 三层记忆
    memory: MemorySystem,
}

pub struct AgentMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub avatar_url: Option<String>,
    pub model: String,
    pub provider: String,
    pub connection_type: ConnectionType,
    pub capabilities: AgentCapabilities,
    pub system_prompt: String,
    pub tags: Vec<String>,
    pub created_at: i64,
}

pub enum ConnectionType {
    LlmApi,       // 平台直接调用 LLM API（rig）
    HttpWs,       // Webhook/HTTP 回调
    Sdk,          // SDK 接入（WebSocket/HTTP/Polling）
    A2A,          // A2A 协议远程 Agent
}

pub struct AgentCapabilities {
    pub tools: Vec<String>,
    pub skills: Vec<String>,
    pub max_context_length: usize,
    pub supports_streaming: bool,
    pub supports_function_calling: bool,
    pub supports_image: bool,
    pub supports_audio: bool,
    pub input_content_types: Vec<String>,
    pub output_content_types: Vec<String>,
}
```

### 5.3 SOUL.md 人格系统（借鉴 MapleClaw）

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

SOUL.md 注入流程: Agent 收到消息 → 加载 SOUL.md 作为 System Prompt 前缀 → 注入记忆 → 注入技能清单 → 注入工作流上下文 → 组装最终 LLM 请求

### 5.4 三层记忆系统（借鉴 MapleClaw）

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

记忆注入流程: `buildLlmContext()` → 加载 SOUL.md → 注入工作记忆 → 注入情景记忆（高相关性） → 注入语义记忆 → 注入工作流上下文 → 注入技能清单 → 应用智能上下文窗口 → 组装 LLM 请求

### 5.5 Agent 生命周期（借鉴 rig PromptHook）

```rust
pub struct MapleAgentHook {
    group_id: String,
    agent_id: String,
    rules_engine: Arc<GroupRulesEngine>,
    event_bus: Arc<EventBus>,
}

impl PromptHook<openai::CompletionModel> for MapleAgentHook {
    async fn on_completion_call(&self, prompt: &Message, history: &[Message]) -> HookAction {
        // 1. 检查群规则（rate limit, time window）
        // 2. 记录审计日志
        // 3. 广播 "agent thinking" 状态
        HookAction::Continue
    }

    async fn on_tool_call(&self, tool_name: &str, call_id: &str, args: &str) -> ToolCallHookAction {
        // 1. 检查工具权限（tool_restriction 规则）
        // 2. 检查审批需求（auto_approve 规则）
        // 3. 广播工具调用消息到群聊
        ToolCallHookAction::Continue
    }

    async fn on_tool_result(&self, tool_name: &str, call_id: &str, args: &str, result: &str) -> HookAction {
        // 1. 广播工具结果到群聊
        // 2. 记录到知识库（可选）
        HookAction::Continue
    }

    async fn on_completion_response(&self, prompt: &Message, response: &CompletionResponse) -> HookAction {
        // 1. 广播 Agent 回复到群聊
        // 2. 更新会话上下文
        // 3. 触发后续任务（如有）
        HookAction::Continue
    }
}
```

### 5.6 Agent 注册表（修复现有问题）

修复 `analyze-mapleos` 发现的 11 个问题：

- 完整的 `AgentSchema` 字段（description, avatar_url, model, provider, system_prompt, tags, capabilities 完整版）
- `list_agents` 返回完整数据而非仅 `(id, name, status)`
- 修复 stale-sweep 逻辑 bug（只标记真正过期的 Agent）
- 增加 `last_heartbeat` 到前端展示
- 增加健康级别（Healthy/Degraded/Unhealthy）
- 前端增加自动轮询或 WebSocket 订阅

---

## 六、工作流系统设计

### 6.1 三模式工作流

1. **5 阶段结构化工作流**（借鉴 MapleClaw: 分析→规划→执行→验证→报告）
2. **可视化编辑**（现有 React Flow 编辑器增强）
3. **Agent 创建的 YAML 工作流**（新功能）

### 6.2 5 阶段结构化工作流（借鉴 MapleClaw）

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
| **分析** | 理解任务需求 | 原始消息/任务描述 | 需求分析报告 |
| **规划** | 制定执行方案 | 需求分析 | 执行计划 (步骤、工具、资源) |
| **执行** | 按计划执行 | 执行计划 | 执行结果 |
| **验证** | 检验执行结果 | 执行结果 + 原始需求 | 验证报告 (通过/未通过/部分通过) |
| **报告** | 生成最终报告 | 所有前序结果 | 最终报告 + 建议 |

工作流激活: `shouldActivateWorkflow()` 关键词匹配（"分析", "规划", "执行", "验证", "报告"）+ 模式匹配（复杂任务描述）

### 6.3 YAML 工作流定义

```yaml
# Agent 可以创建和执行的工作流定义
name: "代码审查工作流"
description: "自动审查代码变更并生成报告"
version: "1.0"
triggers:
  - type: webhook
    path: /webhook/pr
  - type: cron
    expr: "0 9 * * 1-5"  # 工作日每天 9 点

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
    tools: [think, file_read]

  - id: check_security
    type: agent
    agent: security-reviewer
    prompt: "检查安全问题: {{fetch_pr.output}}"
    tools: [think]

  - id: merge_reports
    type: llm
    model: "{{default_model}}"
    prompt: "合并审查报告: 分析={{analyze_code.output}} 安全={{check_security.output}}"

  - id: notify
    type: tool
    tool: group.send_message
    args:
      group_id: "{{config.group_id}}"
      content: "{{merge_reports.output}}"

edges:
  - from: fetch_pr
    to: [analyze_code, check_security]
  - from: [analyze_code, check_security]
    to: merge_reports
  - from: merge_reports
    to: notify
```

### 6.4 工作流节点类型

```rust
pub enum NodeType {
    // 已有
    Llm { model: String, prompt: String },
    Tool { tool_name: String, args: Value },
    Condition { expression: String },
    HumanReview { assignee: String },
    Trigger { trigger_type: TriggerType },

    // 新增
    Agent {
        agent_id: String,
        prompt: String,
        tools: Option<Vec<String>>,
        max_turns: Option<usize>,
    },
    SubWorkflow {
        workflow_id: String,
        args: Value,
    },
    Parallel {
        branches: Vec<Node>,
    },
    Loop {
        condition: String,
        body: Vec<Node>,
        max_iterations: usize,
    },
    Skill {
        skill_name: String,
        args: Value,
    },
    HttpRequest {
        method: String,
        url: String,
        headers: Value,
        body: Value,
    },
    GroupMessage {
        group_id: String,
        message_type: MessageType,
        content: String,
    },
}
```

---

## 七、Agent Hooks 与调度系统（借鉴 MapleClaw）

### 7.1 Hooks 事件系统

| 事件类别 | 事件名 | 触发时机 |
|----------|--------|----------|
| 消息事件 | `message.created` | 新消息创建 |
| 消息事件 | `message.updated` | 消息被编辑 |
| 消息事件 | `message.deleted` | 消息被删除 |
| 群组事件 | `group.member_joined` | 成员加入群组 |
| 群组事件 | `group.member_left` | 成员离开群组 |
| Agent 事件 | `agent.online` | Agent 上线 |
| Agent 事件 | `agent.offline` | Agent 离线 |
| Agent 事件 | `agent.error` | Agent 错误 |
| 工作流事件 | `workflow.step_completed` | 工作流步骤完成 |
| 工作流事件 | `workflow.completed` | 工作流完成 |
| 任务事件 | `task.created` | 任务创建 |
| 任务事件 | `task.completed` | 任务完成 |

### 7.2 Hook 动作类型

| 动作类型 | 说明 | 参数 |
|----------|------|------|
| `execute_tool` | 执行工具 | `{ tool, args }` |
| `send_message` | 发送消息 | `{ content, group_id }` |
| `trigger_skill` | 触发技能 | `{ skill_id, params }` |
| `call_webhook` | 调用外部 Webhook | `{ url, method, body }` |
| `update_memory` | 更新记忆 | `{ memory_type, key, content }` |
| `a2a_delegate` | 委托 A2A Agent | `{ agent_id, task }` |
| `chain` | 链式执行多个动作 | `{ actions: [...] }` |

### 7.3 Hook 执行流程

```
事件发生 → HookDispatcher.dispatch(event_type, event_data) →
  1. 加载所有启用的 Hooks（按 agent_id 过滤）
  2. 按 event_type 匹配
  3. 评估 condition 条件
  4. 按 priority 排序
  5. 逐个执行 action
  6. 记录 agent_hook_logs（status, result, error）
```

### 7.4 Schedule 定时任务

```rust
pub enum ScheduleAction {
    ExecuteTool { tool: String, args: Value },
    SendMessage { group_id: String, content: String },
    TriggerSkill { skill_id: String, params: Value },
    ExecuteWorkflow { workflow_id: String, args: Value },
}
```

Schedule 执行流程:
```
ScheduleScheduler（定时检查）→
  1. 从 DB 加载所有启用的 Schedule
  2. 检查 next_run_at <= now
  3. 执行 action
  4. 记录 agent_schedule_logs
  5. 更新 last_run_at + 计算下次 next_run_at
```

手动触发: `POST /api/agents/{id}/schedules/{schedule_id}/trigger` → 绕过 cron 检查，立即执行

---

## 八、治理与护栏系统（借鉴 SCALE Engine）

### 8.1 证据驱动治理

Agent 的所有声明必须有证据支撑：

```rust
pub struct Evidence {
    pub id: String,
    pub evidence_type: EvidenceType,  // Command | File | Scan | Manual
    pub command: Option<String>,       // 执行的命令
    pub stdout: Option<String>,        // 标准输出
    pub stderr: Option<String>,        // 标准错误
    pub exit_code: Option<i32>,        // 退出码
    pub file_path: Option<String>,     // 相关文件
    pub hash: String,                  // 内容哈希（防篡改）
    pub timestamp: i64,
    pub compressed_size: usize,        // 压缩后大小
}

pub struct GateResult {
    pub gate: GateStage,
    pub passed: bool,
    pub evidence: Vec<Evidence>,
    pub blockers: Vec<String>,
    pub duration_ms: u64,
}
```

### 8.2 质量门控（Gate System）

```rust
pub enum GateStage {
    G0_Build,           // 构建/类型检查
    G1_Exploration,     // 探索充分性（歧义评分 <= 20%）
    G2_Planning,        // 计划完整性（边界分析、异常处理、回滚策略）
    G3_TDD,             // TDD 证据（RED→GREEN→REFACTOR）
    G4_Lint,            // Lint 通过
    G5_Test,            // 测试通过
    G6_Coverage,        // 覆盖率 >= 80%
    G7_Security,        // 安全扫描（OWASP、密钥泄露、危险命令）
    G8_ProductSmoke,    // 产品冒烟测试
    G9_Visual,          // UI 视觉审查
}
```

### 8.3 Guardrails Gateway（护栏网关）

借鉴 SCALE 的检测器系统，在 Agent 执行前后拦截：

```rust
pub trait Detector: Send + Sync {
    fn name(&self) -> &str;
    fn severity(&self) -> Severity;  // Info | Warn | Block
    async fn pre_tool(&self, ctx: &ToolContext) -> Option<DetectorVerdict>;
    async fn post_tool(&self, ctx: &ToolContext, result: &ToolResult) -> Option<DetectorVerdict>;
}

pub enum DetectorVerdict {
    Allow,
    Warn { reason: String },
    Block { reason: String },
}

// 内置检测器
pub struct BruteRetryDetector;      // 暴力重试检测
pub struct IdleToolDetector;        // 空闲工具检测
pub struct PrematureDoneDetector;   // 过早完成检测
pub struct DangerousCommandDetector; // 危险命令检测（rm -rf, DROP TABLE 等）
pub struct SecretLeakDetector;      // 密钥泄露检测
pub struct ScopeCreepDetector;      // 范围蔓延检测
pub struct HallucinationDetector;   // 幻觉检测
```

### 8.4 认知工作流层

```rust
/// 歧义评分器 — 7 维度加权评分
pub struct AmbiguityScorer {
    pub dimensions: Vec<AmbiguityDimension>,  // 目标清晰度、IO 边界、技术约束、时间、质量、风险、验收标准
    pub threshold: f64,                        // > 0.2 触发苏格拉底提问
}

/// 共识规划器 — 三角色迭代
pub struct ConsensusPlanner {
    pub roles: Vec<PlannerRole>,  // Planner → Architect → Critic
    pub max_iterations: usize,
}

/// 苏格拉底提问器
pub struct SocraticQuestioner {
    pub max_questions: usize,
    pub ambiguity_threshold: f64,
}
```

### 8.5 FSM 工件状态机

工作流中的每个工件（需求、计划、任务、变更、证据）都有状态机：

```rust
pub enum ArtifactStatus {
    Draft,
    InProgress,
    Review,
    Approved,
    Rejected,
    Done,
    Archived,
}

pub struct ArtifactTransition {
    pub from: ArtifactStatus,
    pub to: ArtifactStatus,
    pub guard: Option<GateCondition>,  // 质量门控条件
    pub actor: ActorType,              // Human | Agent | System
}
```

---

## 九、Skills + MCP + CLI 系统

### 9.1 Skills 系统（借鉴 cc-haha + hermes-agent）

```rust
pub struct Skill {
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub tools: Vec<ToolDefinition>,    // Skill 提供的工具
    pub prompts: Vec<PromptTemplate>,  // Skill 提供的 prompt 模板
    pub config: SkillConfig,
}

pub enum SkillSource {
    Builtin,                    // 内置 Skill
    Directory(PathBuf),         // 本地目录
    Git { url: String, ref_: String },  // Git 仓库
    Mcp { server: String },     // MCP 服务器
    Plugin { name: String },    // 插件注册
}
```

### 9.2 MCP 中间件（借鉴 OpenVibeCoding）

```rust
pub struct McpMiddleware {
    pub name: String,
    pub priority: i32,
    pub handler: Box<dyn McpMiddlewareHandler>,
}

pub trait McpMiddlewareHandler: Send + Sync {
    /// 工具调用前拦截
    async fn before_call(&self, ctx: &mut McpContext) -> Result<MiddlewareAction>;
    /// 工具调用后拦截
    async fn after_call(&self, ctx: &mut McpContext, result: &ToolResult) -> Result<ToolResult>;
}

pub enum MiddlewareAction {
    Continue,
    Skip { reason: String },
    Modify { new_args: Value },
    Block { reason: String },
}
```

### 9.3 CLI 设计

```bash
# 群聊管理
maple group list                    # 列出所有群聊
maple group create --name "开发群"   # 创建群聊
maple group join <group-id>         # 加入群聊
maple group send <group-id> "消息"  # 发送消息

# Agent 管理
maple agent list                    # 列出所有 Agent
maple agent create --name "coder" --model gpt-4  # 创建 Agent
maple agent chat <agent-id>         # 与 Agent 对话
maple agent status <agent-id>       # 查看 Agent 状态

# 工作流
maple workflow list                 # 列出工作流
maple workflow run <workflow-id>    # 运行工作流
maple workflow create --yaml file.yaml  # 从 YAML 创建
maple workflow edit <workflow-id>   # 可视化编辑

# 任务
maple task list --group <group-id>  # 列出任务
maple task create --title "..."     # 创建任务
maple task assign <task-id> <agent-id>  # 分配任务

# Skills
maple skill list                    # 列出 Skills
maple skill install <skill-name>    # 安装 Skill
maple skill run <skill-name>        # 运行 Skill

# 定时任务
maple cron list                     # 列出定时任务
maple cron create --expr "0 9 * * *" --prompt "生成日报"  # 创建定时任务
```

---

## 十、前端设计

### 10.1 信息架构（重构）

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

### 10.2 群聊界面设计

群聊界面是核心体验，借鉴 Discord + Slack：

```
┌─────────────────────────────────────────────────────────────┐
│ 群聊名称  │  成员(5)  │  规则  │  设置  │  搜索            │
├───────────┼───────────┴───────┴────────┴────────────────────┤
│           │ ┌─────────────────────────────────────────────┐ │
│  消息列表  │ │ [Agent] 我已经分析了代码，发现 3 个问题:      │ │
│           │ │ 1. SQL 注入风险...                           │ │
│  2026/6/3 │ │ 2. XSS 漏洞...                              │ │
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

### 10.3 Agent 健康监控面板（修复现有问题）

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
│ assistant  │ 🟢   │ 8s ago     │ gpt-3.5  │ 3       │ [💬]│
│ monitor    │ 🔴   │ 5min ago   │ llama-3  │ 0       │ [💬]│
└─────────────────────────────────────────────────────────────┘
```

---

## 十一、多渠道接入与 IM 适配器（借鉴 MapleClaw + cc-haha）

### 11.1 渠道管理器架构（借鉴 MapleClaw）

```
┌─────────────────────────────────────────────┐
│             Channel Manager                  │
├─────────────────────────────────────────────┤
│  ┌──────────────┐  ┌───────────────────┐   │
│  │ WebAdapter   │  │ IM Adapters       │   │
│  │              │  │ · FeishuAdapter   │   │
│  │ · Socket.io  │  │ · WeChatAdapter   │   │
│  │ · REST API   │  │ · DingTalkAdapter │   │
│  │ · SSE        │  │ · TelegramAdapter │   │
│  └──────────────┘  └───────────────────┘   │
│                                             │
│  统一消息格式:                                │
│  {                                          │
│    channel_type: "web" | "feishu" | ...,   │
│    message_type: "text" | "image" | ...,   │
│    content: "...",                          │
│    sender_id: "...",                        │
│    group_id: "...",                         │
│    metadata: {}                             │
│  }                                          │
└─────────────────────────────────────────────┘
```

### 11.2 通用适配器 Trait

```rust
pub trait ChannelAdapter: Send + Sync {
    /// 适配器名称
    fn name(&self) -> &str;
    /// 渠道类型
    fn channel_type(&self) -> ChannelType;

    /// 启动适配器
    async fn start(&self, config: ChannelConfig) -> Result<()>;
    /// 发送消息
    async fn send_message(&self, channel: &str, message: OutgoingMessage) -> Result<()>;
    /// 注册消息回调
    fn on_message(&self, handler: Box<dyn Fn(IncomingMessage) + Send + Sync>);
    /// 查询状态
    async fn status(&self) -> ChannelStatus;
    /// 停止适配器
    async fn stop(&self) -> Result<()>;
}

pub enum ChannelType {
    Web,
    Feishu,
    WeChat,
    DingTalk,
    Telegram,
    Slack,
    Discord,
}

pub struct IncomingMessage {
    pub channel_type: ChannelType,
    pub channel_id: String,         // 外部渠道 ID
    pub sender_id: String,          // 外部用户 ID
    pub sender_name: String,
    pub content: MessageContent,
    pub timestamp: i64,
    pub reply_to: Option<String>,
    pub metadata: serde_json::Value,
}
```

### 11.3 渠道消息流

```
外部渠道 → POST /api/channels/message → ChannelManager.route() →
  1. 根据 channel_type 选择 Adapter
  2. Adapter.normalize() 标准化消息
  3. 写入 group_messages 表 (source_channel = channel_type)
  4. broadcast 到群组
  5. 触发 Agent Hooks (message.created)
```

### 11.4 飞书适配器

```rust
pub struct FeishuAdapter {
    app_id: String,
    app_secret: String,
    webhook_url: String,
    event_handler: Option<Box<dyn Fn(IncomingMessage) + Send + Sync>>,
}

impl ImAdapter for FeishuAdapter {
    // 使用飞书 Open API
    // 接收: 飞书事件订阅 (webhook)
    // 发送: 飞书消息 API
}
```

---

## 十二、迁移计划

### Phase 1: 基础重构（2 周）

1. 集成 rig-core 作为依赖
2. 重构 `maple-agent` 使用 rig Agent
3. 修复 Agent 注册表已知 bug
4. 建立 `maple-group` 模块基础

### Phase 2: 群聊核心（2 周）

1. 实现群聊 CRUD + 消息系统
2. 实现群规则引擎（扩展现有代码）
3. 实现会话管理
4. 前端群聊界面

### Phase 3: 工作流 + 任务（2 周）

1. 工作流引擎对接 rig
2. YAML 工作流解析和执行
3. 任务系统 CRUD + 看板
4. 定时任务调度器

### Phase 4: Skills + MCP + CLI（2 周）

1. Skills 加载和注册系统
2. MCP 中间件框架
3. CLI 工具
4. 内置工具（bash, file_ops, web_search, think）

### Phase 5: IM 适配器 + 增强（2 周）

1. 飞书适配器
2. 钉钉适配器
3. Agent 健康监控增强
4. 前端完善（监控面板、设置页）

### Phase 6: 测试 + 优化（1 周）

1. E2E 测试覆盖
2. 性能优化
3. 安全审计
4. 文档更新

---

## 十三、技术决策

| 决策 | 选择 | 理由 |
|------|------|------|
| Agent 框架 | rig-core | Rust 原生、类型安全、25+ provider、Tool trait 设计优秀 |
| 用户模型 | 统一 User 表（人/Agent 共享） | 借鉴 MapleClaw，简化关系模型 |
| 群聊消息存储 | SQLite | 已有基础设施，适合本地优先架构 |
| 实时通信 | Socket.io + SSE | Socket.io 用于双向（借鉴 MapleClaw），SSE 用于单向推送 |
| 工作流定义 | YAML + JSON + 5 阶段模型 | YAML 便于 Agent 创建，JSON 便于可视化编辑，5 阶段提供结构化执行 |
| 前端框架 | Next.js (保留) | 已有基础，组件可复用 |
| 桌面端 | Tauri (保留) | 已有基础 |
| Skills 格式 | TOML + Rust | 与 rig 生态一致 |
| IM 适配器 | 渠道管理器 + 适配器模式 | 借鉴 MapleClaw Channel Manager + cc-haha 侧车架构 |
| Agent 认证 | HMAC-SHA256 签名 | 借鉴 MapleClaw，`${timestamp}.${body}` 规范格式 |
| SSRF 防护 | URL 验证 + DNS 二次检查 | 借鉴 MapleClaw ssrf-protection.ts |
| Agent 人格 | SOUL.md（Markdown） | 借鉴 MapleClaw，WYSIWYG 编辑 |
| 记忆系统 | 三层架构（工作/情景/语义） | 借鉴 MapleClaw，支持记忆整合和过期清理 |

---

## 十四、与现有代码的兼容性

### 保留的模块
- `maple-llm/` — 对接 rig providers
- `maple-kb/` — 增强后继续使用
- `maple-engine/` — 重构为 `maple-workflow/`
- `maple-sync/` — 保留
- `server/` — 重构 API 层
- `apps/web/` — 重构 UI，保留组件可复用部分

### 替换的模块
- `maple-collab/` → `maple-group/`（群聊替代协作空间）
- Agent 注册表 → 基于 rig 的完整实现

### 新增的模块
- `maple-task/` — 任务系统
- `maple-cron/` — 定时任务
- `maple-im/` — 外部 IM 渠道
- `maple-tools/` — 工具系统（rig ToolServer）

---

## 十五、风险和缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| rig API 不稳定（v0.37） | 高 | 锁定版本，封装适配层 |
| 迁移期间功能断裂 | 高 | 渐进式迁移，保持旧 API 兼容 |
| 前端重构工作量大 | 中 | 复用现有组件，分阶段替换 |
| IM 适配器审核周期长 | 中 | 先实现飞书，其他渠道后补 |
| 性能问题（大量群消息） | 低 | SQLite 分表 + 消息归档 |

---

## 十六、成功指标

1. **Agent 响应延迟** < 2s（首 token）
2. **群聊消息吞吐** > 100 msg/s
3. **工作流执行成功率** > 95%
4. **Agent 健康检查准确率** 100%（修复 stale-sweep bug）
5. **E2E 测试覆盖率** > 80%
6. **CLI 命令响应时间** < 500ms
