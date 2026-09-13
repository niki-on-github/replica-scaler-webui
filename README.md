# NVidia Replica Scaler WebUI

A web-based on/off switch for **GPU workloads** (Deployments and StatefulSets)
on Kubernetes. Start (1 replica) / Stop (0 replicas) any workload that uses a
GPU, with a simple table UI.

The container **auto-discovers** GPU workloads from inside the cluster — no
hardcoded target list. Workloads sitting at `replicas: 0` are still detected and
listed, so you can start them from the UI.

> [!WARNING]
> The code was specifically designed for my k8s setup (Flux GitOps + bjw-s
> `app-template` HelmReleases). It is not a universal tool. Review the
> conditions carefully or fork with the necessary modifications.

## Features

- **Auto-discovery**: lists every Deployment/StatefulSet that looks like a GPU
  workload (see [GPU detection](#gpu-detection)), regardless of its current
  replica count.
- **Start / Stop**: scale any discovered workload to `1` or `0` replicas via the
  scale subresource. `replicas` is clamped to `{0, 1}` — never more than one.
- **Persisted desired state**: the last requested replica count is stored in a
  ConfigMap and re-applied on startup, so a workload that gets deleted and
  recreated (Flux prune / helm upgrade) doesn't drift back to its chart default.
- **Auto-refresh**: the dashboard polls the Kubernetes API periodically.

## GPU detection

A workload is considered a GPU workload if its pod template matches **any** of:

1. `spec.template.spec.runtimeClassName` equals `GPU_RUNTIME_CLASS`
   (default `nvidia`), **or**
2. any container env var is named `NVIDIA_VISIBLE_DEVICES`, **or**
3. any container resource `limits`/`requests` key (lowercased) contains `gpu`
   (covers `nvidia.com/gpu`, `intel.com/gpu`, ...).

Only matching workloads are listed by default. Set `INCLUDE_NON_GPU=true` to
list everything. The matched reason is logged and shown in the UI.

## Deployment

An example Flux deployment lives in `./example/`.

### Kubernetes

The container expects to run inside the cluster with a ServiceAccount that can
read workloads and scale them.

```bash
kubectl apply -f example/helm-release.yaml   # adjust RBAC / targets first
```

### RBAC Permissions

Minimal `ClusterRole` used by the ServiceAccount (see
`example/helm-release.yaml`):

| Resource | Verbs |
|---|---|
| `deployments`, `deployments/status`, `deployments/scale` | `get, list, patch, update` |
| `statefulsets`, `statefulsets/status`, `statefulsets/scale` | `get, list, patch, update` |
| `configmaps` (own state) | `get, list, create, update, patch` |
| `pods` | `get, list` |
| `persistentvolumeclaims` | `get, list` |

The app runs a startup RBAC check that probes each endpoint and logs whether the
permissions are present (non-fatal).

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Logging level (`trace`, `debug`, `info`, `warn`, `error`) |
| `NAMESPACES` | *(all)* | Comma-separated namespaces to scan. Empty = all namespaces |
| `GPU_RUNTIME_CLASS` | `nvidia` | Runtime class name used in the GPU heuristic |
| `INCLUDE_NON_GPU` | `false` | Also list workloads that are not detected as GPU |
| `STATE_CONFIGMAP` | `replica-scaler-state` | Name of the ConfigMap that persists desired state |
| `STATE_NAMESPACE` | *(pod's own namespace)* | Namespace of the state ConfigMap |
| `KUBERNETES_SERVICE_HOST` | auto | Kubernetes API host (auto-detected in cluster) |

## Interplay with Flux GitOps

The whole point of this tool is to scale to `0` **without fighting Flux**.

Because Flux (server-side apply) reverts any field it owns to the git-declared
value, the *workload manifests themselves* must **not** declare `spec.replicas`.
With the bjw-s `app-template` Helm chart this is done by setting:

```yaml
controllers:
  my-workload:
    replicas: null   # chart renders NO spec.replicas → Flux doesn't own it
```

The same applies to any HPA-driven deployment. When `spec.replicas` is absent
from the manifest, Flux/Helm releases ownership of the field and this web UI's
patches to `.spec.replicas` (via the `scale` subresource) are never reverted.

> ⚠️ If your manifests still declare `replicas: 1`, Flux will reset the value on
> the next reconcile and the UI will appear to "not work". Remove `spec.replicas`
> from the manifests first.

## API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Liveness probe |
| `GET` | `/api/targets` | List discovered workloads with replica state |
| `POST` | `/api/targets/{namespace}/{name}/scale` | Body `{"replicas": 0\|1}` — scale up/down |

See `docs/api.md` for details.

## Development

See `docs/development.md` and the `AGENTS.md` file. Dev shell:

```bash
nix develop --accept-flake-config
```

## License

MIT
