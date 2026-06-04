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