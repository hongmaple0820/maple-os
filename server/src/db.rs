pub async fn run_migrations(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS workflows (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            version INTEGER NOT NULL DEFAULT 1,
            yaml_content TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'draft',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS workflow_executions (
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
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS checkpoints (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            exec_id TEXT NOT NULL,
            node_id TEXT NOT NULL,
            output TEXT NOT NULL,
            context_snapshot TEXT NOT NULL,
            created_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS agents (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            transport_type TEXT NOT NULL,
            transport_config TEXT NOT NULL,
            capabilities TEXT NOT NULL,
            triggers TEXT,
            tags TEXT,
            status TEXT NOT NULL DEFAULT 'offline',
            last_heartbeat INTEGER,
            max_concurrent_tasks INTEGER NOT NULL DEFAULT 3,
            created_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    // Migrate: add new columns to existing agents table (safe if already exist)
    for stmt in [
        "ALTER TABLE agents ADD COLUMN description TEXT",
        "ALTER TABLE agents ADD COLUMN tags TEXT",
    ] {
        let _ = sqlx::query(stmt).execute(pool).await; // ignore "duplicate column" errors
    }

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS kb_documents (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            title TEXT NOT NULL,
            source_type TEXT NOT NULL,
            source_url TEXT,
            content TEXT NOT NULL,
            chunk_count INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS kb_chunks (
            id TEXT PRIMARY KEY,
            document_id TEXT NOT NULL,
            content TEXT NOT NULL,
            embedding BLOB,
            term_freqs TEXT,
            created_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            type TEXT NOT NULL,
            sender_id TEXT NOT NULL,
            sender_type TEXT NOT NULL,
            content TEXT NOT NULL,
            metadata TEXT,
            created_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS chat_messages (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            metadata TEXT,
            created_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS kv_store (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS prompt_versions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            prompt_ref TEXT NOT NULL,
            version INTEGER NOT NULL,
            content TEXT NOT NULL,
            change_reason TEXT,
            ab_test_result TEXT,
            created_at INTEGER NOT NULL,
            UNIQUE(prompt_ref, version)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS scheduled_jobs (
            id TEXT PRIMARY KEY,
            workflow_id TEXT NOT NULL,
            cron_expr TEXT NOT NULL,
            timezone TEXT NOT NULL DEFAULT 'UTC',
            last_run_at INTEGER,
            next_run_at INTEGER NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS sync_state (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            last_sync_at INTEGER,
            local_version INTEGER NOT NULL DEFAULT 0,
            remote_version INTEGER,
            pending_changes INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS memories (
            id TEXT PRIMARY KEY,
            memory_type TEXT NOT NULL,
            content TEXT NOT NULL,
            metadata TEXT,
            created_at INTEGER NOT NULL,
            access_count INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS workspaces (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            owner_id TEXT NOT NULL,
            max_agents INTEGER DEFAULT 10,
            auto_approve INTEGER DEFAULT 0,
            knowledge_base_enabled INTEGER DEFAULT 1,
            created_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS workspace_members (
            workspace_id TEXT NOT NULL,
            member_id TEXT NOT NULL,
            name TEXT NOT NULL,
            member_type TEXT NOT NULL,
            role TEXT NOT NULL,
            PRIMARY KEY (workspace_id, member_id),
            FOREIGN KEY (workspace_id) REFERENCES workspaces(id)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_workflow_exec_workflow ON workflow_executions(workflow_id)",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_checkpoints_exec ON checkpoints(exec_id)")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_messages_workspace ON messages(workspace_id, created_at)",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_kb_chunks_document ON kb_chunks(document_id)")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_kb_documents_workspace ON kb_documents(workspace_id)",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_chat_messages_session ON chat_messages(session_id, created_at)")
        .execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_next ON scheduled_jobs(next_run_at) WHERE enabled = 1")
        .execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_workspace_members_workspace ON workspace_members(workspace_id)")
        .execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(memory_type)")
        .execute(pool)
        .await?;

    // Collaboration: Kanban board tasks
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS board_tasks (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL DEFAULT 'default',
            title TEXT NOT NULL,
            description TEXT,
            status TEXT NOT NULL DEFAULT 'todo',
            priority TEXT NOT NULL DEFAULT 'medium',
            assignee_name TEXT,
            assignee_avatar TEXT,
            due_date TEXT,
            tags TEXT NOT NULL DEFAULT '[]',
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    // Collaboration: Comments on tasks
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS board_comments (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            parent_id TEXT,
            author_name TEXT NOT NULL,
            author_avatar TEXT,
            author_role TEXT,
            content TEXT NOT NULL,
            likes INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (task_id) REFERENCES board_tasks(id) ON DELETE CASCADE
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_board_tasks_workspace ON board_tasks(workspace_id, status)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_board_comments_task ON board_comments(task_id, created_at)",
    )
    .execute(pool)
    .await?;

    // Board attachments
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS board_attachments (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            comment_id TEXT,
            filename TEXT NOT NULL,
            content_type TEXT,
            size INTEGER NOT NULL DEFAULT 0,
            data BLOB NOT NULL,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (task_id) REFERENCES board_tasks(id) ON DELETE CASCADE
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_board_attachments_task ON board_attachments(task_id)",
    )
    .execute(pool)
    .await?;

    // Group rules persistence
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS group_rules (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            rule_type TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    // Activity feed
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS activity_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            workspace_id TEXT NOT NULL DEFAULT 'default',
            actor_name TEXT NOT NULL,
            action TEXT NOT NULL,
            target TEXT,
            details TEXT,
            created_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_activity_log_workspace ON activity_log(workspace_id, created_at DESC)")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS task_queue (
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
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_task_queue_status_priority
         ON task_queue(status, priority DESC, next_run_at)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_task_queue_type
         ON task_queue(task_type, status)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            email TEXT,
            role TEXT NOT NULL DEFAULT 'user',
            created_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS refresh_tokens (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            token_hash TEXT NOT NULL UNIQUE,
            expires_at INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(id)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_users_username ON users(username)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user ON refresh_tokens(user_id)")
        .execute(pool)
        .await?;

    // ================================================================
    // MapleOS v3 Migrations
    // ================================================================

    // --- 002: Unified users + groups + group_members + group_messages ---
    run_v3_migration_002(pool).await?;
    // --- 003: Tasks v3 ---
    run_v3_migration_003(pool).await?;
    // --- 004: Approvals ---
    run_v3_migration_004(pool).await?;
    // --- 005: Workflow runs ---
    run_v3_migration_005(pool).await?;
    // --- 006: Agent memories ---
    run_v3_migration_006(pool).await?;
    // --- 007: Sessions + cron jobs ---
    run_v3_migration_007(pool).await?;
    // --- 008: Message aux tables ---
    run_v3_migration_008(pool).await?;
    // --- 009: DM tables ---
    run_v3_migration_009(pool).await?;
    // --- 010: Group cron jobs ---
    run_v3_migration_010(pool).await?;
    // --- 011: Message attachments ---
    run_v3_migration_011(pool).await?;
    // --- 012: Agent hooks ---
    run_v3_migration_012(pool).await?;
    // --- 013: Workflow runs group_id ---
    run_v3_migration_013(pool).await?;
    // --- 014: Execution events (unified execution fact chain) ---
    run_v3_migration_014(pool).await?;
    // --- 015: Tool invocations (structured per-call audit record) ---
    run_v3_migration_015(pool).await?;
    // --- 016: Learning governance (candidates + blocklist) ---
    run_v3_migration_016(pool).await?;
    // --- 017: Workflow version history ---
    run_v3_migration_017(pool).await?;
    // --- 018: Workflow triggers (event + message) ---
    run_v3_migration_018(pool).await?;
    // --- 019: System agents (#24) ---
    run_v3_migration_019(pool).await?;
    // --- 020: Audit logs (#18) ---
    run_v3_migration_020(pool).await?;
    // --- 021: Performance composite indexes ---
    run_v3_migration_021(pool).await?;

    tracing::info!("Database migrations completed (including v3)");
    Ok(())
}

async fn run_v3_migration_002(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // Unified users table (human + agent)
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users_v3 (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT,
            password_hash TEXT,
            avatar_url TEXT,
            user_type TEXT NOT NULL DEFAULT 'human' CHECK(user_type IN ('human', 'agent')),
            status TEXT NOT NULL DEFAULT 'offline'
                CHECK(status IN ('online', 'away', 'busy', 'offline', 'error')),
            platform_role TEXT NOT NULL DEFAULT 'user'
                CHECK(platform_role IN ('platform_admin', 'user', 'viewer')),
            soul_config TEXT,
            memory_config TEXT,
            agent_config TEXT,
            connection_type TEXT
                CHECK(connection_type IN ('llm-api', 'http-ws', 'sdk', 'a2a', 'rig') OR connection_type IS NULL),
            connection_config TEXT,
            llm_provider TEXT,
            llm_model TEXT,
            llm_api_key_encrypted TEXT,
            llm_base_url TEXT,
            agent_api_key TEXT,
            agent_api_secret_encrypted TEXT,
            rig_provider TEXT,
            rig_model TEXT,
            tools_config TEXT,
            skills_config TEXT,
            last_heartbeat INTEGER,
            health_status TEXT DEFAULT 'unknown'
                CHECK(health_status IN ('healthy', 'degraded', 'unhealthy', 'unknown')),
            active_task_count INTEGER DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            deleted_at INTEGER
        )",
    )
    .execute(pool)
    .await?;

    // Migrate existing users -> users_v3
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO users_v3 (
            id, name, email, password_hash, user_type, status, platform_role,
            created_at, updated_at
        )
        SELECT
            id, username, email, password_hash, 'human', 'offline',
            CASE WHEN role = 'admin' THEN 'platform_admin' ELSE 'user' END,
            created_at, created_at
        FROM users",
    )
    .execute(pool)
    .await;

    // Migrate existing agents -> users_v3
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO users_v3 (
            id, name, user_type, status, platform_role,
            connection_type, connection_config,
            last_heartbeat, health_status,
            agent_config, tools_config,
            created_at, updated_at
        )
        SELECT
            id, name, 'agent',
            CASE WHEN status = 'online' THEN 'online' WHEN status = 'busy' THEN 'busy' ELSE 'offline' END,
            'user',
            CASE transport_type
                WHEN 'websocket' THEN 'http-ws' WHEN 'webhook' THEN 'http-ws'
                WHEN 'mcp' THEN 'sdk' WHEN 'rest' THEN 'llm-api'
                WHEN 'sse' THEN 'llm-api' ELSE 'llm-api'
            END,
            transport_config, last_heartbeat,
            CASE WHEN status = 'online' THEN 'healthy' WHEN status = 'busy' THEN 'degraded' ELSE 'unknown' END,
            capabilities, capabilities, created_at, created_at
        FROM agents",
    )
    .execute(pool)
    .await;

    for idx in [
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_users_v3_email ON users_v3(email) WHERE email IS NOT NULL",
        "CREATE INDEX IF NOT EXISTS idx_users_v3_heartbeat ON users_v3(user_type, last_heartbeat) WHERE user_type = 'agent'",
        "CREATE INDEX IF NOT EXISTS idx_users_v3_status ON users_v3(user_type, status)",
        "CREATE INDEX IF NOT EXISTS idx_users_v3_type ON users_v3(user_type) WHERE deleted_at IS NULL",
    ] {
        let _ = sqlx::query(idx).execute(pool).await;
    }

    // Groups table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS groups (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            avatar_url TEXT,
            group_type TEXT NOT NULL DEFAULT 'collaboration'
                CHECK(group_type IN ('collaboration', 'project', 'channel', 'dm')),
            owner_id TEXT NOT NULL,
            settings TEXT NOT NULL DEFAULT '{}',
            dm_pair_key TEXT,
            dm_type TEXT CHECK(dm_type IN ('human_human', 'human_agent', 'agent_agent') OR dm_type IS NULL),
            member_count INTEGER NOT NULL DEFAULT 0,
            message_count INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            archived_at INTEGER,
            deleted_at INTEGER
        )",
    )
    .execute(pool)
    .await?;

    let _ = sqlx::query(
        "INSERT OR IGNORE INTO groups (id, name, description, group_type, owner_id, settings, member_count, created_at, updated_at)
         SELECT id, name, description, 'collaboration', owner_id,
                json_object('max_agents', max_agents, 'auto_approve', auto_approve, 'knowledge_base_enabled', knowledge_base_enabled),
                0, created_at, created_at
         FROM workspaces",
    )
    .execute(pool)
    .await;

    let _ = sqlx::query(
        "UPDATE groups SET member_count = (SELECT COUNT(*) FROM workspace_members WHERE workspace_members.workspace_id = groups.id) WHERE member_count = 0",
    )
    .execute(pool)
    .await;

    for idx in [
        "CREATE INDEX IF NOT EXISTS idx_groups_owner ON groups(owner_id)",
        "CREATE INDEX IF NOT EXISTS idx_groups_type ON groups(group_type) WHERE deleted_at IS NULL",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_dm_pair ON groups(dm_pair_key) WHERE group_type = 'dm' AND deleted_at IS NULL",
    ] {
        let _ = sqlx::query(idx).execute(pool).await;
    }

    // Group members
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS group_members (
            group_id TEXT NOT NULL,
            member_id TEXT NOT NULL,
            member_type TEXT NOT NULL CHECK(member_type IN ('human', 'agent')),
            role TEXT NOT NULL DEFAULT 'member'
                CHECK(role IN ('owner', 'admin', 'member', 'viewer')),
            nickname TEXT,
            can_approve INTEGER NOT NULL DEFAULT 0,
            approval_scope TEXT,
            joined_at INTEGER NOT NULL,
            last_active_at INTEGER,
            muted_until INTEGER,
            PRIMARY KEY (group_id, member_id)
        )",
    )
    .execute(pool)
    .await?;

    let _ = sqlx::query(
        "INSERT OR IGNORE INTO group_members (group_id, member_id, member_type, role, joined_at)
         SELECT workspace_id, member_id, member_type, role, strftime('%s', 'now') * 1000
         FROM workspace_members",
    )
    .execute(pool)
    .await;

    for idx in [
        "CREATE INDEX IF NOT EXISTS idx_gm_member ON group_members(member_id)",
        "CREATE INDEX IF NOT EXISTS idx_gm_role ON group_members(group_id, role)",
    ] {
        let _ = sqlx::query(idx).execute(pool).await;
    }

    // Group messages
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS group_messages (
            id TEXT PRIMARY KEY,
            group_id TEXT NOT NULL,
            sender_id TEXT NOT NULL,
            sender_type TEXT NOT NULL CHECK(sender_type IN ('human', 'agent', 'system')),
            message_type TEXT NOT NULL CHECK(message_type IN (
                'text', 'markdown', 'image', 'file', 'voice',
                'tool_call', 'tool_result', 'thinking',
                'approval_request', 'approval_response',
                'workflow_run', 'workflow_step', 'workflow_complete', 'workflow_failed',
                'skill_call', 'skill_result',
                'task_created', 'task_updated', 'task_completed',
                'system', 'member_join', 'member_leave',
                'cron_trigger', 'external_message'
            )),
            content TEXT NOT NULL,
            reply_to_id TEXT,
            thread_root_id TEXT,
            thread_reply_count INTEGER NOT NULL DEFAULT 0,
            source_channel TEXT NOT NULL DEFAULT 'web'
                CHECK(source_channel IN (
                    'web', 'api', 'sdk', 'cli', 'webhook',
                    'im_feishu', 'im_wechat', 'im_dingtalk', 'im_telegram', 'im_slack'
                )),
            external_message_id TEXT,
            external_channel_id TEXT,
            pinned INTEGER NOT NULL DEFAULT 0,
            edited_at INTEGER,
            deleted_at INTEGER,
            created_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    let _ = sqlx::query(
        "INSERT OR IGNORE INTO group_messages (id, group_id, sender_id, sender_type, message_type, content, source_channel, created_at)
         SELECT id, workspace_id, sender_id, sender_type,
                CASE type WHEN 'text' THEN 'text' WHEN 'system' THEN 'system' ELSE 'text' END,
                CASE WHEN metadata IS NOT NULL THEN json_object('text', content, 'metadata', json(metadata)) ELSE json_object('text', content) END,
                'web', created_at
         FROM messages",
    )
    .execute(pool)
    .await;

    for idx in [
        "CREATE INDEX IF NOT EXISTS idx_gm_group_time ON group_messages(group_id, created_at DESC) WHERE deleted_at IS NULL",
        "CREATE INDEX IF NOT EXISTS idx_gm_thread ON group_messages(thread_root_id, created_at) WHERE thread_root_id IS NOT NULL",
        "CREATE INDEX IF NOT EXISTS idx_gm_sender ON group_messages(sender_id, created_at DESC)",
    ] {
        let _ = sqlx::query(idx).execute(pool).await;
    }

    // Group unread counts
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS group_unread_counts (
            group_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            unread_count INTEGER NOT NULL DEFAULT 0,
            last_read_message_id TEXT,
            last_read_at INTEGER,
            PRIMARY KEY (group_id, user_id)
        )",
    )
    .execute(pool)
    .await?;

    tracing::info!("v3 migration 002 (users+groups+messages) completed");
    Ok(())
}

async fn run_v3_migration_003(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // Projects
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY,
            group_id TEXT NOT NULL,
            name TEXT NOT NULL,
            description TEXT,
            status TEXT NOT NULL DEFAULT 'active'
                CHECK(status IN ('active', 'paused', 'completed', 'archived')),
            owner_id TEXT NOT NULL,
            start_date TEXT,
            end_date TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_projects_group ON projects(group_id, status)").execute(pool).await;

    // Tasks v3
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS tasks_v3 (
            id TEXT PRIMARY KEY,
            group_id TEXT NOT NULL,
            project_id TEXT,
            parent_task_id TEXT,
            title TEXT NOT NULL,
            description TEXT,
            status TEXT NOT NULL DEFAULT 'backlog'
                CHECK(status IN ('backlog', 'todo', 'in_progress', 'review', 'done', 'cancelled', 'blocked')),
            priority TEXT NOT NULL DEFAULT 'medium'
                CHECK(priority IN ('critical', 'high', 'medium', 'low', 'urgent')),
            assignee_id TEXT,
            assignee_type TEXT CHECK(assignee_type IN ('human', 'agent') OR assignee_type IS NULL),
            creator_id TEXT NOT NULL,
            source_message_id TEXT,
            due_date INTEGER,
            completed_at INTEGER,
            estimated_hours REAL,
            actual_hours REAL,
            tags TEXT NOT NULL DEFAULT '[]',
            metadata TEXT NOT NULL DEFAULT '{}',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            deleted_at INTEGER
        )",
    )
    .execute(pool)
    .await?;

    for idx in [
        "CREATE INDEX IF NOT EXISTS idx_tasks_v3_group ON tasks_v3(group_id, status, created_at DESC) WHERE deleted_at IS NULL",
        "CREATE INDEX IF NOT EXISTS idx_tasks_v3_assignee ON tasks_v3(assignee_id, status) WHERE deleted_at IS NULL",
        "CREATE INDEX IF NOT EXISTS idx_tasks_v3_due ON tasks_v3(due_at) WHERE due_at IS NOT NULL AND deleted_at IS NULL",
        "CREATE INDEX IF NOT EXISTS idx_tasks_v3_parent ON tasks_v3(parent_task_id) WHERE parent_task_id IS NOT NULL",
    ] {
        let _ = sqlx::query(idx).execute(pool).await;
    }

    // Task status history
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS task_status_history (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            old_status TEXT NOT NULL,
            new_status TEXT NOT NULL,
            changed_by TEXT NOT NULL,
            reason TEXT,
            changed_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_tsh_task ON task_status_history(task_id, changed_at DESC)").execute(pool).await;

    // Task comments v3
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS task_comments_v3 (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            author_id TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            edited_at INTEGER,
            deleted_at INTEGER
        )",
    )
    .execute(pool)
    .await?;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_tc_v3_task ON task_comments_v3(task_id, created_at DESC) WHERE deleted_at IS NULL").execute(pool).await;

    // Task attachments
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS task_attachments (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            uploader_id TEXT NOT NULL,
            file_name TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            file_type TEXT NOT NULL,
            storage_path TEXT NOT NULL,
            created_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_ta_task ON task_attachments(task_id)").execute(pool).await;

    tracing::info!("v3 migration 003 (tasks) completed");
    Ok(())
}

async fn run_v3_migration_004(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // Approval requests
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS approval_requests (
            id TEXT PRIMARY KEY,
            group_id TEXT NOT NULL,
            title TEXT NOT NULL,
            description TEXT,
            request_type TEXT NOT NULL DEFAULT 'general',
            requester_id TEXT NOT NULL,
            urgency TEXT NOT NULL DEFAULT 'normal'
                CHECK(urgency IN ('low', 'normal', 'high', 'critical')),
            quorum_type TEXT NOT NULL DEFAULT 'any'
                CHECK(quorum_type IN ('any', 'all', 'majority')),
            required_count INTEGER NOT NULL DEFAULT 1,
            approver_spec TEXT NOT NULL,
            context TEXT,
            execution_status TEXT NOT NULL DEFAULT 'pending',
            timeout_at INTEGER,
            auto_action TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            resolved_at INTEGER
        )",
    )
    .execute(pool)
    .await?;

    for idx in [
        "CREATE INDEX IF NOT EXISTS idx_ar_group ON approval_requests(group_id, execution_status, created_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_ar_requester ON approval_requests(requester_id)",
    ] {
        let _ = sqlx::query(idx).execute(pool).await;
    }

    // Approval votes
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS approval_votes (
            id TEXT PRIMARY KEY,
            approval_id TEXT NOT NULL,
            voter_id TEXT NOT NULL,
            decision TEXT NOT NULL CHECK(decision IN ('approve', 'reject', 'abstain')),
            comment TEXT,
            voted_at INTEGER NOT NULL,
            UNIQUE(approval_id, voter_id)
        )",
    )
    .execute(pool)
    .await?;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_av_approval ON approval_votes(approval_id, voted_at)").execute(pool).await;

    // Approval timeout logs
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS approval_timeout_logs (
            id TEXT PRIMARY KEY,
            approval_id TEXT NOT NULL,
            timeout_action TEXT NOT NULL
                CHECK(timeout_action IN ('auto_reject', 'auto_approve', 'escalate', 'notify')),
            processed_at INTEGER NOT NULL,
            result TEXT
        )",
    )
    .execute(pool)
    .await?;

    tracing::info!("v3 migration 004 (approvals) completed");
    Ok(())
}

async fn run_v3_migration_005(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // Workflow runs
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS workflow_runs (
            id TEXT PRIMARY KEY,
            workflow_id TEXT NOT NULL,
            group_id TEXT NOT NULL,
            trigger_type TEXT NOT NULL
                CHECK(trigger_type IN ('manual', 'webhook', 'cron', 'event', 'message')),
            trigger_payload TEXT,
            triggered_by TEXT,
            run_message_id TEXT,
            status TEXT NOT NULL DEFAULT 'pending'
                CHECK(status IN ('pending', 'running', 'paused', 'success', 'failed', 'cancelled', 'waiting_approval')),
            current_step_id TEXT,
            completed_steps INTEGER NOT NULL DEFAULT 0,
            total_steps INTEGER,
            context TEXT NOT NULL DEFAULT '{}',
            output TEXT,
            error TEXT,
            started_at INTEGER,
            completed_at INTEGER,
            created_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    for idx in [
        "CREATE INDEX IF NOT EXISTS idx_wr_workflow ON workflow_runs(workflow_id, created_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_wr_group ON workflow_runs(group_id, status, created_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_wr_status ON workflow_runs(status) WHERE status IN ('pending', 'running', 'waiting_approval')",
    ] {
        let _ = sqlx::query(idx).execute(pool).await;
    }

    // Workflow step executions
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS workflow_step_executions (
            id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            step_id TEXT NOT NULL,
            step_name TEXT NOT NULL,
            step_type TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending'
                CHECK(status IN ('pending', 'running', 'success', 'failed', 'skipped', 'waiting_approval')),
            input TEXT,
            output TEXT,
            error TEXT,
            started_at INTEGER,
            completed_at INTEGER,
            duration_ms INTEGER,
            approval_id TEXT
        )",
    )
    .execute(pool)
    .await?;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_wse_run ON workflow_step_executions(run_id, step_id)").execute(pool).await;

    // Migrate existing workflow executions
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO workflow_runs (id, workflow_id, group_id, trigger_type, status, output, error, started_at, completed_at, created_at)
         SELECT we.id, we.workflow_id, COALESCE((SELECT id FROM groups LIMIT 1), 'default'), 'manual',
                CASE we.status WHEN 'running' THEN 'running' WHEN 'completed' THEN 'success' WHEN 'failed' THEN 'failed' WHEN 'cancelled' THEN 'cancelled' ELSE 'pending' END,
                we.output, we.error, we.started_at, we.completed_at, we.started_at
         FROM workflow_executions we",
    )
    .execute(pool)
    .await;

    tracing::info!("v3 migration 005 (workflow runs) completed");
    Ok(())
}

async fn run_v3_migration_006(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // Agent memories (3-layer)
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS agent_memories (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            memory_type TEXT NOT NULL CHECK(memory_type IN ('working', 'episodic', 'semantic')),
            content TEXT NOT NULL,
            summary TEXT,
            embedding BLOB,
            embedding_model TEXT,
            source_type TEXT CHECK(source_type IN ('chat', 'skill', 'workflow', 'task', 'manual', 'import') OR source_type IS NULL),
            source_id TEXT,
            group_id TEXT,
            relevance_score REAL NOT NULL DEFAULT 0.7,
            access_count INTEGER NOT NULL DEFAULT 0,
            last_accessed_at INTEGER,
            expires_at INTEGER,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    for idx in [
        "CREATE INDEX IF NOT EXISTS idx_am_agent_type ON agent_memories(agent_id, memory_type, created_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_am_agent_group ON agent_memories(agent_id, group_id) WHERE group_id IS NOT NULL",
        "CREATE INDEX IF NOT EXISTS idx_am_expires ON agent_memories(expires_at) WHERE expires_at IS NOT NULL AND memory_type = 'working'",
    ] {
        let _ = sqlx::query(idx).execute(pool).await;
    }

    // FTS5 virtual table for full-text search on memory content
    let _ = sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS agent_memories_fts USING fts5(content, content='agent_memories', content_rowid='rowid')"
    ).execute(pool).await;

    // Triggers to keep FTS in sync
    let _ = sqlx::query(
        "CREATE TRIGGER IF NOT EXISTS am_ai AFTER INSERT ON agent_memories BEGIN
            INSERT INTO agent_memories_fts(rowid, content) VALUES (new.rowid, new.content);
        END"
    ).execute(pool).await;
    let _ = sqlx::query(
        "CREATE TRIGGER IF NOT EXISTS am_ad AFTER DELETE ON agent_memories BEGIN
            INSERT INTO agent_memories_fts(agent_memories_fts, rowid, content) VALUES('delete', old.rowid, old.content);
        END"
    ).execute(pool).await;
    let _ = sqlx::query(
        "CREATE TRIGGER IF NOT EXISTS am_au AFTER UPDATE ON agent_memories BEGIN
            INSERT INTO agent_memories_fts(agent_memories_fts, rowid, content) VALUES('delete', old.rowid, old.content);
            INSERT INTO agent_memories_fts(rowid, content) VALUES (new.rowid, new.content);
        END"
    ).execute(pool).await;

    // Migrate existing memories
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO agent_memories (id, agent_id, memory_type, content, source_type, relevance_score, access_count, created_at, updated_at)
         SELECT m.id, COALESCE(json_extract(m.metadata, '$.agent_id'), 'system'),
                CASE m.memory_type WHEN 'working' THEN 'working' WHEN 'episodic' THEN 'episodic' WHEN 'semantic' THEN 'semantic' ELSE 'episodic' END,
                m.content, json_extract(m.metadata, '$.source_type'), 0.7, m.access_count, m.created_at, m.created_at
         FROM memories m",
    )
    .execute(pool)
    .await;

    tracing::info!("v3 migration 006 (agent memories) completed");
    Ok(())
}

async fn run_v3_migration_007(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // Sessions
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            group_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            session_type TEXT NOT NULL DEFAULT 'chat'
                CHECK(session_type IN ('chat', 'task', 'workflow', 'cron')),
            related_task_id TEXT,
            related_workflow_run_id TEXT,
            status TEXT NOT NULL DEFAULT 'active'
                CHECK(status IN ('active', 'paused', 'completed', 'archived')),
            conversation_history TEXT NOT NULL DEFAULT '[]',
            context TEXT NOT NULL DEFAULT '{}',
            message_count INTEGER NOT NULL DEFAULT 0,
            tool_call_count INTEGER NOT NULL DEFAULT 0,
            token_count INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            archived_at INTEGER
        )",
    )
    .execute(pool)
    .await?;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_group_agent ON sessions(group_id, agent_id, status)").execute(pool).await;

    // Cron jobs
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS cron_jobs (
            id TEXT PRIMARY KEY,
            group_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            creator_id TEXT NOT NULL,
            name TEXT NOT NULL,
            description TEXT,
            cron_expr TEXT NOT NULL,
            timezone TEXT NOT NULL DEFAULT 'Asia/Shanghai',
            prompt TEXT,
            workflow_id TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            last_run_at INTEGER,
            last_run_status TEXT CHECK(last_run_status IN ('success', 'failed', 'running') OR last_run_status IS NULL),
            next_run_at INTEGER,
            run_count INTEGER NOT NULL DEFAULT 0,
            success_count INTEGER NOT NULL DEFAULT 0,
            failure_count INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    // Cron run logs
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS cron_run_logs (
            id TEXT PRIMARY KEY,
            cron_job_id TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('success', 'failed', 'timeout')),
            triggered_at INTEGER NOT NULL,
            started_at INTEGER,
            completed_at INTEGER,
            duration_ms INTEGER,
            output TEXT,
            error TEXT,
            session_id TEXT,
            trigger_message_id TEXT
        )",
    )
    .execute(pool)
    .await?;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_crl_job ON cron_run_logs(cron_job_id, triggered_at DESC)").execute(pool).await;

    tracing::info!("v3 migration 007 (sessions+cron) completed");
    Ok(())
}

