# Development

## Quick start

```bash
nix develop --accept-flake-config   # rustc, cargo, openssl, pkg-config, nodejs

# Backend
cargo check -p nvidia-replica-scaler-webui-backend

# Frontend (dev server proxies /api -> localhost:8080)
cd frontend && npm install && npm run dev

# Backend in another terminal
cargo run -p nvidia-replica-scaler-webui-backend

# Production frontend build
cd frontend && npx tsc -b && npx vite build
```

## Local (outside cluster) run

Without a `KUBERNETES_SERVICE_HOST`, the backend connects to
`http://localhost:8080` and has no bearer token. `/api/targets` will fail to
list any workloads, which is expected. Run it against a real cluster via
`kubectl port-forward` or run the container in-cluster.

## Code layout

- `backend/src/k8s.rs` — raw Kubernetes API client (SA token + CA, retries,
  `GET`/`PATCH`/`POST`). This is the only place that talks to the cluster.
- `backend/src/state.rs` — desired-replica persistence in a ConfigMap
  (`desired.json`), re-applied on startup.
- `backend/src/api.rs` — HTTP handlers + `AppState`.
- `frontend/src/components/targets-table.tsx` — the table UI. Buttons call
  `api.scaleTarget`.

## Changing the GPU heuristic

Edit `is_gpu()` in `backend/src/k8s.rs`. It returns `(matched, reason)` and is
also used to decide which workloads become scale targets. `INCLUDE_NON_GPU=true`
lists everything regardless.
