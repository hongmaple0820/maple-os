-- MapleOS v3 Migration 014: Execution Events (Unified Execution Fact Chain)
--
-- Purpose:
--   Single source of truth for everything that happens during an execution.
--   Chat / Workflow / Task / Approval / Audit / Activity all append to this
--   table; UI panels read from it via GET /api/executions/:id/events.
--
-- Design:
--   - execution_id is the universal correlation id; one per user-triggered
--     execution (chat send, workflow run, agent run, etc.)
--   - parent_execution_id links sub-executions (agent delegation, workflow
--     subflow) back to the parent for nested trace views
--   - source identifies which subsystem wrote the event so projections can
--     filter (chat panel sees chat + tool + approval, workflow panel sees
--     workflow + node + approval, etc.)
--   - event_type is a closed enum — see docs/execution-fact-chain-spec.md
--   - payload is JSON: shape varies by event_type, validated at the recorder
--     layer, not by the DB
--   - actor is who/what caused the event (user_id / agent_id / "system")
--
-- Refs: docs/MapleOS_Implementation_Plan_2026Q3.md Track 1
--       docs/execution-fact-chain-spec.md
--       Issue #92

CREATE TABLE IF NOT EXISTS execution_events (
    id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL,
    parent_execution_id TEXT,
    source TEXT NOT NULL
        CHECK(source IN ('chat', 'workflow', 'task', 'approval', 'agent', 'tool', 'scheduler', 'system')),
    event_type TEXT NOT NULL
        CHECK(event_type IN (
            'started',           -- execution began
            'delta',             -- incremental LLM token / message chunk
            'tool_call',         -- agent requested a tool invocation
            'tool_result',       -- tool returned (success or failure)
            'node_started',      -- workflow node began executing
            'node_finished',     -- workflow node completed (any status)
            'artifact',          -- artifact produced (KB write, file, etc.)
            'usage',             -- token / cost / timing accounting
            'approval_requested',
            'approval_decided',  -- approved | rejected | modified
            'retry',             -- node / tool retrying
            'cancelled',
            'resumed',           -- execution resumed after pause / approval
            'paused',            -- execution paused (waiting approval, etc.)
            'done',              -- execution finished successfully
            'error'              -- execution failed
        )),
    payload TEXT NOT NULL,           -- JSON; shape per event_type
    actor TEXT,                      -- user_id | agent_id | "system" | "scheduler"
    actor_type TEXT
        CHECK(actor_type IN ('human', 'agent', 'system') OR actor_type IS NULL),
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_exec_events_id ON execution_events(execution_id, created_at);
CREATE INDEX IF NOT EXISTS idx_exec_events_parent ON execution_events(parent_execution_id, created_at);
CREATE INDEX IF NOT EXISTS idx_exec_events_source ON execution_events(source, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_exec_events_type ON execution_events(event_type, created_at DESC);

-- ============================================================
-- Execution metadata (aggregate view, one row per execution_id)
-- ============================================================
CREATE TABLE IF NOT EXISTS executions (
    id TEXT PRIMARY KEY,             -- same as execution_id in events
    parent_execution_id TEXT,
    source TEXT NOT NULL,            -- chat | workflow | task | approval | agent
    status TEXT NOT NULL DEFAULT 'running'
        CHECK(status IN ('pending', 'running', 'paused', 'success', 'failed', 'cancelled')),
    actor TEXT,
    actor_type TEXT
        CHECK(actor_type IN ('human', 'agent', 'system') OR actor_type IS NULL),
    trigger_type TEXT,               -- manual | webhook | cron | event | message | api
    trigger_payload TEXT,            -- JSON
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    error TEXT,
    event_count INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_exec_status ON executions(status, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_exec_parent ON executions(parent_execution_id);
CREATE INDEX IF NOT EXISTS idx_exec_actor ON executions(actor, started_at DESC);