async fn run_v3_migration_008(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // Message edit history
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS message_edit_history (
            id TEXT PRIMARY KEY,
            message_id TEXT NOT NULL,
            old_content TEXT NOT NULL,
            edited_by TEXT NOT NULL,
            edited_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_meh_message ON message_edit_history(message_id, edited_at DESC)").execute(pool).await;

    // Message reads
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS message_reads (
            message_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            read_at INTEGER NOT NULL,
            PRIMARY KEY (message_id, user_id)
        )",
    )
    .execute(pool)
    .await?;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_mr_user_group ON message_reads(user_id)").execute(pool).await;

    // Message reactions
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS message_reactions (
            id TEXT PRIMARY KEY,
            message_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            emoji TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            UNIQUE(message_id, user_id, emoji)
        )",
    )
    .execute(pool)
    .await?;

    // Message bookmarks
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS message_bookmarks (
            id TEXT PRIMARY KEY,
            message_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            note TEXT,
            created_at INTEGER NOT NULL,
            UNIQUE(message_id, user_id)
        )",
    )
    .execute(pool)
    .await?;

    // Pinned messages
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS pinned_messages (
            id TEXT PRIMARY KEY,
            message_id TEXT NOT NULL,
            group_id TEXT NOT NULL,
            pinned_by TEXT NOT NULL,
            pinned_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_pm_group ON pinned_messages(group_id, pinned_at DESC)").execute(pool).await;

    // Expanded group rules v3
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS group_rules_v3 (
            id TEXT PRIMARY KEY,
            group_id TEXT NOT NULL,
            name TEXT NOT NULL,
            description TEXT,
            rule_type TEXT NOT NULL CHECK(rule_type IN (
                'auto_assign', 'auto_approve', 'rate_limit', 'time_window',
                'tool_restriction', 'knowledge_scope', 'workflow_permission',
                'prompt_template', 'approval_policy'
            )),
            config TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            priority INTEGER NOT NULL DEFAULT 0,
            condition_expr TEXT,
            created_by TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_gr_v3_group ON group_rules_v3(group_id, enabled, priority DESC)").execute(pool).await;

    tracing::info!("v3 migration 008 (message aux + group rules) completed");
    Ok(())
}

async fn run_v3_migration_009(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // DM columns on groups table
    for stmt in [
        "ALTER TABLE groups ADD COLUMN dm_type TEXT",
        "ALTER TABLE groups ADD COLUMN dm_pair_key TEXT",
    ] {
        let _ = sqlx::query(stmt).execute(pool).await;
    }

    // A2A delegations
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS a2a_delegations (
            id TEXT PRIMARY KEY,
            dm_group_id TEXT NOT NULL,
            delegator_id TEXT NOT NULL,
            executor_id TEXT NOT NULL,
            task_id TEXT,
            prompt TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            result TEXT,
            visible_to TEXT NOT NULL DEFAULT 'both',
            created_at INTEGER NOT NULL,
            completed_at INTEGER
        )",
    ).execute(pool).await?;

    // DM tool grants
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS dm_tool_grants (
            id TEXT PRIMARY KEY,
            dm_group_id TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            granted_by TEXT NOT NULL,
            granted_at INTEGER NOT NULL,
            expires_at INTEGER,
            scope TEXT
        )",
    ).execute(pool).await?;

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_a2a_dm ON a2a_delegations(dm_group_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_dg_dm ON dm_tool_grants(dm_group_id)").execute(pool).await;

    tracing::info!("v3 migration 009 (DM) completed");
    Ok(())
}

