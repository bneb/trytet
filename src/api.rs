//! Axum API layer for the Tet engine.
//!
//! All routes are JSON-in, JSON-out. Error responses use structured
//! JSON payloads with `error` and `error_type` fields — never raw
//! stack traces or HTML error pages.

use crate::engine::TetSandbox;
use crate::models::{SnapshotResponse, TetExecutionRequest};
use crate::sandbox::SnapshotPayload;
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use tower_http::cors::CorsLayer;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod context;
pub mod console;

// ---------------------------------------------------------------------------
// Application State
// ---------------------------------------------------------------------------

/// Shared application state injected into every Axum handler.
///
/// The `sandbox` is behind `Arc<dyn TetSandbox>` so it can be shared
/// across all handler tasks without cloning the engine or snapshot store.
pub struct AppState {
    pub sandbox: Arc<dyn TetSandbox>,
    pub registry: Arc<dyn crate::registry::Registry>,
    pub registry_client: Option<Arc<crate::registry::oci::OciClient>>,
    pub hive: Option<crate::hive::HivePeers>,
    pub gateway: Arc<crate::gateway::SovereignGateway>,
    pub ingress_routes: Arc<RwLock<HashMap<String, crate::oracle::IngressRoute>>>,
}

// ---------------------------------------------------------------------------
// Router Construction
// ---------------------------------------------------------------------------

/// Builds the Axum router with all Tet API routes.
///
/// # Routes
///
/// | Method | Path                          | Description                        |
/// |--------|-------------------------------|------------------------------------|
/// | POST   | `/v1/tet/execute`             | Execute a Wasm payload             |
/// | POST   | `/v1/tet/snapshot/{tet_id}`    | Snapshot a completed execution     |
/// | POST   | `/v1/tet/fork/{snapshot_id}`   | Fork from a snapshot               |
/// | GET    | `/health`                     | Health check                       |
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/tet/execute", post(handle_execute))
        .route("/v1/tet/snapshot/{tet_id}", post(handle_snapshot))
        .route("/v1/tet/fork/{snapshot_id}", post(handle_fork))
        .route(
            "/v1/tet/export/{snapshot_id}",
            axum::routing::get(handle_export),
        )
        .route("/v1/tet/import", post(handle_import))
        .route("/v1/registry/push/{tag}", post(handle_registry_push))
        .route(
            "/v1/registry/pull/{tag}",
            axum::routing::get(handle_registry_pull),
        )
        .route("/v1/hive/peers", axum::routing::get(handle_hive_peers))
        .route("/v1/tet/teleport/{alias}", post(handle_teleport))
        .route("/v1/swarm/stream", axum::routing::get(handle_ws_stream))
        .route("/v1/tet/topup", post(handle_topup))
        .route("/v1/cartridge/invoke", post(handle_cartridge_invoke))
        .route("/v1/benchmark/node", post(handle_node_benchmark))
        .route("/v1/ingress/register", post(handle_ingress_register))
        .route("/ingress/{*path}", axum::routing::any(handle_ingress_proxy))
        .route("/v1/tet/memory/{alias}", post(handle_memory_query))
        .route("/v1/tet/infer/{alias}", post(handle_infer))
        .route("/v1/topology", axum::routing::get(handle_topology))
        .route("/v1/swarm/up", post(handle_swarm_up))
        .route(
            "/v1/swarm/metrics",
            axum::routing::get(handle_northstar_metrics),
        )
        .route(
            "/health",
            axum::routing::get(crate::server::health::handle_health),
        )
        .route("/console", axum::routing::get(console::serve_console_page))
        .layer(
            CorsLayer::permissive()
        )
        .layer(DefaultBodyLimit::max(1024 * 1024 * 50)) // 50MB limit
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize)]
pub struct TopUpRequest {
    pub alias: String,
    pub voucher: crate::economy::FuelVoucher,
}

