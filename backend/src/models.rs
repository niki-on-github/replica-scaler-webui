use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    /// Workload kind: "Deployment" or "StatefulSet".
    pub kind: String,
    pub name: String,
    pub namespace: String,
    /// spec.replicas (absent when the manifest does not set it — the web UI owns it).
    pub replicas: Option<i64>,
    pub ready_replicas: Option<i64>,
    pub available_replicas: Option<i64>,
    /// Persisted desired state (what the web UI last requested).
    pub desired_replicas: Option<i64>,
    /// True when the workload matched the GPU heuristic.
    pub gpu: bool,
    /// Why it was matched (e.g. "runtimeClassName=nvidia").
    pub match_reason: String,
    /// "running" | "stopped" | "pending".
    pub state: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScaleRequest {
    /// Clamped to 0 or 1 by the server.
    pub replicas: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}
