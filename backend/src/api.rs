use crate::config::Config;
use crate::k8s::KubeClient;
use crate::models::{ErrorResponse, ScaleRequest, Target};
use crate::state::StateStore;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::sync::Arc;

pub struct AppState {
    pub kube: KubeClient,
    pub state: StateStore,
    pub cfg: Config,
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, Json(self)).into_response()
    }
}

pub async fn health() -> &'static str {
    "ok"
}

pub async fn list_targets(
    State(app): State<Arc<AppState>>,
) -> Result<Json<Vec<Target>>, ErrorResponse> {
    let mut targets = app.kube.discover(&app.cfg).await;

    app.state.apply_defaults(&targets).await;

    for t in &mut targets {
        let key = StateStore::key(&t.kind, &t.namespace, &t.name);
        if let Some(desired) = app.state.get(&key) {
            t.desired_replicas = Some(desired);
        }
    }

    Ok(Json(targets))
}

pub async fn scale_target(
    State(app): State<Arc<AppState>>,
    Path((ns, name)): Path<(String, String)>,
    Json(req): Json<ScaleRequest>,
) -> Result<Json<Target>, ErrorResponse> {
    // Always only one instance: clamp to 0 or 1.
    let replicas = req.replicas.min(1);

    let targets = app.kube.discover(&app.cfg).await;
    let target = targets
        .into_iter()
        .find(|t| t.namespace == ns && t.name == name)
        .ok_or_else(|| ErrorResponse {
            error: format!(
                "Workload {}/{} not found (or not managed by this scaler)",
                ns, name
            ),
        })?;

    app.kube
        .scale(&ns, &name, &target.kind, replicas)
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to scale {} {}/{}: {}", target.kind, ns, name, e),
        })?;

    let key = StateStore::key(&target.kind, &ns, &name);
    if let Err(e) = app.state.set(&key, replicas as i64).await {
        tracing::warn!("Scaled but failed to persist desired state ({}): {}", key, e);
    }

    // Re-read the workload for a fresh status.
    let mut updated = app
        .kube
        .discover(&app.cfg)
        .await
        .into_iter()
        .find(|t| t.namespace == ns && t.name == name)
        .unwrap_or(target);
    updated.desired_replicas = Some(replicas as i64);

    Ok(Json(updated))
}
