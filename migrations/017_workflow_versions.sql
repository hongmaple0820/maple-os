-- MapleOS v3 Migration 017: Workflow Version History (Track 2 / T2-2)
--
-- Purpose:
--   Stores every saved version of a workflow definition so users can
--   list past versions, diff them, and roll back to a previous version.
--   The `workflows` table keeps the current (active) version; this table
--   holds the full history.
--
-- Refs: docs/MapleOS_Implementation_Plan_2026Q3.md Track 2
--       Issue #90, #17

CREATE TABLE IF NOT EXISTS workflow_versions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workflow_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    yaml_content TEXT NOT NULL,
    -- Who saved this version (user_id or 'system')
    saved_by TEXT,
    -- Short auto-generated summary of what changed vs the previous version
    change_summary TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE(workflow_id, version)
);

CREATE INDEX IF NOT EXISTS idx_wv_workflow ON workflow_versions(workflow_id, version DESC);
CREATE INDEX IF NOT EXISTS idx_wv_created ON workflow_versions(created_at DESC);
