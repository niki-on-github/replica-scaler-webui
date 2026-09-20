# Replica Scaler WebUI

A web-based on/off switch for **Kubernetes workloads** (Deployments and
StatefulSets). Start (1 replica) / Stop (0 replicas) any workload that carries a
selector label you opt in with, through a simple table UI.

The container **auto-discovers** managed workloads from inside the cluster — no
hardcoded target list. Workloads sitting at `replicas: 0` are still detected and
listed, so you can start them from the UI.

> [!WARNING]
> The code was specifically designed for my k8s setup (Flux GitOps + bjw-s
> `app-template` HelmReleases). It is not a universal tool. Review the
> conditions carefully or fork with the necessary modifications.

## Features

- **Auto-discovery**: lists every Deployment/StatefulSet that carries the
  selector label (see [Selection](#selection)), regardless of its current replica
  count. The label query runs server-side via `?labelSelector=`, so only matching
  workloads are ever transferred.
- **Start / Stop**: scale any discovered workload to `1` or `0` replicas via the
  scale subresource. `replicas` is clamped to `{0, 1}` — never more than one.
- **Persisted desired state**: the last requested replica count is stored in a
  ConfigMap and re-applied on startup, so a workload that gets deleted and
  recreated (Flux prune / helm upgrade) doesn't drift back to its chart default.
- **Default state**: a workload with no persisted state yet is brought to its
  declared default — `active` (1) or `inactive` (0) — via the default-state
  label, falling back to the `DEFAULT_STATE` env (default `inactive`).
- **Auto-refresh**: the dashboard polls the Kubernetes API periodically.

## Selection

A workload is manageable when its `metadata.labels` contains the configured
selector label:

- `SELECTOR_KEY` (default `replica-scaler.webui.io/managed`)
- `SELECTOR_VALUE` (default `true`)

Only workloads carrying that label are listed and can be scaled. Add the label to
any Deployment/StatefulSet you want to manage — for a bjw-s `app-template`
HelmRelease, set it on the generated workload's metadata via your chart values
(e.g. `controllers.<name>.labels`), or annotate the chart to render it.

```yaml
metadata:
  labels:
    replica-scaler.webui.io/managed: "true"
```

## Default State

When a workload has **no persisted desired state** (first time it is seen, or
after the state ConfigMap is cleared), the UI applies a default instead of
leaving it untouched. The default is resolved in this order:

1. The workload's **default-state label** — `replica-scaler.webui.io/default-state: active|inactive`
   (configurable via `DEFAULT_STATE_LABEL`; `1`/`0` also accepted).
2. The **global fallback** `DEFAULT_STATE` env (`active` or `inactive`).
3. If neither is set, the fallback defaults to **`inactive`**.

```yaml
metadata:
  labels:
    replica-scaler.webui.io/managed: "true"
    replica-scaler.webui.io/default-state: "inactive"
```

The resolved default is scaled in and then persisted to the state ConfigMap, so
it applies **once** — any later Start/Stop from the UI, or a manual scale, wins
over the label from then on.

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
  name: replica-scaler-webui
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
      replica-scaler-webui:
        enabled: true

    rbac:
      roles:
        replica-scaler-webui:
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
        replica-scaler-webui:
          enabled: true
          type: ClusterRoleBinding
          roleRef:
            identifier: replica-scaler-webui
          subjects:
            - identifier: replica-scaler-webui

    controllers:
      replica-scaler-webui:
        containers:
          app:
            image:
              repository: ghcr.io/niki-on-github/replica-scaler-webui
              tag: "v0.1.0"
            env:
              NAMESPACES: "gpu"          # comma-separated; empty = all
              SELECTOR_KEY: "replica-scaler.webui.io/managed"
              SELECTOR_VALUE: "true"
            probes:
              liveness:
                enabled: true
                custom: true
                spec:
                  httpGet: { path: /health, port: 8080 }
                  initialDelaySeconds: 5

    service:
      webui:
        controller: replica-scaler-webui
        ports:
          http: { port: 8080 }

    ingress:
      webui:
        className: traefik
        annotations:
          traefik.ingress.kubernetes.io/router.entrypoints: websecure
        hosts:
          - host: &ingress "replica-scaler-webui.example.com"
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
> *"No managed workloads discovered."* With `app-template` **4.6.2** the pod
> automatically uses the ServiceAccount whose key matches the controller name.
> On **older chart versions (e.g. 4.0.1) that auto-wiring does NOT happen** —
> the pod stays on `default` — so pin the SA explicitly:
>
> ```yaml
> controllers:
>   replica-scaler-webui:
>     serviceAccount:
>       name: replica-scaler-webui
>     containers:
>       app: ...
> ```

The web UI itself is **not** a managed workload (it does not carry the
`replica-scaler.webui.io/managed` label), so it never lists itself.

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
| `SELECTOR_KEY` | `replica-scaler.webui.io/managed` | Label key that marks a workload as manageable |
| `SELECTOR_VALUE` | `true` | Label value a workload must carry to be managed |
| `DEFAULT_STATE_LABEL` | `replica-scaler.webui.io/default-state` | Label key describing a workload's initial state (`active`/`inactive`) |
| `DEFAULT_STATE` | `inactive` | Fallback initial state for workloads with no default-state label |
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
