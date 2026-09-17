# Replica Scaler WebUI - Development Guide

## Project Structure

```
replica-scaler-webui/
├── backend/                    # Axum REST API
│   └── src/
│       ├── main.rs             # Entry point, router setup
│       ├── api.rs              # HTTP handlers
│       ├── k8s.rs              # Kubernetes client (reqwest)
│       ├── state.rs            # Desired-state persistence (ConfigMap)
│       └── models.rs           # Data structures
├── frontend/                   # React + Vite + Tailwind (shadcn-style)
│   ├── index.html
│   ├── package.json
│   ├── vite.config.ts
│   ├── tailwind.config.ts
│   ├── tsconfig*.json
│   └── src/
│       ├── main.tsx
│       ├── App.tsx
│       ├── api.ts
│       ├── types.ts
│       ├── index.css
│       ├── lib/utils.ts
│       └── components/
│           ├── ui/             # shadcn-style primitives (button, badge, card)
│           └── targets-table.tsx
├── flake.nix                   # Nix dev shell
├── Dockerfile                  # Multi-stage build
└── example/                    # Example Flux HelmRelease
```

## Development Commands

```bash
cd replica-scaler-webui

# Enter dev shell
nix develop --accept-flake-config

# Backend
cargo check -p replica-scaler-webui-backend
cargo build -p replica-scaler-webui-backend

# Frontend
cd frontend && npm install && npm run dev

# Build frontend for production
cd frontend && npx tsc -b && npx vite build

# Format
cargo fmt
```

## Key Dependencies

### Backend
- `axum 0.7` - Web framework
- `tokio` - Async runtime
- `reqwest` - HTTP client (Kubernetes API)
- `serde`/`serde_json` - Serialization
- `tracing` - Logging
- `tower-http` - CORS middleware

### Frontend
- `react 18` - UI framework
- `vite 6` - Build tool
- `tailwindcss 3` - CSS framework
- `lucide-react` - Icons

## Kubernetes API Paths

| Endpoint | Purpose |
|----------|---------|
| `/apis/apps/v1/deployments` (+ `?labelSelector`/namespace) | List Deployments |
| `/apis/apps/v1/statefulsets` | List StatefulSets |
| `/apis/apps/v1/namespaces/{ns}/deployments/{name}` | Read a Deployment |
| `/apis/apps/v1/namespaces/{ns}/deployments/{name}/scale` | Scale a Deployment |
| `/apis/apps/v1/namespaces/{ns}/statefulsets/{name}/scale` | Scale a StatefulSet |
| `/api/v1/namespaces/{ns}/configmaps/{name}` | Persist desired state |
| `/api/v1/namespaces` | (optional) resolve namespace list |

## Docker Build

```bash
docker build -t replica-scaler-webui:latest .
docker run -p 8080:8080 replica-scaler-webui:latest
```

## Code Conventions

- Backend uses a custom `KubeError` enum (not `thiserror`) — same style as
  `volsync-webui`.
- All cluster access goes through `KubeClient::request()` (SA token + CA, retry
  with backoff, JSON body).
- Frontend uses React function components with hooks. All API responses are JSON.
- `replicas` is a clamped `u8` (`0` or `1`) at the API boundary.

## Common Issues

- **SSL errors**: System certs not in nix shell — use `nix develop --accept-flake-config`.
- **K8s client**: backend reads `KUBERNETES_SERVICE_HOST/PORT` and the
  service-account token/CA from `/var/run/secrets/kubernetes.io/serviceaccount/`.
  Outside the cluster, request paths still work but scaling fails (no RBAC).
- **Frontend dev**: run backend on port 8080; Vite proxies `/api` and `/health`.
