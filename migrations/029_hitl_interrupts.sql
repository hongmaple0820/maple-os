-- 029
CREATE TABLE IF NOT EXISTS execution_interrupts (id TEXT PRIMARY KEY, execution_id TEXT NOT NULL, node_id TEXT, interrupt_type TEXT NOT NULL CHECK(interrupt_type IN ('approval', 'edit_state', 'choose_option', 'custom')), interrupt_payload TEXT NOT NULL, state_snapshot TEXT, created_at INTEGER NOT NULL, resolved_at INTEGER, resolution_type TEXT CHECK(resolution_type IN ('resume', 'resume_with_patch', 'cancel', 'branch')), resolution_payload TEXT);
CREATE INDEX IF NOT EXISTS idx_interrupt_exec ON execution_interrupts(execution_id);
CREATE TABLE IF NOT EXISTS execution_checkpoints (id TEXT PRIMARY KEY, execution_id TEXT NOT NULL, node_id TEXT NOT NULL, checkpoint_data TEXT NOT NULL, created_at INTEGER NOT NULL, parent_checkpoint_id TEXT, branch_of TEXT);
CREATE INDEX IF NOT EXISTS idx_cp_exec ON execution_checkpoints(execution_id, created_at);
CREATE INDEX IF NOT EXISTS idx_cp_parent ON execution_checkpoints(parent_checkpoint_id);
