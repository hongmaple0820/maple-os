-- 021
CREATE INDEX IF NOT EXISTS idx_memories_type_created ON memories(memory_type, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_exec_events_id_type ON execution_events(execution_id, event_type, created_at);
CREATE INDEX IF NOT EXISTS idx_tool_inv_tool_status ON tool_invocations(tool_name, status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_lc_status_score ON learning_candidates(status, score DESC, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_we_workflow_status ON workflow_executions(workflow_id, status, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_path_created ON audit_logs(path, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_wt_workflow_enabled ON workflow_triggers(workflow_id, enabled);
CREATE INDEX IF NOT EXISTS idx_wv_workflow_version ON workflow_versions(workflow_id, version DESC);
