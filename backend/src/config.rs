use std::env;
use std::fs;

pub struct Config {
    /// Namespaces to scan. Empty = all namespaces.
    pub namespaces: Vec<String>,
    /// Label key used to select manageable workloads.
    pub selector_key: String,
    /// Label value that a workload must carry to be manageable.
    pub selector_value: String,
    /// Name of the ConfigMap that persists desired state.
    pub state_configmap: String,
    /// Namespace of the state ConfigMap (defaults to the pod's namespace).
    pub state_namespace: String,
}

impl Config {
    pub fn from_env() -> Self {
        let namespaces = env::var("NAMESPACES")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let selector_key = env::var("SELECTOR_KEY")
            .unwrap_or_else(|_| "replica-scaler.webui.io/managed".to_string());

        let selector_value = env::var("SELECTOR_VALUE").unwrap_or_else(|_| "true".to_string());

        let state_configmap = env::var("STATE_CONFIGMAP")
            .unwrap_or_else(|_| "replica-scaler-state".to_string());

        let state_namespace = env::var("STATE_NAMESPACE")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                fs::read_to_string(
                    "/var/run/secrets/kubernetes.io/serviceaccount/namespace",
                )
                .ok()
                .map(|s| s.trim().to_string())
            })
            .unwrap_or_else(|| "default".to_string());

        Self {
            namespaces,
            selector_key,
            selector_value,
            state_configmap,
            state_namespace,
        }
    }
}
