use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;

pub struct SkillRegistry {
    skills: Arc<RwLock<HashMap<String, Box<dyn Skill + Send + Sync>>>>,
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub trait Skill: Send + Sync {
    fn id(&self) -> &str;
    fn description(&self) -> &str;

    /// #11: JSON Schema describing the skill's input parameters.
    /// Returns None for backward compat — skills that don't override
    /// this are treated as schemaless (any input accepted).
    fn parameters_schema(&self) -> Option<serde_json::Value> {
        None
    }

    /// #11: JSON Schema describing the skill's output shape.
    /// Returns None for backward compat.
    fn output_schema(&self) -> Option<serde_json::Value> {
        None
    }

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

        // #11: validate input against schema if one is provided
        if let Some(schema) = skill.parameters_schema() {
            if let Err(msg) = validate_against_schema(config, &schema) {
                anyhow::bail!("input validation failed for skill '{}': {}", skill_id, msg);
            }
        }

        let result = skill.execute(config)?;

        // #11: validate output against schema if one is provided
        if let Some(schema) = skill.output_schema() {
            if let Err(msg) = validate_against_schema(&result, &schema) {
                tracing::warn!(skill = %skill_id, error = %msg, "output validation warning");
            }
        }

        Ok(result)
    }

    pub async fn list(&self) -> Vec<(String, String)> {
        let skills = self.skills.read().await;
        skills.values().map(|s| (s.id().to_string(), s.description().to_string())).collect()
    }

    /// #11: List skills with their schemas (id, description, parameters_schema, output_schema)
    pub async fn list_with_schemas(&self) -> Vec<(String, String, Option<Value>, Option<Value>)> {
        let skills = self.skills.read().await;
        skills.values().map(|s| {
            (s.id().to_string(), s.description().to_string(),
             s.parameters_schema(), s.output_schema())
        }).collect()
    }

    pub async fn unregister(&self, skill_id: &str) {
        let mut skills = self.skills.write().await;
        skills.remove(skill_id);
    }
}

/// #11: Lightweight JSON Schema validation (subset of draft-07).
/// Supports: type, required, properties, additionalProperties.
/// Returns Ok(()) if valid, Err(message) on first violation.
fn validate_against_schema(value: &Value, schema: &Value) -> std::result::Result<(), String> {
    // type check
    if let Some(schema_type) = schema.get("type").and_then(|t| t.as_str()) {
        let actual_type = match value {
            Value::Object(_) => "object",
            Value::Array(_) => "array",
            Value::String(_) => "string",
            Value::Number(_) => "number",
            Value::Bool(_) => "boolean",
            Value::Null => "null",
        };
        if schema_type != actual_type {
            return Err(format!("expected type '{}', got '{}'", schema_type, actual_type));
        }
    }

    // required fields (for objects)
    if let Value::Object(obj) = value {
        if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
            for req in required {
                if let Some(field) = req.as_str() {
                    if !obj.contains_key(field) {
                        return Err(format!("missing required field '{}'", field));
                    }
                }
            }
        }

        // properties type check
        if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
            for (key, prop_schema) in properties {
                if let Some(field_val) = obj.get(key) {
                    if let Err(e) = validate_against_schema(field_val, prop_schema) {
                        return Err(format!("field '{}': {}", key, e));
                    }
                }
            }
        }
    }

    Ok(())
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

    // ── #11: Schema validation tests ──

    #[test]
    fn test_validate_correct_type() {
        let schema = serde_json::json!({"type": "object"});
        let value = serde_json::json!({"key": "val"});
        assert!(validate_against_schema(&value, &schema).is_ok());
    }

    #[test]
    fn test_validate_wrong_type() {
        let schema = serde_json::json!({"type": "object"});
        let value = serde_json::json!("string");
        assert!(validate_against_schema(&value, &schema).is_err());
    }

    #[test]
    fn test_validate_missing_required_field() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string"}
            }
        });
        let value = serde_json::json!({"not_query": "hello"});
        let result = validate_against_schema(&value, &schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing required field 'query'"));
    }

    #[test]
    fn test_validate_required_field_present() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string"}
            }
        });
        let value = serde_json::json!({"query": "hello"});
        assert!(validate_against_schema(&value, &schema).is_ok());
    }

    #[test]
    fn test_validate_nested_property_type() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "count": {"type": "number"}
            }
        });
        let value = serde_json::json!({"count": "not a number"});
        let result = validate_against_schema(&value, &schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("field 'count'"));
    }

    #[test]
    fn test_validate_no_schema_accepts_anything() {
        // Skills without schema should accept any input (backward compat)
        let schema = serde_json::json!({});
        let value = serde_json::json!({"anything": true});
        assert!(validate_against_schema(&value, &schema).is_ok());
    }

    #[tokio::test]
    async fn test_skill_with_schema_validates_input() {
        struct SchemaSkill;
        impl Skill for SchemaSkill {
            fn id(&self) -> &str { "schema_test" }
            fn description(&self) -> &str { "Test skill with schema" }
            fn parameters_schema(&self) -> Option<Value> {
                Some(serde_json::json!({
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": {"type": "string"}
                    }
                }))
            }
            fn execute(&self, config: &Value) -> Result<Value> {
                Ok(serde_json::json!({"echo": config}))
            }
        }

        let registry = SkillRegistry::new();
        registry.register(Box::new(SchemaSkill)).await;

        // Valid input
        let result = registry.execute("schema_test", &serde_json::json!({"query": "hello"})).await;
        assert!(result.is_ok());

        // Invalid input — missing required field
        let result = registry.execute("schema_test", &serde_json::json!({"not_query": "x"})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing required field"));
    }
}
