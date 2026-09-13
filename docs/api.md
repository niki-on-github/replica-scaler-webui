# NVidia Replica Scaler WebUI - API

## `GET /health`

Liveness probe. Returns `ok`.

## `GET /api/targets`

Lists all discovered GPU workloads (Deployments and StatefulSets), including
those currently at `replicas: 0`.

Response: `200 OK`

```json
[
  {
    "kind": "Deployment",
    "name": "vllm-dsv4",
    "namespace": "gpu",
    "replicas": 0,
    "ready_replicas": 0,
    "available_replicas": 0,
    "desired_replicas": 1,
    "gpu": true,
    "match_reason": "runtimeClassName=nvidia",
    "state": "stopped"
  }
]
```

| Field | Description |
|-------|-------------|
| `replicas` | `.spec.replicas` (may be `null` when the manifest does not set it) |
| `ready_replicas` / `available_replicas` | status fields |
| `desired_replicas` | the persisted replica count requested via the UI (`null` = never scaled) |
| `gpu` | whether the GPU heuristic matched |
| `match_reason` | what triggered the match, e.g. `runtimeClassName=nvidia`, `env NVIDIA_VISIBLE_DEVICES`, `resources.limits.nvidia.com/gpu` |
| `state` | `running` \| `stopped` \| `pending` |

## `POST /api/targets/{namespace}/{name}/scale`

Scales a workload up (1) or down (0). The value is clamped to `0..=1` — you can
never run more than one instance via this API.

Request body:

```json
{ "replicas": 1 }
```

Response: `200 OK` with the refreshed target object.

Errors: `400` with `{ "error": "..." }` if the workload does not exist (or does
not match the GPU filter) or the scale PATCH failed.
