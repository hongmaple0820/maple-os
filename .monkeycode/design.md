# 共创功能技术设计文档

## 1. 架构概览

共创功能横跨 MapleOS 六层架构中的 L2 协作层和 L3 编排层，核心依赖现有 maple-collab、maple-sync、maple-agent 模块。

```
┌─────────────────────────────────────────────────────────────┐
│                    L1 界面层 (Web/Desktop)                   │
│         共创面板 / 成员列表 / 任务看板 / 协同编辑器           │
├─────────────────────────────────────────────────────────────┤
│                    L2 协作层 (新增)                           │
│     CoCreationWorkspace / MemberManager / TaskAllocator      │
├─────────────────────────────────────────────────────────────┤
│                    L3 编排层 (扩展现有)                       │
│     WorkflowEngine + AgentOrchestrator + EventBus            │
├─────────────────────────────────────────────────────────────┤
│                    L5 智能层                                  │
│     LLM Router (能力匹配) + KnowledgeBase (共享知识)         │
├─────────────────────────────────────────────────────────────┤
│                    L6 存储层                                  │
│     SQLite + Automerge CRDT + WebDAV                        │
└─────────────────────────────────────────────────────────────┘
```

## 2. 模块设计

### 2.1 新增模块: maple-cocreation

位于 `core/maple-cocreation/`，负责共创工作空间的核心逻辑。

```
maple-cocreation/
├── src/
│   ├── lib.rs              # 模块入口
│   ├── workspace.rs        # 工作空间管理
│   ├── member.rs           # 成员管理与权限
│   ├── task_allocator.rs   # 智能任务分配
│   ├── presence.rs         # 在线状态与感知
│   ├── conflict.rs         # 冲突检测与解决
│   └── types.rs            # 共享类型定义
├── Cargo.toml
└── tests/
```

### 2.2 扩展现有模块

| 现有模块 | 扩展内容 |
|----------|----------|
| maple-collab | 增加 CRDT 文档支持、操作历史、版本回滚 |
| maple-sync | 修复数据一致性 Bug，增加工作空间级同步 |
| maple-agent | 增加任务认领、能力匹配、协作协议 |
| maple-engine | 增加协作节点类型（HumanReview、AgentHandoff） |
| server | 增加共创相关 API 端点 |

## 3. 数据模型

### 3.1 核心表结构

```sql
-- 共创工作空间
CREATE TABLE co_workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    owner_id TEXT NOT NULL,
    status TEXT DEFAULT 'active',  -- active | archived
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (owner_id) REFERENCES users(id)
);

-- 工作空间成员
CREATE TABLE co_members (
    workspace_id TEXT NOT NULL,
    member_id TEXT NOT NULL,
    member_type TEXT NOT NULL,  -- human | agent
    role TEXT DEFAULT 'member',  -- owner | member | viewer
    capabilities JSON,  -- Agent 能力描述
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, member_id),
    FOREIGN KEY (workspace_id) REFERENCES co_workspaces(id)
);

-- 共创任务
CREATE TABLE co_tasks (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    parent_task_id TEXT,  -- 父任务 ID，NULL 为根任务
    title TEXT NOT NULL,
    description TEXT,
    status TEXT DEFAULT 'pending',  -- pending | assigned | in_progress | review | done | blocked
    assigned_to TEXT,  -- 成员 ID
    priority INTEGER DEFAULT 0,
    dependencies JSON,  -- 依赖的任务 ID 列表
    result JSON,  -- 任务产出
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (workspace_id) REFERENCES co_workspaces(id),
    FOREIGN KEY (parent_task_id) REFERENCES co_tasks(id)
);

-- 任务评论
CREATE TABLE co_comments (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    author_id TEXT NOT NULL,
    author_type TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (task_id) REFERENCES co_tasks(id)
);

-- 操作审计日志
CREATE TABLE co_audit_log (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    actor_type TEXT NOT NULL,
    action TEXT NOT NULL,
    target_type TEXT,  -- task | document | member
    target_id TEXT,
    details JSON,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (workspace_id) REFERENCES co_workspaces(id)
);

-- CRDT 文档
CREATE TABLE crdt_documents (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    doc_type TEXT NOT NULL,  -- workflow | note | code
    title TEXT NOT NULL,
    automerge_doc BLOB,  -- Automerge 序列化数据
    version INTEGER DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (workspace_id) REFERENCES co_workspaces(id)
);

-- 成员在线状态
CREATE TABLE member_presence (
    workspace_id TEXT NOT NULL,
    member_id TEXT NOT NULL,
    status TEXT DEFAULT 'offline',  -- online | away | offline
    current_task_id TEXT,
    cursor_position JSON,  -- 协同编辑时的光标位置
    last_seen_at INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, member_id)
);
```

