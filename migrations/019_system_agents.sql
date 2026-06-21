-- MapleOS v3 Migration 019: System Agents (#24)
-- Seeds 4 built-in system agents that run as background processes:
-- scheduler, reviewer, monitor, evolver.

INSERT OR IGNORE INTO agents (id, name, status, description, tags) VALUES
('agent-scheduler', 'Scheduler', 'offline', 'Automatically receives tasks, decomposes them, and dispatches to specialized agents', '["system","scheduler"]'),
('agent-reviewer', 'Reviewer', 'offline', 'Reviews agent outputs and workflow results for quality and compliance', '["system","reviewer"]'),
('agent-monitor', 'Monitor', 'offline', 'Monitors system health, agent uptime, and task queue depth; alerts on anomalies', '["system","monitor"]'),
('agent-evolver', 'Evolver', 'offline', 'Extracts learnings from completed executions and proposes knowledge updates', '["system","evolver"]');
