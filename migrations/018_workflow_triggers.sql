-- 018
CREATE TABLE IF NOT EXISTS workflow_triggers (id TEXT PRIMARY KEY, workflow_id TEXT NOT NULL, trigger_config TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS idx_wt_workflow ON workflow_triggers(workflow_id);
