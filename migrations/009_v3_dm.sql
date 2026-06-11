-- DM (Direct Message) extensions for MapleOS v3
-- Extends groups table and adds A2A delegation + tool grants

-- DM type classification
ALTER TABLE groups ADD COLUMN dm_type TEXT
    CHECK(dm_type IN ('human_human', 'human_agent', 'agent_agent') OR dm_type IS NULL);

-- DM pair key for idempotent lookup (min_id:max_id)
ALTER TABLE groups ADD COLUMN dm_pair_key TEXT;

-- Unique index on DM pair key
CREATE UNIQUE INDEX IF NOT EXISTS idx_dm_pair ON groups(dm_pair_key)
    WHERE group_type = 'dm' AND deleted_at IS NULL;

-- A2A delegation tracking
CREATE TABLE IF NOT EXISTS a2a_delegations (
    id TEXT PRIMARY KEY,
    dm_group_id TEXT NOT NULL REFERENCES groups(id),
    delegator_id TEXT NOT NULL REFERENCES users_v3(id),
    executor_id TEXT NOT NULL REFERENCES users_v3(id),
    task_id TEXT,
    prompt TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending', 'running', 'completed', 'failed')),
    result TEXT,
    visible_to TEXT NOT NULL DEFAULT '[]',  -- JSON: user_ids who can observe
    created_at INTEGER NOT NULL,
    completed_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_a2a_dm ON a2a_delegations(dm_group_id, status);
CREATE INDEX IF NOT EXISTS idx_a2a_executor ON a2a_delegations(executor_id, status);

-- DM tool grants (human authorizes tools for Agent in DM context)
CREATE TABLE IF NOT EXISTS dm_tool_grants (
    id TEXT PRIMARY KEY,
    dm_group_id TEXT NOT NULL REFERENCES groups(id),
    tool_name TEXT NOT NULL,
    granted_by TEXT NOT NULL REFERENCES users_v3(id),
    granted_at INTEGER NOT NULL,
    expires_at INTEGER,
    scope TEXT,  -- optional: restrict scope, e.g. "path:/home/user/project"
    UNIQUE(dm_group_id, tool_name, granted_by)
);

CREATE INDEX IF NOT EXISTS idx_dtg_dm ON dm_tool_grants(dm_group_id);
