use std::env;
use std::fs;

pub struct Config {
    /// Namespaces to scan. Empty = all namespaces.
    pub namespaces: Vec<String>,
    /// RuntimeClassName used in the GPU heuristic.
    pub gpu_runtime_class: String,
    /// Also list workloads that are not detected as GPU.
    pub include_non_gpu: bool,
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

        let gpu_runtime_class =
            env::var("GPU_RUNTIME_CLASS").unwrap_or_else(|_| "nvidia".to_string());

        let include_non_gpu = env::var("INCLUDE_NON_GPU")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

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
            gpu_runtime_class,
            include_non_gpu,
            state_configmap,
            state_namespace,
        }
    }
}
