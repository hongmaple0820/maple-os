-- MapleOS v3 Migration 013: Tool Invocations
--
-- Purpose:
--   Persistent record of every tool call made by an agent. Complements
--   execution_events: the event stream carries the chronological narrative
--   (tool_call → tool_result), while this table carries the structured
--   per-invocation record (input, output, duration, permission, approval,
--   status) for audit and replay.
--
-- Design:
--   - execution_id links back to the unified execution fact chain
--   - approval_id links to approval_requests when the tool required approval
--   - permission_level is the safety tier (ReadOnly < WorkspaceWrite < Danger)
--   - status covers the full lifecycle including approval gating
--
-- Refs: docs/MapleOS_Implementation_Plan_2026Q3.md Track 1
--       docs/execution-fact-chain-spec.md
--       Issues #92, #89, #18 (audit), #69 (plugin governance)

CREATE TABLE IF NOT EXISTS tool_invocations (
    id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    -- What the agent asked the tool to do
    input TEXT,                      -- JSON; tool-specific schema
    -- What the tool returned
    output TEXT,                     -- JSON; tool-specific schema
    error TEXT,                      -- populated when status = 'failed'
    -- Permission / approval chain
    permission_level TEXT NOT NULL
        CHECK(permission_level IN ('read_only', 'workspace_write', 'prompt', 'allow', 'danger')),
    approval_id TEXT,                -- REFERENCES approval_requests(id) -- soft FK to avoid cycle
    -- Lifecycle
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN (
            'pending',               -- not yet started (e.g. waiting approval)
            'running',
            'approved',              -- approval granted, about to execute
            'rejected',              -- approval denied; will not execute
            'success',
            'failed',
            'cancelled',
            'timeout'
        )),
    -- Timing
    started_at INTEGER,
    completed_at INTEGER,
    duration_ms INTEGER,
    -- Origin (which agent / node invoked this tool)
    invoked_by TEXT,                 -- agent_id | node_id | "system"
    invoked_by_type TEXT
        CHECK(invoked_by_type IN ('agent', 'workflow_node', 'system') OR invoked_by_type IS NULL),
    -- Optional retry chain
    retry_of TEXT,                   -- REFERENCES tool_invocations(id) -- if this is a retry
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tool_inv_exec ON tool_invocations(execution_id, started_at);
CREATE INDEX IF NOT EXISTS idx_tool_inv_status ON tool_invocations(status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_tool_inv_tool ON tool_invocations(tool_name, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_tool_inv_approval ON tool_invocations(approval_id) WHERE approval_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tool_inv_retry ON tool_invocations(retry_of) WHERE retry_of IS NOT NULL;