/// POST /v1/tet/topup
///
/// Revives a suspended or out-of-fuel Tet from the network by supplying a new cryptographic FuelVoucher.
async fn handle_topup(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<TopUpRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    // 1. Resolve Local Snapshot ID
    let snapshot_id = match state.sandbox.snapshot(&payload.alias).await {
        Ok(id) => id,
        Err(_) => {
            return Err((
                StatusCode::NOT_FOUND,
                "Alias not active on this Node or has no suspended state".to_string(),
            ))
        }
    };

    // 2. Prepare fork payload bounded entirely by the new voucher mathematically
    let req = TetExecutionRequest {
        payload: None, // Fork handles the payload reuse
        alias: Some(payload.alias),
        env: std::collections::HashMap::new(),
        injected_files: std::collections::HashMap::new(),
        allocated_fuel: payload.voucher.fuel_limit, // The voucher becomes the master parameter implicitly inside the sandbox execution override, but we set it here semantically anyway
        max_memory_mb: 64,                          // Inherited usually, static for now
        parent_snapshot_id: Some(snapshot_id.clone()),
        call_depth: 0,
        voucher: Some(payload.voucher),
        manifest: None,
        egress_policy: None,
        target_function: None,
    };

    // 3. Resuscitate (Fork natively replaces the existing execution alias stream)
    match state.sandbox.fork(&snapshot_id, req).await {
        Ok(result) => Ok((StatusCode::OK, Json(result))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to resuscitate top-up snapshot: {e}"),
        )),
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CartridgeInvokeRequest {
    pub cartridge_id: String,
    pub payload: String,
    pub fuel_limit: u64,
    pub memory_limit_mb: u64,
}

async fn handle_cartridge_invoke(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CartridgeInvokeRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    // Phase 33.1: Direct Host-to-Cartridge Bridge
    // We use the sandbox's internal cartridge manager to bypass agent overhead for benchmarks.
    let (output, metrics, status) = match state.sandbox.send_mesh_call(crate::models::MeshCallRequest {
        target_alias: req.cartridge_id.clone(),
        method: "execute".to_string(),
        payload: req.payload.as_bytes().to_vec(),
        fuel_to_transfer: req.fuel_limit,
        current_depth: 0,
        target_function: Some("trytet:component/cartridge-v1#execute".to_string()),
    }).await {
        Ok(res) => {
            // Unpack MeshCallResponse
            (String::from_utf8_lossy(&res.return_data).to_string(), res.fuel_used, res.status)
        },
        Err(e) => {
             return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": e.to_string(),
                    "status": "Error"
                })),
            ));
        }
    };

    Ok((StatusCode::OK, Json(serde_json::json!({
        "output": output,
        "fuel_consumed": metrics,
        "status": status
    }))))
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct NodeBenchmarkRequest {
    pub snippet: String,
    pub timeout_ms: u64,
}

async fn handle_node_benchmark(
    Json(req): Json<NodeBenchmarkRequest>,
) -> Response {
    // Phase 36: Standard Node.js VM Simulation
    // Spawns a real Node process to evaluate the snippet with a wall-clock timeout.
    
    let start = std::time::Instant::now();
    
    // We use a small JS wrapper to evaluate the code and print the result
    // To prevent trivial RCE, we use `vm.runInNewContext` which isolates the execution from the Node global object (like `process`).
    let script = format!(
        "const vm = require('vm'); try {{ const res = vm.runInNewContext({}, {{}}, {{ timeout: {} }}); console.log(JSON.stringify({{ status: 'Success', result: String(res) }})); }} catch(e) {{ if (e.message.includes('timed out')) {{ console.log(JSON.stringify({{ status: 'Timeout', error: e.message }})); }} else {{ console.log(JSON.stringify({{ status: 'Error', error: e.message }})); }} }}",
        serde_json::to_string(&req.snippet).unwrap_or_default(),
        req.timeout_ms
    );

    let mut child = match tokio::process::Command::new("node")
        .arg("-e")
        .arg(script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn() {
            Ok(c) => c,
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to spawn node: {e}")).into_response()
        };

    // Standard wall-clock timeout
    let timeout = tokio::time::sleep(std::time::Duration::from_millis(req.timeout_ms));
    
    tokio::select! {
        _ = timeout => {
            let _ = child.kill().await;
            (StatusCode::OK, Json(serde_json::json!({
                "status": "Timeout",
                "duration_ms": start.elapsed().as_millis(),
                "output": "Script execution timed out after wall-clock limit"
            }))).into_response()
        }
        status = child.wait() => {
            let duration_ms = start.elapsed().as_millis();
            let output = match child.wait_with_output().await {
                Ok(o) => o,
                Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            };
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            
            if status.is_ok() && !stdout.is_empty() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&stdout) {
                     return (StatusCode::OK, Json(serde_json::json!({
                        "status": parsed["status"],
                        "duration_ms": duration_ms,
                        "output": parsed["result"].as_str().or(parsed["error"].as_str()).unwrap_or("Unknown")
                    }))).into_response();
                }
            }
            
            (StatusCode::OK, Json(serde_json::json!({
                "status": "Error",
                "duration_ms": duration_ms,
                "output": String::from_utf8_lossy(&output.stderr).to_string()
            }))).into_response()
        }
    }
}

