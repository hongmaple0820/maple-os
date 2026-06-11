use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;
use chrono::Timelike;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupRule {
    pub id: String,
    pub name: String,
    pub rule_type: GroupRuleType,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GroupRuleType {
    AutoAssign {
        keyword: String,
        agent_id: String,
    },
    AutoApprove {
        agent_id: String,
        confidence_threshold: f32,
        auto_approve_roles: Vec<String>,
    },
    RateLimit {
        agent_id: String,
        max_messages_per_minute: u32,
    },
    TimeWindow {
        agent_id: String,
        allowed_hours: String,
        timezone: String,
    },
}

#[derive(Debug, Clone)]
struct RateLimitEntry {
    timestamps: Vec<Instant>,
}

pub struct GroupRulesEngine {
    rules: Vec<GroupRule>,
    rate_limit_tracker: HashMap<String, RateLimitEntry>,
}

impl Default for GroupRulesEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl GroupRulesEngine {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            rate_limit_tracker: HashMap::new(),
        }
    }

    pub fn add_rule(&mut self, rule: GroupRule) {
        self.rules.push(rule);
    }

    pub fn remove_rule(&mut self, rule_id: &str) -> bool {
        let len_before = self.rules.len();
        self.rules.retain(|r| r.id != rule_id);
        self.rules.len() < len_before
    }

    pub fn list_rules(&self) -> Vec<GroupRule> {
        self.rules.clone()
    }

    pub fn get_rule(&self, rule_id: &str) -> Option<GroupRule> {
        self.rules.iter().find(|r| r.id == rule_id).cloned()
    }

    pub fn update_rule(&mut self, rule_id: &str, updated: GroupRule) -> bool {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == rule_id) {
            *rule = updated;
            true
        } else {
            false
        }
    }

    pub fn evaluate(&self, context: &RuleContext) -> Vec<RuleMatch> {
        self.rules.iter()
            .filter(|r| r.enabled && self.matches(r, context))
            .map(|r| RuleMatch {
                rule: r.clone(),
                action: self.determine_action(r, context),
            })
            .collect()
    }

    fn matches(&self, rule: &GroupRule, context: &RuleContext) -> bool {
        match &rule.rule_type {
            GroupRuleType::AutoAssign { keyword, agent_id: _ } => {
                context.message.to_lowercase().contains(&keyword.to_lowercase())
            }
            GroupRuleType::AutoApprove { agent_id: _, confidence_threshold: _, auto_approve_roles } => {
                auto_approve_roles.is_empty() || auto_approve_roles.contains(&context.sender_role)
            }
            GroupRuleType::RateLimit { agent_id, max_messages_per_minute } => {
                self.check_rate_limit(agent_id, *max_messages_per_minute)
            }
            GroupRuleType::TimeWindow { agent_id: _, allowed_hours, timezone } => {
                self.check_time_window(allowed_hours, timezone)
            }
        }
    }

    fn check_rate_limit(&self, agent_id: &str, max_per_minute: u32) -> bool {
        if let Some(entry) = self.rate_limit_tracker.get(agent_id) {
            let now = Instant::now();
            let recent = entry.timestamps.iter()
                .filter(|t| now.duration_since(**t).as_secs() < 60)
                .count();
            recent < max_per_minute as usize
        } else {
            true
        }
    }

    fn check_time_window(&self, allowed_hours: &str, timezone: &str) -> bool {
        let allowed: Vec<u32> = allowed_hours.split(',')
            .filter_map(|h| h.trim().parse::<u32>().ok())
            .collect();

        if allowed.is_empty() {
            return true;
        }

        let now = chrono::Utc::now();
        let local_hour = match timezone {
            "UTC" => now.hour(),
            tz => {
                let offset: i32 = tz.parse().unwrap_or(0);
                let adjusted = now.hour() as i32 + offset;
                ((adjusted % 24 + 24) % 24) as u32
            }
        };

        allowed.contains(&local_hour)
    }

    fn determine_action(&self, rule: &GroupRule, _context: &RuleContext) -> RuleAction {
        match &rule.rule_type {
            GroupRuleType::AutoAssign { keyword: _, agent_id } => {
                RuleAction::AssignToAgent { agent_id: agent_id.clone() }
            }
            GroupRuleType::AutoApprove { agent_id, confidence_threshold, .. } => {
                RuleAction::AutoApprove {
                    agent_id: agent_id.clone(),
                    confidence_threshold: *confidence_threshold,
                }
            }
            GroupRuleType::RateLimit { agent_id, max_messages_per_minute } => {
                RuleAction::RateLimited {
                    agent_id: agent_id.clone(),
                    remaining: *max_messages_per_minute,
                }
            }
            GroupRuleType::TimeWindow { agent_id, allowed_hours, .. } => {
                RuleAction::WithinTimeWindow {
                    agent_id: agent_id.clone(),
                    allowed_hours: allowed_hours.clone(),
                }
            }
        }
    }

    pub fn record_message(&mut self, agent_id: &str) {
        let entry = self.rate_limit_tracker.entry(agent_id.to_string())
            .or_insert(RateLimitEntry { timestamps: Vec::new() });
        entry.timestamps.push(Instant::now());

        let now = Instant::now();
        entry.timestamps.retain(|t| now.duration_since(*t).as_secs() < 120);
    }

    pub fn evaluate_single(&self, rule: &GroupRule, context: &RuleContext) -> Vec<RuleMatch> {
        if rule.enabled && self.matches(rule, context) {
            vec![RuleMatch {
                rule: rule.clone(),
                action: self.determine_action(rule, context),
            }]
        } else {
            Vec::new()
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuleMatch {
    pub rule: GroupRule,
    pub action: RuleAction,
}

#[derive(Debug, Clone)]
pub enum RuleAction {
    AssignToAgent { agent_id: String },
    AutoApprove { agent_id: String, confidence_threshold: f32 },
    RateLimited { agent_id: String, remaining: u32 },
    WithinTimeWindow { agent_id: String, allowed_hours: String },
}

pub struct RuleContext {
    pub message: String,
    pub sender_id: String,
    pub sender_type: String,
    pub sender_role: String,
}

// ── DB-backed service ───────────────────────────────────────────

/// Persistent rule stored in `group_rules_v3`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentRule {
    pub id: String,
    pub group_id: String,
    pub name: String,
    pub description: Option<String>,
    pub rule_type: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub priority: i64,
    pub condition_expr: Option<String>,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Request to create a new rule.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateGroupRuleRequest {
    pub name: String,
    pub description: Option<String>,
    pub rule_type: String,
    pub config: serde_json::Value,
    pub priority: Option<i64>,
    pub condition_expr: Option<String>,
}

/// Service that persists group rules to SQLite and syncs with the in-memory engine.
pub struct GroupRulesService {
    pool: sqlx::SqlitePool,
    engine: std::sync::Arc<tokio::sync::RwLock<GroupRulesEngine>>,
}

impl GroupRulesService {
    pub fn new(
        pool: sqlx::SqlitePool,
        engine: std::sync::Arc<tokio::sync::RwLock<GroupRulesEngine>>,
    ) -> Self {
        Self { pool, engine }
    }

    /// Load all enabled rules for a group from DB into the in-memory engine.
    pub async fn load_group_rules(&self, group_id: &str) -> Result<(), anyhow::Error> {
        let rows = sqlx::query_as::<_, (String, String, String, String, i64)>(
            "SELECT id, name, rule_type, config, priority FROM group_rules_v3 WHERE group_id = ? AND enabled = 1 ORDER BY priority DESC"
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await?;

        let mut engine = self.engine.write().await;
        engine.rules.retain(|r| !r.id.starts_with(&format!("{}:", group_id)));

        for (id, name, rule_type, config_json, _priority) in rows {
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&config_json) {
                if let Some(rule) = Self::build_in_memory_rule(&id, &name, &rule_type, &config) {
                    engine.rules.push(rule);
                }
            }
        }
        Ok(())
    }

    /// Create a new rule in DB and add to in-memory engine.
    pub async fn create_rule(
        &self,
        group_id: &str,
        created_by: &str,
        req: CreateGroupRuleRequest,
    ) -> Result<PersistentRule, anyhow::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        let config_str = serde_json::to_string(&req.config)?;
        let priority = req.priority.unwrap_or(0);

        sqlx::query(
            "INSERT INTO group_rules_v3 (id, group_id, name, description, rule_type, config, enabled, priority, condition_expr, created_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(group_id)
        .bind(&req.name)
        .bind(&req.description)
        .bind(&req.rule_type)
        .bind(&config_str)
        .bind(priority)
        .bind(&req.condition_expr)
        .bind(created_by)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        if let Some(rule) = Self::build_in_memory_rule(&id, &req.name, &req.rule_type, &req.config) {
            let mut engine = self.engine.write().await;
            engine.rules.push(rule);
        }

        Ok(PersistentRule {
            id,
            group_id: group_id.to_string(),
            name: req.name,
            description: req.description,
            rule_type: req.rule_type,
            config: req.config,
            enabled: true,
            priority,
            condition_expr: req.condition_expr,
            created_by: created_by.to_string(),
            created_at: now,
            updated_at: now,
        })
    }

    /// List all rules for a group from DB.
    pub async fn list_rules(&self, group_id: &str) -> Result<Vec<PersistentRule>, anyhow::Error> {
        let rows = sqlx::query_as::<_, (String, String, String, Option<String>, String, String, i64, i64, Option<String>, String, i64, i64)>(
            "SELECT id, group_id, name, description, rule_type, config, enabled, priority, condition_expr, created_by, created_at, updated_at FROM group_rules_v3 WHERE group_id = ? ORDER BY priority DESC, created_at DESC"
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| PersistentRule {
            id: r.0,
            group_id: r.1,
            name: r.2,
            description: r.3,
            rule_type: r.4,
            config: serde_json::from_str(&r.5).unwrap_or(serde_json::json!({})),
            enabled: r.6 != 0,
            priority: r.7,
            condition_expr: r.8,
            created_by: r.9,
            created_at: r.10,
            updated_at: r.11,
        }).collect())
    }

    /// Update a rule in DB and sync to in-memory engine.
    pub async fn update_rule(
        &self,
        rule_id: &str,
        config: Option<serde_json::Value>,
        priority: Option<i64>,
        enabled: Option<bool>,
        condition_expr: Option<String>,
    ) -> Result<bool, anyhow::Error> {
        let now = chrono::Utc::now().timestamp();
        let mut sets = Vec::new();

        if config.is_some() { sets.push("config = ?"); }
        if priority.is_some() { sets.push("priority = ?"); }
        if enabled.is_some() { sets.push("enabled = ?"); }
        if condition_expr.is_some() { sets.push("condition_expr = ?"); }

        if sets.is_empty() {
            return Ok(false);
        }

        sets.push("updated_at = ?");
        let sql = format!("UPDATE group_rules_v3 SET {} WHERE id = ?", sets.join(", "));

        let mut query = sqlx::query(&sql);
        if let Some(ref c) = config { query = query.bind(serde_json::to_string(c)?); }
        if let Some(p) = priority { query = query.bind(p); }
        if let Some(e) = enabled { query = query.bind(if e { 1i64 } else { 0 }); }
        if let Some(ref ce) = condition_expr { query = query.bind(ce.as_str()); }
        query = query.bind(now);
        query = query.bind(rule_id);
        let result = query.execute(&self.pool).await?;

        if result.rows_affected() > 0 {
            if let Some((gid,)) = sqlx::query_as::<_, (String,)>("SELECT group_id FROM group_rules_v3 WHERE id = ?")
                .bind(rule_id)
                .fetch_optional(&self.pool)
                .await?
            {
                let _ = self.load_group_rules(&gid).await;
            }
        }

        Ok(result.rows_affected() > 0)
    }

    /// Delete a rule from DB and remove from in-memory engine.
    pub async fn delete_rule(&self, rule_id: &str) -> Result<bool, anyhow::Error> {
        let group_id = sqlx::query_as::<_, (String,)>("SELECT group_id FROM group_rules_v3 WHERE id = ?")
            .bind(rule_id)
            .fetch_optional(&self.pool)
            .await?;

        let result = sqlx::query("DELETE FROM group_rules_v3 WHERE id = ?")
            .bind(rule_id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() > 0 {
            let mut engine = self.engine.write().await;
            engine.remove_rule(rule_id);
            if let Some((gid,)) = group_id {
                drop(engine);
                let _ = self.load_group_rules(&gid).await;
            }
        }

        Ok(result.rows_affected() > 0)
    }

    fn build_in_memory_rule(id: &str, name: &str, rule_type: &str, config: &serde_json::Value) -> Option<GroupRule> {
        let rt = match rule_type {
            "auto_assign" => GroupRuleType::AutoAssign {
                keyword: config["keyword"].as_str().unwrap_or("").to_string(),
                agent_id: config["agent_id"].as_str().unwrap_or("").to_string(),
            },
            "auto_approve" => GroupRuleType::AutoApprove {
                agent_id: config["agent_id"].as_str().unwrap_or("").to_string(),
                confidence_threshold: config["confidence_threshold"].as_f64().unwrap_or(0.8) as f32,
                auto_approve_roles: config["auto_approve_roles"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
            },
            "rate_limit" => GroupRuleType::RateLimit {
                agent_id: config["agent_id"].as_str().unwrap_or("").to_string(),
                max_messages_per_minute: config["max_messages_per_minute"].as_u64().unwrap_or(10) as u32,
            },
            "time_window" => GroupRuleType::TimeWindow {
                agent_id: config["agent_id"].as_str().unwrap_or("").to_string(),
                allowed_hours: config["allowed_hours"].as_str().unwrap_or("").to_string(),
                timezone: config["timezone"].as_str().unwrap_or("UTC").to_string(),
            },
            _ => return None,
        };

        Some(GroupRule {
            id: id.to_string(),
            name: name.to_string(),
            rule_type: rt,
            enabled: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_assign_keyword_match() {
        let engine = GroupRulesEngine::new();
        let rule = GroupRule {
            id: "r1".to_string(),
            name: "assign-coder".to_string(),
            rule_type: GroupRuleType::AutoAssign {
                keyword: "cod".to_string(),
                agent_id: "coder-agent".to_string(),
            },
            enabled: true,
        };

        let ctx = RuleContext {
            message: "I need help with coding".to_string(),
            sender_id: "user1".to_string(),
            sender_type: "human".to_string(),
            sender_role: "member".to_string(),
        };

        let matches = engine.evaluate_single(&rule, &ctx);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_auto_assign_no_match() {
        let engine = GroupRulesEngine::new();
        let rule = GroupRule {
            id: "r1".to_string(),
            name: "assign-coder".to_string(),
            rule_type: GroupRuleType::AutoAssign {
                keyword: "python".to_string(),
                agent_id: "coder-agent".to_string(),
            },
            enabled: true,
        };

        let ctx = RuleContext {
            message: "I need help with cooking".to_string(),
            sender_id: "user1".to_string(),
            sender_type: "human".to_string(),
            sender_role: "member".to_string(),
        };

        let matches = engine.evaluate_single(&rule, &ctx);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_time_window_allowed() {
        let engine = GroupRulesEngine::new();
        let current_hour = chrono::Utc::now().hour();

        let rule = GroupRule {
            id: "r2".to_string(),
            name: "daytime-only".to_string(),
            rule_type: GroupRuleType::TimeWindow {
                agent_id: "bot".to_string(),
                allowed_hours: format!("{}", current_hour),
                timezone: "UTC".to_string(),
            },
            enabled: true,
        };

        let ctx = RuleContext {
            message: "hello".to_string(),
            sender_id: "user1".to_string(),
            sender_type: "human".to_string(),
            sender_role: "member".to_string(),
        };

        let matches = engine.evaluate_single(&rule, &ctx);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_rate_limit_allowed() {
        let engine = GroupRulesEngine::new();
        let rule = GroupRule {
            id: "r3".to_string(),
            name: "rate-limit".to_string(),
            rule_type: GroupRuleType::RateLimit {
                agent_id: "bot".to_string(),
                max_messages_per_minute: 10,
            },
            enabled: true,
        };

        let ctx = RuleContext {
            message: "hello".to_string(),
            sender_id: "user1".to_string(),
            sender_type: "human".to_string(),
            sender_role: "member".to_string(),
        };

        let matches = engine.evaluate_single(&rule, &ctx);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_auto_approve_role_match() {
        let engine = GroupRulesEngine::new();
        let rule = GroupRule {
            id: "r4".to_string(),
            name: "admin-approve".to_string(),
            rule_type: GroupRuleType::AutoApprove {
                agent_id: "admin-bot".to_string(),
                confidence_threshold: 0.8,
                auto_approve_roles: vec!["admin".to_string()],
            },
            enabled: true,
        };

        let ctx = RuleContext {
            message: "approve this".to_string(),
            sender_id: "admin1".to_string(),
            sender_type: "human".to_string(),
            sender_role: "admin".to_string(),
        };

        let matches = engine.evaluate_single(&rule, &ctx);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_auto_approve_role_no_match() {
        let engine = GroupRulesEngine::new();
        let rule = GroupRule {
            id: "r4".to_string(),
            name: "admin-approve".to_string(),
            rule_type: GroupRuleType::AutoApprove {
                agent_id: "admin-bot".to_string(),
                confidence_threshold: 0.8,
                auto_approve_roles: vec!["admin".to_string()],
            },
            enabled: true,
        };

        let ctx = RuleContext {
            message: "approve this".to_string(),
            sender_id: "member1".to_string(),
            sender_type: "human".to_string(),
            sender_role: "member".to_string(),
        };

        let matches = engine.evaluate_single(&rule, &ctx);
        assert_eq!(matches.len(), 0);
    }
}