-- MapleOS v3 Migration 016: Learning Governance (Track 3 / T3-6..T3-11)
--
-- Purpose:
--   Learning candidates are the audit-trail entries that the Evolver
--   produces BEFORE writing to KB / Memory / Prompt. They carry score,
--   evidence, source execution id, and a suggested target. A candidate
--   must be approved by a human (or auto-approved if score >= threshold)
--   before its content is persisted to long-term storage.
--
--   Rejected candidates go into learning_blocklist so the same content
--   is never re-proposed.
--
-- Refs: docs/MapleOS_Implementation_Plan_2026Q3.md Track 3
--       Issue #91

CREATE TABLE IF NOT EXISTS learning_candidates (
    id TEXT PRIMARY KEY,
    -- What kind of learning (memory | kb_doc | prompt)
    target_type TEXT NOT NULL
        CHECK(target_type IN ('memory', 'kb_doc', 'prompt')),
    -- For memory: 'episodic' | 'semantic' | 'working'
    -- For kb_doc: document title
    -- For prompt: prompt key
    target_key TEXT,
    -- The actual content to be persisted (takeaway, doc body, prompt text)
    content TEXT NOT NULL,
    -- Quality score 0.0-1.0 (normalized from Evolver's 0-10 scale)
    score REAL NOT NULL,
    -- LLM-generated evidence / reasoning for why this is worth learning
    evidence TEXT,
    -- execution_id from the unified fact chain (links back to the run
    -- that produced this candidate — see docs/execution-fact-chain-spec.md)
    source_execution_id TEXT,
    -- Secondary source descriptor (free-form JSON)
    source_metadata TEXT,
    -- Suggested target id (memory_id, kb_doc_id, prompt_id) — filled
    -- when the candidate is approved and persisted
    persisted_target_id TEXT,
    -- Lifecycle
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending', 'approved', 'rejected', 'auto_approved', 'revoked', 'persisted')),
    -- Who decided (user_id or 'system' for auto-approve)
    decided_by TEXT,
    decided_at INTEGER,
    -- Rejection reason (only when status = 'rejected')
    rejection_reason TEXT,
    -- Metadata for the approval flow
    approval_threshold REAL NOT NULL DEFAULT 0.7,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_lc_status ON learning_candidates(status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_lc_target ON learning_candidates(target_type, target_key);
CREATE INDEX IF NOT EXISTS idx_lc_source_exec ON learning_candidates(source_execution_id);
CREATE INDEX IF NOT EXISTS idx_lc_score ON learning_candidates(score DESC);

-- ============================================================
-- Blocklist: rejected candidates' content hashes so we don't re-propose
-- ============================================================
CREATE TABLE IF NOT EXISTS learning_blocklist (
    id TEXT PRIMARY KEY,
    -- SHA-256 of candidate content (lowercased, trimmed) — used for dedup
    content_hash TEXT NOT NULL UNIQUE,
    -- Reference to the rejected candidate that produced this blocklist entry
    source_candidate_id TEXT NOT NULL REFERENCES learning_candidates(id),
    -- Why it was blocked (rejection reason from the candidate)
    reason TEXT,
    blocked_at INTEGER NOT NULL,
    blocked_by TEXT
);

CREATE INDEX IF NOT EXISTS idx_lb_hash ON learning_blocklist(content_hash);
