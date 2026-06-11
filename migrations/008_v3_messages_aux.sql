-- MapleOS v3 Migration 008: Message Auxiliary Tables + Expanded Group Rules

-- ============================================================
-- 1. Message edit history
-- ============================================================
CREATE TABLE IF NOT EXISTS message_edit_history (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES group_messages(id),
    old_content TEXT NOT NULL,
    edited_by TEXT NOT NULL REFERENCES users_v3(id),
    edited_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_meh_message ON message_edit_history(message_id, edited_at DESC);

-- ============================================================
-- 2. Message read status
-- ============================================================
CREATE TABLE IF NOT EXISTS message_reads (
    message_id TEXT NOT NULL REFERENCES group_messages(id),
    user_id TEXT NOT NULL REFERENCES users_v3(id),
    read_at INTEGER NOT NULL,
    PRIMARY KEY (message_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_mr_user_group ON message_reads(user_id);

-- ============================================================
-- 3. Message reactions
-- ============================================================
CREATE TABLE IF NOT EXISTS message_reactions (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES group_messages(id),
    user_id TEXT NOT NULL REFERENCES users_v3(id),
    emoji TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(message_id, user_id, emoji)
);

-- ============================================================
-- 4. Message bookmarks
-- ============================================================
CREATE TABLE IF NOT EXISTS message_bookmarks (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES group_messages(id),
    user_id TEXT NOT NULL REFERENCES users_v3(id),
    note TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE(message_id, user_id)
);

-- ============================================================
-- 5. Pinned messages
-- ============================================================
CREATE TABLE IF NOT EXISTS pinned_messages (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES group_messages(id),
    group_id TEXT NOT NULL REFERENCES groups(id),
    pinned_by TEXT NOT NULL REFERENCES users_v3(id),
    pinned_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_pm_group ON pinned_messages(group_id, pinned_at DESC);

-- ============================================================
-- 6. Expanded group rules (v3 - 10 types)
-- ============================================================
CREATE TABLE IF NOT EXISTS group_rules_v3 (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES groups(id),
    name TEXT NOT NULL,
    description TEXT,
    rule_type TEXT NOT NULL CHECK(rule_type IN (
        'auto_assign', 'auto_approve', 'rate_limit', 'time_window',
        'tool_restriction', 'knowledge_scope', 'workflow_permission',
        'prompt_template', 'approval_policy'
    )),
    config TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 0,
    condition_expr TEXT,
    created_by TEXT NOT NULL REFERENCES users_v3(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_gr_v3_group ON group_rules_v3(group_id, enabled, priority DESC);
