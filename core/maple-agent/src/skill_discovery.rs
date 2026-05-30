use serde::{Deserialize, Serialize};

/// Dynamic Skill Discovery — conditional activation by context
///
/// Skills are activated/deactivated at runtime based on:
/// - File path patterns (*.rs → Rust skill)
/// - Directory structure (has tests/ → testing skill)
/// - Task type (code_gen → code skill)
/// - Keywords in user messages
/// - User preferences (always-on)

/// Skill definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Tools this skill provides
    pub tools: Vec<String>,
    /// Activation rules (ALL must match for activation)
    pub activation_rules: Vec<ActivationRule>,
    /// Whether currently active
    pub active: bool,
    /// Priority (higher = preferred when conflicts)
    pub priority: i32,
}

/// Activation rule — condition that must be true for skill to activate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActivationRule {
    /// File path glob pattern matches (e.g., "*.rs", "src/**/*.ts")
    FilePath { pattern: String },
    /// Directory exists in workspace
    DirectoryExists { path: String },
    /// User message contains keywords
    Keyword { words: Vec<String> },
    /// File contains specific content
    FileContains { path: String, pattern: String },
    /// Always active when registered
    Always,
    /// Never auto-activate (manual only)
    Never,
    /// All sub-rules must match (AND)
    All(Vec<ActivationRule>),
    /// Any sub-rule must match (OR)
    Any(Vec<ActivationRule>),
    /// Negation
    Not(Box<ActivationRule>),
}

/// Context for evaluating activation rules
#[derive(Debug)]
pub struct SkillContext {
    pub workspace_root: String,
    /// Files currently in workspace
    pub current_files: Vec<String>,
    /// User's latest message
    pub user_message: String,
    /// Currently active skill IDs
    pub active_skills: Vec<String>,
}

/// Result of skill evaluation
#[derive(Debug, Clone)]
pub struct SkillActivation {
    pub skill_id: String,
    pub active: bool,
    pub reason: String,
}

/// Changes since last evaluation
#[derive(Debug, Clone)]
pub struct SkillDiff {
    pub activated: Vec<String>,
    pub deactivated: Vec<String>,
}

/// Skill discovery engine
pub struct SkillDiscovery {
    skills: Vec<Skill>,
    previous_active: Vec<String>,
}

impl SkillDiscovery {
    pub fn new() -> Self {
        Self {
            skills: Vec::new(),
            previous_active: Vec::new(),
        }
    }

    /// Register a skill
    pub fn register(&mut self, skill: Skill) {
        self.skills.push(skill);
    }

    /// Evaluate all skills against context, returning activation changes
    pub fn evaluate(&mut self, context: &SkillContext) -> Vec<SkillActivation> {
        let mut results = Vec::new();

        for skill in &mut self.skills {
            let should_activate = skill.activation_rules.iter().all(|rule| {
                evaluate_rule(rule, context)
            });

            let changed = skill.active != should_activate;
            skill.active = should_activate;

            results.push(SkillActivation {
                skill_id: skill.id.clone(),
                active: should_activate,
                reason: if changed {
                    format!("State changed to {}", if should_activate { "active" } else { "inactive" })
                } else {
                    "No change".into()
                },
            });
        }

        results
    }

    /// Get diff from previous evaluation
    pub fn diff(&self) -> SkillDiff {
        let current_active: Vec<String> = self.skills.iter()
            .filter(|s| s.active)
            .map(|s| s.id.clone())
            .collect();

        let activated: Vec<String> = current_active.iter()
            .filter(|id| !self.previous_active.contains(id))
            .cloned()
            .collect();

        let deactivated: Vec<String> = self.previous_active.iter()
            .filter(|id| !current_active.contains(id))
            .cloned()
            .collect();

        SkillDiff { activated, deactivated }
    }

    /// Get all currently active tools
    pub fn active_tools(&self) -> Vec<String> {
        self.skills.iter()
            .filter(|s| s.active)
            .flat_map(|s| s.tools.iter().cloned())
            .collect()
    }

