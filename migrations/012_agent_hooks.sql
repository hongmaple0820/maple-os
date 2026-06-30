-- 012
CREATE TABLE IF NOT EXISTS agent_hooks (id TEXT PRIMARY KEY, group_id TEXT NOT NULL, agent_id TEXT NOT NULL, event_types TEXT NOT NULL DEFAULT '[]', condition_expr TEXT, action_type TEXT NOT NULL DEFAULT 'notify', action_config TEXT NOT NULL DEFAULT '{}', enabled INTEGER NOT NULL DEFAULT 1, priority INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS idx_ah_group ON agent_hooks(group_id);
CREATE INDEX IF NOT EXISTS idx_ah_agent ON agent_hooks(agent_id);