async fn run_v3_migration_010(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS group_cron_jobs (
            id TEXT PRIMARY KEY,
            group_id TEXT NOT NULL,
            name TEXT NOT NULL,
            cron_expr TEXT NOT NULL,
            message_template TEXT NOT NULL,
            job_type TEXT NOT NULL DEFAULT 'system_broadcast',
            target_agent_id TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_by TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            last_run_at INTEGER,
            next_run_at INTEGER NOT NULL,
            run_count INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (group_id) REFERENCES groups(id) ON DELETE CASCADE
        )",
    ).execute(pool).await?;

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_gcj_group ON group_cron_jobs(group_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_gcj_next ON group_cron_jobs(next_run_at)").execute(pool).await;

    tracing::info!("v3 migration 010 (group cron) completed");
    Ok(())
}

async fn run_v3_migration_011(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS message_attachments (
            id TEXT PRIMARY KEY,
            group_id TEXT NOT NULL,
            message_id TEXT,
            uploader_id TEXT NOT NULL,
            filename TEXT NOT NULL,
            content_type TEXT NOT NULL,
            size INTEGER NOT NULL,
            data BLOB NOT NULL,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (group_id) REFERENCES groups(id) ON DELETE CASCADE
        )",
    ).execute(pool).await?;

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_ma_group ON message_attachments(group_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_ma_message ON message_attachments(message_id)").execute(pool).await;

    tracing::info!("v3 migration 011 (message attachments) completed");
    Ok(())
}

