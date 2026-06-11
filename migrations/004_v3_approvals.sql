-- MapleOS v3 Migration 004: Approval System

-- ============================================================
-- 1. Approval requests
-- ============================================================
CREATE TABLE IF NOT EXISTS approval_requests (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES groups(id),
    initiator_id TEXT NOT NULL REFERENCES users_v3(id),
    initiator_type TEXT NOT NULL CHECK(initiator_type IN ('human', 'agent')),
    request_message_id TEXT REFERENCES group_messages(id),
    action_type TEXT NOT NULL,
    action_description TEXT NOT NULL,
    action_payload TEXT NOT NULL,
    urgency TEXT NOT NULL DEFAULT 'normal'
        CHECK(urgency IN ('low', 'normal', 'high', 'critical')),
    approver_spec TEXT NOT NULL,
    quorum_type TEXT NOT NULL DEFAULT 'any'
        CHECK(quorum_type IN ('any', 'n_of', 'all', 'majority')),
    quorum_n INTEGER,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending', 'approved', 'rejected', 'expired', 'cancelled', 'modified')),
    expires_at INTEGER NOT NULL,
    final_payload TEXT,
    resolved_at INTEGER,
    resolution_comment TEXT,
    execution_status TEXT
        CHECK(execution_status IN ('pending_execution', 'executing', 'executed', 'execution_failed') OR execution_status IS NULL),
    execution_result TEXT,
    execution_error TEXT,
    workflow_run_id TEXT,
    task_id TEXT REFERENCES tasks_v3(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ar_group ON approval_requests(group_id, status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ar_initiator ON approval_requests(initiator_id);
CREATE INDEX IF NOT EXISTS idx_ar_expires ON approval_requests(expires_at, status) WHERE status = 'pending';

-- ============================================================
-- 2. Approval votes
-- ============================================================
CREATE TABLE IF NOT EXISTS approval_votes (
    id TEXT PRIMARY KEY,
    approval_id TEXT NOT NULL REFERENCES approval_requests(id),
    voter_id TEXT NOT NULL REFERENCES users_v3(id),
    decision TEXT NOT NULL CHECK(decision IN ('approved', 'rejected', 'modified')),
    modified_payload TEXT,
    comment TEXT,
    response_message_id TEXT REFERENCES group_messages(id),
    voted_at INTEGER NOT NULL,
    UNIQUE(approval_id, voter_id)
);

CREATE INDEX IF NOT EXISTS idx_av_approval ON approval_votes(approval_id, voted_at);

-- ============================================================
-- 3. Approval timeout logs
-- ============================================================
CREATE TABLE IF NOT EXISTS approval_timeout_logs (
    id TEXT PRIMARY KEY,
    approval_id TEXT NOT NULL REFERENCES approval_requests(id),
    timeout_action TEXT NOT NULL
        CHECK(timeout_action IN ('auto_reject', 'auto_approve', 'escalate', 'notify')),
    processed_at INTEGER NOT NULL,
    result TEXT
);
