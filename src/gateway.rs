use crate::engine::TetSandbox;
use dashmap::DashMap;
use futures_util::future::BoxFuture;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("Route Not Found")]
    RouteNotFound,
    #[error("Node Unreachable: {0}")]
    NodeUnreachable(String),
    #[error("DHT Error: {0}")]
    DhtError(String),
    #[error("Execution Failed: {0}")]
    ExecutionFailed(String),
}

#[derive(Debug, Clone)]
pub struct IngressRoute {
    pub alias: String,
    pub path: String,
    pub handler_func: String,
}

pub trait GlobalRegistry: Send + Sync {
    /// O(log N) pseudo-lookup to find the Node IP that hosts the given alias
    fn resolve_alias(&self, alias: &str) -> BoxFuture<'_, Result<Option<String>, GatewayError>>;

    /// Update the registry to point an alias to a new Node IP (used during teleport)
    fn update_route(
        &self,
        alias: &str,
        node_ip: &str,
        signature: &str,
    ) -> BoxFuture<'_, Result<(), GatewayError>>;
}

pub struct GatewayRequest {
    pub alias: String,
    pub path: String,
    pub method: String,
    pub body: Vec<u8>,
    pub headers: HashMap<String, String>,
}

pub struct SovereignGateway {
    pub local_routes: DashMap<String, Vec<IngressRoute>>,
    pub dht: Arc<dyn GlobalRegistry>,
    pub reqwest_client: Client,
}

impl SovereignGateway {
    pub fn new(dht: Arc<dyn GlobalRegistry>) -> Self {
        Self {
            local_routes: DashMap::new(),
            dht,
            reqwest_client: Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap(),
        }
    }

    pub fn register_route(&self, alias: String, path: String, handler_func: String) {
        let mut routes = self
            .local_routes
            .entry(alias.clone())
            .or_default();
        // Avoid duplicates mapping the exact same path
        if !routes.iter().any(|r| r.path == path) {
            routes.push(IngressRoute {
                alias,
                path,
                handler_func,
            });
        }
    }

    pub async fn handle_request(
        &self,
        req: GatewayRequest,
        sandbox: Arc<dyn TetSandbox>,
    ) -> Result<Vec<u8>, GatewayError> {
        // 1. Check local routes
        if let Some(routes) = self.local_routes.get(&req.alias) {
            if let Some(route) = routes.iter().find(|r| req.path.starts_with(&r.path)) {
                // If local: Invoke Wasm Guest via WasmtimeSandbox MeshCall logic (but pointing to handler)
                let call_req = crate::models::MeshCallRequest {
                    target_alias: route.alias.clone(),
                    method: req.method.clone(), // or "invoke"
                    payload: req.body,
                    fuel_to_transfer: 10_000_000,
                    current_depth: 0,
                    target_function: Some(route.handler_func.clone()),
                };

                let res = sandbox.send_mesh_call(call_req).await.map_err(
                    |e: crate::engine::TetError| GatewayError::ExecutionFailed(e.to_string()),
                )?;

                if res.status != crate::models::ExecutionStatus::Success {
                    return Err(GatewayError::ExecutionFailed(format!(
                        "Guest trapped or exceeded limits: {:?}",
                        res.status
                    )));
                }

                return Ok(res.return_data);
            }
        }

        // 2. If missing locally or path mis-mapped locally, query GlobalRegistry (DHT) for target Node IP
        let target_ip = match self.dht.resolve_alias(&req.alias).await? {
            Some(ip) => ip,
            None => return Err(GatewayError::RouteNotFound),
        };

        // 3. Proxy request to target Node IP, preserving Sovereign headers
        let target_url = format!("http://{}/ingress/{}{}", target_ip, req.alias, req.path);

        // Reconstruct inbound reqwest call to proxy
        let mut request_builder = match req.method.as_str() {
            "GET" => self.reqwest_client.get(&target_url),
            "POST" => self.reqwest_client.post(&target_url),
            "PUT" => self.reqwest_client.put(&target_url),
            "DELETE" => self.reqwest_client.delete(&target_url),
            _ => self.reqwest_client.post(&target_url),
        };

        // Preserve sovereign Identity headers mathematically signed from Origin if available
        for (k, v) in req.headers {
            if k.to_lowercase().starts_with("x-trytet-") {
                request_builder = request_builder.header(k, v);
            }
        }

        let resp = request_builder
            .body(req.body)
            .send()
            .await
            .map_err(|e| GatewayError::NodeUnreachable(e.to_string()))?;

        if resp.status().is_client_error() || resp.status().is_server_error() {
            return Err(GatewayError::ExecutionFailed(format!(
                "Proxy hop returned {}",
                resp.status()
            )));
        }

        let out = resp
            .bytes()
            .await
            .map_err(|e| GatewayError::NodeUnreachable(e.to_string()))?;
        Ok(out.to_vec())
    }
}

pub struct NoopRegistry;
impl GlobalRegistry for NoopRegistry {
    fn resolve_alias(&self, _alias: &str) -> BoxFuture<'_, Result<Option<String>, GatewayError>> {
        Box::pin(async { Ok(None) })
    }
    fn update_route(
        &self,
        _alias: &str,
        _ip: &str,
        _sig: &str,
    ) -> BoxFuture<'_, Result<(), GatewayError>> {
        Box::pin(async { Ok(()) })
    }
}

impl Default for SovereignGateway {
    fn default() -> Self {
        Self::new(Arc::new(NoopRegistry))
    }
}
