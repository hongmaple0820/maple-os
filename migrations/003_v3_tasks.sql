-- MapleOS v3 Migration 003: Task System (v3 schema)

-- ============================================================
-- 1. Projects (optional task container)
-- ============================================================
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES groups(id),
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK(status IN ('active', 'paused', 'completed', 'archived')),
    owner_id TEXT NOT NULL REFERENCES users_v3(id),
    start_date TEXT,
    end_date TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_projects_group ON projects(group_id, status);

-- ============================================================
-- 2. Tasks (v3 schema)
-- ============================================================
CREATE TABLE IF NOT EXISTS tasks_v3 (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES groups(id),
    project_id TEXT REFERENCES projects(id),
    parent_task_id TEXT REFERENCES tasks_v3(id),
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'todo'
        CHECK(status IN ('backlog', 'todo', 'in_progress', 'review', 'done', 'cancelled', 'blocked')),
    priority TEXT NOT NULL DEFAULT 'medium'
        CHECK(priority IN ('critical', 'high', 'medium', 'low')),
    assignee_id TEXT REFERENCES users_v3(id),
    assignee_type TEXT CHECK(assignee_type IN ('human', 'agent') OR assignee_type IS NULL),
    creator_id TEXT NOT NULL REFERENCES users_v3(id),
    source_message_id TEXT REFERENCES group_messages(id),
    due_at INTEGER,
    started_at INTEGER,
    completed_at INTEGER,
    estimated_minutes INTEGER,
    actual_minutes INTEGER,
    labels TEXT NOT NULL DEFAULT '[]',
    completion_message_id TEXT REFERENCES group_messages(id),
    subtask_count INTEGER NOT NULL DEFAULT 0,
    subtask_done_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_tasks_v3_group ON tasks_v3(group_id, status, created_at DESC) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_tasks_v3_assignee ON tasks_v3(assignee_id, status) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_tasks_v3_due ON tasks_v3(due_at) WHERE due_at IS NOT NULL AND deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_tasks_v3_parent ON tasks_v3(parent_task_id) WHERE parent_task_id IS NOT NULL;

-- ============================================================
-- 3. Task status history
-- ============================================================
CREATE TABLE IF NOT EXISTS task_status_history (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks_v3(id),
    old_status TEXT NOT NULL,
    new_status TEXT NOT NULL,
    changed_by TEXT NOT NULL REFERENCES users_v3(id),
    reason TEXT,
    changed_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tsh_task ON task_status_history(task_id, changed_at DESC);

-- ============================================================
-- 4. Task comments
-- ============================================================
CREATE TABLE IF NOT EXISTS task_comments_v3 (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks_v3(id),
    author_id TEXT NOT NULL REFERENCES users_v3(id),
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    edited_at INTEGER,
    deleted_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_tc_v3_task ON task_comments_v3(task_id, created_at DESC) WHERE deleted_at IS NULL;

-- ============================================================
-- 5. Task attachments
-- ============================================================
CREATE TABLE IF NOT EXISTS task_attachments (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks_v3(id),
    uploader_id TEXT NOT NULL REFERENCES users_v3(id),
    file_name TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    file_type TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ta_task ON task_attachments(task_id);
