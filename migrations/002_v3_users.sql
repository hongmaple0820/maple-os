-- MapleOS v3 Migration 002: Unified User Model + Groups + Group Members + Group Messages

-- ============================================================
-- 1. Unified users table (human + agent)
-- ============================================================
CREATE TABLE IF NOT EXISTS users_v3 (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT,
    password_hash TEXT,
    avatar_url TEXT,
    user_type TEXT NOT NULL DEFAULT 'human' CHECK(user_type IN ('human', 'agent')),
    status TEXT NOT NULL DEFAULT 'offline'
        CHECK(status IN ('online', 'away', 'busy', 'offline', 'error')),
    platform_role TEXT NOT NULL DEFAULT 'user'
        CHECK(platform_role IN ('platform_admin', 'user', 'viewer')),

    -- Agent-specific fields (NULL for human users)
    soul_config TEXT,
    memory_config TEXT,
    agent_config TEXT,
    connection_type TEXT
        CHECK(connection_type IN ('llm-api', 'http-ws', 'sdk', 'a2a', 'rig') OR connection_type IS NULL),
    connection_config TEXT,
    llm_provider TEXT,
    llm_model TEXT,
    llm_api_key_encrypted TEXT,
    llm_base_url TEXT,
    agent_api_key TEXT,
    agent_api_secret_encrypted TEXT,

    -- rig Agent config
    rig_provider TEXT,
    rig_model TEXT,
    tools_config TEXT,
    skills_config TEXT,

    -- Health monitoring (agent-specific)
    last_heartbeat INTEGER,
    health_status TEXT DEFAULT 'unknown'
        CHECK(health_status IN ('healthy', 'degraded', 'unhealthy', 'unknown')),
    active_task_count INTEGER DEFAULT 0,

    -- Audit
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER
);

-- Migrate existing users
INSERT OR IGNORE INTO users_v3 (
    id, name, email, password_hash, user_type, status, platform_role,
    created_at, updated_at
)
SELECT
    id, username, email, password_hash, 'human', 'offline',
    CASE WHEN role = 'admin' THEN 'platform_admin' ELSE 'user' END,
    created_at, created_at
FROM users;

-- Migrate existing agents
INSERT OR IGNORE INTO users_v3 (
    id, name, user_type, status, platform_role,
    connection_type, connection_config,
    last_heartbeat, health_status,
    agent_config, tools_config,
    created_at, updated_at
)
SELECT
    id, name,
    'agent',
    CASE
        WHEN status = 'online' THEN 'online'
        WHEN status = 'busy' THEN 'busy'
        ELSE 'offline'
    END,
    'user',
    CASE transport_type
        WHEN 'websocket' THEN 'http-ws'
        WHEN 'webhook' THEN 'http-ws'
        WHEN 'mcp' THEN 'sdk'
        WHEN 'rest' THEN 'llm-api'
        WHEN 'sse' THEN 'llm-api'
        ELSE 'llm-api'
    END,
    transport_config,
    last_heartbeat,
    CASE
        WHEN status = 'online' THEN 'healthy'
        WHEN status = 'busy' THEN 'degraded'
        ELSE 'unknown'
    END,
    capabilities,
    capabilities,
    created_at, created_at
FROM agents;

-- Indexes
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_v3_email ON users_v3(email) WHERE email IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_users_v3_heartbeat ON users_v3(user_type, last_heartbeat) WHERE user_type = 'agent';
CREATE INDEX IF NOT EXISTS idx_users_v3_status ON users_v3(user_type, status);
CREATE INDEX IF NOT EXISTS idx_users_v3_type ON users_v3(user_type) WHERE deleted_at IS NULL;

-- ============================================================
-- 2. Groups table
-- ============================================================
CREATE TABLE IF NOT EXISTS groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    avatar_url TEXT,
    group_type TEXT NOT NULL DEFAULT 'collaboration'
        CHECK(group_type IN ('collaboration', 'project', 'channel', 'dm')),
    owner_id TEXT NOT NULL REFERENCES users_v3(id),
    settings TEXT NOT NULL DEFAULT '{}',
    dm_pair_key TEXT,
    dm_type TEXT CHECK(dm_type IN ('human_human', 'human_agent', 'agent_agent') OR dm_type IS NULL),
    member_count INTEGER NOT NULL DEFAULT 0,
    message_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    archived_at INTEGER,
    deleted_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_groups_owner ON groups(owner_id);
CREATE INDEX IF NOT EXISTS idx_groups_type ON groups(group_type) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_dm_pair ON groups(dm_pair_key) WHERE group_type = 'dm' AND deleted_at IS NULL;