### 3.2 核心 Rust 类型

```rust
// core/maple-cocreation/src/types.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoWorkspace {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: String,
    pub status: WorkspaceStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkspaceStatus {
    Active,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub workspace_id: String,
    pub member_id: String,
    pub member_type: MemberType,
    pub role: MemberRole,
    pub capabilities: Option<serde_json::Value>,
    pub joined_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemberType {
    Human,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemberRole {
    Owner,
    Member,
    Viewer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoTask {
    pub id: String,
    pub workspace_id: String,
    pub parent_task_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub assigned_to: Option<String>,
    pub priority: i32,
    pub dependencies: Vec<String>,
    pub result: Option<serde_json::Value>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Assigned,
    InProgress,
    Review,
    Done,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceInfo {
    pub workspace_id: String,
    pub member_id: String,
    pub status: PresenceStatus,
    pub current_task_id: Option<String>,
    pub cursor_position: Option<CursorPosition>,
    pub last_seen_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PresenceStatus {
    Online,
    Away,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPosition {
    pub document_id: String,
    pub offset: usize,
    pub selection: Option<(usize, usize)>,
}
```

## 4. 核心服务设计

### 4.1 工作空间服务

```rust
// core/maple-cocreation/src/workspace.rs

use crate::types::*;
use anyhow::Result;

pub struct WorkspaceService {
    db: sqlx::SqlitePool,
}

impl WorkspaceService {
    pub fn new(db: sqlx::SqlitePool) -> Self {
        Self { db }
    }

    /// 创建共创工作空间
    pub async fn create_workspace(
        &self,
        name: &str,
        description: Option<&str>,
        owner_id: &str,
    ) -> Result<CoWorkspace> {
        // 1. 创建工作空间记录
        // 2. 将创建者添加为 Owner 成员
        // 3. 记录审计日志
        todo!()
    }

    /// 邀请成员加入工作空间
    pub async fn invite_member(
        &self,
        workspace_id: &str,
        member_id: &str,
        member_type: MemberType,
        role: MemberRole,
    ) -> Result<Member> {
        // 1. 验证调用者权限（必须是 Owner）
        // 2. 检查成员是否已在工作空间中
        // 3. 添加成员记录
        // 4. 通过 WebSocket 通知其他成员
        // 5. 记录审计日志
        todo!()
    }

    /// 获取工作空间成员列表
    pub async fn list_members(&self, workspace_id: &str) -> Result<Vec<Member>> {
        todo!()
    }

    /// 移除成员
    pub async fn remove_member(
        &self,
        workspace_id: &str,
        member_id: &str,
    ) -> Result<()> {
        // 1. 验证权限
        // 2. 不能移除 Owner
        // 3. 重新分配该成员的未完成任务
        // 4. 通知其他成员
        // 5. 记录审计日志
        todo!()
    }
}
```

### 4.2 智能任务分配服务