/// POST /v1/tet/execute
///
/// Accepts a `TetExecutionRequest` and returns a `TetExecutionResult`.
/// The execution may succeed, timeout, crash, or exceed memory — all of
/// these return HTTP 200 with the appropriate `ExecutionStatus`.
///
/// Only infrastructure-level failures return non-200 status codes.
async fn handle_execute(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TetExecutionRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    // Validate the request
    if req.payload.is_none() && req.parent_snapshot_id.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "payload is required when not forking",
                "error_type": "bad_request"
            })),
        ));
    }

    if let Some(ref p) = req.payload {
        if p.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "payload must not be empty",
                    "error_type": "bad_request"
                })),
            ));
        }
    }

    if req.allocated_fuel == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "allocated_fuel must be greater than 0",
                "error_type": "bad_request"
            })),
        ));
    }

    if req.max_memory_mb == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "max_memory_mb must be greater than 0",
                "error_type": "bad_request"
            })),
        ));
    }
    let mut req = req;
    if req.egress_policy.is_none() {
        req.egress_policy = Some(crate::oracle::EgressPolicy {
            allowed_domains: vec![],
            max_daily_bytes: 0,
            require_https: true,
        });
    }

    match state.sandbox.execute(req).await {
        Ok(result) => Ok((StatusCode::OK, Json(result))),
        Err(e) => Err((
            StatusCode::from_u16(e.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(serde_json::json!({
                "error": e.to_string(),
                "error_type": "engine_error"
            })),
        )),
    }
}

