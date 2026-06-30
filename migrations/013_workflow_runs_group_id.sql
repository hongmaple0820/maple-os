-- 013
ALTER TABLE workflow_executions ADD COLUMN group_id TEXT;
CREATE INDEX IF NOT EXISTS idx_we_group ON workflow_executions(group_id);
