-- 020
CREATE TABLE IF NOT EXISTS audit_logs (id INTEGER PRIMARY KEY AUTOINCREMENT, method TEXT NOT NULL, path TEXT NOT NULL, query TEXT, status INTEGER NOT NULL, duration_ms INTEGER NOT NULL, user_agent TEXT, client_ip TEXT, actor TEXT, created_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_logs(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_path ON audit_logs(path, created_at DESC);