    /// Get all registered skills
    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    /// Get active skills
    pub fn active_skills(&self) -> Vec<&Skill> {
        self.skills.iter().filter(|s| s.active).collect()
    }
}

impl Default for SkillDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Evaluate a single activation rule against context
fn evaluate_rule(rule: &ActivationRule, context: &SkillContext) -> bool {
    match rule {
        ActivationRule::FilePath { pattern } => {
            context.current_files.iter().any(|f| glob_match(pattern, f))
        }
        ActivationRule::DirectoryExists { path } => {
            let full_path = format!("{}/{}", context.workspace_root, path);
            std::path::Path::new(&full_path).exists()
        }
        ActivationRule::Keyword { words } => {
            let lower_msg = context.user_message.to_lowercase();
            words.iter().any(|w| lower_msg.contains(&w.to_lowercase()))
        }
        ActivationRule::FileContains { path, pattern } => {
            let full_path = format!("{}/{}", context.workspace_root, path);
            if let Ok(content) = std::fs::read_to_string(&full_path) {
                content.contains(pattern)
            } else {
                false
            }
        }
        ActivationRule::Always => true,
        ActivationRule::Never => false,
        ActivationRule::All(rules) => {
            rules.iter().all(|r| evaluate_rule(r, context))
        }
        ActivationRule::Any(rules) => {
            rules.iter().any(|r| evaluate_rule(r, context))
        }
        ActivationRule::Not(inner) => {
            !evaluate_rule(inner, context)
        }
    }
}

/// Simple glob pattern matching (supports * and **)
fn glob_match(pattern: &str, text: &str) -> bool {
    // Simple implementation: convert glob to segments
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let text_parts: Vec<&str> = text.split('/').collect();

    glob_match_parts(&pattern_parts, &text_parts)
}

fn glob_match_parts(pattern: &[&str], text: &[&str]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }

    if pattern[0] == "**" {
        // ** matches zero or more directories
        if glob_match_parts(&pattern[1..], text) {
            return true;
        }
        if !text.is_empty() && glob_match_parts(pattern, &text[1..]) {
            return true;
        }
        return false;
    }

    if text.is_empty() {
        return false;
    }

    if segment_match(pattern[0], text[0]) {
        glob_match_parts(&pattern[1..], &text[1..])
    } else {
        false
    }
}

fn segment_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let mut pi = 0;
    let mut ti = 0;
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();

    while pi < p.len() && ti < t.len() {
        if p[pi] == '*' {
            // Try matching zero or more chars
            while ti <= t.len() {
                if segment_match(&pattern[pi + 1..], &text[ti..]) {
                    return true;
                }
                if ti < t.len() {
                    ti += 1;
                } else {
                    break;
                }
            }
            return false;
        } else if p[pi] != t[ti] {
            return false;
        } else {
            pi += 1;
            ti += 1;
        }
    }

    // Handle trailing *
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }

    pi == p.len() && ti == t.len()
}

/// Predefined skills for common development scenarios
pub mod presets {
    use super::*;

    /// Rust development skill
    pub fn rust_dev() -> Skill {
        Skill {
            id: "rust_dev".into(),
            name: "Rust Development".into(),
            description: "Rust-specific tools and patterns".into(),
            tools: vec![
                "cargo_build".into(),
                "cargo_test".into(),
                "cargo_clippy".into(),
                "cargo_fmt".into(),
            ],
            activation_rules: vec![
                ActivationRule::Any(vec![
                    ActivationRule::FilePath { pattern: "*.rs".into() },
                    ActivationRule::FilePath { pattern: "**/*.rs".into() },
                    ActivationRule::FileContains {
                        path: "Cargo.toml".into(),
                        pattern: "[package]".into(),
                    },
                ]),
            ],
            active: false,
            priority: 10,
        }
    }

