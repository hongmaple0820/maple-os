-- MapleOS SQLite Schema Migration 001

-- Workflows
CREATE TABLE IF NOT EXISTS workflows (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    yaml_content TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Workflow execution records
CREATE TABLE IF NOT EXISTS workflow_executions (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL,
    workflow_version INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'running',
    context_snapshot TEXT,
    input TEXT,
    output TEXT,
    error TEXT,
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    agent_id TEXT
);

-- Checkpoints
CREATE TABLE IF NOT EXISTS checkpoints (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    exec_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    output TEXT NOT NULL,
    context_snapshot TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

-- Agent registry
CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    transport_type TEXT NOT NULL,
    transport_config TEXT NOT NULL,
    capabilities TEXT NOT NULL,
    triggers TEXT,
    status TEXT NOT NULL DEFAULT 'offline',
    last_heartbeat INTEGER,
    max_concurrent_tasks INTEGER NOT NULL DEFAULT 3,
    created_at INTEGER NOT NULL
);

-- Knowledge base documents
CREATE TABLE IF NOT EXISTS kb_documents (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    title TEXT NOT NULL,
    source_type TEXT NOT NULL,
    source_url TEXT,
    content TEXT NOT NULL,
    chunk_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

-- Knowledge base chunks (vector + BM25)
CREATE TABLE IF NOT EXISTS kb_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    content TEXT NOT NULL,
    embedding BLOB,
    term_freqs TEXT,
    created_at INTEGER NOT NULL
);

-- Prompt versions
CREATE TABLE IF NOT EXISTS prompt_versions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    prompt_ref TEXT NOT NULL,
    version INTEGER NOT NULL,
    content TEXT NOT NULL,
    change_reason TEXT,
    ab_test_result TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE(prompt_ref, version)
);

-- Collaboration messages
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    type TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    sender_type TEXT NOT NULL,
    content TEXT NOT NULL,
    metadata TEXT,
    created_at INTEGER NOT NULL
);

-- Chat messages
CREATE TABLE IF NOT EXISTS chat_messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    metadata TEXT,
    created_at INTEGER NOT NULL
);

-- Key-value store (CRDT sync)
CREATE TABLE IF NOT EXISTS kv_store (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Scheduled jobs
CREATE TABLE IF NOT EXISTS scheduled_jobs (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL,
    cron_expr TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    last_run_at INTEGER,
    next_run_at INTEGER NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1
);

-- Sync state
CREATE TABLE IF NOT EXISTS sync_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    last_sync_at INTEGER,
    local_version INTEGER NOT NULL DEFAULT 0,
    remote_version INTEGER,
    pending_changes INTEGER NOT NULL DEFAULT 0
);

-- Memories (working/episodic/semantic)
CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY,
    memory_type TEXT NOT NULL,
    content TEXT NOT NULL,
    metadata TEXT,
    created_at INTEGER NOT NULL,
    access_count INTEGER NOT NULL DEFAULT 0
);

-- Workspaces
CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    owner_id TEXT NOT NULL,
    max_agents INTEGER DEFAULT 10,
    auto_approve INTEGER DEFAULT 0,
    knowledge_base_enabled INTEGER DEFAULT 1,
    created_at INTEGER NOT NULL
);

-- Workspace members
CREATE TABLE IF NOT EXISTS workspace_members (
    workspace_id TEXT NOT NULL,
    member_id TEXT NOT NULL,
    name TEXT NOT NULL,
    member_type TEXT NOT NULL,
    role TEXT NOT NULL,
    PRIMARY KEY (workspace_id, member_id),
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id)
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_workflow_exec_workflow ON workflow_executions(workflow_id);
CREATE INDEX IF NOT EXISTS idx_checkpoints_exec ON checkpoints(exec_id);
CREATE INDEX IF NOT EXISTS idx_messages_workspace ON messages(workspace_id, created_at);
CREATE INDEX IF NOT EXISTS idx_messages_sender ON messages(sender_id);
CREATE INDEX IF NOT EXISTS idx_kb_chunks_document ON kb_chunks(document_id);
CREATE INDEX IF NOT EXISTS idx_kb_documents_workspace ON kb_documents(workspace_id);
CREATE INDEX IF NOT EXISTS idx_chat_messages_session ON chat_messages(session_id, created_at);
CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_next ON scheduled_jobs(next_run_at) WHERE enabled = 1;
CREATE INDEX IF NOT EXISTS idx_workspace_members_workspace ON workspace_members(workspace_id);
CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(memory_type);

-- Task queue
CREATE TABLE IF NOT EXISTS task_queue (
    id TEXT PRIMARY KEY,
    task_type TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    payload TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    next_run_at INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    error_message TEXT,
    agent_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_task_queue_status_priority ON task_queue(status, priority DESC, next_run_at);
CREATE INDEX IF NOT EXISTS idx_task_queue_type ON task_queue(task_type, status);

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    email TEXT,
    role TEXT NOT NULL DEFAULT 'user',
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS refresh_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id)
);

CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user ON refresh_tokens(user_id);
