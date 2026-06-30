-- 026
CREATE TABLE IF NOT EXISTS agent_core_memory (agent_id TEXT NOT NULL, block_type TEXT NOT NULL CHECK(block_type IN ('persona', 'goals', 'pinned_facts', 'custom')), block_key TEXT NOT NULL, block_value TEXT NOT NULL, updated_at INTEGER NOT NULL, PRIMARY KEY (agent_id, block_type, block_key));
CREATE INDEX IF NOT EXISTS idx_cm_agent ON agent_core_memory(agent_id);
CREATE INDEX IF NOT EXISTS idx_cm_agent_type ON agent_core_memory(agent_id, block_type);
ALTER TABLE agent_memories ADD COLUMN importance_score REAL DEFAULT 0.5;
CREATE INDEX IF NOT EXISTS idx_am_importance ON agent_memories(importance_score DESC);
