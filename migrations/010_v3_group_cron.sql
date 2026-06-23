-- Group cron jobs
CREATE TABLE IF NOT EXISTS group_cron_jobs (
    id            TEXT PRIMARY KEY,
    group_id      TEXT NOT NULL,
    name          TEXT NOT NULL,
    cron_expr     TEXT NOT NULL,
    message_template TEXT NOT NULL,
    job_type      TEXT NOT NULL DEFAULT 'system_broadcast',
    target_agent_id TEXT,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_by    TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    last_run_at   INTEGER,
    next_run_at   INTEGER NOT NULL,
    run_count     INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (group_id) REFERENCES groups(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_group_cron_jobs_group_id ON group_cron_jobs(group_id);
CREATE INDEX IF NOT EXISTS idx_group_cron_jobs_enabled ON group_cron_jobs(enabled);
CREATE INDEX IF NOT EXISTS idx_group_cron_jobs_next_run ON group_cron_jobs(next_run_at);