    /// TypeScript/JavaScript development skill
    pub fn typescript_dev() -> Skill {
        Skill {
            id: "typescript_dev".into(),
            name: "TypeScript Development".into(),
            description: "TypeScript/JavaScript tools".into(),
            tools: vec![
                "npm_install".into(),
                "npm_test".into(),
                "tsc_check".into(),
                "eslint".into(),
                "prettier".into(),
            ],
            activation_rules: vec![
                ActivationRule::Any(vec![
                    ActivationRule::FilePath { pattern: "*.ts".into() },
                    ActivationRule::FilePath { pattern: "*.tsx".into() },
                    ActivationRule::FilePath { pattern: "*.js".into() },
                    ActivationRule::FileContains {
                        path: "package.json".into(),
                        pattern: "typescript".into(),
                    },
                ]),
            ],
            active: false,
            priority: 10,
        }
    }

    /// Testing skill
    pub fn testing() -> Skill {
        Skill {
            id: "testing".into(),
            name: "Testing".into(),
            description: "Test execution and coverage tools".into(),
            tools: vec![
                "run_tests".into(),
                "coverage_report".into(),
                "test_watch".into(),
            ],
            activation_rules: vec![
                ActivationRule::Any(vec![
                    ActivationRule::DirectoryExists { path: "tests".into() },
                    ActivationRule::DirectoryExists { path: "test".into() },
                    ActivationRule::DirectoryExists { path: "__tests__".into() },
                    ActivationRule::Keyword { words: vec!["test".into(), "testing".into(), "coverage".into()] },
                ]),
            ],
            active: false,
            priority: 5,
        }
    }

