use crate::webdav::WebDavClient;
use crate::crdt::CrdtManager;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;

const SYNC_REMOTE_PATH: &str = "/mapleos/sync/state.json";
const SYNC_META_PATH: &str = "/mapleos/sync/meta.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMeta {
    pub last_sync_version: u64,
    pub last_sync_at: chrono::DateTime<chrono::Utc>,
    pub local_version: u64,
}

#[derive(Debug, Clone)]
pub struct SyncResult {
    pub pushed_count: usize,
    pub pulled_count: usize,
    pub conflicts: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum ConflictResolution {
    LocalWins,
    RemoteWins,
    Merged,
}

#[derive(Clone)]
pub struct SyncEngine {
    local_state: Arc<RwLock<serde_json::Map<String, Value>>>,
    webdav: Option<WebDavClient>,
    sync_interval_secs: u64,
    pending_changes: Arc<RwLock<Vec<String>>>,
    meta: Arc<RwLock<SyncMeta>>,
}

impl SyncEngine {
    pub fn new(webdav: Option<WebDavClient>, sync_interval_secs: u64) -> Self {
        let meta = SyncMeta {
            last_sync_version: 0,
            last_sync_at: chrono::Utc::now(),
            local_version: 0,
        };
        Self {
            local_state: Arc::new(RwLock::new(serde_json::Map::new())),
            webdav,
            sync_interval_secs,
            pending_changes: Arc::new(RwLock::new(Vec::new())),
            meta: Arc::new(RwLock::new(meta)),
        }
    }

    pub async fn write(&self, key: &str, value: Value) -> Result<()> {
        let mut state = self.local_state.write().await;
        state.insert(key.to_string(), value);

        let mut pending = self.pending_changes.write().await;
        if !pending.contains(&key.to_string()) {
            pending.push(key.to_string());
        }

        let mut meta = self.meta.write().await;
        meta.local_version += 1;

        Ok(())
    }

    pub async fn read(&self, key: &str) -> Option<Value> {
        let state = self.local_state.read().await;
        state.get(key).cloned()
    }

    pub async fn full_sync(&self) -> Result<SyncResult> {
        let Some(dav) = &self.webdav else {
            return Ok(SyncResult {
                pushed_count: 0,
                pulled_count: 0,
                conflicts: 0,
            });
        };

        let pending_keys: Vec<String> = {
            let mut p = self.pending_changes.write().await;
            std::mem::take(&mut *p)
        };

        let remote_state = match Self::fetch_remote(dav).await {
            Ok(Some(state)) => Some(state),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!("Sync pull failed: {}", e);
                let mut p = self.pending_changes.write().await;
            p.extend(pending_keys.clone());
                return Ok(SyncResult {
                    pushed_count: 0,
                    pulled_count: 0,
                    conflicts: 0,
                });
            }
        };

        let local_state = self.local_state.read().await;
        let (merged_state, pulled_count, conflicts) = match remote_state {
            Some(remote) => {
                let (merged, pulled, conf) = self.merge_states(&local_state, &remote, &pending_keys);
                (merged, pulled, conf)
            }
            None => (local_state.clone(), 0, 0),
        };
        drop(local_state);

        {
            let mut state = self.local_state.write().await;
            *state = merged_state.clone();
        }

        let pushed_count = pending_keys.len();

        let payload = serde_json::to_vec(&Value::Object(merged_state))?;
        if let Err(e) = dav.put(SYNC_REMOTE_PATH, &payload).await {
            tracing::warn!("Sync push failed: {}", e);
            let mut p = self.pending_changes.write().await;
            p.extend(pending_keys);
        } else {
            tracing::info!(
                pushed = pushed_count,
                pulled = pulled_count,
                conflicts,
                "Sync completed"
            );
        }

        {
            let mut meta = self.meta.write().await;
            meta.last_sync_version = meta.local_version;
            meta.last_sync_at = chrono::Utc::now();
        }

        let _ = self.persist_meta(dav).await;

        Ok(SyncResult {
            pushed_count,
            pulled_count,
            conflicts,
        })
    }

    async fn fetch_remote(dav: &WebDavClient) -> Result<Option<serde_json::Map<String, Value>>> {
        if !dav.exists(SYNC_REMOTE_PATH).await? {
            return Ok(None);
        }
        let data = dav.get(SYNC_REMOTE_PATH).await?;
        let value: Value = serde_json::from_slice(&data)?;
        match value {
            Value::Object(map) => Ok(Some(map)),
            _ => Ok(None),
        }
    }

    fn merge_states(
        &self,
        local: &serde_json::Map<String, Value>,
        remote: &serde_json::Map<String, Value>,
        pending_keys: &[String],
    ) -> (serde_json::Map<String, Value>, usize, usize) {
        let pending_set: std::collections::HashSet<&String> = pending_keys.iter().collect();
        let mut merged = local.clone();
        let mut pulled_count = 0;
        let mut conflicts = 0;

        for (key, remote_val) in remote {
            match (merged.get(key), pending_set.contains(key)) {
                (Some(local_val), true) => {
                    if local_val != remote_val {
                        let resolution = Self::resolve_conflict(local_val, remote_val);
                        match resolution {
                            ConflictResolution::LocalWins => {}
                            ConflictResolution::RemoteWins => {
                                merged.insert(key.clone(), remote_val.clone());
                            }
                            ConflictResolution::Merged => {
                                let merged_val = CrdtManager::merge(local_val, remote_val);
                                merged.insert(key.clone(), merged_val);
                            }
                        }
                        conflicts += 1;
                    }
                }
                (Some(_local_val), false) => {
                    merged.insert(key.clone(), remote_val.clone());
                    pulled_count += 1;
                }
                (None, _) => {
                    merged.insert(key.clone(), remote_val.clone());
                    pulled_count += 1;
                }
            }
        }

        (merged, pulled_count, conflicts)
    }

    fn resolve_conflict(local: &Value, remote: &Value) -> ConflictResolution {
        match (local, remote) {
            (Value::Object(_), Value::Object(_)) => ConflictResolution::Merged,
            (Value::Array(_), Value::Array(_)) => ConflictResolution::Merged,
            (Value::Number(ln), Value::Number(rn)) => {
                if let (Some(l), Some(r)) = (ln.as_f64(), rn.as_f64()) {
                    if r > l {
                        return ConflictResolution::RemoteWins;
                    }
                }
                ConflictResolution::LocalWins
            }
            (Value::String(ls), Value::String(rs)) => {
                if ls.len() > rs.len() {
                    ConflictResolution::LocalWins
                } else {
                    ConflictResolution::RemoteWins
                }
            }
            _ => ConflictResolution::LocalWins,
        }
    }

    async fn persist_meta(&self, dav: &WebDavClient) -> Result<()> {
        let meta = self.meta.read().await;
        let payload = serde_json::to_vec(&*meta)?;
        dav.put(SYNC_META_PATH, &payload).await
    }

    pub async fn start_background_sync(&self) {
        let interval = self.sync_interval_secs;
        let engine = self.clone();

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval));
            loop {
                ticker.tick().await;
                let pending_count = {
                    let p = engine.pending_changes.read().await;
                    p.len()
                };
                if pending_count > 0 {
                    tracing::info!(count = pending_count, "Background sync triggered");
                    match engine.full_sync().await {
                        Ok(result) => {
                            tracing::info!(
                                pushed = result.pushed_count,
                                pulled = result.pulled_count,
                                conflicts = result.conflicts,
                                "Background sync completed"
                            );
                        }
                        Err(e) => {
                            tracing::warn!("Background sync failed: {}", e);
                        }
                    }
                }
            }
        });
    }
}
