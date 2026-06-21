use serde::{Deserialize, Serialize};
use sqlx::{SqlitePool, Row};
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub group_id: String,
    pub title: String,
    pub description: Option<String>,
    pub request_type: String,
    pub requester_id: String,
    pub urgency: ApprovalUrgency,
    pub quorum_type: QuorumType,
    pub required_count: i64,
    pub approver_spec: String,
    pub context: Option<String>,
    pub execution_status: String,
    pub timeout_at: Option<i64>,
    pub auto_action: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalUrgency {
    Low,
    Normal,
    High,
    Critical,
}

impl ApprovalUrgency {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "low" => Self::Low,
            "high" => Self::High,
            "critical" => Self::Critical,
            _ => Self::Normal,
        }
    }

    pub fn default_timeout_secs(&self) -> i64 {
        match self {
            Self::Low => 86400,
            Self::Normal => 3600,
            Self::High => 600,
            Self::Critical => 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuorumType {
    Any,
    All,
    Majority,
    ExactCount(u32),
}

impl QuorumType {
    pub fn as_str(&self) -> String {
        match self {
            Self::Any => "any".to_string(),
            Self::All => "all".to_string(),
            Self::Majority => "majority".to_string(),
            Self::ExactCount(n) => format!("n_of_{}", n),
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "any" => Self::Any,
            "all" => Self::All,
            "majority" => Self::Majority,
            other => {
                if let Some(n) = other.strip_prefix("n_of_") {
                    if let Ok(count) = n.parse::<u32>() {
                        return Self::ExactCount(count);
                    }
                }
                Self::Any
            }
        }
    }

    pub fn is_satisfied(&self, approve_count: i64, _reject_count: i64, total_approvers: i64) -> bool {
        match self {
            Self::Any => approve_count >= 1,
            Self::All => approve_count >= total_approvers && total_approvers > 0,
            Self::Majority => approve_count > total_approvers / 2,
            Self::ExactCount(n) => approve_count >= *n as i64,
        }
    }

    pub fn is_impossible(&self, approve_count: i64, reject_count: i64, total_approvers: i64) -> bool {
        match self {
            Self::Any => false,
            Self::All => reject_count >= 1,
            Self::Majority => reject_count > total_approvers / 2,
            Self::ExactCount(n) => {
                let remaining = total_approvers - approve_count - reject_count;
                approve_count + remaining < *n as i64
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalVote {
    pub id: String,
    pub approval_id: String,
    pub voter_id: String,
    pub decision: VoteDecision,
    pub comment: Option<String>,
    pub voted_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VoteDecision {
    Approve,
    Reject,
    Abstain,
}

impl VoteDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Abstain => "abstain",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "reject" => Self::Reject,
            "abstain" => Self::Abstain,
            _ => Self::Approve,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalOutcome {
    pub approved: bool,
    pub approve_count: i64,
    pub reject_count: i64,
    pub abstain_count: i64,
    pub total_approvers: i64,
    pub quorum_met: bool,
}

const APPROVAL_COLUMNS: &str = "id, group_id, title, description, request_type, requester_id, urgency, quorum_type, required_count, approver_spec, context, execution_status, timeout_at, auto_action, created_at, updated_at, resolved_at";

fn row_to_approval(row: &sqlx::sqlite::SqliteRow) -> ApprovalRequest {
    ApprovalRequest {
        id: row.get(0),
        group_id: row.get(1),
        title: row.get(2),
        description: row.get(3),
        request_type: row.get(4),
        requester_id: row.get(5),
        urgency: ApprovalUrgency::from_str(row.get::<&str, _>(6)),
        quorum_type: QuorumType::from_str(row.get::<&str, _>(7)),
        required_count: row.get(8),
        approver_spec: row.get(9),
        context: row.get(10),
        execution_status: row.get(11),
        timeout_at: row.get(12),
        auto_action: row.get(13),
        created_at: row.get(14),
        updated_at: row.get(15),
        resolved_at: row.get(16),
    }
}

pub struct ApprovalService {
    pool: SqlitePool,
    /// Optional unified execution fact chain recorder (Track 1 / T1-6).
    /// When set, create_request / vote / check_quorum append events to the
    /// provided execution_id so the approval lifecycle is visible in the
    /// same timeline as chat / workflow / agent traces.
    /// See docs/execution-fact-chain-spec.md §7.3.
    recorder: Option<crate::execution_chain::ExecutionRecorder>,
}

impl ApprovalService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool, recorder: None }
    }

    /// Attach an ExecutionRecorder so approval events flow into the unified
    /// execution fact chain. Idempotent — calling twice replaces the recorder.
    pub fn with_recorder(mut self, recorder: crate::execution_chain::ExecutionRecorder) -> Self {
        self.recorder = Some(recorder);
        self
    }

    pub async fn create_request(
        &self,
        group_id: &str,
        title: &str,
        description: Option<&str>,
        request_type: &str,
        requester_id: &str,
        urgency: ApprovalUrgency,
        quorum_type: QuorumType,
        approver_spec: &str,
        context: Option<&str>,
    ) -> Result<ApprovalRequest> {
        self.create_request_with_execution(group_id, title, description, request_type,
            requester_id, urgency, quorum_type, approver_spec, context, None).await
    }

    /// Like `create_request` but also records an `approval_requested` event
    /// into the unified execution fact chain under `execution_id`.
    /// `execution_id` may be `None` (no fact chain link) — use this when the
    /// approval is triggered from a chat / workflow / agent context that
    /// already opened an execution.
    pub async fn create_request_with_execution(
        &self,
        group_id: &str,
        title: &str,
        description: Option<&str>,
        request_type: &str,
        requester_id: &str,
        urgency: ApprovalUrgency,
        quorum_type: QuorumType,
        approver_spec: &str,
        context: Option<&str>,
        execution_id: Option<&str>,
    ) -> Result<ApprovalRequest> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        let timeout_secs = urgency.default_timeout_secs();
        let timeout_at = now + timeout_secs;

        let required_count = match &quorum_type {
            QuorumType::Any => 1,
            QuorumType::All | QuorumType::Majority => {
                let approvers: Vec<&str> = approver_spec.split(',').filter(|s| !s.trim().is_empty()).collect();
                match &quorum_type {
                    QuorumType::All => approvers.len() as i64,
                    _ => (approvers.len() / 2 + 1) as i64,
                }
            }
            QuorumType::ExactCount(n) => *n as i64,
        };

        sqlx::query(
            "INSERT INTO approval_requests (id, group_id, title, description, request_type, requester_id,
             urgency, quorum_type, required_count, approver_spec, context, execution_status,
             timeout_at, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?)"
        )
        .bind(&id)
        .bind(group_id)
        .bind(title)
        .bind(description)
        .bind(request_type)
        .bind(requester_id)
        .bind(urgency.as_str())
        .bind(quorum_type.as_str())
        .bind(required_count)
        .bind(approver_spec)
        .bind(context)
        .bind(timeout_at)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        // ── T1-6: append 'approval_requested' event to the fact chain ──
        if let (Some(rec), Some(eid)) = (&self.recorder, execution_id) {
            let _ = rec.append(
                eid,
                "approval",
                "approval_requested",
                serde_json::json!({
                    "approval_id": id,
                    "action_type": request_type,
                    "description": title,
                    "urgency": urgency.as_str(),
                    "expires_at": timeout_at,
                    "group_id": group_id,
                    "requester_id": requester_id,
                }),
                Some(requester_id),
                Some("human"),
            ).await;
        }

        Ok(ApprovalRequest {
            id,
            group_id: group_id.to_string(),
            title: title.to_string(),
            description: description.map(|s| s.to_string()),
            request_type: request_type.to_string(),
            requester_id: requester_id.to_string(),
            urgency,
            quorum_type,
            required_count,
            approver_spec: approver_spec.to_string(),
            context: context.map(|s| s.to_string()),
            execution_status: "pending".to_string(),
            timeout_at: Some(timeout_at),
            auto_action: None,
            created_at: now,
            updated_at: now,
            resolved_at: None,
        })
    }

    pub async fn get_request(&self, approval_id: &str) -> Result<Option<ApprovalRequest>> {
        let sql = format!(
            "SELECT {} FROM approval_requests WHERE id = ?",
            APPROVAL_COLUMNS
        );
        let row = sqlx::query(&sql)
            .bind(approval_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| row_to_approval(&r)))
    }

    pub async fn vote(
        &self,
        approval_id: &str,
        voter_id: &str,
        decision: VoteDecision,
        comment: Option<&str>,
    ) -> Result<ApprovalOutcome> {
        self.vote_with_execution(approval_id, voter_id, decision, comment, None).await
    }

    /// Like `vote` but also records an `approval_decided` event into the
    /// unified execution fact chain under `execution_id`.
    pub async fn vote_with_execution(
        &self,
        approval_id: &str,
        voter_id: &str,
        decision: VoteDecision,
        comment: Option<&str>,
        execution_id: Option<&str>,
    ) -> Result<ApprovalOutcome> {
        let now = chrono::Utc::now().timestamp();
        let vote_id = uuid::Uuid::new_v4().to_string();

        let result = sqlx::query(
            "INSERT OR IGNORE INTO approval_votes (id, approval_id, voter_id, decision, comment, voted_at)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(&vote_id)
        .bind(approval_id)
        .bind(voter_id)
        .bind(decision.as_str())
        .bind(comment)
        .bind(now)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            anyhow::bail!("Voter has already voted on this approval");
        }

        // ── T1-6: append 'approval_decided' event ──
        // We append the vote itself; check_quorum will append the terminal
        // 'approval_decided' event with the final outcome (approved/rejected).
        if let (Some(rec), Some(eid)) = (&self.recorder, execution_id) {
            let _ = rec.append(
                eid,
                "approval",
                "approval_decided",
                serde_json::json!({
                    "approval_id": approval_id,
                    "decision": decision.as_str(),
                    "voter_id": voter_id,
                    "comment": comment,
                    "is_terminal": false, // intermediate vote, not final outcome
                }),
                Some(voter_id),
                Some("human"),
            ).await;
        }

        self.check_quorum_with_execution(approval_id, execution_id).await
    }

    pub async fn check_quorum(&self, approval_id: &str) -> Result<ApprovalOutcome> {
        self.check_quorum_with_execution(approval_id, None).await
    }

    /// Like `check_quorum` but appends a terminal `approval_decided` event
    /// (with `is_terminal: true` and the final outcome) when quorum is reached.
    pub async fn check_quorum_with_execution(
        &self,
        approval_id: &str,
        execution_id: Option<&str>,
    ) -> Result<ApprovalOutcome> {
        let request = self.get_request(approval_id).await?
            .ok_or_else(|| anyhow::anyhow!("Approval not found: {}", approval_id))?;

        let (approve_count, reject_count, abstain_count) = self.count_votes(approval_id).await?;

        let total_approvers: i64 = request.approver_spec
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .count() as i64;

        let quorum_met = request.quorum_type.is_satisfied(approve_count, reject_count, total_approvers);
        let is_impossible = request.quorum_type.is_impossible(approve_count, reject_count, total_approvers);

        let now = chrono::Utc::now().timestamp();

        if quorum_met {
            sqlx::query(
                "UPDATE approval_requests SET execution_status = 'approved', resolved_at = ?, updated_at = ? WHERE id = ?"
            )
            .bind(now).bind(now).bind(approval_id)
            .execute(&self.pool).await?;
        } else if is_impossible {
            sqlx::query(
                "UPDATE approval_requests SET execution_status = 'rejected', resolved_at = ?, updated_at = ? WHERE id = ?"
            )
            .bind(now).bind(now).bind(approval_id)
            .execute(&self.pool).await?;
        }

        // ── T1-6: append terminal approval_decided event if resolved ──
        if let (Some(rec), Some(eid)) = (&self.recorder, execution_id) {
            if quorum_met || is_impossible {
                let final_decision = if quorum_met { "approved" } else { "rejected" };
                let _ = rec.append(
                    eid,
                    "approval",
                    "approval_decided",
                    serde_json::json!({
                        "approval_id": approval_id,
                        "decision": final_decision,
                        "is_terminal": true,
                        "approve_count": approve_count,
                        "reject_count": reject_count,
                        "abstain_count": abstain_count,
                        "total_approvers": total_approvers,
                    }),
                    None,
                    Some("system"),
                ).await;
            }
        }

        Ok(ApprovalOutcome {
            approved: quorum_met,
            approve_count,
            reject_count,
            abstain_count,
            total_approvers,
            quorum_met,
        })
    }

    async fn count_votes(&self, approval_id: &str) -> Result<(i64, i64, i64)> {
        let approve: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM approval_votes WHERE approval_id = ? AND decision = 'approve'"
        )
        .bind(approval_id)
        .fetch_one(&self.pool)
        .await?;

        let reject: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM approval_votes WHERE approval_id = ? AND decision = 'reject'"
        )
        .bind(approval_id)
        .fetch_one(&self.pool)
        .await?;

        let abstain: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM approval_votes WHERE approval_id = ? AND decision = 'abstain'"
        )
        .bind(approval_id)
        .fetch_one(&self.pool)
        .await?;

        Ok((approve, reject, abstain))
    }

    pub async fn list_votes(&self, approval_id: &str) -> Result<Vec<ApprovalVote>> {
        let rows = sqlx::query(
            "SELECT id, approval_id, voter_id, decision, comment, voted_at
             FROM approval_votes WHERE approval_id = ? ORDER BY voted_at ASC"
        )
        .bind(approval_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| ApprovalVote {
            id: r.get(0),
            approval_id: r.get(1),
            voter_id: r.get(2),
            decision: VoteDecision::from_str(r.get::<&str, _>(3)),
            comment: r.get(4),
            voted_at: r.get(5),
        }).collect())
    }

    pub async fn list_pending_for_user(&self, user_id: &str, group_id: Option<&str>) -> Result<Vec<ApprovalRequest>> {
        let sql = if group_id.is_some() {
            format!(
                "SELECT {} FROM approval_requests
                 WHERE execution_status = 'pending' AND group_id = ?
                 AND approver_spec LIKE ?",
                APPROVAL_COLUMNS
            )
        } else {
            format!(
                "SELECT {} FROM approval_requests
                 WHERE execution_status = 'pending'
                 AND approver_spec LIKE ?",
                APPROVAL_COLUMNS
            )
        };

        let query = sqlx::query(&sql);
        let query = if let Some(gid) = group_id { query.bind(gid) } else { query };
        let query = query.bind(format!("%{}%", user_id));

        let rows = query.fetch_all(&self.pool).await?;
        Ok(rows.iter().map(row_to_approval).collect())
    }

    pub async fn handle_timeout(&self, approval_id: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let request = self.get_request(approval_id).await?
            .ok_or_else(|| anyhow::anyhow!("Approval not found"))?;

        if request.execution_status != "pending" {
            return Ok(());
        }

        let action = request.auto_action.as_deref().unwrap_or("reject");
        let new_status = match action {
            "approve" => "approved",
            _ => "rejected",
        };

        sqlx::query(
            "UPDATE approval_requests SET execution_status = ?, resolved_at = ?, updated_at = ? WHERE id = ?"
        )
        .bind(new_status)
        .bind(now)
        .bind(now)
        .bind(approval_id)
        .execute(&self.pool)
        .await?;

        let log_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO approval_timeout_logs (id, approval_id, timeout_at, action_taken)
             VALUES (?, ?, ?, ?)"
        )
        .bind(&log_id)
        .bind(approval_id)
        .bind(now)
        .bind(action)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();

        sqlx::query("CREATE TABLE users_v3 (id TEXT PRIMARY KEY, name TEXT NOT NULL, user_type TEXT NOT NULL DEFAULT 'human', status TEXT NOT NULL DEFAULT 'offline', platform_role TEXT NOT NULL DEFAULT 'user', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO users_v3 (id, name, created_at, updated_at) VALUES ('user1', 'User 1', 0, 0)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO users_v3 (id, name, created_at, updated_at) VALUES ('user2', 'User 2', 0, 0)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO users_v3 (id, name, created_at, updated_at) VALUES ('user3', 'User 3', 0, 0)")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE groups (id TEXT PRIMARY KEY, name TEXT NOT NULL, group_type TEXT NOT NULL DEFAULT 'collaboration', owner_id TEXT NOT NULL, settings TEXT NOT NULL DEFAULT '{}', member_count INTEGER NOT NULL DEFAULT 0, message_count INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE group_messages (id TEXT PRIMARY KEY, group_id TEXT NOT NULL, sender_id TEXT NOT NULL, sender_type TEXT NOT NULL, message_type TEXT NOT NULL, content TEXT NOT NULL, thread_reply_count INTEGER NOT NULL DEFAULT 0, source_channel TEXT NOT NULL DEFAULT 'api', pinned INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL)")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE tasks_v3 (id TEXT PRIMARY KEY, group_id TEXT NOT NULL, title TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'todo', priority TEXT NOT NULL DEFAULT 'medium', creator_id TEXT NOT NULL, labels TEXT NOT NULL DEFAULT '[]', subtask_count INTEGER NOT NULL DEFAULT 0, subtask_done_count INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE approval_requests (id TEXT PRIMARY KEY, group_id TEXT NOT NULL, title TEXT NOT NULL, description TEXT, request_type TEXT NOT NULL, requester_id TEXT NOT NULL, urgency TEXT NOT NULL DEFAULT 'normal', quorum_type TEXT NOT NULL DEFAULT 'any', required_count INTEGER NOT NULL DEFAULT 1, approver_spec TEXT NOT NULL, context TEXT, execution_status TEXT, timeout_at INTEGER NOT NULL, auto_action TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, resolved_at INTEGER)")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE approval_votes (id TEXT PRIMARY KEY, approval_id TEXT NOT NULL, voter_id TEXT NOT NULL, decision TEXT NOT NULL, comment TEXT, voted_at INTEGER NOT NULL, UNIQUE(approval_id, voter_id))")
            .execute(&pool).await.unwrap();

        sqlx::query("CREATE TABLE approval_timeout_logs (id TEXT PRIMARY KEY, approval_id TEXT NOT NULL, timeout_at INTEGER NOT NULL, action_taken TEXT NOT NULL)")
            .execute(&pool).await.unwrap();

        pool
    }

    #[tokio::test]
    async fn test_create_and_get() {
        let pool = setup_db().await;
        let svc = ApprovalService::new(pool);

        let req = svc.create_request(
            "g1", "Deploy to prod", Some("deploy v2"), "deployment",
            "user1", ApprovalUrgency::Normal, QuorumType::Any,
            "user1,user2", None,
        ).await.unwrap();

        assert_eq!(req.title, "Deploy to prod");
        assert_eq!(req.required_count, 1);

        let fetched = svc.get_request(&req.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, req.id);
    }

    #[tokio::test]
    async fn test_vote_any_quorum() {
        let pool = setup_db().await;
        let svc = ApprovalService::new(pool);

        let req = svc.create_request(
            "g1", "Approve", None, "action", "user1",
            ApprovalUrgency::Normal, QuorumType::Any,
            "user1,user2,user3", None,
        ).await.unwrap();

        let outcome = svc.vote(&req.id, "user1", VoteDecision::Approve, None).await.unwrap();
        assert!(outcome.quorum_met);
        assert!(outcome.approved);
    }

    #[tokio::test]
    async fn test_vote_all_quorum() {
        let pool = setup_db().await;
        let svc = ApprovalService::new(pool);

        let req = svc.create_request(
            "g1", "All agree", None, "action", "user1",
            ApprovalUrgency::Normal, QuorumType::All,
            "user1,user2,user3", None,
        ).await.unwrap();

        let outcome = svc.vote(&req.id, "user1", VoteDecision::Approve, None).await.unwrap();
        assert!(!outcome.quorum_met);

        let outcome = svc.vote(&req.id, "user2", VoteDecision::Approve, None).await.unwrap();
        assert!(!outcome.quorum_met);

        let outcome = svc.vote(&req.id, "user3", VoteDecision::Approve, None).await.unwrap();
        assert!(outcome.quorum_met);
        assert!(outcome.approved);
    }

    #[tokio::test]
    async fn test_vote_reject() {
        let pool = setup_db().await;
        let svc = ApprovalService::new(pool);

        let req = svc.create_request(
            "g1", "Maybe", None, "action", "user1",
            ApprovalUrgency::Normal, QuorumType::All,
            "user1,user2", None,
        ).await.unwrap();

        svc.vote(&req.id, "user1", VoteDecision::Reject, None).await.unwrap();
        let outcome = svc.vote(&req.id, "user2", VoteDecision::Approve, None).await.unwrap();
        assert!(!outcome.approved);
    }

    #[tokio::test]
    async fn test_duplicate_vote() {
        let pool = setup_db().await;
        let svc = ApprovalService::new(pool);

        let req = svc.create_request(
            "g1", "Test", None, "action", "user1",
            ApprovalUrgency::Normal, QuorumType::Any,
            "user1,user2", None,
        ).await.unwrap();

        svc.vote(&req.id, "user1", VoteDecision::Approve, None).await.unwrap();
        let result = svc.vote(&req.id, "user1", VoteDecision::Approve, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_majority_quorum() {
        let pool = setup_db().await;
        let svc = ApprovalService::new(pool);

        let req = svc.create_request(
            "g1", "Majority vote", None, "action", "user1",
            ApprovalUrgency::Normal, QuorumType::Majority,
            "user1,user2,user3", None,
        ).await.unwrap();

        let outcome = svc.vote(&req.id, "user1", VoteDecision::Approve, None).await.unwrap();
        assert!(!outcome.quorum_met);

        let outcome = svc.vote(&req.id, "user2", VoteDecision::Approve, None).await.unwrap();
        assert!(outcome.quorum_met);
        assert!(outcome.approved);
    }
}