-- Migrate existing workspaces to groups
INSERT OR IGNORE INTO groups (
    id, name, description, group_type, owner_id, settings,
    member_count, created_at, updated_at
)
SELECT
    id, name, description, 'collaboration', owner_id,
    json_object(
        'max_agents', max_agents,
        'auto_approve', auto_approve,
        'knowledge_base_enabled', knowledge_base_enabled
    ),
    0, created_at, created_at
FROM workspaces;

-- Update member counts
UPDATE groups SET member_count = (
    SELECT COUNT(*) FROM workspace_members WHERE workspace_members.workspace_id = groups.id
);

-- ============================================================
-- 3. Group members table
-- ============================================================
CREATE TABLE IF NOT EXISTS group_members (
    group_id TEXT NOT NULL REFERENCES groups(id),
    member_id TEXT NOT NULL REFERENCES users_v3(id),
    member_type TEXT NOT NULL CHECK(member_type IN ('human', 'agent')),
    role TEXT NOT NULL DEFAULT 'member'
        CHECK(role IN ('owner', 'admin', 'member', 'viewer')),
    nickname TEXT,
    can_approve INTEGER NOT NULL DEFAULT 0,
    approval_scope TEXT,
    joined_at INTEGER NOT NULL,
    last_active_at INTEGER,
    muted_until INTEGER,
    PRIMARY KEY (group_id, member_id)
);

CREATE INDEX IF NOT EXISTS idx_gm_member ON group_members(member_id);
CREATE INDEX IF NOT EXISTS idx_gm_role ON group_members(group_id, role);

-- Migrate existing workspace members
INSERT OR IGNORE INTO group_members (
    group_id, member_id, member_type, role, joined_at
)
SELECT
    workspace_id, member_id, member_type, role, strftime('%s', 'now') * 1000
FROM workspace_members;

-- ============================================================
-- 4. Group messages table
-- ============================================================
CREATE TABLE IF NOT EXISTS group_messages (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES groups(id),
    sender_id TEXT NOT NULL REFERENCES users_v3(id),
    sender_type TEXT NOT NULL CHECK(sender_type IN ('human', 'agent', 'system')),
    message_type TEXT NOT NULL CHECK(message_type IN (
        'text', 'markdown', 'image', 'file', 'voice',
        'tool_call', 'tool_result', 'thinking',
        'approval_request', 'approval_response',
        'workflow_run', 'workflow_step', 'workflow_complete', 'workflow_failed',
        'skill_call', 'skill_result',
        'task_created', 'task_updated', 'task_completed',
        'system', 'member_join', 'member_leave',
        'cron_trigger', 'external_message'
    )),
    content TEXT NOT NULL,
    reply_to_id TEXT REFERENCES group_messages(id),
    thread_root_id TEXT REFERENCES group_messages(id),
    thread_reply_count INTEGER NOT NULL DEFAULT 0,
    source_channel TEXT NOT NULL DEFAULT 'web'
        CHECK(source_channel IN (
            'web', 'api', 'sdk', 'cli', 'webhook',
            'im_feishu', 'im_wechat', 'im_dingtalk', 'im_telegram', 'im_slack'
        )),
    external_message_id TEXT,
    external_channel_id TEXT,
    pinned INTEGER NOT NULL DEFAULT 0,
    edited_at INTEGER,
    deleted_at INTEGER,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_gm_group_time ON group_messages(group_id, created_at DESC) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_gm_thread ON group_messages(thread_root_id, created_at) WHERE thread_root_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_gm_sender ON group_messages(sender_id, created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_gm_external_unique ON group_messages(source_channel, external_message_id) WHERE external_message_id IS NOT NULL;

-- FTS5 virtual table for full-text search
CREATE VIRTUAL TABLE IF NOT EXISTS group_messages_fts USING fts5(
    content, content='group_messages', content_rowid='rowid'
);

-- Migrate existing collaboration messages
INSERT OR IGNORE INTO group_messages (
    id, group_id, sender_id, sender_type, message_type, content,
    source_channel, created_at
)
SELECT
    id, workspace_id, sender_id, sender_type,
    CASE type
        WHEN 'text' THEN 'text'
        WHEN 'system' THEN 'system'
        ELSE 'text'
    END,
    CASE
        WHEN metadata IS NOT NULL THEN json_object('text', content, 'metadata', json(metadata))
        ELSE json_object('text', content)
    END,
    'web', created_at
FROM messages;

-- ============================================================
-- 5. Group unread counts (performance optimization)
-- ============================================================
CREATE TABLE IF NOT EXISTS group_unread_counts (
    group_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    unread_count INTEGER NOT NULL DEFAULT 0,
    last_read_message_id TEXT,
    last_read_at INTEGER,
    PRIMARY KEY (group_id, user_id)
);
