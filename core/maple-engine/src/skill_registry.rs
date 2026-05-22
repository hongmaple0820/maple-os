use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;

pub struct SkillRegistry {
    skills: Arc<RwLock<HashMap<String, Box<dyn Skill + Send + Sync>>>>,
}

pub trait Skill: Send + Sync {
    fn id(&self) -> &str;
    fn description(&self) -> &str;
    fn execute(&self, config: &Value) -> Result<Value>;
}

#[allow(dead_code)]
struct BuiltinSkill {
    id: String,
    description: String,
    handler: fn(&Value) -> Result<Value>,
}

impl Skill for BuiltinSkill {
    fn id(&self) -> &str { &self.id }
    fn description(&self) -> &str { &self.description }
    fn execute(&self, config: &Value) -> Result<Value> {
        (self.handler)(config)
    }
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, skill: Box<dyn Skill + Send + Sync>) {
        let mut skills = self.skills.write().await;
        let id = skill.id().to_string();
        skills.insert(id, skill);
    }

    pub async fn execute(&self, skill_id: &str, config: &Value) -> Result<Value> {
        let skills = self.skills.read().await;
        let skill = skills.get(skill_id)
            .ok_or_else(|| anyhow::anyhow!("Skill not found: {}", skill_id))?;
        skill.execute(config)
    }

    pub async fn list(&self) -> Vec<(String, String)> {
        let skills = self.skills.read().await;
        skills.values().map(|s| (s.id().to_string(), s.description().to_string())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestSkill {
        id: String,
        desc: String,
    }

    impl TestSkill {
        fn new(id: &str, desc: &str) -> Self {
            Self { id: id.to_string(), desc: desc.to_string() }
        }
    }

    impl Skill for TestSkill {
        fn id(&self) -> &str { &self.id }
        fn description(&self) -> &str { &self.desc }
        fn execute(&self, config: &Value) -> Result<Value> {
            Ok(serde_json::json!({ "skill": self.id, "input": config }))
        }
    }

    #[tokio::test]
    async fn test_register_and_execute() {
        let registry = SkillRegistry::new();
        registry.register(Box::new(TestSkill::new("echo", "Echo skill"))).await;

        let result = registry.execute("echo", &serde_json::json!({"msg": "hello"})).await.unwrap();
        assert_eq!(result["skill"], "echo");
        assert_eq!(result["input"]["msg"], "hello");
    }

    #[tokio::test]
    async fn test_skill_not_found() {
        let registry = SkillRegistry::new();
        let result = registry.execute("nonexistent", &Value::Null).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_skills() {
        let registry = SkillRegistry::new();
        registry.register(Box::new(TestSkill::new("a", "Skill A"))).await;
        registry.register(Box::new(TestSkill::new("b", "Skill B"))).await;

        let list = registry.list().await;
        assert_eq!(list.len(), 2);
    }
}