async fn run_v3_migration_012(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS agent_hooks (
            id TEXT PRIMARY KEY,
            group_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            event_types TEXT NOT NULL DEFAULT '[]',
            condition_expr TEXT,
            action_type TEXT NOT NULL DEFAULT 'notify',
            action_config TEXT NOT NULL DEFAULT '{}',
            enabled INTEGER NOT NULL DEFAULT 1,
            priority INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS agent_hook_logs (
            id TEXT PRIMARY KEY,
            hook_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            event_data TEXT,
            status TEXT NOT NULL DEFAULT 'success',
            result TEXT,
            error TEXT,
            executed_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_ah_group ON agent_hooks(group_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_ah_agent ON agent_hooks(agent_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_ahl_hook ON agent_hook_logs(hook_id)").execute(pool).await;

    tracing::info!("v3 migration 012 (agent hooks) completed");
    Ok(())
}

async fn run_v3_migration_013(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // Add group_id column to workflow_executions (safe if already exists)
    let _ = sqlx::query("ALTER TABLE workflow_executions ADD COLUMN group_id TEXT")
        .execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_we_group ON workflow_executions(group_id)")
        .execute(pool).await;

    tracing::info!("v3 migration 013 (workflow runs group_id) completed");
    Ok(())
}

async fn run_v3_migration_014(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // ============================================================
    // Execution events — unified execution fact chain.
    // See docs/execution-fact-chain-spec.md and migrations/014_execution_events.sql
    // ============================================================
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS execution_events (
            id TEXT PRIMARY KEY,
            execution_id TEXT NOT NULL,
            parent_execution_id TEXT,
            source TEXT NOT NULL
                CHECK(source IN ('chat', 'workflow', 'task', 'approval', 'agent', 'tool', 'scheduler', 'system')),
            event_type TEXT NOT NULL
                CHECK(event_type IN (
                    'started', 'delta', 'tool_call', 'tool_result',
                    'node_started', 'node_finished', 'artifact', 'usage',
                    'approval_requested', 'approval_decided',
                    'retry', 'cancelled', 'resumed', 'paused',
                    'done', 'error'
                )),
            payload TEXT NOT NULL,
            actor TEXT,
            actor_type TEXT
                CHECK(actor_type IN ('human', 'agent', 'system') OR actor_type IS NULL),
            created_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_exec_events_id ON execution_events(execution_id, created_at)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_exec_events_parent ON execution_events(parent_execution_id, created_at)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_exec_events_source ON execution_events(source, created_at DESC)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_exec_events_type ON execution_events(event_type, created_at DESC)").execute(pool).await;

    // Aggregate view — one row per execution_id
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS executions (
            id TEXT PRIMARY KEY,
            parent_execution_id TEXT,
            source TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'running'
                CHECK(status IN ('pending', 'running', 'paused', 'success', 'failed', 'cancelled')),
            actor TEXT,
            actor_type TEXT
                CHECK(actor_type IN ('human', 'agent', 'system') OR actor_type IS NULL),
            trigger_type TEXT,
            trigger_payload TEXT,
            started_at INTEGER NOT NULL,
            completed_at INTEGER,
            error TEXT,
            event_count INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_exec_status ON executions(status, started_at DESC)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_exec_parent ON executions(parent_execution_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_exec_actor ON executions(actor, started_at DESC)").execute(pool).await;

    tracing::info!("v3 migration 014 (execution_events + executions) completed");
    Ok(())
}

async fn run_v3_migration_015(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // ============================================================
    // Tool invocations — structured per-call audit record.
    // See docs/execution-fact-chain-spec.md and migrations/015_tool_invocations.sql
    // ============================================================
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS tool_invocations (
            id TEXT PRIMARY KEY,
            execution_id TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            input TEXT,
            output TEXT,
            error TEXT,
            permission_level TEXT NOT NULL
                CHECK(permission_level IN ('read_only', 'workspace_write', 'prompt', 'allow', 'danger')),
            approval_id TEXT,
            status TEXT NOT NULL DEFAULT 'pending'
                CHECK(status IN (
                    'pending', 'running', 'approved', 'rejected',
                    'success', 'failed', 'cancelled', 'timeout'
                )),
            started_at INTEGER,
            completed_at INTEGER,
            duration_ms INTEGER,
            invoked_by TEXT,
            invoked_by_type TEXT
                CHECK(invoked_by_type IN ('agent', 'workflow_node', 'system') OR invoked_by_type IS NULL),
            retry_of TEXT,
            created_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_tool_inv_exec ON tool_invocations(execution_id, started_at)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_tool_inv_status ON tool_invocations(status, created_at DESC)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_tool_inv_tool ON tool_invocations(tool_name, created_at DESC)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_tool_inv_approval ON tool_invocations(approval_id) WHERE approval_id IS NOT NULL").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_tool_inv_retry ON tool_invocations(retry_of) WHERE retry_of IS NOT NULL").execute(pool).await;

    tracing::info!("v3 migration 015 (tool_invocations) completed");
    Ok(())
}

async fn run_v3_migration_016(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // ============================================================
    // Learning governance — candidates + blocklist (Track 3 / T3-6..T3-11).
    // See migrations/016_learning_governance.sql and Issue #91.
    // ============================================================
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS learning_candidates (
            id TEXT PRIMARY KEY,
            target_type TEXT NOT NULL
                CHECK(target_type IN ('memory', 'kb_doc', 'prompt')),
            target_key TEXT,
            content TEXT NOT NULL,
            score REAL NOT NULL,
            evidence TEXT,
            source_execution_id TEXT,
            source_metadata TEXT,
            persisted_target_id TEXT,
            status TEXT NOT NULL DEFAULT 'pending'
                CHECK(status IN ('pending', 'approved', 'rejected', 'auto_approved', 'revoked', 'persisted')),
            decided_by TEXT,
            decided_at INTEGER,
            rejection_reason TEXT,
            approval_threshold REAL NOT NULL DEFAULT 0.7,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_lc_status ON learning_candidates(status, created_at DESC)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_lc_target ON learning_candidates(target_type, target_key)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_lc_source_exec ON learning_candidates(source_execution_id)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_lc_score ON learning_candidates(score DESC)").execute(pool).await;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS learning_blocklist (
            id TEXT PRIMARY KEY,
            content_hash TEXT NOT NULL UNIQUE,
            source_candidate_id TEXT NOT NULL REFERENCES learning_candidates(id),
            reason TEXT,
            blocked_at INTEGER NOT NULL,
            blocked_by TEXT
        )"
    ).execute(pool).await?;

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_lb_hash ON learning_blocklist(content_hash)").execute(pool).await;

    tracing::info!("v3 migration 016 (learning_candidates + learning_blocklist) completed");
    Ok(())
}

async fn run_v3_migration_017(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // ============================================================
    // Workflow version history (Track 2 / T2-2).
    // See migrations/017_workflow_versions.sql and Issue #90, #17.
    // ============================================================
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS workflow_versions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            workflow_id TEXT NOT NULL,
            version INTEGER NOT NULL,
            yaml_content TEXT NOT NULL,
            saved_by TEXT,
            change_summary TEXT,
            created_at INTEGER NOT NULL,
            UNIQUE(workflow_id, version)
        )"
    ).execute(pool).await?;

    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_wv_workflow ON workflow_versions(workflow_id, version DESC)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_wv_created ON workflow_versions(created_at DESC)").execute(pool).await;

    tracing::info!("v3 migration 017 (workflow_versions) completed");
    Ok(())
}

async fn run_v3_migration_018(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS workflow_triggers (
            id TEXT PRIMARY KEY,
            workflow_id TEXT NOT NULL,
            trigger_config TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_wt_workflow ON workflow_triggers(workflow_id)").execute(pool).await;
    tracing::info!("v3 migration 018 (workflow_triggers) completed");
    Ok(())
}

async fn run_v3_migration_019(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // #24: Seed 4 built-in system agents
    let _now = chrono::Utc::now().timestamp();
    let agents = vec![
        ("agent-scheduler", "Scheduler", "Automatically receives tasks, decomposes them, and dispatches to specialized agents", "[\"system\",\"scheduler\"]"),
        ("agent-reviewer", "Reviewer", "Reviews agent outputs and workflow results for quality and compliance", "[\"system\",\"reviewer\"]"),
        ("agent-monitor", "Monitor", "Monitors system health, agent uptime, and task queue depth; alerts on anomalies", "[\"system\",\"monitor\"]"),
        ("agent-evolver", "Evolver", "Extracts learnings from completed executions and proposes knowledge updates", "[\"system\",\"evolver\"]"),
    ];
    for (id, name, desc, tags) in agents {
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO agents (id, name, status, description, tags) VALUES (?, ?, 'offline', ?, ?)"
        ).bind(id).bind(name).bind(desc).bind(tags).execute(pool).await;
    }
    tracing::info!("v3 migration 019 (system agents seeded) completed");
    Ok(())
}

async fn run_v3_migration_020(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // #18: Audit logs — persistent record of all API requests
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS audit_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            method TEXT NOT NULL,
            path TEXT NOT NULL,
            query TEXT,
            status INTEGER NOT NULL,
            duration_ms INTEGER NOT NULL,
            user_agent TEXT,
            client_ip TEXT,
            actor TEXT,
            created_at INTEGER NOT NULL
        )"
    ).execute(pool).await?;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_logs(created_at DESC)").execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_path ON audit_logs(path, created_at DESC)").execute(pool).await;
    tracing::info!("v3 migration 020 (audit_logs) completed");
    Ok(())
}

async fn run_v3_migration_021(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // Performance: add composite indexes for common query patterns
    // that the existing single-column indexes don't cover well.

    // 1. Chat handler: search episodic memory by keyword + type
    let _ = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_memories_type_created
         ON memories(memory_type, created_at DESC)"
    ).execute(pool).await?;

    // 2. Execution events: list by execution_id + event_type (used by
    //    audit log projection that filters by event_type)
    let _ = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_exec_events_id_type
         ON execution_events(execution_id, event_type, created_at)"
    ).execute(pool).await?;

    // 3. Tool invocations: filter by tool_name + status (used by
    //    dashboard stats and audit queries)
    let _ = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tool_inv_tool_status
         ON tool_invocations(tool_name, status, created_at DESC)"
    ).execute(pool).await?;

    // 4. Learning candidates: pending list ordered by score (used by
    //    the pending list endpoint)
    let _ = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_lc_status_score
         ON learning_candidates(status, score DESC, created_at DESC)"
    ).execute(pool).await?;

    // 5. Workflow runs: filter by workflow_id + status (used by
    //    the workflow run list endpoint)
    let _ = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_we_workflow_status
         ON workflow_executions(workflow_id, status, started_at DESC)"
    ).execute(pool).await?;

    // 6. Audit logs: filter by path + created_at (used by audit log
    //    query endpoint with path filter)
    let _ = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_audit_path_created
         ON audit_logs(path, created_at DESC)"
    ).execute(pool).await?;

    // 7. Workflow triggers: lookup by workflow_id (used by trigger
    //    manager when checking which workflows to fire)
    let _ = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_wt_workflow_enabled
         ON workflow_triggers(workflow_id, enabled)"
    ).execute(pool).await?;

    // 8. Workflow versions: lookup by workflow_id + version (used by
    //    rollback endpoint)
    let _ = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_wv_workflow_version
         ON workflow_versions(workflow_id, version DESC)"
    ).execute(pool).await?;

    tracing::info!("v3 migration 021 (performance composite indexes) completed");
    Ok(())
}
