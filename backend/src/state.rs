use crate::k8s::{KubeClient, KubeError};
use crate::models::Target;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

const DATA_KEY: &str = "desired.json";

/// Persists the desired replica count per workload so it survives restarts and
/// workload recreation (e.g. a Deployment deleted and recreated by Flux defaults
/// back to 1 replica — we re-apply the persisted value on startup).
#[derive(Clone)]
pub struct StateStore {
    kube: KubeClient,
    configmap: String,
    namespace: String,
    /// key: "{kind_lower}/{namespace}/{name}" -> replicas
    map: Arc<RwLock<HashMap<String, i64>>>,
}

impl StateStore {
    pub fn new(kube: KubeClient, configmap: String, namespace: String) -> Self {
        Self {
            kube,
            configmap,
            namespace,
            map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn key(kind: &str, ns: &str, name: &str) -> String {
        let kind_lower = if kind == "StatefulSet" {
            "statefulset"
        } else {
            "deployment"
        };
        format!("{}/{}/{}", kind_lower, ns, name)
    }

    pub fn get(&self, key: &str) -> Option<i64> {
        let map = self.map.read().unwrap();
        map.get(key).copied()
    }

    pub async fn load(&self) -> Result<(), KubeError> {
        match self.kube.read_configmap(&self.namespace, &self.configmap).await {
            Ok(cm) => {
                if let Some(raw) = cm.pointer(&format!("/data/{}", DATA_KEY)).and_then(|v| v.as_str()) {
                    if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
                        let mut map = self.map.write().unwrap();
                        if let Some(obj) = parsed.as_object() {
                            for (k, v) in obj {
                                if let Some(r) = v.as_i64() {
                                    map.insert(k.clone(), r);
                                }
                            }
                        }
                        tracing::info!(
                            "Loaded {} desired replica entries from ConfigMap {}/{}",
                            map.len(),
                            self.namespace,
                            self.configmap
                        );
                    } else {
                        tracing::warn!("Could not parse {} in ConfigMap {}/{}", DATA_KEY, self.namespace, self.configmap);
                    }
                }
                Ok(())
            }
            Err(KubeError::NotFound(_)) => {
                tracing::info!("State ConfigMap {}/{} not found yet; starting with empty state", self.namespace, self.configmap);
                Ok(())
            }
            Err(e) => {
                tracing::warn!(
                    "Could not load state ConfigMap (continuing with empty state, scale actions will try to persist): {}",
                    e
                );
                Ok(())
            }
        }
    }

    pub async fn set(&self, key: &str, replicas: i64) -> Result<(), KubeError> {
        {
            let mut map = self.map.write().unwrap();
            map.insert(key.to_string(), replicas);
        }
        self.persist().await
    }

    /// Re-apply all persisted desired states to the cluster.
    pub async fn apply_desired(&self) {
        let snapshot = {
            let map = self.map.read().unwrap();
            map.clone()
        };
        for (key, replicas) in snapshot {
            let parts: Vec<&str> = key.splitn(3, '/').collect();
            if parts.len() != 3 {
                continue;
            }
            let kind = if parts[0] == "statefulset" {
                "StatefulSet"
            } else {
                "Deployment"
            };
            let ns = parts[1];
            let name = parts[2];
            match self.kube.scale(ns, name, kind, replicas.clamp(0, 1) as u8).await {
                Ok(_) => tracing::info!("Re-applied desired state: {} {}/{} -> {} replicas", kind, ns, name, replicas),
                Err(e) => tracing::warn!("Failed to re-apply desired state for {}/{}: {}", ns, name, e),
            }
        }
    }

    /// Apply the default state (label or global fallback) to any workload that
    /// has no persisted desired state yet, then persist it so it applies once.
    pub async fn apply_defaults(&self, targets: &[Target]) {
        for t in targets {
            let key = Self::key(&t.kind, &t.namespace, &t.name);
            if self.get(&key).is_some() {
                continue;
            }
            let Some(replicas) = t.default_replicas else {
                continue;
            };
            let clamped = replicas.clamp(0, 1);
            match self.kube.scale(&t.namespace, &t.name, &t.kind, clamped as u8).await {
                Ok(_) => {
                    if let Err(e) = self.set(&key, clamped).await {
                        tracing::warn!("Applied default state for {} but failed to persist: {}", key, e);
                    } else {
                        tracing::info!(
                            "Applied default state: {} {}/{} -> {} replicas",
                            t.kind, t.namespace, t.name, clamped
                        );
                    }
                }
                Err(e) => tracing::warn!("Failed to apply default state for {}/{}: {}", t.namespace, t.name, e),
            }
        }
    }

    async fn persist(&self) -> Result<(), KubeError> {
        let snapshot = {
            let map = self.map.read().unwrap();
            map.clone()
        };
        let mut obj = Map::new();
        for (k, v) in snapshot {
            obj.insert(k, serde_json::json!(v));
        }
        let raw = serde_json::json!(obj).to_string();
        let data = serde_json::json!({ DATA_KEY: raw });
        match self.kube.write_configmap(&self.namespace, &self.configmap, data).await {
            Ok(_) => Ok(()),
            Err(e) => {
                tracing::error!("Failed to persist desired state to ConfigMap {}/{}: {}", self.namespace, self.configmap, e);
                Err(e)
            }
        }
    }
}