```rust
// core/maple-cocreation/src/task_allocator.rs

use crate::types::*;
use maple_agent::AgentRegistry;
use maple_llm::LlmRouter;

pub struct TaskAllocator {
    agent_registry: AgentRegistry,
    llm_router: LlmRouter,
}

impl TaskAllocator {
    pub fn new(agent_registry: AgentRegistry, llm_router: LlmRouter) -> Self {
        Self {
            agent_registry,
            llm_router,
        }
    }

    /// 基于能力匹配推荐任务分配
    pub async fn recommend_assignment(
        &self,
        workspace_id: &str,
        task: &CoTask,
    ) -> Result<Vec<AssignmentRecommendation>> {
        // 1. 获取工作空间所有可用成员
        // 2. 分析任务描述，提取所需能力
        // 3. 匹配成员能力与任务需求
        // 4. 考虑成员当前负载
        // 5. 返回排序后的推荐列表
        todo!()
    }

    /// Agent 自主认领任务
    pub async fn claim_task(
        &self,
        workspace_id: &str,
        task_id: &str,
        agent_id: &str,
    ) -> Result<bool> {
        // 1. 检查任务状态是否为 Pending
        // 2. 验证 Agent 能力是否匹配
        // 3. 检查 Agent 当前负载
        // 4. 更新任务分配
        // 5. 通知相关成员
        todo!()
    }

    /// 分解复杂任务
    pub async fn decompose_task(
        &self,
        task: &CoTask,
    ) -> Result<Vec<SubTaskProposal>> {
        // 使用 LLM 分析任务，生成子任务建议
        todo!()
    }
}

#[derive(Debug, Clone)]
pub struct AssignmentRecommendation {
    pub member_id: String,
    pub member_type: MemberType,
    pub score: f64,  // 匹配分数 0-1
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct SubTaskProposal {
    pub title: String,
    pub description: String,
    pub estimated_effort: String,
    pub suggested_assignee: Option<String>,
    pub dependencies: Vec<usize>,  // 依赖的子任务索引
}
```

### 4.3 实时感知服务

```rust
// core/maple-cocreation/src/presence.rs

use crate::types::*;
use tokio::sync::broadcast;

pub struct PresenceService {
    db: sqlx::SqlitePool,
    event_tx: broadcast::Sender<PresenceEvent>,
}

#[derive(Debug, Clone)]
pub enum PresenceEvent {
    MemberOnline { workspace_id: String, member_id: String },
    MemberOffline { workspace_id: String, member_id: String },
    CursorMoved { workspace_id: String, member_id: String, position: CursorPosition },
    TaskFocusChanged { workspace_id: String, member_id: String, task_id: Option<String> },
}

impl PresenceService {
    pub fn new(db: sqlx::SqlitePool) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self { db, event_tx }
    }

    /// 更新成员在线状态
    pub async fn update_presence(
        &self,
        workspace_id: &str,
        member_id: &str,
        status: PresenceStatus,
    ) -> Result<()> {
        // 1. 更新数据库记录
        // 2. 广播状态变更事件
        todo!()
    }

    /// 更新光标位置（协同编辑）
    pub async fn update_cursor(
        &self,
        workspace_id: &str,
        member_id: &str,
        position: CursorPosition,
    ) -> Result<()> {
        todo!()
    }

    /// 订阅工作空间的感知事件
    pub fn subscribe(&self) -> broadcast::Receiver<PresenceEvent> {
        self.event_tx.subscribe()
    }

    /// 获取工作空间所有成员的在线状态
    pub async fn get_workspace_presence(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<PresenceInfo>> {
        todo!()
    }
}
```

## 5. API 设计

### 5.1 REST API 端点

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/workspaces` | 创建共创工作空间 |
| GET | `/api/workspaces/:id` | 获取工作空间详情 |
| PUT | `/api/workspaces/:id` | 更新工作空间信息 |
| POST | `/api/workspaces/:id/members` | 邀请成员 |
| GET | `/api/workspaces/:id/members` | 获取成员列表 |
| DELETE | `/api/workspaces/:id/members/:member_id` | 移除成员 |
| POST | `/api/workspaces/:id/tasks` | 创建任务 |
| GET | `/api/workspaces/:id/tasks` | 获取任务列表（支持过滤） |
| GET | `/api/workspaces/:id/tasks/:task_id` | 获取任务详情 |
| PUT | `/api/workspaces/:id/tasks/:task_id` | 更新任务 |
| POST | `/api/workspaces/:id/tasks/:task_id/assign` | 分配任务 |
| POST | `/api/workspaces/:id/tasks/:task_id/claim` | Agent 认领任务 |
| POST | `/api/workspaces/:id/tasks/:task_id/comments` | 添加评论 |
| GET | `/api/workspaces/:id/tasks/:task_id/comments` | 获取评论列表 |
| POST | `/api/workspaces/:id/tasks/:task_id/decompose` | AI 分解任务 |
| GET | `/api/workspaces/:id/presence` | 获取成员在线状态 |
| GET | `/api/workspaces/:id/audit-log` | 获取审计日志 |

### 5.2 WebSocket 事件

```typescript
// 客户端订阅格式
interface WorkspaceSubscription {
  type: "subscribe";
  workspace_id: string;
  events: string[];  // ["task_update", "presence", "comment", "document"]
}