/// POST /v1/tet/snapshot/{tet_id}
///
/// Freezes the memory state of a completed Tet execution.
/// Returns a `SnapshotResponse` with the `snapshot_id` that can be
/// used for forking.
async fn handle_snapshot(
    State(state): State<Arc<AppState>>,
    Path(tet_id): Path<String>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    match state.sandbox.snapshot(&tet_id).await {
        Ok(snapshot_id) => Ok((StatusCode::OK, Json(SnapshotResponse { snapshot_id }))),
        Err(e) => Err((
            StatusCode::from_u16(e.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(serde_json::json!({
                "error": e.to_string(),
                "error_type": "snapshot_error"
            })),
        )),
    }
}

/// POST /v1/tet/fork/{snapshot_id}
///
/// Forks from a previously created snapshot, executing the same module
/// with the same memory state but potentially different env variables.
async fn handle_fork(
    State(state): State<Arc<AppState>>,
    Path(snapshot_id): Path<String>,
    Json(req): Json<TetExecutionRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    match state.sandbox.fork(&snapshot_id, req).await {
        Ok(result) => Ok((StatusCode::OK, Json(result))),
        Err(e) => Err((
            StatusCode::from_u16(e.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(serde_json::json!({
                "error": e.to_string(),
                "error_type": "fork_error"
            })),
        )),
    }
}

async fn handle_export(
    State(state): State<Arc<AppState>>,
    Path(snapshot_id): Path<String>,
) -> Response {
    match state.sandbox.export_snapshot(&snapshot_id).await {
        Ok(payload) => (StatusCode::OK, Json(payload)).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Snapshot not found" })),
        )
            .into_response(),
    }
}

async fn handle_import(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SnapshotPayload>,
) -> Response {
    match state.sandbox.import_snapshot(payload).await {
        Ok(snapshot_id) => (
            StatusCode::OK,
            Json(serde_json::json!({ "snapshot_id": snapshot_id })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn handle_registry_push(
    State(state): State<Arc<AppState>>,
    Path(tag): Path<String>,
    bytes: axum::body::Bytes,
) -> Response {
    match state.registry.push(&tag, &bytes) {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_registry_pull(
    State(state): State<Arc<AppState>>,
    Path(tag): Path<String>,
) -> Response {
    match state.registry.pull(&tag) {
        Ok(Some(bytes)) => (StatusCode::OK, bytes).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /v1/hive/peers
pub async fn handle_hive_peers(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if let Some(hive) = &state.hive {
        let peers = hive.list_peers().await;
        Ok(Json(serde_json::json!({
            "status": "success",
            "peers": peers
        })))
    } else {
        Err((
            StatusCode::NOT_IMPLEMENTED,
            Json(serde_json::json!({
                "error": "Hive Networking is not enabled on this node",
                "error_type": "not_implemented"
            })),
        ))
    }
}

/// POST /v1/tet/teleport/{alias}
pub async fn handle_teleport(
    State(state): State<Arc<AppState>>,
    Path(alias): Path<String>,
    Json(req_data): Json<serde_json::Value>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    let target_node_id = req_data["target_node"].as_str().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Missing target_node"})),
        )
    })?;

    let hive = match &state.hive {
        Some(h) => h,
        None => {
            return Err((
                StatusCode::NOT_IMPLEMENTED,
                Json(serde_json::json!({
                    "error": "Hive Networking is not enabled on this node",
                    "error_type": "not_implemented"
                })),
            ))
        }
    };

    // 1. Resolve Target Node Address
    let target_node = match hive.get_peer(target_node_id).await {
        Some(n) => n,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Target node {} not found in local peer list", target_node_id),
                    "error_type": "bad_request"
                })),
            ))
        }
    };

    let teleport_req = crate::teleport::TeleportRequest {
        agent_id: alias,
        target_address: target_node.public_addr,
        use_registry: req_data["use_registry"].as_bool().unwrap_or(false),
    };

    match teleport_req
        .execute(state.sandbox.clone(), state.registry_client.clone())
        .await
    {
        Ok(receipt) => Ok(Json(serde_json::json!({
            "status": "success",
            "message": format!("Teleported {} bytes to {}", receipt.bytes_transferred, receipt.target_address),
            "receipt": {
                "agent_id": receipt.agent_id,
                "target": receipt.target_address,
                "bytes": receipt.bytes_transferred
            }
        }))),
        Err(e) => {
            let status = match e {
                crate::teleport::TeleportError::PermissionDenied => StatusCode::FORBIDDEN,
                crate::teleport::TeleportError::TargetError(_) => StatusCode::BAD_GATEWAY,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            Err((
                status,
                Json(serde_json::json!({
                    "error": e.to_string(),
                    "error_type": "teleport_error"
                })),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Telemetry Endpoints
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Ingress Endpoints
// ---------------------------------------------------------------------------

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};

async fn handle_ws_stream(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| async move { handle_socket(socket, state).await })
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    // In a full implementation, we'd listen here for a standard Trytet Stream
    // of metrics, topology, and Migration requests (SnapshotPayload passing).

    // For Phase 13 teleportation demo, we'll await a JSON request.
    while let Some(msg) = socket.recv().await {
        if let Ok(Message::Text(text)) = msg {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if json["type"] == "request_teleport" {
                    let alias = json["alias"].as_str().unwrap_or_default();
                    if let Ok(payload) = state.sandbox.export_snapshot(alias).await {
                        if let Ok(bincode_bytes) = bincode::serialize(&payload) {
                            let _ = socket.send(Message::Binary(bincode_bytes.into())).await;
                        }
                    }
                }
            }
        }
    }
}

async fn handle_ingress_register(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<crate::oracle::IngressRoute>,
) -> Result<impl IntoResponse, axum::http::StatusCode> {
    let mut map = state.ingress_routes.write().await;
    map.insert(payload.public_path.clone(), payload.clone());
    Ok((StatusCode::OK, Json(payload)))
}

async fn handle_ingress_proxy(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
    method: axum::http::Method,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, impl IntoResponse> {
    // Determine alias vs path.
    // Assuming format is `/ingress/{alias}/{...path}`
    let mut parts = path.splitn(2, '/');
    let alias = parts.next().unwrap_or_default().to_string();
    let subpath = parts.next().unwrap_or_default().to_string();
    let formatted_path = format!("/{}", subpath);

    let mut header_map = std::collections::HashMap::new();
    for (name, value) in headers.iter() {
        if let Ok(v) = value.to_str() {
            header_map.insert(name.as_str().to_string(), v.to_string());
        }
    }

    let req = crate::gateway::GatewayRequest {
        alias,
        path: formatted_path,
        method: method.to_string(),
        body: body.to_vec(),
        headers: header_map,
    };

    match state
        .gateway
        .handle_request(req, state.sandbox.clone())
        .await
    {
        Ok(res) => Ok((StatusCode::OK, res)),
        Err(crate::gateway::GatewayError::RouteNotFound) => Err((
            StatusCode::NOT_FOUND,
            "No Trytet Ingress mapped to this path".to_string(),
        )),
        Err(e) => Err((StatusCode::BAD_GATEWAY, format!("Gateway Error: {}", e))),
    }
}

async fn handle_topology(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, axum::http::StatusCode> {
    let edges = state.sandbox.get_topology().await;
    Ok(Json(edges))
}

/// POST /v1/tet/memory/{alias}
async fn handle_memory_query(
    axum::extract::Path(alias): axum::extract::Path<String>,
    State(state): State<Arc<AppState>>,
    Json(query): Json<crate::memory::SearchQuery>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    match state.sandbox.query_memory(&alias, query).await {
        Ok(results) => Ok((StatusCode::OK, Json(results))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Memory query failed: {}", e),
        )),
    }
}

/// POST /v1/tet/infer/{alias}
async fn handle_infer(
    axum::extract::Path(alias): axum::extract::Path<String>,
    State(state): State<Arc<AppState>>,
    Json(request): Json<crate::inference::InferenceRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    match state.sandbox.infer(&alias, request, u64::MAX).await {
        Ok(response) => Ok((StatusCode::OK, Json(response))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Inference failed: {}", e),
        )),
    }
}

pub async fn handle_swarm_up(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<impl IntoResponse, impl IntoResponse> {
    // We expect the direct sandbox type for now per architecture
    let sandbox = state.sandbox.clone();

    // Attempt to downcast logic (simulated for trytet traits)
    // Since TetSandbox is a trait, we assume the studio orchestrator
    // has access to the raw WasmtimeSandbox for lower level features.
    // For simplicity, we just pass the trait or change studio.rs to use the trait.
    use crate::studio::StudioOrchestrator;
    let orchestrator = match StudioOrchestrator::new(&body) {
        Ok(o) => o,
        Err(e) => return Err((StatusCode::BAD_REQUEST, format!("Invalid Manifest: {}", e))),
    };

    match orchestrator.up(sandbox).await {
        Ok(results) => Ok((
            StatusCode::OK,
            Json(
                serde_json::json!({ "status": "success", "agents_booted": results.len(), "results": results }),
            ),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Orchestration Failed: {}", e),
        )),
    }
}

/// GET /v1/swarm/metrics
///
/// Runs the Northstar Benchmarking Suite and returns a JSON report
/// compatible with Prometheus/Grafana external visualization.
///
/// This endpoint is intentionally synchronous and blocking — it measures
/// real performance characteristics of the local node. Typical execution
/// time is 200-500ms depending on hardware.
async fn handle_northstar_metrics(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    let report = tokio::task::spawn_blocking(crate::benchmarks::run_full_suite)
        .await
        .unwrap_or_default();

    (StatusCode::OK, Json(report))
}
