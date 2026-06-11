# MapleOS v3 — 完整产品设计与技术实现方案

> 版本: 3.0.0-complete | 日期: 2026-06-05  
> 基于现有 v2.0.8 代码库，补全评估发现的全部设计缺口

---

## 目录

1. [产品定位与核心理念](#一产品定位与核心理念)
2. [用户角色与权限模型](#二用户角色与权限模型)
3. [完整数据模型](#三完整数据模型)
4. [Agent 系统完整设计](#四agent-系统完整设计)
5. [群聊系统完整设计](#五群聊系统完整设计)
6. [任务系统完整设计](#六任务系统完整设计)
7. [审批流完整设计](#七审批流完整设计)
8. [工作流系统完整设计](#八工作流系统完整设计)
9. [记忆系统完整设计](#九记忆系统完整设计)
10. [IM 适配器完整设计](#十im-适配器完整设计)
11. [前端完整 UI 设计](#十一前端完整-ui-设计)
12. [API 接口设计](#十二api-接口设计)
13. [安全与治理设计](#十三安全与治理设计)
14. [性能优化设计](#十四性能优化设计)
15. [工程实施计划](#十五工程实施计划)

---

## 一、产品定位与核心理念

### 1.1 产品定义

**MapleOS v3** 是以群聊为核心交互载体的 **人机共生 Agent OS**。

不同于单 Agent 对话工具，MapleOS 的核心场景是：**一群人 + 一群 Agent 在同一个群聊空间里协同完成复杂任务**，人负责决策与审批，Agent 负责执行与验证，工作流保障可追溯与可审计。

```
核心公式: 群聊(Group) × Agent(rig) × 工作流(Workflow) × 任务(Task) × 审批(Approval) × 记忆(Memory)
```

### 1.2 核心差异化

| 维度 | MapleOS v3 | 传统 Agent 工具 |
|------|-----------|----------------|
| 交互模型 | 群聊协作（多人 + 多 Agent） | 单人单 Agent 对话 |
| 人机关系 | 对等成员（共享用户模型） | 工具调用关系 |
| 任务管理 | 原生任务系统 + 看板 | 无或外挂 |
| 审批机制 | 完整状态机 + 超时处理 | 无 |
| 记忆体系 | 三层记忆 + 向量检索 | 无或简单历史 |
| 工作流 | 三模式（结构化/可视化/YAML） | 单一链式 |
| 渠道 | Web + Desktop + CLI + IM 适配器 | 仅 API/Chat |

### 1.3 六层架构总览

```
┌─────────────────────────────────────────────────────────────────────┐
│  L1 Interface: Web(Next.js) │ Desktop(Tauri) │ CLI │ IM Adapters    │
├─────────────────────────────────────────────────────────────────────┤
│  L2 Collaboration: GroupChat │ Task │ Approval │ Presence │ Session  │
├─────────────────────────────────────────────────────────────────────┤
│  L3 Orchestration: Workflow │ EventBus │ AgentOrchestrator │ Cron    │
├─────────────────────────────────────────────────────────────────────┤
│  L4 Capabilities: ToolServer │ Skills │ MCP │ KnowledgeBase          │
├─────────────────────────────────────────────────────────────────────┤
│  L5 Intelligence: rig Agent │ LLM Router │ Memory │ Streaming        │
├─────────────────────────────────────────────────────────────────────┤
│  L6 Storage: SQLite │ CRDT │ VectorDB │ FileSystem                   │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 二、用户角色与权限模型

### 2.1 平台角色

| 角色 | 描述 | 核心权限 |
|------|------|---------|
| `platform_admin` | 平台管理员 | 全部操作，含用户/Agent 管理、系统配置 |
| `user` | 普通用户 | 创建群/任务/工作流，与 Agent 交互 |
| `viewer` | 只读用户 | 查看群聊/任务/工作流，不可发消息 |
| `agent` | AI Agent | 响应消息、执行工具、运行工作流 |

### 2.2 群组角色

| 角色 | 描述 | 权限 |
|------|------|------|
| `owner` | 群主 | 全部权限，含解散群、移除成员 |
| `admin` | 管理员 | 管理规则、配置 Agent、审批委托 |
| `member` | 普通成员 | 发消息、创建任务、查看所有内容 |
| `viewer` | 只读成员 | 查看消息和任务，不可发消息 |

### 2.3 审批权限模型

```rust
pub struct ApprovalPermission {
    /// 谁可以审批（AND/OR 逻辑）
    pub approvers: ApproverSpec,
    /// 最低通过票数（N-of-M 模式）
    pub quorum: ApprovalQuorum,
    /// 超时策略
    pub timeout: TimeoutPolicy,
}

pub enum ApproverSpec {
    /// 指定角色（群内有此角色的所有人）
    Role { roles: Vec<GroupRole> },
    /// 指定成员
    Members { user_ids: Vec<String> },
    /// 任意一个管理员即可
    AnyAdmin,
    /// 发起者不能审批自己
    AnyMemberExceptInitiator,
}

pub enum ApprovalQuorum {
    /// 任意一人通过即可
    Any,
    /// 需要 N 人通过
    NOf(usize),
    /// 需要全部通过
    All,
    /// 需要超过半数
    Majority,
}
```

---

## 三、完整数据模型

### 3.1 统一用户表（修正版）

```sql
CREATE TABLE users (
    id TEXT PRIMARY KEY,                 -- nanoid
    name TEXT NOT NULL,
    email TEXT,                          -- NULLABLE，Agent 无邮箱
    password_hash TEXT,                  -- NULLABLE，Agent 无密码
    avatar_url TEXT,
    user_type TEXT NOT NULL DEFAULT 'human' CHECK(user_type IN ('human', 'agent')),
    status TEXT NOT NULL DEFAULT 'offline'
        CHECK(status IN ('online', 'away', 'busy', 'offline', 'error')),
    platform_role TEXT NOT NULL DEFAULT 'user'
        CHECK(platform_role IN ('platform_admin', 'user', 'viewer')),

    -- Agent 专属字段（human 时为 NULL）
    soul_config TEXT,                    -- SOUL.md 内容
    memory_config TEXT,                  -- JSON: 记忆策略配置
    agent_config TEXT,                   -- JSON: 运行时配置
    connection_type TEXT
        CHECK(connection_type IN ('llm-api', 'http-ws', 'sdk', 'a2a', 'rig') OR connection_type IS NULL),
    connection_config TEXT,              -- JSON
    llm_provider TEXT,
    llm_model TEXT,
    llm_api_key_encrypted TEXT,          -- AES-256-GCM 加密
    llm_base_url TEXT,
    agent_api_key TEXT,                  -- 外部 Agent 接入密钥
    agent_api_secret_encrypted TEXT,

    -- rig Agent 配置
    rig_provider TEXT,
    rig_model TEXT,
    tools_config TEXT,                   -- JSON: 工具白/黑名单
    skills_config TEXT,                  -- JSON: 已加载的 Skill

    -- 健康监控（Agent 专属）
    last_heartbeat INTEGER,              -- Unix timestamp (ms)
    health_status TEXT DEFAULT 'unknown'
        CHECK(health_status IN ('healthy', 'degraded', 'unhealthy', 'unknown')),
    active_task_count INTEGER DEFAULT 0,

    -- 审计
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER                   -- 软删除
);

-- 人类用户：邮箱唯一（排除 NULL）
CREATE UNIQUE INDEX idx_users_email ON users(email) WHERE email IS NOT NULL;
-- Agent 心跳查询
CREATE INDEX idx_users_heartbeat ON users(user_type, last_heartbeat)
    WHERE user_type = 'agent';
-- 状态查询
CREATE INDEX idx_users_status ON users(user_type, status);
```

### 3.2 群聊表

```sql
CREATE TABLE groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    avatar_url TEXT,
    group_type TEXT NOT NULL DEFAULT 'collaboration'
        CHECK(group_type IN ('collaboration', 'project', 'channel', 'dm')),
    owner_id TEXT NOT NULL REFERENCES users(id),
    settings TEXT NOT NULL DEFAULT '{}',  -- JSON
    -- settings 包含:
    -- { "is_public": bool, "allow_bot_invite": bool,
    --   "default_agent_id": str|null, "max_members": int,
    --   "message_retention_days": int|null }
    member_count INTEGER NOT NULL DEFAULT 0,  -- 冗余计数，避免 COUNT(*) 查询
    message_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    archived_at INTEGER,                 -- 归档（不再活跃）
    deleted_at INTEGER
);

CREATE INDEX idx_groups_owner ON groups(owner_id);
CREATE INDEX idx_groups_type ON groups(group_type) WHERE deleted_at IS NULL;
```

### 3.3 群成员表

```sql
CREATE TABLE group_members (
    group_id TEXT NOT NULL REFERENCES groups(id),
    member_id TEXT NOT NULL REFERENCES users(id),
    member_type TEXT NOT NULL CHECK(member_type IN ('human', 'agent')),
    role TEXT NOT NULL DEFAULT 'member'
        CHECK(role IN ('owner', 'admin', 'member', 'viewer')),
    nickname TEXT,
    -- 成员级别的审批委托
    can_approve INTEGER NOT NULL DEFAULT 0,
    approval_scope TEXT,                 -- JSON: 可审批的操作类型列表
    joined_at INTEGER NOT NULL,
    last_active_at INTEGER,
    muted_until INTEGER,                 -- 禁言到期时间
    PRIMARY KEY (group_id, member_id)
);

CREATE INDEX idx_gm_member ON group_members(member_id);
CREATE INDEX idx_gm_role ON group_members(group_id, role);
```

### 3.4 群消息表（完整版）

```sql
CREATE TABLE group_messages (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES groups(id),
    sender_id TEXT NOT NULL REFERENCES users(id),
    sender_type TEXT NOT NULL CHECK(sender_type IN ('human', 'agent', 'system')),
    message_type TEXT NOT NULL CHECK(message_type IN (
        'text', 'markdown', 'image', 'file', 'voice',
        'tool_call', 'tool_result', 'thinking',
        'approval_request', 'approval_response',
        'workflow_run', 'workflow_step', 'workflow_complete', 'workflow_failed',
        'skill_call', 'skill_result',
        'task_created', 'task_updated', 'task_completed',
        'system', 'member_join', 'member_leave',
        'cron_trigger',
        'external_message'
    )),
    content TEXT NOT NULL,               -- JSON 结构化内容（见下方 Content Schema）
    -- 回复与线程
    reply_to_id TEXT REFERENCES group_messages(id),
    thread_root_id TEXT REFERENCES group_messages(id),  -- NULL = 本身是根
    thread_reply_count INTEGER NOT NULL DEFAULT 0,
    -- 来源渠道
    source_channel TEXT NOT NULL DEFAULT 'web'
        CHECK(source_channel IN (
            'web', 'api', 'sdk', 'cli', 'webhook',
            'im_feishu', 'im_wechat', 'im_dingtalk', 'im_telegram', 'im_slack'
        )),
    -- 外部 IM 幂等 key
    external_message_id TEXT,
    external_channel_id TEXT,
    -- 状态
    pinned INTEGER NOT NULL DEFAULT 0,
    edited_at INTEGER,
    deleted_at INTEGER,                  -- 软删除
    created_at INTEGER NOT NULL,

    -- 幂等约束：同一外部渠道同一消息 ID 只允许一条
    UNIQUE(source_channel, external_message_id)
        -- 过滤掉非外部渠道的消息
        -- SQLite partial unique index 用 WHERE 子句
);

-- 核心查询索引：群内分页（最重要的查询）
CREATE INDEX idx_gm_group_time ON group_messages(group_id, created_at DESC)
    WHERE deleted_at IS NULL;
-- 线程查询
CREATE INDEX idx_gm_thread ON group_messages(thread_root_id, created_at)
    WHERE thread_root_id IS NOT NULL;
-- 全文搜索（SQLite FTS5）
CREATE VIRTUAL TABLE group_messages_fts USING fts5(
    content, content='group_messages', content_rowid='rowid'
);
```

### 3.5 消息 Content Schema

不同消息类型的 `content` JSON 结构：

```typescript
// text / markdown
{ "text": string, "mentions": string[] }

// tool_call
{
  "tool_name": string,
  "call_id": string,
  "args": object,
  "status": "pending" | "running" | "success" | "failed",
  "started_at": number
}

// tool_result
{
  "call_id": string,    // 关联 tool_call
  "tool_name": string,
  "output": any,
  "error": string | null,
  "duration_ms": number
}

// approval_request
{
  "approval_id": string,    // 关联 approval_requests 表
  "action_type": string,    // 要执行的操作类型
  "action_description": string,
  "action_payload": object, // 待审批的具体操作
  "urgency": "low" | "normal" | "high" | "critical",
  "expires_at": number
}

// approval_response
{
  "approval_id": string,
  "decision": "approved" | "rejected" | "modified",
  "modified_payload": object | null,  // 修改后重新提交
  "comment": string
}

// workflow_step
{
  "workflow_run_id": string,
  "step_id": string,
  "step_name": string,
  "status": "running" | "success" | "failed" | "skipped",
  "progress": number,   // 0-100
  "output_summary": string
}
```

### 3.6 消息辅助表

```sql
-- 消息编辑历史
CREATE TABLE message_edit_history (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES group_messages(id),
    old_content TEXT NOT NULL,
    edited_by TEXT NOT NULL REFERENCES users(id),
    edited_at INTEGER NOT NULL
);
CREATE INDEX idx_meh_message ON message_edit_history(message_id, edited_at DESC);

-- 消息已读状态
CREATE TABLE message_reads (
    message_id TEXT NOT NULL REFERENCES group_messages(id),
    user_id TEXT NOT NULL REFERENCES users(id),
    read_at INTEGER NOT NULL,
    PRIMARY KEY (message_id, user_id)
);
-- 未读数查询：按群聊统计
CREATE INDEX idx_mr_user_group ON message_reads(user_id);

-- 消息表情反应
CREATE TABLE message_reactions (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES group_messages(id),
    user_id TEXT NOT NULL REFERENCES users(id),
    emoji TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(message_id, user_id, emoji)
);

-- 消息书签
CREATE TABLE message_bookmarks (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES group_messages(id),
    user_id TEXT NOT NULL REFERENCES users(id),
    note TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE(message_id, user_id)
);

-- 置顶消息
CREATE TABLE pinned_messages (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES group_messages(id),
    group_id TEXT NOT NULL REFERENCES groups(id),
    pinned_by TEXT NOT NULL REFERENCES users(id),
    pinned_at INTEGER NOT NULL
);
CREATE INDEX idx_pm_group ON pinned_messages(group_id, pinned_at DESC);
```

### 3.7 任务系统表（补全版）

```sql
-- 任务主表
CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES groups(id),
    project_id TEXT REFERENCES projects(id),   -- 可选的项目归属
    parent_task_id TEXT REFERENCES tasks(id),  -- 子任务支持
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'todo'
        CHECK(status IN ('backlog', 'todo', 'in_progress', 'review', 'done', 'cancelled', 'blocked')),
    priority TEXT NOT NULL DEFAULT 'medium'
        CHECK(priority IN ('critical', 'high', 'medium', 'low')),
    -- 分配
    assignee_id TEXT REFERENCES users(id),     -- 当前执行者（人或 Agent）
    assignee_type TEXT CHECK(assignee_type IN ('human', 'agent') OR assignee_type IS NULL),
    creator_id TEXT NOT NULL REFERENCES users(id),
    -- 来源：从哪条消息创建的
    source_message_id TEXT REFERENCES group_messages(id),
    -- 时间
    due_at INTEGER,
    started_at INTEGER,
    completed_at INTEGER,
    estimated_minutes INTEGER,
    actual_minutes INTEGER,
    -- 标签与分类
    labels TEXT NOT NULL DEFAULT '[]',         -- JSON array of strings
    -- 任务完成后回传的消息 ID（形成消息-任务双向关联）
    completion_message_id TEXT REFERENCES group_messages(id),
    -- 子任务完成计数（冗余）
    subtask_count INTEGER NOT NULL DEFAULT 0,
    subtask_done_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER
);

CREATE INDEX idx_tasks_group ON tasks(group_id, status, created_at DESC)
    WHERE deleted_at IS NULL;
CREATE INDEX idx_tasks_assignee ON tasks(assignee_id, status)
    WHERE deleted_at IS NULL;
CREATE INDEX idx_tasks_due ON tasks(due_at) WHERE due_at IS NOT NULL AND deleted_at IS NULL;
CREATE INDEX idx_tasks_parent ON tasks(parent_task_id) WHERE parent_task_id IS NOT NULL;

-- 任务状态变更日志
CREATE TABLE task_status_history (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    old_status TEXT NOT NULL,
    new_status TEXT NOT NULL,
    changed_by TEXT NOT NULL REFERENCES users(id),  -- 可以是 Agent
    reason TEXT,
    changed_at INTEGER NOT NULL
);
CREATE INDEX idx_tsh_task ON task_status_history(task_id, changed_at DESC);

-- 任务评论
CREATE TABLE task_comments (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    author_id TEXT NOT NULL REFERENCES users(id),
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    edited_at INTEGER,
    deleted_at INTEGER
);

-- 任务附件
CREATE TABLE task_attachments (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    uploader_id TEXT NOT NULL REFERENCES users(id),
    file_name TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    file_type TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

-- 项目（可选的任务容器）
CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES groups(id),
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK(status IN ('active', 'paused', 'completed', 'archived')),
    owner_id TEXT NOT NULL REFERENCES users(id),
    start_date TEXT,         -- YYYY-MM-DD（纯日期，不转换时区）
    end_date TEXT,           -- YYYY-MM-DD
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

### 3.8 审批流表（补全版）

```sql
-- 审批请求主表
CREATE TABLE approval_requests (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES groups(id),
    -- 发起者
    initiator_id TEXT NOT NULL REFERENCES users(id),
    initiator_type TEXT NOT NULL CHECK(initiator_type IN ('human', 'agent')),
    -- 关联的消息（群里的审批请求消息）
    request_message_id TEXT REFERENCES group_messages(id),
    -- 审批配置
    action_type TEXT NOT NULL,           -- 操作类型，如 'code_deploy' | 'file_delete' | 'workflow_run'
    action_description TEXT NOT NULL,
    action_payload TEXT NOT NULL,        -- JSON: 待审批的完整操作参数
    urgency TEXT NOT NULL DEFAULT 'normal'
        CHECK(urgency IN ('low', 'normal', 'high', 'critical')),
    -- 审批规则
    approver_spec TEXT NOT NULL,         -- JSON: ApproverSpec
    quorum_type TEXT NOT NULL DEFAULT 'any'
        CHECK(quorum_type IN ('any', 'n_of', 'all', 'majority')),
    quorum_n INTEGER,                    -- 当 quorum_type = 'n_of' 时有效
    -- 状态
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending', 'approved', 'rejected', 'expired', 'cancelled', 'modified')),
    -- 超时
    expires_at INTEGER NOT NULL,         -- 必须设置过期时间
    -- 结果
    final_payload TEXT,                  -- JSON: 最终执行参数（可能被修改）
    resolved_at INTEGER,
    resolution_comment TEXT,
    -- 执行状态
    execution_status TEXT
        CHECK(execution_status IN ('pending_execution', 'executing', 'executed', 'execution_failed') OR execution_status IS NULL),
    execution_result TEXT,               -- JSON
    execution_error TEXT,
    -- 关联
    workflow_run_id TEXT,                -- 如果是工作流审批节点触发的
    task_id TEXT REFERENCES tasks(id),  -- 关联任务（如果有）
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_ar_group ON approval_requests(group_id, status, created_at DESC);
CREATE INDEX idx_ar_initiator ON approval_requests(initiator_id);
CREATE INDEX idx_ar_expires ON approval_requests(expires_at, status)
    WHERE status = 'pending';

-- 审批投票记录
CREATE TABLE approval_votes (
    id TEXT PRIMARY KEY,
    approval_id TEXT NOT NULL REFERENCES approval_requests(id),
    voter_id TEXT NOT NULL REFERENCES users(id),
    decision TEXT NOT NULL CHECK(decision IN ('approved', 'rejected', 'modified')),
    modified_payload TEXT,               -- 如果 decision = 'modified'
    comment TEXT,
    -- 关联的群消息（审批响应消息）
    response_message_id TEXT REFERENCES group_messages(id),
    voted_at INTEGER NOT NULL,
    -- 每人每条审批只能投一次，但可以修改（取最后一次）
    UNIQUE(approval_id, voter_id)
);
CREATE INDEX idx_av_approval ON approval_votes(approval_id, voted_at);

-- 审批超时处理日志
CREATE TABLE approval_timeout_logs (
    id TEXT PRIMARY KEY,
    approval_id TEXT NOT NULL REFERENCES approval_requests(id),
    timeout_action TEXT NOT NULL
        CHECK(timeout_action IN ('auto_reject', 'auto_approve', 'escalate', 'notify')),
    processed_at INTEGER NOT NULL,
    result TEXT
);
```

### 3.9 工作流表

```sql
-- 工作流定义
CREATE TABLE workflows (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES groups(id),
    creator_id TEXT NOT NULL REFERENCES users(id),
    name TEXT NOT NULL,
    description TEXT,
    version TEXT NOT NULL DEFAULT '1.0',
    -- 类型
    workflow_type TEXT NOT NULL DEFAULT 'yaml'
        CHECK(workflow_type IN ('yaml', 'visual', 'structured_5phase')),
    -- 定义
    yaml_content TEXT,                   -- YAML 原文
    node_graph TEXT,                     -- JSON: DAG 节点图（可视化编辑器格式）
    -- 触发器配置
    triggers TEXT NOT NULL DEFAULT '[]', -- JSON array of TriggerConfig
    -- 执行配置
    max_parallel_nodes INTEGER NOT NULL DEFAULT 5,
    timeout_seconds INTEGER,
    retry_policy TEXT,                   -- JSON
    -- 状态
    is_active INTEGER NOT NULL DEFAULT 1,
    last_run_at INTEGER,
    run_count INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- 工作流运行实例
CREATE TABLE workflow_runs (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL REFERENCES workflows(id),
    group_id TEXT NOT NULL REFERENCES groups(id),
    trigger_type TEXT NOT NULL
        CHECK(trigger_type IN ('manual', 'webhook', 'cron', 'event', 'message')),
    trigger_payload TEXT,                -- JSON: 触发时的输入数据
    -- 发起者
    triggered_by TEXT REFERENCES users(id),
    -- 关联的群消息（工作流运行通知）
    run_message_id TEXT REFERENCES group_messages(id),
    -- 状态
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending', 'running', 'paused', 'success', 'failed', 'cancelled', 'waiting_approval')),
    -- 进度
    current_step_id TEXT,
    completed_steps INTEGER NOT NULL DEFAULT 0,
    total_steps INTEGER,
    -- 上下文
    context TEXT NOT NULL DEFAULT '{}',  -- JSON: 运行时变量
    -- 结果
    output TEXT,                         -- JSON: 最终输出
    error TEXT,
    -- 时间
    started_at INTEGER,
    completed_at INTEGER,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_wr_workflow ON workflow_runs(workflow_id, created_at DESC);
CREATE INDEX idx_wr_group ON workflow_runs(group_id, status, created_at DESC);
CREATE INDEX idx_wr_status ON workflow_runs(status) WHERE status IN ('pending', 'running', 'waiting_approval');

-- 工作流步骤执行记录
CREATE TABLE workflow_step_executions (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES workflow_runs(id),
    step_id TEXT NOT NULL,
    step_name TEXT NOT NULL,
    step_type TEXT NOT NULL,             -- llm | tool | agent | condition | human | parallel | ...
    -- 执行结果
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending', 'running', 'success', 'failed', 'skipped', 'waiting_approval')),
    input TEXT,                          -- JSON
    output TEXT,                         -- JSON
    error TEXT,
    -- 时间
    started_at INTEGER,
    completed_at INTEGER,
    duration_ms INTEGER,
    -- 关联审批（如果是 human 节点）
    approval_id TEXT REFERENCES approval_requests(id)
);

CREATE INDEX idx_wse_run ON workflow_step_executions(run_id, step_id);
```

### 3.10 会话表

```sql
-- Agent 会话（Agent 为主体的对话上下文）
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES groups(id),
    agent_id TEXT NOT NULL REFERENCES users(id),
    session_type TEXT NOT NULL DEFAULT 'chat'
        CHECK(session_type IN ('chat', 'task', 'workflow', 'cron')),
    -- 关联对象（任务/工作流）
    related_task_id TEXT REFERENCES tasks(id),
    related_workflow_run_id TEXT REFERENCES workflow_runs(id),
    -- 状态
    status TEXT NOT NULL DEFAULT 'active'
        CHECK(status IN ('active', 'paused', 'completed', 'archived')),
    -- rig ConversationMemory 序列化
    conversation_history TEXT NOT NULL DEFAULT '[]',  -- JSON: Message[]
    -- 运行时上下文
    context TEXT NOT NULL DEFAULT '{}',  -- JSON: 工具状态、变量等
    -- 统计
    message_count INTEGER NOT NULL DEFAULT 0,
    tool_call_count INTEGER NOT NULL DEFAULT 0,
    token_count INTEGER NOT NULL DEFAULT 0,
    -- 时间
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    archived_at INTEGER
);

CREATE INDEX idx_sessions_group_agent ON sessions(group_id, agent_id, status);
```

### 3.11 定时任务表

```sql
CREATE TABLE cron_jobs (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES groups(id),
    agent_id TEXT NOT NULL REFERENCES users(id),
    creator_id TEXT NOT NULL REFERENCES users(id),
    name TEXT NOT NULL,
    description TEXT,
    -- 调度配置
    cron_expr TEXT NOT NULL,             -- 标准 cron 表达式
    timezone TEXT NOT NULL DEFAULT 'Asia/Shanghai',
    -- 执行配置
    prompt TEXT,                         -- 自然语言任务描述（可选）
    workflow_id TEXT REFERENCES workflows(id),  -- 可选：执行指定工作流
    -- 至少一个：prompt 或 workflow_id 不为空
    -- 状态
    enabled INTEGER NOT NULL DEFAULT 1,
    -- 执行历史
    last_run_at INTEGER,
    last_run_status TEXT
        CHECK(last_run_status IN ('success', 'failed', 'running') OR last_run_status IS NULL),
    next_run_at INTEGER,
    run_count INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- 定时任务执行日志
CREATE TABLE cron_run_logs (
    id TEXT PRIMARY KEY,
    cron_job_id TEXT NOT NULL REFERENCES cron_jobs(id),
    status TEXT NOT NULL CHECK(status IN ('success', 'failed', 'timeout')),
    triggered_at INTEGER NOT NULL,
    started_at INTEGER,
    completed_at INTEGER,
    duration_ms INTEGER,
    output TEXT,
    error TEXT,
    -- 关联的会话和消息
    session_id TEXT REFERENCES sessions(id),
    trigger_message_id TEXT REFERENCES group_messages(id)
);
CREATE INDEX idx_crl_job ON cron_run_logs(cron_job_id, triggered_at DESC);
```

### 3.12 Agent 记忆表

```sql
-- Agent 三层记忆
CREATE TABLE agent_memories (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES users(id),
    memory_type TEXT NOT NULL
        CHECK(memory_type IN ('working', 'episodic', 'semantic')),
    -- 内容
    content TEXT NOT NULL,
    summary TEXT,                        -- AI 生成的摘要（用于检索显示）
    -- 向量嵌入（存储为 BLOB 或 base64）
    embedding BLOB,
    embedding_model TEXT,               -- 生成 embedding 时使用的模型
    -- 来源
    source_type TEXT
        CHECK(source_type IN ('chat', 'skill', 'workflow', 'task', 'manual', 'import') OR source_type IS NULL),
    source_id TEXT,                      -- 来源对象 ID（消息 ID/任务 ID 等）
    group_id TEXT REFERENCES groups(id), -- 所属群（情景记忆有群上下文）
    -- 相关性
    relevance_score REAL NOT NULL DEFAULT 0.7,
    access_count INTEGER NOT NULL DEFAULT 0,
    last_accessed_at INTEGER,
    -- 生命周期
    expires_at INTEGER,                  -- 工作记忆过期时间
    -- 审计
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_am_agent_type ON agent_memories(agent_id, memory_type, created_at DESC);
CREATE INDEX idx_am_agent_group ON agent_memories(agent_id, group_id)
    WHERE group_id IS NOT NULL;
-- 工作记忆过期清理
CREATE INDEX idx_am_expires ON agent_memories(expires_at)
    WHERE expires_at IS NOT NULL AND memory_type = 'working';
```

### 3.13 群规则表

```sql
CREATE TABLE group_rules (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES groups(id),
    name TEXT NOT NULL,
    description TEXT,
    rule_type TEXT NOT NULL CHECK(rule_type IN (
        'auto_assign', 'auto_approve', 'rate_limit', 'time_window',
        'tool_restriction', 'knowledge_scope', 'workflow_permission',
        'prompt_template', 'approval_policy'
    )),
    config TEXT NOT NULL,                -- JSON: 规则配置（见下方各类型 schema）
    enabled INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 0, -- 越大越先执行
    -- 生效条件（可选，用于条件性规则）
    condition_expr TEXT,                 -- 如 "message.sender_type == 'agent'"
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_gr_group ON group_rules(group_id, enabled, priority DESC);
```

---

## 四、Agent 系统完整设计

### 4.1 MapleAgent 完整封装

```rust
use rig::agent::Agent;
use rig::providers::openai;
use std::sync::Arc;
use tokio::sync::RwLock;

/// MapleAgent = rig Agent + 扩展层
pub struct MapleAgent {
    /// rig 核心（实际 LLM 交互）
    inner: Agent<openai::CompletionModel>,
    /// 元数据
    meta: AgentMeta,
    /// 健康状态（实时更新）
    health: Arc<RwLock<AgentHealth>>,
    /// 三层记忆
    memory: Arc<dyn MemorySystem>,
    /// SOUL.md 人格
    soul: SoulConfig,
    /// 事件总线（广播到群聊）
    event_bus: Arc<EventBus>,
    /// 会话管理器
    session_mgr: Arc<SessionManager>,
}

pub struct AgentMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub avatar_url: Option<String>,
    pub model: String,
    pub provider: String,
    pub capabilities: AgentCapabilities,
    pub system_prompt: String,
    pub tags: Vec<String>,
    pub version: String,
    pub created_at: i64,
}

pub struct AgentCapabilities {
    pub tools: Vec<ToolCapability>,
    pub skills: Vec<String>,
    pub max_context_length: usize,
    pub supports_streaming: bool,
    pub supports_function_calling: bool,
    pub supports_vision: bool,
    pub supports_audio: bool,
    pub max_parallel_tasks: usize,
}

pub struct AgentHealth {
    pub status: HealthStatus,
    pub last_heartbeat: i64,
    pub active_task_count: usize,
    pub error_rate_1m: f64,
    pub avg_response_ms: f64,
    pub consecutive_failures: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Unhealthy { reason: String },
    Unknown,
}
```

### 4.2 Agent 注册表（修复版）

```rust
pub struct AgentRegistry {
    db: Arc<Database>,
    /// 内存缓存：agent_id -> MapleAgent
    agents: DashMap<String, Arc<MapleAgent>>,
    /// 心跳超时阈值
    heartbeat_timeout_ms: u64,
    /// 健康检查间隔
    health_check_interval: Duration,
}

impl AgentRegistry {
    /// 注册 Agent（完整字段，不再只存 id+name）
    pub async fn register(&self, schema: AgentSchema) -> Result<String> {
        let agent_id = nanoid::nanoid!();
        sqlx::query!(
            r#"INSERT INTO users (
                id, name, email, user_type, status, platform_role,
                soul_config, memory_config, agent_config,
                llm_provider, llm_model, rig_provider, rig_model,
                tools_config, skills_config,
                last_heartbeat, health_status,
                created_at, updated_at
            ) VALUES (?, ?, NULL, 'agent', 'offline', 'user',
                ?, ?, ?,
                ?, ?, ?, ?,
                ?, ?,
                ?, 'unknown',
                unixepoch('now') * 1000, unixepoch('now') * 1000)"#,
            agent_id, schema.name,
            schema.soul_config, schema.memory_config_json, schema.agent_config_json,
            schema.llm_provider, schema.llm_model, schema.rig_provider, schema.rig_model,
            schema.tools_config_json, schema.skills_config_json,
            schema.initial_heartbeat,
        )
        .execute(&self.db.pool)
        .await?;

        Ok(agent_id)
    }

    /// 心跳更新（修复旧版 stale-sweep bug）
    pub async fn heartbeat(&self, agent_id: &str) -> Result<()> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        sqlx::query!(
            "UPDATE users SET last_heartbeat = ?, status = 'online', updated_at = ?
             WHERE id = ? AND user_type = 'agent'",
            now_ms, now_ms, agent_id
        )
        .execute(&self.db.pool)
        .await?;
        Ok(())
    }

    /// stale sweep：只标记真正超时的 Agent（修复旧版 bug）
    pub async fn sweep_stale(&self) -> Result<usize> {
        let threshold_ms = chrono::Utc::now().timestamp_millis()
            - self.heartbeat_timeout_ms as i64;
        // 只修改 online/away/busy 中超时的 Agent
        // 已经是 offline/error 的不重复修改
        let affected = sqlx::query!(
            r#"UPDATE users
               SET status = 'error',
                   health_status = 'unhealthy',
                   updated_at = unixepoch('now') * 1000
               WHERE user_type = 'agent'
                 AND status IN ('online', 'away', 'busy')
                 AND last_heartbeat < ?
                 AND last_heartbeat IS NOT NULL"#,
            threshold_ms
        )
        .execute(&self.db.pool)
        .await?
        .rows_affected();

        Ok(affected as usize)
    }

    /// 列出所有 Agent（完整数据，修复旧版只返回 id+name）
    pub async fn list_agents(&self) -> Result<Vec<AgentSummary>> {
        sqlx::query_as!(
            AgentSummary,
            r#"SELECT
                id, name, avatar_url, status, health_status,
                last_heartbeat, llm_provider, llm_model,
                active_task_count, tools_config, skills_config,
                created_at, updated_at
               FROM users
               WHERE user_type = 'agent' AND deleted_at IS NULL
               ORDER BY name ASC"#
        )
        .fetch_all(&self.db.pool)
        .await
    }
}
```

### 4.3 Agent PromptHook 生命周期

```rust
pub struct MapleAgentHook {
    group_id: String,
    agent_id: String,
    session_id: String,
    rules_engine: Arc<GroupRulesEngine>,
    event_bus: Arc<EventBus>,
    db: Arc<Database>,
}

impl PromptHook for MapleAgentHook {
    /// LLM 调用前：检查速率限制、广播 typing 状态
    async fn on_completion_call(
        &self,
        prompt: &Message,
        history: &[Message],
    ) -> HookAction {
        // 1. 检查速率限制规则
        if let Err(e) = self.rules_engine.check_rate_limit(&self.group_id, &self.agent_id).await {
            return HookAction::Abort { reason: e.to_string() };
        }
        // 2. 检查时间窗口规则
        if let Err(e) = self.rules_engine.check_time_window(&self.group_id).await {
            return HookAction::Abort { reason: e.to_string() };
        }
        // 3. 广播 Agent 思考中状态
        self.event_bus.publish(GroupEvent::AgentThinking {
            group_id: self.group_id.clone(),
            agent_id: self.agent_id.clone(),
            session_id: self.session_id.clone(),
        }).await;
        HookAction::Continue
    }

    /// 工具调用前：检查权限、决定是否需要审批
    async fn on_tool_call(
        &self,
        tool_name: &str,
        call_id: &str,
        args: &str,
    ) -> ToolCallHookAction {
        // 1. 检查工具限制规则
        if self.rules_engine.is_tool_denied(&self.group_id, tool_name).await {
            return ToolCallHookAction::Block {
                reason: format!("Tool '{}' is restricted in this group", tool_name),
            };
        }
        // 2. 发布工具调用消息到群聊
        let msg_id = self.publish_tool_call_message(tool_name, call_id, args).await;
        // 3. 检查是否需要人工审批
        if let Some(approval_config) = self.rules_engine
            .get_approval_requirement(&self.group_id, tool_name, args).await
        {
            let approval_id = self.create_approval_request(
                tool_name, args, approval_config, &msg_id
            ).await;
            // 等待审批（最长等到 expires_at）
            return ToolCallHookAction::WaitForApproval { approval_id };
        }
        ToolCallHookAction::Continue
    }

    /// 工具调用后：更新消息状态、记录到记忆
    async fn on_tool_result(
        &self,
        tool_name: &str,
        call_id: &str,
        args: &str,
        result: &str,
    ) -> HookAction {
        // 1. 发布工具结果消息
        self.publish_tool_result_message(tool_name, call_id, result).await;
        // 2. 关键工具结果记录到情景记忆
        if self.is_memorable_result(tool_name, result) {
            self.memory.store_episodic(
                &self.agent_id,
                &format!("工具 {} 的执行结果: {}", tool_name, result),
                Some(self.group_id.clone()),
            ).await;
        }
        HookAction::Continue
    }

    /// LLM 回复后：广播到群聊、更新会话
    async fn on_completion_response(
        &self,
        _prompt: &Message,
        response: &CompletionResponse,
    ) -> HookAction {
        // 1. 广播 Agent 回复到群聊
        self.publish_agent_reply(response).await;
        // 2. 更新会话消息计数
        self.update_session_stats().await;
        // 3. 停止 typing 状态
        self.event_bus.publish(GroupEvent::AgentStopTyping {
            group_id: self.group_id.clone(),
            agent_id: self.agent_id.clone(),
        }).await;
        HookAction::Continue
    }
}
```

---

## 五、群聊系统完整设计

### 5.1 群聊触发逻辑（优先级链）

Agent 触发的优先级链，从高到低：

```
1. 直接 @ 提及（@agent_name 或 @agent_id）  [最高优先级]
2. 群规则 auto_assign 匹配（关键词触发）
3. 群默认 Agent（settings.default_agent_id）
4. 无响应（不触发任何 Agent）                [最低优先级]
```

```rust
pub struct MessageRouter {
    rules_engine: Arc<GroupRulesEngine>,
    registry: Arc<AgentRegistry>,
}

impl MessageRouter {
    pub async fn route_message(
        &self,
        message: &GroupMessage,
    ) -> Vec<String> { // 返回应响应此消息的 agent_id 列表
        let mut responders = Vec::new();

        // P1: 显式 @ 提及
        for mention in &message.extract_mentions() {
            if let Ok(agent_id) = self.registry.resolve_mention(mention).await {
                responders.push(agent_id);
            }
        }

        // P2: 规则引擎匹配（只在无显式 @ 时生效）
        if responders.is_empty() {
            let matched = self.rules_engine
                .match_auto_assign(&message.group_id, &message.content)
                .await;
            responders.extend(matched);
        }

        // P3: 群默认 Agent（只在无匹配时生效）
        if responders.is_empty() {
            if let Some(default_agent) = self.get_group_default_agent(&message.group_id).await {
                responders.push(default_agent);
            }
        }

        responders.dedup();
        responders
    }
}
```

### 5.2 群规则引擎（完整版）

```rust
pub enum RuleConfig {
    AutoAssign {
        keywords: Vec<String>,
        keyword_mode: KeywordMode,   // any | all | regex
        agent_id: String,
        max_parallel: usize,         // 同时最多触发几次
    },
    AutoApprove {
        roles: Vec<GroupRole>,
        confidence_threshold: f64,
        tool_patterns: Vec<String>,  // 支持通配符，如 "file_*"
    },
    RateLimit {
        max_per_minute: u32,
        max_per_hour: u32,
        burst_allowance: u32,        // 突发允许额度
        scope: RateLimitScope,       // per_agent | per_group | per_user
    },
    TimeWindow {
        schedule: Vec<TimeSlot>,     // 支持多个时间段
        timezone: String,
        outside_action: OutsideAction, // reject | queue | notify
    },
    ToolRestriction {
        allowed_tools: Option<Vec<String>>,  // None = 允许所有
        denied_tools: Option<Vec<String>>,
        require_approval_tools: Vec<String>, // 需要审批的工具
    },
    KnowledgeScope {
        allowed_kb_ids: Vec<String>,
        allow_web_search: bool,
    },
    WorkflowPermission {
        can_create: bool,
        can_execute: bool,
        can_modify: bool,
        max_concurrent_runs: u32,
    },
    PromptTemplate {
        prefix: String,              // 注入到 system prompt 前缀
        suffix: String,              // 注入到 system prompt 后缀
        variables: HashMap<String, String>,
    },
    ApprovalPolicy {
        triggers: Vec<ApprovalTrigger>,
        approver_spec: ApproverSpec,
        quorum: ApprovalQuorum,
        timeout_seconds: u64,
        timeout_action: TimeoutAction, // auto_reject | auto_approve | escalate
        escalate_to: Option<Vec<String>>, // 升级审批人
    },
}
```

### 5.3 消息分页与归档策略

```rust
/// 消息分页（游标分页，不用 offset）
pub struct MessagePageRequest {
    pub group_id: String,
    pub limit: usize,                        // 默认 50，最大 100
    pub before_id: Option<String>,           // 游标：加载此 ID 之前的消息
    pub after_id: Option<String>,            // 游标：加载此 ID 之后的消息
    pub message_types: Option<Vec<String>>,  // 过滤消息类型
    pub thread_root_id: Option<String>,      // 加载特定线程
    pub search_query: Option<String>,        // 全文搜索
}

/// SQL 实现
pub async fn get_messages_before(
    group_id: &str,
    before_id: &str,
    limit: usize,
) -> Result<Vec<GroupMessage>> {
    sqlx::query_as!(
        GroupMessage,
        r#"SELECT * FROM group_messages
           WHERE group_id = ?
             AND deleted_at IS NULL
             AND created_at < (
                 SELECT created_at FROM group_messages WHERE id = ?
             )
           ORDER BY created_at DESC
           LIMIT ?"#,
        group_id, before_id, limit as i64
    )
    .fetch_all(&pool)
    .await
}

/// 消息归档：超过 retention 天数的消息移入归档表
pub async fn archive_old_messages(
    group_id: &str,
    retention_days: u32,
) -> Result<usize> {
    let cutoff = chrono::Utc::now().timestamp_millis()
        - (retention_days as i64 * 86400 * 1000);
    // 插入归档表
    sqlx::query!(
        "INSERT INTO group_messages_archive SELECT * FROM group_messages
         WHERE group_id = ? AND created_at < ? AND deleted_at IS NULL",
        group_id, cutoff
    ).execute(&pool).await?;
    // 删除主表
    let count = sqlx::query!(
        "DELETE FROM group_messages WHERE group_id = ? AND created_at < ?",
        group_id, cutoff
    ).execute(&pool).await?.rows_affected();
    Ok(count as usize)
}
```

---

## 六、任务系统完整设计

### 6.1 任务状态机

```
                         ┌──────────┐
                         │ backlog  │ ← 待规划
                         └────┬─────┘
                              │ 规划
                              ▼
                         ┌──────────┐
    ┌─────────────────── │   todo   │ ← 待处理
    │ 取消               └────┬─────┘
    ▼                         │ 开始
┌──────────┐            ┌─────▼────┐
│cancelled │            │in_progress│ ← 执行中
└──────────┘            └─────┬─────┘
    ▲                         │ 提交审查
    │ 取消             ┌──────▼──────┐
    │             ┌──  │   review    │ ← 审查中
    │             │    └──────┬──────┘
    │       拒回  │           │ 通过
    │             │    ┌──────▼──────┐
    │             └──► │    done     │ ← 完成
    │                  └─────────────┘
    │                         ▲
    │                         │ 解除阻塞
    └──────────────────── ┌───┴──────┐
                          │ blocked  │ ← 被阻塞
                          └──────────┘
```

### 6.2 任务-消息双向关联

```rust
/// 从消息创建任务（人工标注或 Agent 提取）
pub async fn create_task_from_message(
    message_id: &str,
    task_data: CreateTaskRequest,
) -> Result<String> {
    let task_id = nanoid::nanoid!();
    // 1. 创建任务
    sqlx::query!(
        "INSERT INTO tasks (id, group_id, title, description,
         source_message_id, creator_id, assignee_id, assignee_type,
         status, priority, due_at, labels, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'todo', ?, ?, ?, ?, ?)",
        task_id, task_data.group_id, task_data.title, task_data.description,
        message_id, task_data.creator_id, task_data.assignee_id,
        task_data.assignee_type, task_data.priority, task_data.due_at,
        serde_json::to_string(&task_data.labels)?,
        now_ms(), now_ms()
    ).execute(&pool).await?;

    // 2. 在群聊发布任务创建消息
    let task_msg_id = publish_task_message(
        &task_data.group_id, &task_id, "task_created"
    ).await?;

    Ok(task_id)
}

/// 任务完成时回传到来源消息线程
pub async fn complete_task(
    task_id: &str,
    completed_by: &str,
    result: &str,
) -> Result<()> {
    // 1. 更新任务状态
    let task = get_task(task_id).await?;
    update_task_status(task_id, "done", completed_by).await?;

    // 2. 在来源消息线程回复
    if let Some(source_msg_id) = task.source_message_id {
        let reply_id = publish_group_message(GroupMessageRequest {
            group_id: task.group_id.clone(),
            sender_id: completed_by.to_string(),
            message_type: "task_completed".to_string(),
            content: json!({
                "task_id": task_id,
                "task_title": task.title,
                "result": result,
            }),
            reply_to_id: Some(source_msg_id),
            thread_root_id: Some(source_msg_id),
        }).await?;

        // 3. 更新任务的 completion_message_id
        sqlx::query!(
            "UPDATE tasks SET completion_message_id = ? WHERE id = ?",
            reply_id, task_id
        ).execute(&pool).await?;
    }
    Ok(())
}
```

### 6.3 智能任务分配

```rust
/// Agent 自主创建任务（基于消息内容）
pub struct TaskExtractor;

impl TaskExtractor {
    /// 让 Agent 从消息中提取任务
    pub async fn extract_from_message(
        agent: &MapleAgent,
        message: &GroupMessage,
    ) -> Result<Vec<ExtractedTask>> {
        let prompt = format!(
            r#"分析以下消息，提取其中的可执行任务。
消息内容：{}
返回 JSON 数组，每个元素包含：
- title: 任务标题（简洁，< 50字符）
- description: 任务详细描述
- priority: critical|high|medium|low
- estimated_minutes: 预计耗时（分钟）
- suggested_assignee_type: human|agent（建议分配给谁）
- due_suggestion: 建议截止时间（ISO 8601，如无法判断则 null）
只返回 JSON，不要其他内容。"#,
            message.content_text()
        );
        let response = agent.inner.prompt(&prompt).await?;
        let tasks: Vec<ExtractedTask> = serde_json::from_str(&response)?;
        Ok(tasks)
    }
}
```

---

## 七、审批流完整设计

### 7.1 审批状态机

```
                    ┌─────────┐
               ─► │ pending  │ ← 等待审批
              /    └────┬─────┘
  创建              ┌───┤ 投票事件
             │      ▼   ▼   ▼
             │    ─────────────
             │   │  check_quorum │
             │    ─────────────
             │      ┌───┼───────┐
             │      ▼   ▼       ▼
          ┌──┴──┐  ┌────┐  ┌────────┐
          │expir│  │appr│  │rejected│
          │ed   │  │oved│  └────────┘
          └──┬──┘  └──┬─┘
             │         │ 需要执行
             ▼         ▼
          ┌─────────────────┐
          │  auto_reject /  │
          │  auto_approve / │    ┌──────────────────┐
          │  escalate       │    │ pending_execution │
          └─────────────────┘    └────────┬─────────┘
                                           │
                                    ┌──────▼──────┐
                                    │  executing  │
                                    └──────┬──────┘
                                      ┌────┤
                                      ▼    ▼
                                  ┌──────┐ ┌──────────────────┐
                                  │exec'd│ │execution_failed  │
                                  └──────┘ └──────────────────┘
```

### 7.2 审批请求创建与处理

```rust
pub struct ApprovalService {
    db: Arc<Database>,
    event_bus: Arc<EventBus>,
    timeout_scheduler: Arc<TimeoutScheduler>,
}

impl ApprovalService {
    /// 创建审批请求
    pub async fn create_approval(
        &self,
        req: CreateApprovalRequest,
    ) -> Result<ApprovalRequest> {
        let approval_id = nanoid::nanoid!();
        let expires_at = req.expires_at.unwrap_or_else(|| {
            // 默认超时：根据 urgency 决定
            let timeout_seconds = match req.urgency {
                Urgency::Critical => 900,   // 15 分钟
                Urgency::High => 3600,       // 1 小时
                Urgency::Normal => 86400,    // 24 小时
                Urgency::Low => 259200,      // 72 小时
            };
            chrono::Utc::now().timestamp() + timeout_seconds
        });

        // 1. 插入数据库
        sqlx::query!(
            "INSERT INTO approval_requests (...) VALUES (...)",
            approval_id, req.group_id, req.initiator_id, req.initiator_type,
            // ...
        ).execute(&self.db.pool).await?;

        // 2. 发布审批请求消息到群聊
        let msg_id = self.publish_approval_message(&approval_id, &req).await?;
        sqlx::query!(
            "UPDATE approval_requests SET request_message_id = ? WHERE id = ?",
            msg_id, approval_id
        ).execute(&self.db.pool).await?;

        // 3. 注册超时任务
        self.timeout_scheduler.schedule(
            approval_id.clone(),
            expires_at,
            |id| async move { self.handle_timeout(&id).await },
        ).await;

        // 4. 通知审批人（如果是高优先级）
        if req.urgency >= Urgency::High {
            self.notify_approvers(&approval_id, &req).await;
        }

        self.get_approval(&approval_id).await
    }

    /// 处理投票
    pub async fn vote(
        &self,
        approval_id: &str,
        voter_id: &str,
        decision: ApprovalDecision,
    ) -> Result<ApprovalStatus> {
        // 1. 检查投票者权限
        let approval = self.get_approval(approval_id).await?;
        self.validate_voter(&approval, voter_id).await?;

        // 2. 检查审批是否仍在 pending
        if approval.status != ApprovalStatus::Pending {
            return Err(ApprovalError::NotPending);
        }

        // 3. 记录投票（UPSERT：允许改变投票）
        sqlx::query!(
            r#"INSERT INTO approval_votes (id, approval_id, voter_id, decision, comment, voted_at)
               VALUES (?, ?, ?, ?, ?, ?)
               ON CONFLICT(approval_id, voter_id) DO UPDATE SET
               decision = excluded.decision,
               comment = excluded.comment,
               voted_at = excluded.voted_at"#,
            nanoid::nanoid!(), approval_id, voter_id,
            decision.decision_type.to_str(), decision.comment,
            now_ms()
        ).execute(&self.db.pool).await?;

        // 4. 发布审批响应消息到群聊
        self.publish_vote_message(approval_id, voter_id, &decision, &approval.request_message_id).await?;

        // 5. 检查是否达到 quorum
        let new_status = self.check_quorum(approval_id).await?;

        // 6. 如果已决定，执行后续动作
        if new_status == ApprovalStatus::Approved {
            self.execute_approved_action(approval_id).await?;
        }

        Ok(new_status)
    }

    /// 超时处理
    async fn handle_timeout(&self, approval_id: &str) -> Result<()> {
        let approval = self.get_approval(approval_id).await?;
        if approval.status != ApprovalStatus::Pending { return Ok(()); }

        // 获取群的审批超时策略
        let policy = self.get_approval_policy(&approval.group_id).await?;
        match policy.timeout_action {
            TimeoutAction::AutoReject => {
                self.update_status(approval_id, ApprovalStatus::Expired).await?;
                self.publish_timeout_message(approval_id, "auto_reject").await?;
            }
            TimeoutAction::AutoApprove => {
                self.update_status(approval_id, ApprovalStatus::Approved).await?;
                self.execute_approved_action(approval_id).await?;
                self.publish_timeout_message(approval_id, "auto_approve").await?;
            }
            TimeoutAction::Escalate { to } => {
                // 升级：通知上级审批人，延长超时时间
                self.escalate(approval_id, &to).await?;
            }
        }
        Ok(())
    }

    /// Quorum 检查
    async fn check_quorum(&self, approval_id: &str) -> Result<ApprovalStatus> {
        let approval = self.get_approval(approval_id).await?;
        let votes = self.get_votes(approval_id).await?;

        let approved_count = votes.iter().filter(|v| v.decision == "approved").count();
        let rejected_count = votes.iter().filter(|v| v.decision == "rejected").count();
        let eligible_count = self.count_eligible_voters(&approval).await?;

        let reached = match approval.quorum_type {
            QuorumType::Any => approved_count >= 1,
            QuorumType::NOf(n) => approved_count >= n,
            QuorumType::All => approved_count == eligible_count,
            QuorumType::Majority => approved_count > eligible_count / 2,
        };

        // 如果否决票已经足以阻止通过，直接 rejected
        let blocked = rejected_count > eligible_count - match approval.quorum_type {
            QuorumType::NOf(n) => n,
            QuorumType::Majority => eligible_count / 2 + 1,
            _ => 1,
        };

        if reached { Ok(ApprovalStatus::Approved) }
        else if blocked { Ok(ApprovalStatus::Rejected) }
        else { Ok(ApprovalStatus::Pending) }
    }
}
```

---

## 八、工作流系统完整设计

### 8.1 工作流 DAG 执行引擎

```rust
pub struct WorkflowEngine {
    db: Arc<Database>,
    agent_registry: Arc<AgentRegistry>,
    tool_server: Arc<ToolServer>,
    event_bus: Arc<EventBus>,
    approval_service: Arc<ApprovalService>,
    petgraph: PetgraphBackend,
}

impl WorkflowEngine {
    pub async fn execute_run(
        &self,
        run_id: &str,
        context: Value,
    ) -> Result<Value> {
        let run = self.get_run(run_id).await?;
        let workflow = self.get_workflow(&run.workflow_id).await?;
        let dag = self.parse_dag(&workflow)?;

        // 拓扑排序
        let execution_order = petgraph::algo::toposort(&dag, None)
            .map_err(|_| WorkflowError::CyclicDependency)?;

        let mut ctx = WorkflowContext::new(context);
        let mut completed = HashSet::new();

        for batch in self.get_parallel_batches(&dag, &execution_order)? {
            // 同一批次的节点并行执行
            let handles: Vec<_> = batch.into_iter().map(|node_id| {
                let engine = self.clone();
                let node = dag[node_id].clone();
                let ctx_snapshot = ctx.snapshot();
                tokio::spawn(async move {
                    engine.execute_node(&run_id, &node, ctx_snapshot).await
                })
            }).collect();

            // 等待批次完成
            for handle in handles {
                let (node_id, output) = handle.await??;
                ctx.set_node_output(&node_id, output);
                completed.insert(node_id);

                // 检查是否需要审批暂停
                if ctx.is_waiting_approval() {
                    self.update_run_status(run_id, "waiting_approval").await?;
                    // 等待审批完成后续
                    return self.wait_and_resume(run_id, ctx).await;
                }
            }

            // 评估条件节点，决定下一批
        }

        Ok(ctx.final_output())
    }

    async fn execute_node(
        &self,
        run_id: &str,
        node: &WorkflowNode,
        ctx: ContextSnapshot,
    ) -> Result<(String, Value)> {
        let step_id = nanoid::nanoid!();
        self.record_step_start(run_id, &step_id, node).await?;

        let output = match &node.node_type {
            NodeType::Llm { model, prompt } => {
                let resolved_prompt = ctx.resolve_template(prompt)?;
                let agent = self.agent_registry.get_ephemeral_agent(model).await?;
                let response = agent.inner.prompt(&resolved_prompt).await?;
                json!({ "text": response })
            }
            NodeType::Tool { tool_name, args } => {
                let resolved_args = ctx.resolve_template_value(args)?;
                self.tool_server.call(tool_name, resolved_args).await?
            }
            NodeType::Agent { agent_id, prompt, tools, max_turns } => {
                let resolved_prompt = ctx.resolve_template(prompt)?;
                let agent = self.agent_registry.get_agent(agent_id).await?;
                agent.run_task(&resolved_prompt, tools, *max_turns).await?
            }
            NodeType::Human { action_type, description, payload } => {
                // 创建审批请求，挂起工作流
                let approval_id = self.approval_service.create_approval(
                    CreateApprovalRequest {
                        group_id: ctx.group_id.clone(),
                        initiator_id: ctx.initiator_id.clone(),
                        action_type: action_type.clone(),
                        action_description: ctx.resolve_template(description)?,
                        action_payload: ctx.resolve_template_value(payload)?,
                        urgency: Urgency::Normal,
                        expires_at: None,
                        workflow_run_id: Some(run_id.to_string()),
                    }
                ).await?;
                ctx.set_waiting_approval(approval_id);
                json!({ "approval_id": approval_id, "status": "pending" })
            }
            NodeType::Condition { expression } => {
                let result = ctx.eval_expression(expression)?;
                json!({ "result": result })
            }
            NodeType::Parallel { branches } => {
                // 并行执行所有分支
                let results = futures::future::join_all(
                    branches.iter().map(|branch| {
                        self.execute_branch(run_id, branch, ctx.clone())
                    })
                ).await;
                let outputs: Vec<Value> = results.into_iter().collect::<Result<_>>()?;
                json!({ "branches": outputs })
            }
            NodeType::HttpRequest { method, url, headers, body } => {
                let client = reqwest::Client::new();
                let resp = client
                    .request(method.parse()?, ctx.resolve_template(url)?)
                    .json(&ctx.resolve_template_value(body)?)
                    .send().await?;
                resp.json::<Value>().await?
            }
            NodeType::GroupMessage { group_id, message_type, content } => {
                let msg_id = self.publish_workflow_message(
                    group_id, message_type, &ctx.resolve_template(content)?
                ).await?;
                json!({ "message_id": msg_id })
            }
            // ... 其他节点类型
        };

        self.record_step_complete(run_id, &step_id, &output).await?;
        Ok((node.id.clone(), output))
    }
}
```

### 8.2 YAML 工作流验证

```rust
pub struct YamlWorkflowValidator;

impl YamlWorkflowValidator {
    pub fn validate(yaml: &str) -> Result<WorkflowDefinition, Vec<ValidationError>> {
        let def: WorkflowDefinition = serde_yaml::from_str(yaml)?;
        let mut errors = Vec::new();

        // 1. 检查 DAG 无环
        let graph = self.build_graph(&def);
        if petgraph::algo::is_cyclic_directed(&graph) {
            errors.push(ValidationError::CyclicDependency);
        }

        // 2. 检查节点引用有效
        for edge in &def.edges {
            for target in edge.to.iter().flatten() {
                if !def.nodes.iter().any(|n| &n.id == target) {
                    errors.push(ValidationError::UnknownNodeRef(target.clone()));
                }
            }
        }

        // 3. 检查模板变量有效性
        for node in &def.nodes {
            let templates = node.extract_templates();
            for template in templates {
                if !self.is_valid_template(&template, &def) {
                    errors.push(ValidationError::InvalidTemplate(template));
                }
            }
        }

        if errors.is_empty() { Ok(def) } else { Err(errors) }
    }
}
```

---

## 九、记忆系统完整设计

### 9.1 三层记忆检索设计

```rust
pub struct MemorySystem {
    db: Arc<Database>,
    vector_store: Arc<dyn VectorStore>,
    embedding_model: Arc<dyn EmbeddingModel>,
}

impl MemorySystem {
    /// 注入记忆到 Agent prompt（自动组合三层）
    pub async fn build_memory_context(
        &self,
        agent_id: &str,
        query: &str,
        group_id: Option<&str>,
        session_id: &str,
    ) -> Result<MemoryContext> {
        // L1: 工作记忆 — 按 session 拉最近内容（最高优先）
        let working = self.get_working_memory(agent_id, session_id, 20).await?;

        // L2: 情景记忆 — 混合检索（关键词 + 向量相似度 + 时间衰减）
        let episodic = self.search_episodic(
            agent_id, query, group_id, EpisodicSearchConfig {
                top_k: 5,
                time_decay_factor: 0.95, // 越近越好
                relevance_threshold: 0.6,
            }
        ).await?;

        // L3: 语义记忆 — 纯向量相似度搜索
        let semantic = self.search_semantic(
            agent_id, query, SemanticSearchConfig {
                top_k: 3,
                threshold: 0.75,
                rerank: true,
            }
        ).await?;

        Ok(MemoryContext {
            working_memories: working,
            episodic_memories: episodic,
            semantic_memories: semantic,
        })
    }

    /// 工作记忆：按 session 拉最近 N 条，并自动过期清理
    async fn get_working_memory(
        &self,
        agent_id: &str,
        session_id: &str,
        limit: i64,
    ) -> Result<Vec<Memory>> {
        // 先清理过期记忆
        sqlx::query!(
            "DELETE FROM agent_memories
             WHERE agent_id = ? AND memory_type = 'working'
               AND expires_at IS NOT NULL AND expires_at < ?",
            agent_id, now_ms()
        ).execute(&self.db.pool).await?;

        // 查询 session 级别的最近工作记忆
        sqlx::query_as!(
            Memory,
            "SELECT * FROM agent_memories
             WHERE agent_id = ? AND memory_type = 'working'
               AND source_id = ?
             ORDER BY created_at DESC LIMIT ?",
            agent_id, session_id, limit
        )
        .fetch_all(&self.db.pool)
        .await
    }

    /// 情景记忆：BM25 + 向量混合检索
    async fn search_episodic(
        &self,
        agent_id: &str,
        query: &str,
        group_id: Option<&str>,
        config: EpisodicSearchConfig,
    ) -> Result<Vec<Memory>> {
        // BM25 关键词搜索
        let bm25_results = sqlx::query_as!(
            Memory,
            "SELECT am.*, bm25(am_fts) AS score
             FROM agent_memories am
             JOIN agent_memories_fts am_fts ON am.rowid = am_fts.rowid
             WHERE am.agent_id = ? AND am.memory_type = 'episodic'
               AND am_fts MATCH ?
               AND (? IS NULL OR am.group_id = ?)
             ORDER BY score DESC LIMIT ?",
            agent_id, query, group_id, group_id, config.top_k as i64
        ).fetch_all(&self.db.pool).await?;

        // 向量相似度搜索
        let query_embedding = self.embedding_model.embed(query).await?;
        let vector_results = self.vector_store.search(
            agent_id, &query_embedding, config.top_k, 0.6
        ).await?;

        // RRF (Reciprocal Rank Fusion) 融合排序
        let merged = self.rrf_merge(bm25_results, vector_results, config.top_k);

        // 时间衰减重排
        let reranked = self.apply_time_decay(merged, config.time_decay_factor);

        // 更新访问计数
        for mem in &reranked {
            sqlx::query!(
                "UPDATE agent_memories SET access_count = access_count + 1,
                 last_accessed_at = ? WHERE id = ?",
                now_ms(), mem.id
            ).execute(&self.db.pool).await?;
        }

        Ok(reranked)
    }

    /// 语义记忆：纯向量搜索 + 可选重排序
    async fn search_semantic(
        &self,
        agent_id: &str,
        query: &str,
        config: SemanticSearchConfig,
    ) -> Result<Vec<Memory>> {
        let query_embedding = self.embedding_model.embed(query).await?;
        let candidates = self.vector_store.search(
            agent_id, &query_embedding, config.top_k * 3, config.threshold
        ).await?;

        if config.rerank && !candidates.is_empty() {
            // Cross-encoder 重排（如果模型支持）
            let reranked = self.rerank_by_relevance(query, candidates).await?;
            Ok(reranked.into_iter().take(config.top_k).collect())
        } else {
            Ok(candidates.into_iter().take(config.top_k).collect())
        }
    }

    /// relevance_score 打分策略
    async fn store_memory(&self, req: StoreMemoryRequest) -> Result<String> {
        let mem_id = nanoid::nanoid!();

        // 生成 embedding
        let embedding = self.embedding_model.embed(&req.content).await?;

        // 自动计算 relevance_score（基于内容质量）
        let relevance = self.score_relevance(&req.content, &req.source_type).await?;

        // 工作记忆：设置过期时间（24小时）
        let expires_at = if req.memory_type == MemoryType::Working {
            Some(now_ms() + 86400 * 1000)
        } else {
            None
        };

        // 插入数据库
        sqlx::query!(
            "INSERT INTO agent_memories (id, agent_id, memory_type, content,
             summary, embedding, source_type, source_id, group_id,
             relevance_score, expires_at, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            mem_id, req.agent_id, req.memory_type.to_str(), req.content,
            req.summary, embedding.as_bytes(), req.source_type.to_str(),
            req.source_id, req.group_id, relevance, expires_at,
            now_ms(), now_ms()
        ).execute(&self.db.pool).await?;

        // 同步到向量存储
        self.vector_store.upsert(&mem_id, &embedding, &req.agent_id).await?;

        Ok(mem_id)
    }
}
```

---

## 十、IM 适配器完整设计

### 10.1 幂等性保障

```rust
/// 外部消息幂等处理
pub struct ExternalMessageProcessor {
    db: Arc<Database>,
    channel_manager: Arc<ChannelManager>,
}

impl ExternalMessageProcessor {
    /// 处理外部 IM 消息（含幂等检查）
    pub async fn process(
        &self,
        adapter: &str,
        raw: RawExternalMessage,
    ) -> Result<ProcessResult> {
        let external_id = &raw.external_message_id;

        // 幂等检查：检查是否已处理
        let existing = sqlx::query!(
            "SELECT id FROM group_messages
             WHERE source_channel = ? AND external_message_id = ?",
            format!("im_{}", adapter), external_id
        ).fetch_optional(&self.db.pool).await?;

        if let Some(row) = existing {
            // 已处理，返回已有消息 ID（幂等）
            return Ok(ProcessResult::Duplicate { message_id: row.id });
        }

        // 标准化消息
        let normalized = self.channel_manager
            .normalize(adapter, raw).await?;

        // 找到或创建对应的群聊（按 channel_id 映射）
        let group_id = self.resolve_or_create_group(
            adapter, &normalized.channel_id
        ).await?;

        // 找到或创建对应的用户（按 sender_id 映射）
        let user_id = self.resolve_or_create_user(
            adapter, &normalized.sender_id, &normalized.sender_name
        ).await?;

        // 写入消息
        let msg_id = self.write_message(
            &group_id, &user_id, &normalized, adapter, external_id
        ).await?;

        // 触发 Agent Hooks
        self.channel_manager.event_bus.publish(GroupEvent::MessageCreated {
            group_id: group_id.clone(),
            message_id: msg_id.clone(),
        }).await;

        Ok(ProcessResult::Success { message_id: msg_id, group_id })
    }
}

/// SQLite partial unique index（幂等约束）
/// 仅对外部渠道消息生效
// CREATE UNIQUE INDEX idx_gm_external_unique
//     ON group_messages(source_channel, external_message_id)
//     WHERE external_message_id IS NOT NULL;
```

### 10.2 飞书适配器

```rust
pub struct FeishuAdapter {
    app_id: String,
    app_secret: String,
    /// 飞书 Webhook 验证 token
    verification_token: String,
    event_handler: Arc<dyn ImEventHandler>,
    http_client: reqwest::Client,
}

impl ImAdapter for FeishuAdapter {
    async fn normalize(&self, raw: RawExternalMessage) -> Result<NormalizedMessage> {
        let feishu_event: FeishuMessageEvent = serde_json::from_value(raw.payload)?;
        Ok(NormalizedMessage {
            external_message_id: feishu_event.message.message_id.clone(),
            channel_id: feishu_event.message.chat_id.clone(),
            sender_id: feishu_event.sender.sender_id.union_id.clone(),
            sender_name: feishu_event.sender.sender_id.user_id.clone(),
            content: self.parse_feishu_content(&feishu_event.message)?,
            timestamp: feishu_event.message.create_time.parse()?,
            reply_to: feishu_event.message.parent_id.clone(),
        })
    }

    async fn send_message(
        &self,
        channel_id: &str,
        msg: OutgoingMessage,
    ) -> Result<()> {
        let access_token = self.get_access_token().await?;
        let body = json!({
            "receive_id": channel_id,
            "msg_type": "interactive",
            "content": self.build_feishu_card(&msg),
        });
        self.http_client
            .post("https://open.feishu.cn/open-apis/im/v1/messages")
            .bearer_auth(access_token)
            .query(&[("receive_id_type", "chat_id")])
            .json(&body)
            .send().await?
            .error_for_status()?;
        Ok(())
    }
}
```

---

## 十一、前端完整 UI 设计

### 11.1 设计系统（完整版）

**色彩规范**

```css
:root {
    /* Primary */
    --color-primary: #2563EB;
    --color-primary-hover: #1D4ED8;
    --color-primary-active: #1E40AF;
    --color-primary-subtle: #EFF6FF;

    /* Surface hierarchy */
    --color-bg: #F8FAFC;
    --color-surface-1: #FFFFFF;      /* 卡片、面板 */
    --color-surface-2: #F1F5F9;      /* 嵌套内容 */
    --color-surface-3: #E2E8F0;      /* 分隔、边框 */

    /* Text hierarchy */
    --color-text-primary: #0F172A;
    --color-text-secondary: #475569;
    --color-text-muted: #94A3B8;     /* WCAG AA 4.5:1 on white */
    --color-text-disabled: #CBD5E1;

    /* Semantic */
    --color-success: #10B981;
    --color-success-subtle: #D1FAE5;
    --color-warning: #F59E0B;
    --color-warning-subtle: #FEF3C7;
    --color-error: #EF4444;
    --color-error-subtle: #FEE2E2;
    --color-info: #3B82F6;
    --color-info-subtle: #DBEAFE;

    /* Agent status */
    --color-agent-online: #10B981;
    --color-agent-degraded: #F59E0B;
    --color-agent-offline: #EF4444;
    --color-agent-unknown: #94A3B8;

    /* Dark mode overrides */
    --radius-sm: 6px;
    --radius-md: 10px;
    --radius-lg: 14px;
    --radius-xl: 20px;

    --shadow-sm: 0 1px 3px rgba(0,0,0,0.06), 0 1px 2px rgba(0,0,0,0.04);
    --shadow-md: 0 4px 12px rgba(0,0,0,0.08), 0 2px 6px rgba(0,0,0,0.04);
    --shadow-lg: 0 10px 30px rgba(0,0,0,0.10), 0 4px 12px rgba(0,0,0,0.06);
}
```

**可访问性修正**：`--color-text-muted: #64748B`（对比度 5.9:1，通过 WCAG AA）

### 11.2 核心页面布局规格

**主布局（三栏）**
```
┌──────────────────────────────────────────────────────────────────┐
│  Top Nav (64px): Logo │ 全局搜索 │ 通知 │ 个人头像               │
├──────────────────────────────────────────────────────────────────┤
│ 侧边栏  │                                             │ 右侧面板  │
│ (240px) │           主内容区                          │ (320px)  │
│         │           (自适应)                          │ (可折叠) │
│         │                                             │          │
│         │                                             │          │
└──────────────────────────────────────────────────────────────────┘
```

**群聊界面布局规格**
```
┌──────────────────────────────────────────────────────────────────┐
│ 群名称 (h-14) │ 成员按钮 │ 规则按钮 │ 设置 │ 搜索 │ [视图切换]  │
├─────────────────────────────────────────────────────────────────┤
│                 消息流 (flex-1, overflow-y: scroll)              │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ 日期分割线: 2026年6月5日                                │    │
│  ├─────────────────────────────────────────────────────────┤    │
│  │ [Avatar] 张三  10:30                                    │    │
│  │ 请审查一下安全漏洞问题                                   │    │
│  ├─────────────────────────────────────────────────────────┤    │
│  │ [Agent🤖] security-bot  10:30                           │    │
│  │ 正在分析...                                             │    │
│  │ ┌─────────────────────────────────────────────────────┐ │    │
│  │ │ 🔧 tool_call: security_scan                        │ │    │
│  │ │ args: { "scope": "full", "severity": "high" }      │ │    │
│  │ │ ▶ 点击展开详情                                      │ │    │
│  │ └─────────────────────────────────────────────────────┘ │    │
│  │ 分析完成，发现 2 个高危漏洞：...                         │    │
│  ├─────────────────────────────────────────────────────────┤    │
│  │ ⏳ 待审批  [approval_card]                             │    │
│  │ 操作：修复 SQL 注入漏洞                                  │    │
│  │ 已批准：1/2  超时：23:45:00 后                          │    │
│  │ [✅ 批准]  [❌ 拒绝]  [✏️ 修改后批准]                  │    │
│  └─────────────────────────────────────────────────────────┘    │
├──────────────────────────────────────────────────────────────────┤
│ 消息输入框 (h-auto, min-h-14, max-h-40)                         │
│ [@] [📎] [emoji] [工具] │        输入消息...         │ [发送]   │
└──────────────────────────────────────────────────────────────────┘
```

### 11.3 消息组件设计规格

各消息类型的视觉规格：

**ToolCall 消息卡片**
```
┌──────────────────────────────────────────────┐
│ 🔧 security_scan                   ▶ 展开    │
│ 状态: ✅ 成功   耗时: 2.3s                   │
│ └─ 查看详情                                  │
└──────────────────────────────────────────────┘
展开后:
┌──────────────────────────────────────────────┐
│ 🔧 security_scan                   ▲ 收起    │
│ ──── 参数 ────                               │
│ { "scope": "full", "severity": "high" }      │
│ ──── 输出 ────                               │
│ 发现 2 个高危漏洞:                            │
│   CVE-2024-1234: SQL注入 [高危]              │
│   CVE-2024-5678: XSS [中危]                  │
└──────────────────────────────────────────────┘
```

**ApprovalRequest 消息卡片**
```
┌──────────────────────────────────────────────┐
│ ⚠️ 审批请求   urgency: high                   │
│ ─────────────────────────────────────────────│
│ 操作: 修复 SQL 注入漏洞（文件: api/users.rs）  │
│ 发起者: security-bot                          │
│ ─────────────────────────────────────────────│
│ 进度: ██░░░░ 1/2 已批准    剩余: 23:45       │
│ ─────────────────────────────────────────────│
│ 张三 ✅  李四 ⏳                               │
│ ─────────────────────────────────────────────│
│ [✅ 批准]  [❌ 拒绝]  [✏️ 修改]  [📋 查看详情]│
└──────────────────────────────────────────────┘
```

**WorkflowStep 消息卡片**
```
┌──────────────────────────────────────────────┐
│ 🔄 代码审查工作流                    3/5 步骤 │
│ ██████████████░░░░░░░░░░░░   60%            │
│ ─────────────────────────────────────────────│
│ ✅ fetch_pr      [2.1s]                      │
│ ✅ analyze_code  [15.3s]                     │
│ ▶  check_security [running...]               │
│ ⏳ merge_reports                             │
│ ⏳ notify                                    │
└──────────────────────────────────────────────┘
```

### 11.4 空状态设计

```
群聊为空时:
┌──────────────────────────────────────────────┐
│                                              │
│              💬                              │
│                                              │
│         还没有任何消息                        │
│                                              │
│      在这里与 Agent 协作，开始你的工作吧      │
│                                              │
│  [➕ 邀请 Agent]    [✍️ 发送第一条消息]       │
│                                              │
└──────────────────────────────────────────────┘

无任务时:
┌──────────────────────────────────────────────┐
│              ✅                              │
│         没有待处理的任务                      │
│    [➕ 创建任务]    [🤖 让 Agent 提取任务]    │
└──────────────────────────────────────────────┘
```

### 11.5 Agent 健康监控面板（完整版）

```
┌──────────────────────────────────────────────────────────────────┐
│ Agent 监控   [实时] ● 已连接    [刷新]  [+ 创建 Agent]           │
├──────────────────────────────────────────────────────────────────┤
│ 总计: 5  │  🟢 健康: 3  │  🟡 降级: 1  │  🔴 异常: 1           │
│ 消息处理: 1,234/小时  │  平均响应: 1.8s  │  成功率: 97.3%       │
├──────────────────────────────────────────────────────────────────┤
│ Agent      │ 状态  │ 心跳      │ 模型      │ 任务 │ 响应  │操作   │
│────────────┼───────┼───────────┼───────────┼──────┼──────┼──────│
│ 🤖 coder   │ 🟢健康│ 2s ago    │ gpt-4o    │  2   │ 1.2s │[💬][⚙]│
│ 🤖 reviewer│ 🟢健康│ 5s ago    │ claude-3.5│  1   │ 2.1s │[💬][⚙]│
│ 🤖 security│ 🟡降级│ 52s ago   │ gpt-4o    │  0   │ 5.8s │[💬][⚙]│
│            │       │ 最后错误: Connection timeout               │
│ 🤖 assistant│🟢健康│ 3s ago   │ gpt-3.5   │  3   │ 0.9s │[💬][⚙]│
│ 🤖 monitor │ 🔴异常│ 5m ago    │ llama-3   │  0   │  N/A │[🔄][⚙]│
└──────────────────────────────────────────────────────────────────┘
```

### 11.6 任务看板视图

```
┌──────────┬──────────┬──────────┬──────────┬──────────┐
│ Backlog  │  待处理  │  进行中  │  审查中  │  已完成  │
│   (3)    │   (5)   │   (4)   │   (2)   │  (12)   │
├──────────┼──────────┼──────────┼──────────┼──────────┤
│┌────────┐│┌────────┐│┌────────┐│┌────────┐│          │
││ T-001  │││ T-004  │││ T-007  │││ T-010  ││          │
││修复登录││ │修复SQL ││ │实现JWT │││代码审查││          │
││ BUG    │││ 注入   │││ 刷新   │││        ││          │
││────────│││────────│││────────│││────────││          │
││🤖 coder│││🤖 sec  │││🤖 coder│││👤 张三 ││          │
││⚡ 高   │││🔴 紧急 │││🟡 中   │││        ││          │
││2d 剩余 │││今天    │││进行中  │││等待中  ││          │
│└────────┘│└────────┘│└────────┘│└────────┘│          │
└──────────┴──────────┴──────────┴──────────┴──────────┘
```

### 11.7 移动端适配

移动端（< 768px）采用单栏布局，底部 Tab Bar：

```
┌────────────────────┐
│ 🍁 MapleOS         │
│ 开发协作群      ≡  │
├────────────────────┤
│                    │
│   消息流            │
│                    │
├────────────────────┤
│ 输入消息...   [发送]│
├────────────────────┤
│ 💬  ✅  🔄  🤖  ⚙ │
│ 群聊 任务 流程 Agent 设置│
└────────────────────┘
```

---

## 十二、API 接口设计

### 12.1 RESTful API 规范

所有接口遵循：
- 基础路径：`/api/v3`
- 认证：`Authorization: Bearer <jwt_token>`
- Agent 认证：`X-Agent-ID: <agent_id>` + `X-Agent-Signature: HMAC-SHA256(timestamp.body)`
- 分页：游标分页 `?limit=50&before=<cursor_id>`

### 12.2 群聊 API

```
# 群聊管理
GET    /api/v3/groups                     # 列出群聊（含未读数）
POST   /api/v3/groups                     # 创建群聊
GET    /api/v3/groups/:id                 # 群聊详情
PATCH  /api/v3/groups/:id                 # 更新群聊
DELETE /api/v3/groups/:id                 # 删除/归档

# 成员
GET    /api/v3/groups/:id/members         # 成员列表
POST   /api/v3/groups/:id/members         # 邀请成员
DELETE /api/v3/groups/:id/members/:uid    # 移除成员
PATCH  /api/v3/groups/:id/members/:uid    # 更新成员角色

# 消息
GET    /api/v3/groups/:id/messages        # 消息列表（游标分页）
POST   /api/v3/groups/:id/messages        # 发送消息
PATCH  /api/v3/messages/:id              # 编辑消息
DELETE /api/v3/messages/:id              # 撤回消息
POST   /api/v3/messages/:id/reactions    # 添加表情反应
POST   /api/v3/messages/:id/bookmark     # 收藏消息

# 规则
GET    /api/v3/groups/:id/rules           # 群规则列表
POST   /api/v3/groups/:id/rules           # 创建规则
PATCH  /api/v3/groups/:id/rules/:rid      # 更新规则
DELETE /api/v3/groups/:id/rules/:rid      # 删除规则
```

### 12.3 审批 API

```
# 审批管理
GET    /api/v3/approvals                  # 我的待审批列表
POST   /api/v3/approvals                  # 创建审批请求
GET    /api/v3/approvals/:id              # 审批详情
POST   /api/v3/approvals/:id/vote         # 投票 (approved/rejected/modified)
DELETE /api/v3/approvals/:id              # 取消审批（只有发起者可取消）
GET    /api/v3/approvals/:id/votes        # 投票记录
```

### 12.4 任务 API

```
# 任务管理
GET    /api/v3/tasks                      # 任务列表（支持过滤）
POST   /api/v3/tasks                      # 创建任务
GET    /api/v3/tasks/:id                  # 任务详情
PATCH  /api/v3/tasks/:id                  # 更新任务
DELETE /api/v3/tasks/:id                  # 删除任务
PATCH  /api/v3/tasks/:id/status          # 更新状态
POST   /api/v3/tasks/:id/assign          # 分配任务
GET    /api/v3/tasks/:id/comments        # 任务评论
POST   /api/v3/tasks/:id/comments        # 添加评论
# 从消息创建任务
POST   /api/v3/messages/:id/extract-tasks # Agent 提取任务
```

### 12.5 WebSocket 事件

连接：`ws://localhost:3001/ws?token=<jwt>`

**服务端推送事件**

```typescript
// 群聊事件
"group:message:created"   { group_id, message: GroupMessage }
"group:message:edited"    { group_id, message_id, new_content }
"group:message:deleted"   { group_id, message_id }
"group:member:joined"     { group_id, member: GroupMember }
"group:member:left"       { group_id, member_id }

// Agent 状态事件
"agent:typing:start"      { group_id, agent_id, session_id }
"agent:typing:stop"       { group_id, agent_id }
"agent:status:changed"    { agent_id, old_status, new_status }
"agent:health:updated"    { agent_id, health: AgentHealth }

// 审批事件
"approval:created"        { group_id, approval: ApprovalRequest }
"approval:voted"          { approval_id, voter_id, decision, progress }
"approval:resolved"       { approval_id, final_status, executed }
"approval:expired"        { approval_id, action_taken }

// 工作流事件
"workflow:step:started"   { run_id, step_id, step_name }
"workflow:step:completed" { run_id, step_id, output_summary }
"workflow:run:completed"  { run_id, status, output }
"workflow:run:failed"     { run_id, error, failed_step_id }

// 任务事件
"task:created"            { group_id, task: Task }
"task:status:changed"     { task_id, old_status, new_status, changed_by }
"task:assigned"           { task_id, assignee_id, assignee_type }
```

**SSE 流式事件**

```
GET /api/v3/groups/:id/stream?session_id=<sid>
Content-Type: text/event-stream

event: agent_token
data: {"session_id": "...", "token": "Hello", "done": false}

event: agent_token
data: {"session_id": "...", "token": " world", "done": true}
```

---

## 十三、安全与治理设计

### 13.1 API 密钥加密

```rust
/// LLM API Key 加密存储
pub struct KeyEncryption {
    master_key: [u8; 32], // 从环境变量或 KMS 加载
}

impl KeyEncryption {
    pub fn encrypt(&self, plaintext: &str) -> Result<String> {
        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&self.master_key);
        let cipher = Aes256Gcm::new(key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher.encrypt(&nonce, plaintext.as_bytes())?;
        // base64(nonce || ciphertext)
        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&ciphertext);
        Ok(base64::encode(combined))
    }

    pub fn decrypt(&self, encoded: &str) -> Result<String> {
        let data = base64::decode(encoded)?;
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&self.master_key);
        let cipher = Aes256Gcm::new(key);
        let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
        let plaintext = cipher.decrypt(nonce, ciphertext)?;
        Ok(String::from_utf8(plaintext)?)
    }
}
```

### 13.2 Agent 认证

```rust
/// Agent API 签名验证（HMAC-SHA256）
pub fn verify_agent_signature(
    agent_id: &str,
    timestamp: &str,
    body: &str,
    provided_sig: &str,
    stored_secret: &str,
) -> Result<()> {
    // 1. 检查时间戳（防重放攻击，5分钟窗口）
    let ts: i64 = timestamp.parse()?;
    let now = chrono::Utc::now().timestamp();
    if (now - ts).abs() > 300 {
        return Err(AuthError::TimestampExpired);
    }

    // 2. 重建签名
    let message = format!("{}.{}", timestamp, body);
    let mut mac = HmacSha256::new_from_slice(stored_secret.as_bytes())?;
    mac.update(message.as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());

    // 3. 常量时间比较（防时序攻击）
    if !constant_time_eq(expected.as_bytes(), provided_sig.as_bytes()) {
        return Err(AuthError::InvalidSignature);
    }
    Ok(())
}
```

### 13.3 护栏检测器（完整版）

```rust
pub enum GuardrailDetector {
    /// 暴力重试检测：同一工具在 5 分钟内调用超过 N 次
    BruteRetry { tool_name: String, count: usize, window_minutes: u32 },

    /// 危险命令检测
    DangerousCommand {
        patterns: Vec<Regex>,  // rm -rf, DROP TABLE, etc.
    },

    /// 密钥泄露检测
    SecretLeak {
        patterns: Vec<Regex>,  // API key patterns, private key patterns
    },

    /// 范围蔓延检测：Agent 访问了未授权的知识库或工具
    ScopeCreep { unauthorized_resources: Vec<String> },

    /// 幻觉检测：声明没有证据支持的事实
    Hallucination { confidence_threshold: f64 },

    /// 过早完成检测：任务还未完成就声明完成
    PrematureDone,

    /// 无限循环检测：同一 prompt 重复超过 N 次
    InfiniteLoop { repetition_threshold: usize },
}
```

---

## 十四、性能优化设计

### 14.1 数据库索引策略

```sql
-- 消息查询（最频繁，O(log n)游标分页）
CREATE INDEX idx_gm_group_time ON group_messages(group_id, created_at DESC)
    WHERE deleted_at IS NULL;

-- 未读消息数（群聊列表侧边栏）
-- 策略：维护 group_unread_counts 缓存表，而非实时 COUNT
CREATE TABLE group_unread_counts (
    group_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    unread_count INTEGER NOT NULL DEFAULT 0,
    last_read_message_id TEXT,
    last_read_at INTEGER,
    PRIMARY KEY (group_id, user_id)
);

-- Agent 心跳扫描（每30秒执行）
CREATE INDEX idx_users_agent_heartbeat ON users(last_heartbeat)
    WHERE user_type = 'agent' AND status IN ('online', 'away', 'busy');

-- 审批超时处理
CREATE INDEX idx_approval_expires ON approval_requests(expires_at)
    WHERE status = 'pending';

-- 定时任务调度
CREATE INDEX idx_cron_next_run ON cron_jobs(next_run_at)
    WHERE enabled = 1;
```

### 14.2 连接池配置

```rust
let pool = SqlitePoolOptions::new()
    .max_connections(20)
    .min_connections(5)
    .acquire_timeout(Duration::from_secs(3))
    .idle_timeout(Duration::from_secs(600))
    .connect_with(
        SqliteConnectOptions::new()
            .filename("maple_os.db")
            .journal_mode(SqliteJournalMode::Wal)  // WAL 模式提升并发
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5))
            .pragma("cache_size", "-64000")          // 64MB 页缓存
            .pragma("temp_store", "memory")
            .pragma("mmap_size", "268435456"),        // 256MB mmap
    )
    .await?;
```

### 14.3 消息发送性能

```rust
/// 批量消息广播（避免 N 个 WebSocket 单独发送）
pub struct MessageBroadcaster {
    /// 按群聊分组的发送队列
    queues: DashMap<String, mpsc::Sender<BroadcastMessage>>,
    /// 每个群聊的批处理器
    batch_size: usize,         // 默认 10
    flush_interval_ms: u64,    // 默认 50ms
}

impl MessageBroadcaster {
    pub async fn broadcast(&self, group_id: &str, msg: BroadcastMessage) {
        if let Some(tx) = self.queues.get(group_id) {
            let _ = tx.send(msg).await;
        }
    }

    async fn batch_flush_loop(&self, group_id: String) {
        let mut batch = Vec::with_capacity(self.batch_size);
        let mut interval = tokio::time::interval(
            Duration::from_millis(self.flush_interval_ms)
        );
        loop {
            tokio::select! {
                msg = rx.recv() => {
                    batch.push(msg);
                    if batch.len() >= self.batch_size {
                        self.flush(&group_id, &mut batch).await;
                    }
                }
                _ = interval.tick() => {
                    if !batch.is_empty() {
                        self.flush(&group_id, &mut batch).await;
                    }
                }
            }
        }
    }
}
```

### 14.4 性能目标与基准

| 指标 | 目标 | 测量方法 |
|------|------|---------|
| 群聊消息吞吐 | > 1000 msg/s | 单实例压测 |
| 消息首字节延迟 | < 50ms | p99 |
| Agent 首 token 延迟 | < 2s | LLM 层 p95 |
| 工作流步骤调度延迟 | < 100ms | 节点间延迟 |
| WebSocket 连接数 | > 10,000 | 单实例 |
| 数据库查询（消息分页）| < 5ms | 有索引 |
| Agent 心跳扫描 | < 10ms | 1000 Agent |

---

## 十五、工程实施计划

### Phase 1：基础重构（2周）

**Week 1**
- 集成 rig-core v0.37，锁定版本
- 重构 `maple-agent`：MapleAgent 封装、修复注册表 11 个 Bug
- 数据库迁移：users 表增加 `last_heartbeat`、`health_status`、`active_task_count`

**Week 2**
- 建立 `maple-group` 模块（群聊 CRUD + 消息 CRUD）
- 实现消息分页（游标分页）
- 前端：群聊列表骨架屏 + 消息流组件

**交付物**：Agent 注册/心跳/健康监控可用；群聊基础消息收发可用

---

### Phase 2：群聊核心（2周）

**Week 3**
- 群规则引擎完整实现（所有 9 种规则类型）
- 消息触发优先级链（@ > auto_assign > default）
- Agent PromptHook 完整实现

**Week 4**
- 会话管理（Session CRUD + ConversationHistory）
- 消息辅助功能（编辑/撤回/表情/书签/置顶）
- 前端：群规则配置 UI + 消息操作菜单

**交付物**：完整群聊体验，Agent 可在群聊中响应消息

---

### Phase 3：任务+审批（2周）

**Week 5**
- 任务系统完整实现（CRUD + 状态机 + 消息双向关联）
- 任务看板视图
- Agent 自主提取/创建任务

**Week 6**
- 审批流完整实现（状态机 + Quorum + 超时处理 + 升级）
- 审批消息卡片 UI
- 工作流 human 节点集成审批

**交付物**：Human-in-the-Loop 完整闭环

---

### Phase 4：工作流+记忆（2周）

**Week 7**
- 工作流 DAG 执行引擎（基于 rig）
- YAML 工作流解析与验证
- 5 阶段结构化工作流

**Week 8**
- 三层记忆系统（存储 + 检索 + 向量化）
- 记忆注入 Agent prompt
- 定时任务调度器

**交付物**：完整工作流执行能力，Agent 具有记忆

---

### Phase 5：Skills+MCP+CLI（2周）

**Week 9**
- rig ToolServer 封装（内置工具：bash/file/web_search/think）
- Skills 加载器（本地目录 + Git 仓库）
- MCP 中间件框架

**Week 10**
- CLI 工具（clap，所有命令组）
- MCP 服务器集成
- 前端 Skills 管理页

**交付物**：完整工具生态

---

### Phase 6：IM 适配器（2周）

**Week 11**
- 飞书适配器（Webhook + 消息发送）
- 钉钉适配器
- 消息幂等处理

**Week 12**
- 微信适配器（仅企业微信）
- Telegram 适配器
- 渠道管理 UI

**交付物**：多渠道消息接入

---

### Phase 7：测试+优化（1周）

- E2E 测试（Playwright）覆盖核心流程
- 性能测试（消息吞吐、WebSocket 并发）
- 安全审计（API Key 加密、Agent 认证、危险命令检测）
- 文档更新（API 文档、部署指南）

---

你说得对，私聊（DM）是缺失的一块。它在数据模型里只有 `group_type = 'dm'` 一行，完全没有展开设计。下面补全它。私聊的设计缺口确实相当大——它在整个产品里的地位不只是"群聊的简化版"，而是承担着三种完全不同的场景：人人私聊、人与 Agent 对话、Agent 与 Agent 通信（A2A）。下面先给完整的产品与技术设计，再上交互预览。

---

## 私聊（DM）完整产品设计

### 一、为什么私聊不等于"小群聊"

群聊的核心是"一群人协作"，私聊的核心是"一对一信任"。两者在以下维度上存在本质差异：

| 维度 | 群聊 | 私聊 |
|------|------|------|
| 消息可见性 | 所有成员可见 | 仅双方可见 |
| Agent 触发 | 规则引擎 + @ 提及 | 直接响应（无需 @） |
| 审批流 | 多人参与 Quorum | 仅消息接收方审批 |
| 历史记忆 | 群级别共享 | 专属于这对关系 |
| 创建方式 | 显式建群 | 发消息即创建（隐式） |
| 消息归档 | 群配置控制 | 双方均可独立归档 |

### 二、三种私聊场景

**场景 A：人 ↔ 人**
最普通的私聊。差异点是：消息中可以拖拽 Agent 进来临时协助（升级为群聊），或者 @ 一个 Agent 让它只在这条对话里帮忙而不暴露给第三方。

**场景 B：人 ↔ Agent**
这是 MapleOS 的核心差异场景。用户直接找 Agent"开私聊"，Agent 拥有：专属于这对关系的记忆上下文、不受群规则约束的工具权限（由用户个人授权）、流式输出、持续会话（不像群聊那样容易被其他消息打断）。

**场景 C：Agent ↔ Agent（A2A）**
Agent 之间的内部协作信道。对用户可见（可监听），但不可中断。典型场景：`coder` Agent 把一个子任务委托给 `security-bot`，双方通过私聊交换中间结果。

### 三、数据模型补全

私聊本质上复用 `groups` 表（`group_type = 'dm'`），但需要专门的索引和几个补充字段：

```sql
-- DM 会话查找索引（快速找到两人之间是否已有私聊）
-- 把两个 user_id 规范化排序后拼接，保证唯一性
CREATE UNIQUE INDEX idx_dm_pair ON groups(
    -- dm_pair_key 是一个新增的计算列
    dm_pair_key
) WHERE group_type = 'dm' AND deleted_at IS NULL;

-- groups 表新增字段
ALTER TABLE groups ADD COLUMN dm_pair_key TEXT;
-- dm_pair_key = min(user_a_id, user_b_id) || ':' || max(user_a_id, user_b_id)
-- 在应用层创建 DM 时写入，保证 A↔B 和 B↔A 查询到同一条记录

ALTER TABLE groups ADD COLUMN dm_type TEXT
    CHECK(dm_type IN ('human_human', 'human_agent', 'agent_agent') OR dm_type IS NULL);

-- DM 专属设置（嵌在 settings JSON 里）
-- {
--   "muted": false,           -- 一方静音（不影响另一方）
--   "archived": false,        -- 一方归档
--   "pinned": false,          -- 置顶
--   "agent_persona": null,    -- human_agent 时可切换 Agent 人格
--   "tool_grants": [],        -- human_agent 时用户额外授权的工具
--   "a2a_visible": true,      -- agent_agent 时是否对相关人类成员可见
--   "a2a_initiator": null     -- agent_agent 时发起委托的原始任务 ID
-- }
```

```rust
/// DM 查找或创建（幂等）
pub async fn find_or_create_dm(
    db: &Database,
    user_a: &str,
    user_b: &str,
) -> Result<String> {
    // 规范化 pair key，保证 A↔B == B↔A
    let (lo, hi) = if user_a < user_b { (user_a, user_b) } else { (user_b, user_a) };
    let pair_key = format!("{}:{}", lo, hi);

    // 先查
    if let Some(row) = sqlx::query!(
        "SELECT id FROM groups WHERE dm_pair_key = ? AND group_type = 'dm'",
        pair_key
    ).fetch_optional(&db.pool).await? {
        return Ok(row.id);
    }

    // 不存在则创建
    let group_id = nanoid::nanoid!();
    // 判断 dm_type
    let user_a_type = get_user_type(db, user_a).await?;
    let user_b_type = get_user_type(db, user_b).await?;
    let dm_type = match (user_a_type.as_str(), user_b_type.as_str()) {
        ("human", "human") => "human_human",
        ("agent", "agent") => "agent_agent",
        _ => "human_agent",
    };

    sqlx::query!(
        r#"INSERT INTO groups (id, name, group_type, dm_type, dm_pair_key,
           owner_id, member_count, message_count, created_at, updated_at)
           VALUES (?, '', 'dm', ?, ?, ?, 2, 0, ?, ?)"#,
        group_id, dm_type, pair_key, user_a,
        now_ms(), now_ms()
    ).execute(&db.pool).await?;

    // 插入双方成员
    for (uid, utype) in [(user_a, user_a_type), (user_b, user_b_type)] {
        sqlx::query!(
            "INSERT INTO group_members (group_id, member_id, member_type, role, joined_at)
             VALUES (?, ?, ?, 'member', ?)",
            group_id, uid, utype, now_ms()
        ).execute(&db.pool).await?;
    }

    Ok(group_id)
}
```

### 四、人 ↔ Agent 私聊的专属设计

**工具授权模型**

在群聊里，工具权限由群规则控制；在私聊里，用户可以自行给 Agent 开放更多权限：

```rust
pub struct DmToolGrant {
    pub tool_name: String,          // "bash" | "file_edit" | "web_search" | ...
    pub granted_by: String,         // 必须是人类用户一方
    pub granted_at: i64,
    pub expires_at: Option<i64>,    // 可设有效期
    pub scope: Option<String>,      // 可选：限制作用域，如 "path:/home/user/project"
}
```

**私聊 Agent 的专属记忆隔离**

```
Agent 的记忆分区：

群聊记忆 (group_id=xxx)         私聊记忆 (dm_pair_key=A:B)
├── 群级情景记忆                  ├── 关系专属情景记忆
│   └── 对所有群成员可见            │   └── 仅 A、B 可见
└── 群级语义记忆                  └── 用户偏好记忆
                                      └── Agent 对这个用户的
                                          个性化认知
```

实现上只需在 `agent_memories` 表的 `group_id` 字段存 DM 的 `group_id`，查询时按 `group_id` 隔离即可。

**私聊 Persona 切换**

用户可以在私聊设置里切换 Agent 的人格（从已注册的 SOUL.md 列表里选）：

```rust
pub struct AgentPersona {
    pub persona_id: String,
    pub name: String,
    pub soul_content: String,   // SOUL.md 内容
    pub avatar_url: Option<String>,
}

// 在组装 LLM prompt 时：私聊的 soul_config 优先于 Agent 默认的
pub fn build_system_prompt(agent: &MapleAgent, dm_settings: &DmSettings) -> String {
    let soul = dm_settings.agent_persona
        .as_ref()
        .map(|p| p.soul_content.as_str())
        .unwrap_or(&agent.soul.content);
    format!("{}\n\n{}", soul, agent.meta.system_prompt)
}
```

### 五、Agent ↔ Agent（A2A）私聊设计

A2A 私聊由系统自动创建，不由人类发起。它的生命周期绑定到委托任务：

```rust
pub struct A2ADelegation {
    pub delegation_id: String,
    pub dm_group_id: String,        // 自动创建的私聊
    pub delegator_agent_id: String, // 委托方
    pub executor_agent_id: String,  // 执行方
    pub task_id: Option<String>,    // 关联任务
    pub workflow_run_id: Option<String>,
    pub prompt: String,             // 委托内容
    pub status: DelegationStatus,  // pending | running | completed | failed
    pub result: Option<Value>,
    pub visible_to: Vec<String>,    // 哪些人类用户可以监听这个 A2A 对话
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

pub enum DelegationStatus { Pending, Running, Completed, Failed }
```

A2A 消息对监听者只读，不能插话（除非手动介入触发审批）。

### 六、私聊列表与排序

DM 列表的排序规则，从高到低：

```
1. 置顶的 DM（pinned = true）
2. 有未读消息的 DM（按最新消息时间倒序）
3. 无未读消息的 DM（按最新消息时间倒序）
4. 被归档的 DM（折叠，需手动展开）
```

### 七、UI 交互逻辑

**发起私聊的入口：**
- 点击任意用户/Agent 头像 → 弹出 Profile 卡片 → "发消息" 按钮
- 搜索栏直接搜索用户名/Agent 名 → 选择 → 进入私聊
- 群聊里右键某成员 → "私聊"
- 工作流委托节点完成后，系统自动在侧边栏显示 A2A 私聊链接

**私聊界面与群聊的差异：**
- 无"规则"按钮（DM 无群规则，只有 DM settings）
- 人 ↔ Agent 时：有"工具授权"按钮、"切换 Persona"按钮、显示 Agent 当前活跃任务数
- Agent ↔ Agent 时：顶部有"监听中"标识，只读视图，有"介入"按钮触发审批暂停

---

现在做交互预览，完整展示三种私聊模式：

## 附录：关键设计决策记录

| 决策 | 选择 | 理由 | 风险 |
|------|------|------|------|
| 用户模型 | 人/Agent 共享 users 表 | 简化关系模型 | Agent 字段污染表结构 |
| 消息分页 | 游标分页 | O(log n)，无跳页问题 | 不支持跳转到第 N 页 |
| 审批超时 | 强制设置 expires_at | 避免无限等待 | 需要可配置的默认超时 |
| 记忆检索 | BM25 + 向量 RRF 融合 | 兼顾精确和语义 | embedding 计算成本 |
| Agent 触发 | 优先级链 | 行为可预期 | @ 和规则同时匹配时冗余 |
| 消息幂等 | source_channel + external_id 联合唯一 | 防 IM 重试重复 | 需要正确提取外部 ID |
| WebSocket | Socket.io + SSE 双栈 | 兼顾双向和流式 | 维护两套连接机制 |
| 数据库 | SQLite WAL | Local-first，零依赖 | 超大规模需迁移 PostgreSQL |

---

*文档版本: 3.0.0-complete | 作者: MapleOS 团队*
