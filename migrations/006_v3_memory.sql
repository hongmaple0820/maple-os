-- MapleOS v3 Migration 006: Agent Memory System (3-layer)

-- ============================================================
-- 1. Agent memories (3-layer: working/episodic/semantic)
-- ============================================================
CREATE TABLE IF NOT EXISTS agent_memories (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES users_v3(id),
    memory_type TEXT NOT NULL
        CHECK(memory_type IN ('working', 'episodic', 'semantic')),
    content TEXT NOT NULL,
    summary TEXT,
    embedding BLOB,
    embedding_model TEXT,
    source_type TEXT
        CHECK(source_type IN ('chat', 'skill', 'workflow', 'task', 'manual', 'import') OR source_type IS NULL),
    source_id TEXT,
    group_id TEXT REFERENCES groups(id),
    relevance_score REAL NOT NULL DEFAULT 0.7,
    access_count INTEGER NOT NULL DEFAULT 0,
    last_accessed_at INTEGER,
    expires_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_am_agent_type ON agent_memories(agent_id, memory_type, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_am_agent_group ON agent_memories(agent_id, group_id) WHERE group_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_am_expires ON agent_memories(expires_at) WHERE expires_at IS NOT NULL AND memory_type = 'working';

-- FTS5 for agent memories
CREATE VIRTUAL TABLE IF NOT EXISTS agent_memories_fts USING fts5(
    content, content='agent_memories', content_rowid='rowid'
);

-- ============================================================
-- 2. Migrate existing memories
-- ============================================================
INSERT OR IGNORE INTO agent_memories (
    id, agent_id, memory_type, content,
    source_type, relevance_score, access_count,
    created_at, updated_at
)
SELECT
    m.id,
    COALESCE(json_extract(m.metadata, '$.agent_id'), 'system'),
    CASE m.memory_type
        WHEN 'working' THEN 'working'
        WHEN 'episodic' THEN 'episodic'
        WHEN 'semantic' THEN 'semantic'
        ELSE 'episodic'
    END,
    m.content,
    json_extract(m.metadata, '$.source_type'),
    0.7,
    m.access_count,
    m.created_at,
    m.created_at
FROM memories m;
