use std::env;
use std::fs;

pub struct Config {
    /// Namespaces to scan. Empty = all namespaces.
    pub namespaces: Vec<String>,
    /// Label key used to select manageable workloads.
    pub selector_key: String,
    /// Label value that a workload must carry to be manageable.
    pub selector_value: String,
    /// Label key describing the initial desired state ("active"/"inactive").
    pub default_state_label: String,
    /// Fallback desired replicas (0 or 1) for workloads with no default-state label.
    pub default_state: i64,
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

        let default_state_label = env::var("DEFAULT_STATE_LABEL")
            .unwrap_or_else(|_| "replica-scaler.webui.io/default-state".to_string());

        // Unset or unrecognized DEFAULT_STATE falls back to inactive (0).
        let default_state = env::var("DEFAULT_STATE")
            .ok()
            .and_then(|s| parse_state(&s))
            .unwrap_or(0);

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
            default_state_label,
            default_state,
            state_configmap,
            state_namespace,
        }
    }
}

/// Parse a default-state value. Accepts `active`/`inactive` and `1`/`0`.
pub fn parse_state(value: &str) -> Option<i64> {
    match value.trim().to_ascii_lowercase().as_str() {
        "active" | "1" | "true" | "on" => Some(1),
        "inactive" | "0" | "false" | "off" => Some(0),
        _ => None,
    }
}
