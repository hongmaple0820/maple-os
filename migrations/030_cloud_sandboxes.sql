-- 030
CREATE TABLE IF NOT EXISTS cloud_sandboxes (id TEXT PRIMARY KEY, image TEXT NOT NULL, cpu_limit REAL NOT NULL DEFAULT 0.5, memory_mb INTEGER NOT NULL DEFAULT 512, network_policy TEXT NOT NULL DEFAULT 'none', fs_policy TEXT NOT NULL DEFAULT 'workspace_write', status TEXT NOT NULL DEFAULT 'pending', created_at INTEGER NOT NULL, hibernated_at INTEGER, destroyed_at INTEGER, env TEXT);
CREATE INDEX IF NOT EXISTS idx_sandbox_status ON cloud_sandboxes(status);
