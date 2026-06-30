-- MapleOS Database Performance Analysis
-- Run after migrations to verify index coverage and query plans.

-- 1. Check all tables and their row counts
SELECT 'memories' AS tbl, COUNT(*) AS rows FROM memories
UNION ALL SELECT 'execution_events', COUNT(*) FROM execution_events
UNION ALL SELECT 'executions', COUNT(*) FROM executions
UNION ALL SELECT 'tool_invocations', COUNT(*) FROM tool_invocations
UNION ALL SELECT 'learning_candidates', COUNT(*) FROM learning_candidates
UNION ALL SELECT 'audit_logs', COUNT(*) FROM audit_logs
UNION ALL SELECT 'workflow_triggers', COUNT(*) FROM workflow_triggers
UNION ALL SELECT 'workflow_versions', COUNT(*) FROM workflow_versions
UNION ALL SELECT 'workflows', COUNT(*) FROM workflows
UNION ALL SELECT 'workflow_executions', COUNT(*) FROM workflow_executions
UNION ALL SELECT 'agents', COUNT(*) FROM agents
UNION ALL SELECT 'group_messages', COUNT(*) FROM group_messages
UNION ALL SELECT 'tasks_v3', COUNT(*) FROM tasks_v3;

-- 2. Check index coverage (EXPLAIN QUERY PLAN for common queries)
EXPLAIN QUERY PLAN
SELECT * FROM execution_events WHERE execution_id = 'exec_test' ORDER BY created_at ASC;

EXPLAIN QUERY PLAN
SELECT * FROM tool_invocations WHERE execution_id = 'exec_test' ORDER BY created_at ASC;

EXPLAIN QUERY PLAN
SELECT * FROM learning_candidates WHERE status = 'pending' ORDER BY created_at DESC LIMIT 50;

EXPLAIN QUERY PLAN
SELECT * FROM audit_logs ORDER BY created_at DESC LIMIT 100;

EXPLAIN QUERY PLAN
SELECT * FROM workflow_executions WHERE workflow_id = 'wf1' AND status = 'running' ORDER BY started_at DESC;

-- 3. Check for tables without indexes on foreign-key-like columns
SELECT name, sql FROM sqlite_master WHERE type='table' AND sql NOT LIKE '%INDEX%';