    /// Git skill
    pub fn git() -> Skill {
        Skill {
            id: "git".into(),
            name: "Git".into(),
            description: "Git version control tools".into(),
            tools: vec![
                "git_status".into(),
                "git_diff".into(),
                "git_log".into(),
                "git_commit".into(),
                "git_push".into(),
            ],
            activation_rules: vec![
                ActivationRule::Always,
            ],
            active: false,
            priority: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context(files: Vec<&str>, msg: &str) -> SkillContext {
        SkillContext {
            workspace_root: "/tmp/test".into(),
            current_files: files.into_iter().map(String::from).collect(),
            user_message: msg.into(),
            active_skills: Vec::new(),
        }
    }

    #[test]
    fn test_always_active() {
        let mut discovery = SkillDiscovery::new();
        discovery.register(Skill {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            tools: vec![],
            activation_rules: vec![ActivationRule::Always],
            active: false,
            priority: 0,
        });

        let ctx = test_context(vec![], "hello");
        let results = discovery.evaluate(&ctx);
        assert!(results[0].active);
    }

    #[test]
    fn test_never_active() {
        let mut discovery = SkillDiscovery::new();
        discovery.register(Skill {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            tools: vec![],
            activation_rules: vec![ActivationRule::Never],
            active: false,
            priority: 0,
        });

        let ctx = test_context(vec![], "hello");
        let results = discovery.evaluate(&ctx);
        assert!(!results[0].active);
    }

    #[test]
    fn test_keyword_activation() {
        let mut discovery = SkillDiscovery::new();
        discovery.register(Skill {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            tools: vec![],
            activation_rules: vec![ActivationRule::Keyword {
                words: vec!["test".into(), "coverage".into()],
            }],
            active: false,
            priority: 0,
        });

        let ctx = test_context(vec![], "please run the tests");
        let results = discovery.evaluate(&ctx);
        assert!(results[0].active);

        let ctx2 = test_context(vec![], "hello world");
        let results2 = discovery.evaluate(&ctx2);
        assert!(!results2[0].active);
    }

    #[test]
    fn test_file_path_activation() {
        let mut discovery = SkillDiscovery::new();
        discovery.register(Skill {
            id: "rust".into(),
            name: "Rust".into(),
            description: "".into(),
            tools: vec![],
            activation_rules: vec![ActivationRule::FilePath {
                pattern: "**/*.rs".into(),
            }],
            active: false,
            priority: 0,
        });

        let ctx = test_context(vec!["src/main.rs", "Cargo.toml"], "hello");
        let results = discovery.evaluate(&ctx);
        assert!(results[0].active);

        let ctx2 = test_context(vec!["src/index.ts"], "hello");
        let results2 = discovery.evaluate(&ctx2);
        assert!(!results2[0].active);
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(glob_match("**/*.rs", "src/main.rs"));
        assert!(glob_match("src/*.rs", "src/main.rs"));
        assert!(!glob_match("*.ts", "main.rs"));
        assert!(glob_match("*", "anything"));
    }

    #[test]
    fn test_diff() {
        let mut discovery = SkillDiscovery::new();
        discovery.register(Skill {
            id: "a".into(),
            name: "A".into(),
            description: "".into(),
            tools: vec![],
            activation_rules: vec![ActivationRule::Keyword {
                words: vec!["activate_a".into()],
            }],
            active: false,
            priority: 0,
        });
        discovery.register(Skill {
            id: "b".into(),
            name: "B".into(),
            description: "".into(),
            tools: vec![],
            activation_rules: vec![ActivationRule::Always],
            active: false,
            priority: 0,
        });

        // First evaluation: b activates
        let ctx = test_context(vec![], "hello");
        discovery.evaluate(&ctx);
        let diff = discovery.diff();
        assert_eq!(diff.activated, vec!["b"]);
        assert!(diff.deactivated.is_empty());

        // Second evaluation: a activates too
        discovery.previous_active = discovery.active_skills().iter().map(|s| s.id.clone()).collect();
        let ctx2 = test_context(vec![], "activate_a please");
        discovery.evaluate(&ctx2);
        let diff2 = discovery.diff();
        assert!(diff2.activated.contains(&"a".to_string()));
    }

    #[test]
    fn test_active_tools() {
        let mut discovery = SkillDiscovery::new();
        discovery.register(Skill {
            id: "rust".into(),
            name: "Rust".into(),
            description: "".into(),
            tools: vec!["cargo_build".into(), "cargo_test".into()],
            activation_rules: vec![ActivationRule::Always],
            active: false,
            priority: 0,
        });

        let ctx = test_context(vec![], "build");
        discovery.evaluate(&ctx);

        let tools = discovery.active_tools();
        assert!(tools.contains(&"cargo_build".to_string()));
        assert!(tools.contains(&"cargo_test".to_string()));
    }

    #[test]
    fn test_all_rule() {
        let mut discovery = SkillDiscovery::new();
        discovery.register(Skill {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            tools: vec![],
            activation_rules: vec![ActivationRule::All(vec![
                ActivationRule::FilePath { pattern: "*.rs".into() },
                ActivationRule::Keyword { words: vec!["build".into()] },
            ])],
            active: false,
            priority: 0,
        });

        // Both conditions met
        let ctx = test_context(vec!["main.rs"], "please build");
        let results = discovery.evaluate(&ctx);
        assert!(results[0].active);

        // Only one condition met
        let ctx2 = test_context(vec!["main.rs"], "hello");
        let results2 = discovery.evaluate(&ctx2);
        assert!(!results2[0].active);
    }

    #[test]
    fn test_not_rule() {
        let mut discovery = SkillDiscovery::new();
        discovery.register(Skill {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            tools: vec![],
            activation_rules: vec![ActivationRule::Not(Box::new(
                ActivationRule::Keyword { words: vec!["skip".into()] },
            ))],
            active: false,
            priority: 0,
        });

        let ctx = test_context(vec![], "hello");
        let results = discovery.evaluate(&ctx);
        assert!(results[0].active);

        let ctx2 = test_context(vec![], "skip this");
        let results2 = discovery.evaluate(&ctx2);
        assert!(!results2[0].active);
    }

    #[test]
    fn test_presets_rust() {
        let skill = presets::rust_dev();
        assert_eq!(skill.id, "rust_dev");
        assert!(skill.tools.contains(&"cargo_build".to_string()));
    }
}