// 服务端推送事件
interface WorkspaceEvent {
  type: "task_created" | "task_updated" | "task_assigned" | 
        "member_online" | "member_offline" | "cursor_moved" |
        "comment_added" | "document_updated";
  workspace_id: string;
  payload: any;
  timestamp: number;
  actor_id: string;
}
```

## 6. 工作流集成

### 6.1 新增节点类型

在 maple-engine 中增加两个协作相关节点：

```rust
// HumanReview 节点：暂停工作流等待人类审批
pub struct HumanReviewNode {
    pub reviewers: Vec<String>,  // 需要审批的成员 ID
    pub min_approvals: usize,    // 最少审批人数
    pub timeout: Option<Duration>,
}

// AgentHandoff 节点：将控制权交给指定 Agent
pub struct AgentHandoffNode {
    pub target_agent: String,
    pub handoff_reason: String,
    pub context: serde_json::Value,
}
```

### 6.2 共创工作流示例

```yaml
# 共创内容生产工作流
name: collaborative-content-creation
nodes:
  - id: plan
    type: llm
    prompt: "分析需求并制定内容大纲"
    
  - id: assign_research
    type: condition
    condition: "task.requires_research == true"
    then: research
    else: draft
    
  - id: research
    type: agent_handoff
    target_agent: "research-agent"
    handoff_reason: "需要深度调研"
    
  - id: draft
    type: parallel
    branches:
      - id: section_a
        type: agent_handoff
        target_agent: "writer-agent-a"
      - id: section_b
        type: agent_handoff
        target_agent: "writer-agent-b"
        
  - id: review
    type: human_review
    reviewers: ["editor-1", "editor-2"]
    min_approvals: 1
    
  - id: publish
    type: tool
    tool: "content_publisher"
```

## 7. 实现计划

### Phase 1: 基础协作（4 周）

| 周次 | 任务 |
|------|------|
| W1 | 创建 maple-cocreation crate，实现数据模型和数据库迁移 |
| W2 | 实现 WorkspaceService 和成员管理 |
| W3 | 实现 CoTaskService 和任务 CRUD |
| W4 | 实现 REST API 端点和基础 WebSocket 通知 |

### Phase 2: 智能编排（3 周）

| 周次 | 任务 |
|------|------|
| W5 | 实现 TaskAllocator 能力匹配算法 |
| W6 | 实现 Agent 任务认领和 LLM 辅助分解 |
| W7 | 集成 maple-engine，添加 HumanReview 和 AgentHandoff 节点 |

### Phase 3: 实时协作（3 周）

| 周次 | 任务 |
|------|------|
| W8 | 修复 maple-sync 数据一致性 Bug |
| W9 | 集成 Automerge CRDT 实现文档协同编辑 |
| W10 | 实现 PresenceService 和光标同步 |

### Phase 4: 前端集成（2 周）

| 周次 | 任务 |
|------|------|
| W11 | 实现共创工作空间面板和成员管理 UI |
| W12 | 实现任务看板和协同编辑器集成 |

## 8. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| CRDT 同步复杂度高 | 高 | 先支持纯文本，渐进增加富媒体 |
| 并发安全问题 | 高 | 修复现有 Bug，增加并发测试 |
| Agent 能力匹配不准 | 中 | 收集反馈持续优化匹配算法 |
| WebSocket 扩展性 | 中 | 实现房间级别的事件分发 |

## 9. 依赖关系

```
maple-cocreation
├── maple-agent (Agent 注册、能力查询)
├── maple-llm (任务分析、能力匹配)
├── maple-engine (工作流集成)
├── maple-sync (CRDT 文档同步)
├── maple-gateway (WebSocket 通信)
└── sqlx (数据持久化)
```
