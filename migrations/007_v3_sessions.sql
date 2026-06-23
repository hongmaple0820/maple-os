-- MapleOS v3 Migration 007: Sessions + Cron Jobs

-- ============================================================
-- 1. Sessions (agent conversations in groups)
-- ============================================================
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES groups(id),
    agent_id TEXT NOT NULL REFERENCES users_v3(id),
    session_type TEXT NOT NULL DEFAULT 'chat'
        CHECK(session_type IN ('chat', 'task', 'workflow', 'cron')),
    related_task_id TEXT REFERENCES tasks_v3(id),
    related_workflow_run_id TEXT REFERENCES workflow_runs(id),
    status TEXT NOT NULL DEFAULT 'active'
        CHECK(status IN ('active', 'paused', 'completed', 'archived')),
    conversation_history TEXT NOT NULL DEFAULT '[]',
    context TEXT NOT NULL DEFAULT '{}',
    message_count INTEGER NOT NULL DEFAULT 0,
    tool_call_count INTEGER NOT NULL DEFAULT 0,
    token_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    archived_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_sessions_group_agent ON sessions(group_id, agent_id, status);

-- ============================================================
-- 2. Cron jobs (linked to groups + agents)
-- ============================================================
CREATE TABLE IF NOT EXISTS cron_jobs (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES groups(id),
    agent_id TEXT NOT NULL REFERENCES users_v3(id),
    creator_id TEXT NOT NULL REFERENCES users_v3(id),
    name TEXT NOT NULL,
    description TEXT,
    cron_expr TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'Asia/Shanghai',
    prompt TEXT,
    workflow_id TEXT REFERENCES workflows(id),
    enabled INTEGER NOT NULL DEFAULT 1,
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

-- ============================================================
-- 3. Cron run logs
-- ============================================================
CREATE TABLE IF NOT EXISTS cron_run_logs (
    id TEXT PRIMARY KEY,
    cron_job_id TEXT NOT NULL REFERENCES cron_jobs(id),
    status TEXT NOT NULL CHECK(status IN ('success', 'failed', 'timeout')),
    triggered_at INTEGER NOT NULL,
    started_at INTEGER,
    completed_at INTEGER,
    duration_ms INTEGER,
    output TEXT,
    error TEXT,
    session_id TEXT REFERENCES sessions(id),
    trigger_message_id TEXT REFERENCES group_messages(id)
);

CREATE INDEX IF NOT EXISTS idx_crl_job ON cron_run_logs(cron_job_id, triggered_at DESC);

-- ============================================================
-- 4. Migrate existing scheduled jobs to cron_jobs
-- ============================================================
INSERT OR IGNORE INTO cron_jobs (
    id, group_id, agent_id, creator_id, name,
    cron_expr, timezone, workflow_id, enabled,
    last_run_at, next_run_at, created_at, updated_at
)
SELECT
    sj.id,
    COALESCE((SELECT id FROM groups LIMIT 1), 'default'),
    'system',
    'system',
    'Scheduled: ' || sj.workflow_id,
    sj.cron_expr,
    sj.timezone,
    sj.workflow_id,
    sj.enabled,
    sj.last_run_at,
    sj.next_run_at,
    sj.next_run_at,
    sj.next_run_at
FROM scheduled_jobs sj;
