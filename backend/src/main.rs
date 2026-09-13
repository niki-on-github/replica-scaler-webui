mod api;
mod config;
mod k8s;
mod models;
mod state;

use api::AppState;
use axum::{Router, body::Body, extract::Request, response::Response};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Serve static files from the `public/` directory with SPA fallback.
async fn serve_static(req: Request) -> Response {
    let path = req.uri().path().to_string();

    tracing::debug!("Serving static file: {}", path);

    if path.contains("..") {
        tracing::debug!("Path containing '..' rejected: {}", path);
        return serve_index().await;
    }
    let clean = path.trim_start_matches('/');
    if clean.is_empty() {
        tracing::debug!("Fallback to index.html (empty or invalid path)");
        return serve_index().await;
    }

    let file_path = format!("public/{}", clean);
    let mime_type = match file_path.rsplit('.').next().unwrap() {
        "html" => "text/html",
        "js" => "application/javascript",
        "wasm" => "application/wasm",
        "css" => "text/css",
        _ => "application/octet-stream",
    };
    match tokio::fs::read(&file_path).await {
        Ok(contents) => {
            tracing::debug!("Served static file: {}", file_path);
            Response::builder()
                .status(200)
                .header("Content-Type", mime_type)
                .body(Body::from(contents))
                .unwrap()
        }
        Err(_) => {
            tracing::debug!("Static file not found, fallback to index.html: {}", file_path);
            serve_index().await
        }
    }
}

/// Serve index.html for SPA client-side routing fallback.
async fn serve_index() -> Response {
    match tokio::fs::read("public/index.html").await {
        Ok(contents) => {
            tracing::debug!("Served index.html");
            Response::builder()
                .status(200)
                .header("Content-Type", "text/html")
                .body(Body::from(contents))
                .unwrap()
        }
        Err(e) => {
            tracing::error!("index.html not found in public/ directory: {}", e);
            Response::builder()
                .status(404)
                .body(Body::from("Not found"))
                .unwrap()
        }
    }
}

#[tokio::main]
async fn main() {
    let rust_log = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into());
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(rust_log.clone()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting NVidia Replica Scaler WebUI (RUST_LOG={})", rust_log);

    let cfg = config::Config::from_env();
    tracing::info!(
        "Config: namespaces={:?} gpu_runtime_class={} include_non_gpu={} state_configmap={} state_namespace={}",
        cfg.namespaces,
        cfg.gpu_runtime_class,
        cfg.include_non_gpu,
        cfg.state_configmap,
        cfg.state_namespace
    );

    let kube = Arc::new(
        k8s::KubeClient::new()
            .await
            .unwrap_or_else(|e| {
                tracing::error!("Failed to create Kubernetes client: {}", e);
                std::process::exit(1);
            }),
    );
    kube.check_rbac(&cfg).await;

    let state = Arc::new(state::StateStore::new(
        (*kube).clone(),
        cfg.state_configmap.clone(),
        cfg.state_namespace.clone(),
    ));
    if let Err(e) = state.load().await {
        tracing::warn!("Could not load initial state: {}", e);
    }
    state.apply_desired().await;

    let app_state = Arc::new(AppState {
        kube: (*kube).clone(),
        state: (*state).clone(),
        cfg,
    });

    // Allow all CORS origins — the frontend is served from the same origin and
    // restricting would complicate access via ingress paths / port-forwarding.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", axum::routing::get(api::health))
        .route("/api/targets", axum::routing::get(api::list_targets))
        .route(
            "/api/targets/:namespace/:name/scale",
            axum::routing::post(api::scale_target),
        )
        .fallback(axum::routing::get(serve_static))
        .with_state(app_state)
        .layer(cors);

    let listener = match tokio::net::TcpListener::bind("0.0.0.0:8080").await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Failed to bind to port 8080: {}", e);
            std::process::exit(1);
        }
    };
    let addr = listener.local_addr().unwrap();
    tracing::info!("Server listening on http://{}", addr);
    tracing::info!("Available routes:");
    tracing::info!("  GET  /health");
    tracing::info!("  GET  /api/targets");
    tracing::info!("  POST /api/targets/:namespace/:name/scale");
    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("Server error: {}", e);
        std::process::exit(1);
    }
}
