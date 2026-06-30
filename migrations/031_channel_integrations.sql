-- 031
CREATE TABLE IF NOT EXISTS channel_integrations (id TEXT PRIMARY KEY, channel_type TEXT NOT NULL, name TEXT NOT NULL, config TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'disconnected', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS idx_channel_type ON channel_integrations(channel_type);
