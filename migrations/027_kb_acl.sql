-- 027
CREATE TABLE IF NOT EXISTS kb_documents_acl (document_id TEXT NOT NULL, principal_type TEXT NOT NULL CHECK(principal_type IN ('user', 'group', 'role')), principal_id TEXT NOT NULL, permission TEXT NOT NULL CHECK(permission IN ('read', 'write', 'admin')), created_at INTEGER NOT NULL, PRIMARY KEY (document_id, principal_type, principal_id));
CREATE INDEX IF NOT EXISTS idx_kb_acl_doc ON kb_documents_acl(document_id);
CREATE INDEX IF NOT EXISTS idx_kb_acl_principal ON kb_documents_acl(principal_type, principal_id);
CREATE TABLE IF NOT EXISTS kb_documents_verification (document_id TEXT PRIMARY KEY, verified_by TEXT NOT NULL, verified_at INTEGER NOT NULL, expires_at INTEGER, verification_notes TEXT);
ALTER TABLE kb_chunks ADD COLUMN acl_hash TEXT;
CREATE INDEX IF NOT EXISTS idx_kb_chunks_acl ON kb_chunks(acl_hash);
