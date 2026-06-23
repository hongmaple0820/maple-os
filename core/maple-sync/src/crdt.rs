use serde_json::Value;
use std::collections::HashSet;
use automerge::AutoCommit;

pub struct CrdtManager {
    doc: AutoCommit,
}

impl Default for CrdtManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CrdtManager {
    pub fn new() -> Self {
        Self { doc: AutoCommit::new() }
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
                if let Some(ts) = remote.get("_timestamp").and_then(|v| v.as_u64())
                    && let Some(local_ts) = merged.get("_timestamp").and_then(|v| v.as_u64())
                    && ts >= local_ts
                {
                    return remote.clone();
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

    pub fn create_automerge_doc(&mut self, key: &str, value: &Value) {
        use automerge::transaction::Transactable;
        self.doc.put(automerge::ROOT, key, Self::json_to_automerge_value(value)).ok();
    }

    pub fn get_automerge_doc(&mut self) -> Vec<u8> {
        self.doc.save()
    }

    pub fn merge_automerge_doc(&mut self, remote: &[u8]) {
        self.doc.load_incremental(remote).ok();
    }

    /// #70: Read a value back from the Automerge doc as JSON.
    pub fn get_json_from_doc(&mut self, key: &str) -> Value {
        // AutoCommit implements ReadDoc which has get()
        use automerge::ReadDoc;
        match self.doc.get(automerge::ROOT, key) {
            Ok(Some((automerge::Value::Scalar(scalar), _))) => {
                Self::automerge_value_to_json(&scalar)
            }
            _ => Value::Null,
        }
    }

    fn automerge_value_to_json(scalar: &automerge::ScalarValue) -> Value {
        match scalar {
            automerge::ScalarValue::Str(s) => Value::String(s.to_string()),
            automerge::ScalarValue::Int(i) => Value::Number((*i).into()),
            automerge::ScalarValue::Uint(u) => Value::Number((*u).into()),
            automerge::ScalarValue::F64(f) => {
                serde_json::Number::from_f64(*f).map(Value::Number).unwrap_or(Value::Null)
            }
            automerge::ScalarValue::Boolean(b) => Value::Bool(*b),
            automerge::ScalarValue::Null => Value::Null,
            _ => Value::Null,
        }
    }

    fn json_to_automerge_value(v: &Value) -> automerge::ScalarValue {
        match v {
            Value::String(s) => automerge::ScalarValue::Str(s.clone().into()),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    automerge::ScalarValue::Int(i)
                } else if let Some(u) = n.as_u64() {
                    automerge::ScalarValue::Uint(u)
                } else {
                    automerge::ScalarValue::F64(n.as_f64().unwrap_or(0.0))
                }
            }
            Value::Bool(b) => automerge::ScalarValue::Boolean(*b),
            Value::Null => automerge::ScalarValue::Null,
            _ => automerge::ScalarValue::Null,
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

    #[test]
    fn test_automerge_doc_sync() {
        let mut local = CrdtManager::new();
        local.create_automerge_doc("version", &json!("0.1.0"));
        local.create_automerge_doc("count", &json!(42));

        let mut remote = CrdtManager::new();
        remote.create_automerge_doc("name", &json!("MapleOS"));
        let remote_bytes = remote.get_automerge_doc();

        local.merge_automerge_doc(&remote_bytes);
        assert!(!local.get_automerge_doc().is_empty());
    }
}