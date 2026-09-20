use crate::config::Config;
use crate::models::Target;
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug)]
pub enum KubeError {
    Api(String),
    NotFound(String),
    InvalidMethod(String),
}

impl std::fmt::Display for KubeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KubeError::Api(s) => write!(f, "Api error: {}", s),
            KubeError::NotFound(s) => write!(f, "Not found: {}", s),
            KubeError::InvalidMethod(s) => write!(f, "Invalid HTTP method: {}", s),
        }
    }
}

impl std::error::Error for KubeError {}

#[derive(Clone)]
pub struct KubeClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl KubeClient {
    pub async fn new() -> Result<Self, KubeError> {
        let host = std::env::var("KUBERNETES_SERVICE_HOST").unwrap_or_default();
        let port = std::env::var("KUBERNETES_SERVICE_PORT")
            .unwrap_or_else(|_| if host.is_empty() { "8080".to_string() } else { "443".to_string() });
        let base_url = if host.is_empty() {
            format!("http://localhost:{}", port)
        } else if host.contains(':') {
            format!("https://[{}]:{}", host, port)
        } else {
            format!("https://{}:{}", host, port)
        };

        let token_path = std::path::Path::new("/var/run/secrets/kubernetes.io/serviceaccount/token");
        let token = if token_path.exists() {
            match tokio::fs::read_to_string(token_path).await {
                Ok(t) => {
                    tracing::info!("Loaded ServiceAccount token from {}", token_path.display());
                    Some(t)
                }
                Err(e) => {
                    tracing::warn!("Failed to read ServiceAccount token: {}", e);
                    None
                }
            }
        } else {
            tracing::info!("No ServiceAccount token found; running outside cluster or without RBAC");
            None
        };

        let client_builder = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10));

        let ca_cert_path = std::path::Path::new("/var/run/secrets/kubernetes.io/serviceaccount/ca.crt");
        let client = if ca_cert_path.exists() {
            match tokio::fs::read(ca_cert_path).await {
                Ok(ca_cert) => match reqwest::Certificate::from_pem(&ca_cert) {
                    Ok(cert) => {
                        client_builder.add_root_certificate(cert).build().map_err(|e| KubeError::Api(e.to_string()))?
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse CA certificate: {}, using default client", e);
                        client_builder.build().map_err(|e| KubeError::Api(e.to_string()))?
                    }
                },
                Err(e) => {
                    tracing::error!("Failed to read CA certificate: {}, using default client", e);
                    client_builder.build().map_err(|e| KubeError::Api(e.to_string()))?
                }
            }
        } else {
            tracing::info!("No CA certificate found; using default client");
            client_builder.build().map_err(|e| KubeError::Api(e.to_string()))?
        };

        Ok(Self { client, base_url, token })
    }

    // ------------------------------------------------------------------
    // Low-level request plumbing
    // ------------------------------------------------------------------

    pub async fn request(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value, KubeError> {
        let text = self.request_text(method, path, body).await?;
        serde_json::from_str(&text).map_err(|e| KubeError::Api(e.to_string()))
    }

    pub async fn request_text(&self, method: &str, path: &str, body: Option<Value>) -> Result<String, KubeError> {
        let url = format!("{}{}", self.base_url, path);

        let max_retries = 3;
        let mut last_err = None;
        for attempt in 0..max_retries {
            match self.do_request(&url, method, body.clone()).await {
                Ok(resp) => {
                    let status = resp.status();

                    if status == reqwest::StatusCode::NOT_FOUND {
                        return Err(KubeError::NotFound(format!("Resource not found at {}", path)));
                    }
                    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                        if attempt < max_retries - 1 {
                            let delay = Duration::from_secs(1 << attempt);
                            tracing::warn!("Rate limited (429) on {}, retrying in {}s", path, delay.as_secs());
                            sleep(delay).await;
                            last_err = Some(KubeError::Api(format!("Rate limited on {}", path)));
                            continue;
                        }
                        return Err(KubeError::Api(format!("Rate limited on {} after {} retries", path, max_retries)));
                    }
                    if status.is_client_error() {
                        let text = resp.text().await.unwrap_or_default();
                        return Err(KubeError::Api(format!(
                            "Client error {} on {}: {}",
                            status, path, text
                        )));
                    }
                    if status.is_server_error() {
                        let text = resp.text().await.unwrap_or_default();
                        if attempt < max_retries - 1 {
                            let delay = Duration::from_secs(1 << attempt);
                            tracing::warn!(
                                "Server error {} on {}, retrying in {}s: {}",
                                status, path, delay.as_secs(), text
                            );
                            sleep(delay).await;
                            last_err = Some(KubeError::Api(format!("Server error {} on {}: {}", status, path, text)));
                            continue;
                        }
                        return Err(KubeError::Api(format!(
                            "Server error {} on {}: {}",
                            status, path, text
                        )));
                    }

                    return resp.text().await.map_err(|e| KubeError::Api(e.to_string()));
                }
                Err(e) => {
                    if attempt < max_retries - 1 {
                        let delay = Duration::from_secs(1 << attempt);
                        tracing::warn!("Request attempt {} failed for {}, retrying in {}s: {}", attempt + 1, path, delay.as_secs(), e);
                        sleep(delay).await;
                        last_err = Some(e);
                        continue;
                    }
                    return Err(KubeError::Api(format!(
                        "Request failed after {} retries for {}: {}",
                        max_retries, path, e
                    )));
                }
            }
        }
        Err(last_err.unwrap())
    }

    async fn do_request(&self, url: &str, method: &str, body: Option<Value>) -> Result<reqwest::Response, KubeError> {
        let method_value = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|_| KubeError::InvalidMethod(format!("Invalid HTTP method: {}", method)))?;

        let mut req = self.client.request(method_value, url);

        if let Some(ref t) = self.token {
            req = req.header("Authorization", format!("Bearer {}", t));
        }

        if let Some(b) = body {
            let content_type = if method == "PATCH" {
                "application/merge-patch+json"
            } else {
                "application/json"
            };
            req = req.header("Content-Type", content_type).json(&b);
        }

        req.send().await.map_err(|e| KubeError::Api(e.to_string()))
    }

    // ------------------------------------------------------------------
    // Discovery
    // ------------------------------------------------------------------

    fn list_urls(kind_plural: &str, namespaces: &[String], label_selector: &str) -> Vec<String> {
        let selector = if label_selector.is_empty() {
            String::new()
        } else {
            format!("?labelSelector={}", urlencode(label_selector))
        };
        if namespaces.is_empty() {
            vec![format!("/apis/apps/v1/{}{}", kind_plural, selector)]
        } else {
            namespaces
                .iter()
                .map(|ns| format!("/apis/apps/v1/namespaces/{}/{}{}", ns, kind_plural, selector))
                .collect()
        }
    }

    pub async fn discover(&self, cfg: &Config) -> Vec<Target> {
        let mut targets = Vec::new();
        let selector = format!("{}={}", cfg.selector_key, cfg.selector_value);

        for (kind, plural) in [("Deployment", "deployments"), ("StatefulSet", "statefulsets")] {
            for path in Self::list_urls(plural, &cfg.namespaces, &selector) {
                match self.request("GET", &path, None).await {
                    Ok(resp) => {
                        let Some(items) = resp.get("items").and_then(|v| v.as_array()) else {
                            continue;
                        };
                        for item in items {
                            let Some(name) = item
                                .pointer("/metadata/name")
                                .and_then(|v| v.as_str())
                                .map(String::from)
                            else {
                                continue;
                            };
                            let Some(ns) = item
                                .pointer("/metadata/namespace")
                                .and_then(|v| v.as_str())
                                .map(String::from)
                            else {
                                continue;
                            };

                            let (selected, reason) = matches_selector(item, &cfg.selector_key, &cfg.selector_value);

                            let default_replicas = default_state(item, &cfg.default_state_label)
                                .or(Some(cfg.default_state));

                            let replicas = item.pointer("/spec/replicas").and_then(|v| v.as_i64());
                            let ready = item.pointer("/status/readyReplicas").and_then(|v| v.as_i64());
                            let available = item.pointer("/status/availableReplicas").and_then(|v| v.as_i64());

                            targets.push(Target {
                                kind: kind.to_string(),
                                name,
                                namespace: ns,
                                replicas,
                                ready_replicas: ready,
                                available_replicas: available,
                                desired_replicas: None,
                                default_replicas,
                                selected,
                                match_reason: reason,
                                state: compute_state(replicas, ready),
                            });
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to list {} at {}: {}", kind, path, e);
                    }
                }
            }
        }

        targets.sort_by(|a, b| {
            a.namespace
                .cmp(&b.namespace)
                .then(a.kind.cmp(&b.kind))
                .then(a.name.cmp(&b.name))
        });
        targets
    }

    // ------------------------------------------------------------------
    // Scaling
    // ------------------------------------------------------------------

    pub async fn scale(&self, ns: &str, name: &str, kind: &str, replicas: u8) -> Result<Value, KubeError> {
        let plural = if kind == "StatefulSet" {
            "statefulsets"
        } else {
            "deployments"
        };
        let path = format!(
            "/apis/apps/v1/namespaces/{}/{}/{}/scale",
            ns, plural, name
        );
        let body = serde_json::json!({ "spec": { "replicas": replicas } });
        self.request("PATCH", &path, Some(body)).await
    }

    // ------------------------------------------------------------------
    // State ConfigMap
    // ------------------------------------------------------------------

    pub async fn read_configmap(&self, ns: &str, name: &str) -> Result<Value, KubeError> {
        let path = format!("/api/v1/namespaces/{}/configmaps/{}", ns, name);
        self.request("GET", &path, None).await
    }

    pub async fn write_configmap(&self, ns: &str, name: &str, data: Value) -> Result<Value, KubeError> {
        let path = format!("/api/v1/namespaces/{}/configmaps/{}", ns, name);
        let body = serde_json::json!({ "data": data });
        match self.request("PATCH", &path, Some(body.clone())).await {
            Ok(v) => Ok(v),
            Err(KubeError::NotFound(_)) => {
                let create_path = format!("/api/v1/namespaces/{}/configmaps", ns);
                let create_body = serde_json::json!({
                    "metadata": { "name": name, "namespace": ns },
                    "data": data,
                });
                self.request("POST", &create_path, Some(create_body)).await
            }
            Err(e) => Err(e),
        }
    }

    // ------------------------------------------------------------------
    // Startup RBAC check
    // ------------------------------------------------------------------

    pub async fn check_rbac(&self, cfg: &Config) {
        let mut checks: Vec<(&str, String)> = vec![
            ("list Deployments", "/apis/apps/v1/deployments".to_string()),
            ("list StatefulSets", "/apis/apps/v1/statefulsets".to_string()),
            ("list Pods", "/api/v1/pods".to_string()),
            ("list PVCs", "/api/v1/persistentvolumeclaims".to_string()),
            (
                "state ConfigMap",
                format!("/api/v1/namespaces/{}/configmaps/{}", cfg.state_namespace, cfg.state_configmap),
            ),
        ];
        if !cfg.namespaces.is_empty() {
            checks.push((
                "list Deployments in namespace",
                format!("/apis/apps/v1/namespaces/{}/deployments", cfg.namespaces[0]),
            ));
        }

        for (name, path) in checks {
            match self.request_text("GET", &path, None).await {
                Ok(_) => tracing::info!("RBAC: {} — OK", name),
                Err(KubeError::Api(msg)) if msg.contains("403") => {
                    tracing::error!("RBAC: {} — MISSING PERMISSION. Grant access via ClusterRole.", name);
                }
                Err(KubeError::NotFound(_)) => {
                    tracing::warn!("RBAC: {} — SKIPPED (resource not found)", name);
                }
                Err(e) => {
                    tracing::warn!("RBAC: {} — SKIPPED ({})", name, e);
                }
            }
        }
    }
}

/// The selection heuristic: a workload is manageable when its `metadata.labels`
/// contains `selector_key=selector_value`. Returns (selected, reason).
pub fn matches_selector(workload: &Value, selector_key: &str, selector_value: &str) -> (bool, String) {
    if let Some(labels) = workload.pointer("/metadata/labels").and_then(|v| v.as_object()) {
        if labels.get(selector_key).and_then(|v| v.as_str()) == Some(selector_value) {
            return (true, format!("label {}={}", selector_key, selector_value));
        }
    }
    (false, String::new())
}

/// Read the per-workload default state from its labels, if present and valid.
pub fn default_state(workload: &Value, label_key: &str) -> Option<i64> {
    let labels = workload.pointer("/metadata/labels")?.as_object()?;
    let raw = labels.get(label_key)?.as_str()?;
    crate::config::parse_state(raw)
}

/// Percent-encode a labelSelector value for a query string.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'=' | b'!' | b'(' | b')' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn compute_state(replicas: Option<i64>, ready: Option<i64>) -> String {
    let r = replicas.unwrap_or(0);
    let ready = ready.unwrap_or(0);
    if r == 0 {
        "stopped".to_string()
    } else if ready >= r {
        "running".to_string()
    } else {
        "pending".to_string()
    }
}
