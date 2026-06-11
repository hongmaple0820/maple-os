-- MapleOS v3 Migration 005: Workflow Group Integration

-- ============================================================
-- 1. Workflow runs (linked to groups)
-- ============================================================
CREATE TABLE IF NOT EXISTS workflow_runs (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL REFERENCES workflows(id),
    group_id TEXT NOT NULL REFERENCES groups(id),
    trigger_type TEXT NOT NULL
        CHECK(trigger_type IN ('manual', 'webhook', 'cron', 'event', 'message')),
    trigger_payload TEXT,
    triggered_by TEXT REFERENCES users_v3(id),
    run_message_id TEXT REFERENCES group_messages(id),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending', 'running', 'paused', 'success', 'failed', 'cancelled', 'waiting_approval')),
    current_step_id TEXT,
    completed_steps INTEGER NOT NULL DEFAULT 0,
    total_steps INTEGER,
    context TEXT NOT NULL DEFAULT '{}',
    output TEXT,
    error TEXT,
    started_at INTEGER,
    completed_at INTEGER,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_wr_workflow ON workflow_runs(workflow_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_wr_group ON workflow_runs(group_id, status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_wr_status ON workflow_runs(status) WHERE status IN ('pending', 'running', 'waiting_approval');

-- ============================================================
-- 2. Workflow step executions
-- ============================================================
CREATE TABLE IF NOT EXISTS workflow_step_executions (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES workflow_runs(id),
    step_id TEXT NOT NULL,
    step_name TEXT NOT NULL,
    step_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending', 'running', 'success', 'failed', 'skipped', 'waiting_approval')),
    input TEXT,
    output TEXT,
    error TEXT,
    started_at INTEGER,
    completed_at INTEGER,
    duration_ms INTEGER,
    approval_id TEXT REFERENCES approval_requests(id)
);

CREATE INDEX IF NOT EXISTS idx_wse_run ON workflow_step_executions(run_id, step_id);

-- ============================================================
-- 3. Migrate existing workflow executions to workflow_runs
-- ============================================================
INSERT OR IGNORE INTO workflow_runs (
    id, workflow_id, group_id, trigger_type, status,
    output, error, started_at, completed_at, created_at
)
SELECT
    we.id, we.workflow_id,
    COALESCE((SELECT id FROM groups LIMIT 1), 'default'),
    'manual',
    CASE we.status
        WHEN 'running' THEN 'running'
        WHEN 'completed' THEN 'success'
        WHEN 'failed' THEN 'failed'
        WHEN 'cancelled' THEN 'cancelled'
        ELSE 'pending'
    END,
    we.output, we.error, we.started_at, we.completed_at, we.started_at
FROM workflow_executions we;
