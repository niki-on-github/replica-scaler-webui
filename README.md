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

The container expects to run inside the cluster with a ServiceAccount that can
read workloads and scale them.

```bash
kubectl apply -f example/helm-release.yaml   # adjust RBAC / targets first
```

### Example (Flux + bjw-s `app-template`)

A self-contained `HelmRelease` (the exact setup my cluster uses):

```yaml
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata:
  name: nvidia-replica-scaler-webui
  namespace: gpu
spec:
  interval: 10m
  chart:
    spec:
      chart: app-template
      version: 4.6.2          # see ServiceAccount gotcha below
      sourceRef:
        kind: HelmRepository
        name: bjw-s-charts
        namespace: flux-system

  values:
    defaultPodOptions:
      automountServiceAccountToken: true

    serviceAccount:
      nvidia-replica-scaler-webui:
        enabled: true

    rbac:
      roles:
        nvidia-replica-scaler-webui:
          enabled: true
          type: ClusterRole
          rules:
            - apiGroups: ["apps"]
              resources: ["deployments", "deployments/status", "deployments/scale",
                           "statefulsets", "statefulsets/status", "statefulsets/scale"]
              verbs: ["get", "list", "patch", "update"]
            - apiGroups: [""]
              resources: ["configmaps"]   # persists desired state
              verbs: ["get", "list", "create", "update", "patch"]
            - apiGroups: [""]
              resources: ["pods", "persistentvolumeclaims"]
              verbs: ["get", "list"]
      bindings:
        nvidia-replica-scaler-webui:
          enabled: true
          type: ClusterRoleBinding
          roleRef:
            identifier: nvidia-replica-scaler-webui
          subjects:
            - identifier: nvidia-replica-scaler-webui

    controllers:
      nvidia-replica-scaler-webui:
        containers:
          app:
            image:
              repository: ghcr.io/niki-on-github/nvidia-replica-scaler-webui
              tag: "v0.1.0"
            env:
              NAMESPACES: "gpu"          # comma-separated; empty = all
              GPU_RUNTIME_CLASS: "nvidia"
            probes:
              liveness:
                enabled: true
                custom: true
                spec:
                  httpGet: { path: /health, port: 8080 }
                  initialDelaySeconds: 5

    service:
      webui:
        controller: nvidia-replica-scaler-webui
        ports:
          http: { port: 8080 }

    ingress:
      webui:
        className: traefik
        annotations:
          traefik.ingress.kubernetes.io/router.entrypoints: websecure
        hosts:
          - host: &ingress "nvidia-replica-scaler-webui.example.com"
            paths:
              - path: /
                pathType: Prefix
                service:
                  identifier: webui
                  port: http
        tls:
          - hosts: [*ingress]
```

> [!IMPORTANT]
> **The pod must run with the RBAC-bound ServiceAccount.** If the pod runs as
> the `default` ServiceAccount, every list hits `403 Forbidden` and the UI shows
> *"No GPU workloads discovered."* With `app-template` **4.6.2** the pod
> automatically uses the ServiceAccount whose key matches the controller name.
> On **older chart versions (e.g. 4.0.1) that auto-wiring does NOT happen** —
> the pod stays on `default` — so pin the SA explicitly:
>
> ```yaml
> controllers:
>   nvidia-replica-scaler-webui:
>     serviceAccount:
>       name: nvidia-replica-scaler-webui
>     containers:
>       app: ...
> ```

The web UI itself is **not** a GPU workload (no `runtimeClassName: nvidia`), so
it never matches its own heuristic and won't list itself.

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
