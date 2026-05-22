use serde_json::Value;
use std::collections::HashSet;

pub struct CrdtManager;

impl CrdtManager {
    pub fn new() -> Self {
        Self
    }

    pub fn merge(local: &Value, remote: &Value) -> Value {
        match (local, remote) {
            (Value::Object(local_map), Value::Object(remote_map)) => {
                let mut merged = local_map.clone();
                for (key, remote_val) in remote_map {
                    match merged.get(key) {
                        Some(local_val) => {
                            merged.insert(key.clone(), Self::merge(local_val, remote_val));
                        }
                        None => {
                            merged.insert(key.clone(), remote_val.clone());
                        }
                    }
                }
                Value::Object(merged)
            }
            (Value::Array(local_arr), Value::Array(remote_arr)) => {
                Self::merge_arrays(local_arr, remote_arr)
            }
            (Value::Object(local_map), _) => {
                let merged = local_map.clone();
                if let Some(ts) = remote.get("_timestamp").and_then(|v| v.as_u64()) {
                    if let Some(local_ts) = merged.get("_timestamp").and_then(|v| v.as_u64()) {
                        if ts >= local_ts {
                            return remote.clone();
                        }
                    }
                }
                Value::Object(merged)
            }
            _ => remote.clone(),
        }
    }

    fn merge_arrays(local: &[Value], remote: &[Value]) -> Value {
        let mut seen = HashSet::new();
        let mut result = Vec::new();

        for item in local {
            let key = Self::value_key(item);
            if seen.insert(key) {
                result.push(item.clone());
            }
        }

        for item in remote {
            let key = Self::value_key(item);
            if seen.insert(key) {
                result.push(item.clone());
            }
        }

        Value::Array(result)
    }

    fn value_key(v: &Value) -> String {
        match v {
            Value::String(s) => format!("s:{}", s),
            Value::Number(n) => format!("n:{}", n),
            Value::Bool(b) => format!("b:{}", b),
            Value::Null => "null".to_string(),
            other => serde_json::to_string(other).unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_merge_objects_no_conflict() {
        let local = json!({"a": 1});
        let remote = json!({"b": 2});
        let result = CrdtManager::merge(&local, &remote);
        assert_eq!(result["a"], 1);
        assert_eq!(result["b"], 2);
    }

    #[test]
    fn test_merge_objects_nested() {
        let local = json!({"config": {"a": 1, "b": 2}});
        let remote = json!({"config": {"b": 3, "c": 4}});
        let result = CrdtManager::merge(&local, &remote);
        let config = result.get("config").unwrap();
        assert_eq!(config["a"], 1);
        assert_eq!(config["c"], 4);
    }

    #[test]
    fn test_merge_arrays_dedup() {
        let local = json!([1, 2, 3]);
        let remote = json!([2, 3, 4]);
        let result = CrdtManager::merge(&local, &remote);
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 4);
    }

    #[test]
    fn test_merge_scalar_remote_wins() {
        let local = json!("old");
        let remote = json!("new");
        let result = CrdtManager::merge(&local, &remote);
        assert_eq!(result, json!("new"));
    }
}
