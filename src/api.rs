//! Axum API layer for the Tet engine.
//!
//! All routes are JSON-in, JSON-out. Error responses use structured
//! JSON payloads with `error` and `error_type` fields — never raw
//! stack traces or HTML error pages.

use crate::engine::TetSandbox;
use crate::models::{SnapshotResponse, TetExecutionRequest};
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use crate::sandbox::SnapshotPayload;

pub mod context;

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
    pub hive: Option<crate::hive::HivePeers>,
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
        .route("/v1/tet/export/{snapshot_id}", axum::routing::get(handle_export))
        .route("/v1/tet/import", post(handle_import))
        .route("/v1/registry/push/{tag}", post(handle_registry_push))
        .route("/v1/registry/pull/{tag}", axum::routing::get(handle_registry_pull))
        .route("/v1/hive/peers", axum::routing::get(handle_hive_peers))
        .route("/v1/tet/teleport/{alias}", post(handle_teleport))
        .route("/v1/swarm/stream", axum::routing::get(handle_ws_stream))
        .route("/v1/tet/topup", post(handle_topup))
        .route("/v1/ingress/register", post(handle_ingress_register))
        .route("/ingress/{*path}", axum::routing::any(handle_ingress_proxy))
        .route("/v1/tet/memory/{alias}", post(handle_memory_query))
        .route("/v1/tet/infer/{alias}", post(handle_infer))
        .route("/v1/topology", axum::routing::get(handle_topology))
        .route("/v1/swarm/up", post(handle_swarm_up))
        .route("/health", axum::routing::get(crate::server::health::handle_health))
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
        Err(_) => return Err((StatusCode::NOT_FOUND, "Alias not active on this Node or has no suspended state".to_string())),
    };

    // 2. Prepare fork payload bounded entirely by the new voucher mathematically
    let req = TetExecutionRequest {
        payload: None, // Fork handles the payload reuse
        alias: Some(payload.alias),
        env: std::collections::HashMap::new(),
        injected_files: std::collections::HashMap::new(),
        allocated_fuel: payload.voucher.fuel_limit, // The voucher becomes the master parameter implicitly inside the sandbox execution override, but we set it here semantically anyway
        max_memory_mb: 64, // Inherited usually, static for now
        parent_snapshot_id: Some(snapshot_id.clone()),
        call_depth: 0,
        voucher: Some(payload.voucher),
        egress_policy: None,
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

/// GET /health
///
/// Returns a simple health check response. Used by load balancers
/// and monitoring systems.
async fn handle_health() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "operational",
            "engine": "tet-core",
            "version": env!("CARGO_PKG_VERSION")
        })),
    )
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
        ).into_response(),
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
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ).into_response(),
    }
}

async fn handle_registry_push(
    State(state): State<Arc<AppState>>,
    Path(tag): Path<String>,
    bytes: axum::body::Bytes,
) -> Response {
    match state.registry.push(&tag, &bytes) {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR, 
            e.to_string()
        ).into_response(),
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

#[derive(serde::Deserialize)]
pub struct TeleportRequest {
    pub target_node: String,
}

/// POST /v1/tet/teleport/{alias}
pub async fn handle_teleport(
    State(state): State<Arc<AppState>>,
    Path(alias): Path<String>,
    Json(req): Json<TeleportRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    let hive = match &state.hive {
        Some(h) => h,
        None => return Err((
            StatusCode::NOT_IMPLEMENTED,
            Json(serde_json::json!({
                "error": "Hive Networking is not enabled on this node",
                "error_type": "not_implemented"
            })),
        )),
    };

    // 1. Resolve Target Node
    let target_node = match hive.get_peer(&req.target_node).await {
        Some(n) => n,
        None => return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Target node {} not found in local peer list", req.target_node),
                "error_type": "bad_request"
            })),
        )),
    };

    // 2. Snapshot the existing Tet natively
    let snapshot_id = match state.sandbox.snapshot(&alias).await {
        Ok(s) => s,
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to snapshot tet {}: {:?}", alias, e),
                "error_type": "engine_error"
            })),
        )),
    };

    let snapshot = match state.sandbox.export_snapshot(&snapshot_id).await {
        Ok(s) => s,
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Snapshot {} not found in local memory: {}", snapshot_id, e),
                "error_type": "engine_error"
            })),
        )),
    };

    let manifest = crate::models::TetManifest {
        name: alias.clone(),
        version: "1.0.0".to_string(),
        created_at: 0,
        author_pubkey: "TELEPORT".to_string(),
        hashes: crate::models::TetHashes {
            wasm_hash: String::new(),
            memory_hash: String::new(),
            vfs_hash: String::new(),
        },
    };

    let envelope = crate::hive::TeleportationEnvelope {
        manifest,
        snapshot,
        transfer_token: uuid::Uuid::new_v4().to_string(),
    };

    let cmd = crate::hive::HiveCommand::MigrateRequest(Box::new(envelope));
    match crate::hive::HiveClient::rpc_call(&target_node.public_addr, cmd).await {
        Ok(_) => {
            // Success, remove locally if we wanted. For now, we leave it since it acts as backup.
            Ok(Json(serde_json::json!({
                "status": "success",
                "message": format!("Tet {} teleported to {}", alias, target_node.node_id)
            })))
        },
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to transmit teleport envelope: {:?}", e),
                "error_type": "network_error"
            })),
        )),
    }
}

// ---------------------------------------------------------------------------
// Telemetry Endpoints
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Ingress Endpoints
// ---------------------------------------------------------------------------

use axum::extract::ws::{WebSocketUpgrade, WebSocket, Message};

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
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, impl IntoResponse> {
    let search_path = format!("/{}", path);
    // Find the longest match in registered routes
    let lock = state.ingress_routes.read().await;
    
    let route = lock.values().find(|r| search_path.starts_with(&r.public_path)).cloned();
    drop(lock);

    if let Some(r) = route {
        if !r.method_filter.contains(&method.to_string()) && !r.method_filter.is_empty() {
            return Err((StatusCode::METHOD_NOT_ALLOWED, "Method not allowed for this Trytet Ingress Route".to_string()));
        }

        // Wrap as Mesh Call
        let req = crate::models::MeshCallRequest {
            target_alias: r.target_alias.clone(),
            method: method.to_string(),
            payload: body.to_vec(),
            fuel_to_transfer: 10_000_000, 
            current_depth: 0,
        };

        match state.sandbox.send_mesh_call(req).await {
            Ok(res) => {
                if res.status != crate::models::ExecutionStatus::Success {
                    return Err((StatusCode::INTERNAL_SERVER_ERROR, "Target Tet returned an Error Status".to_string()));
                }
                
                // Return exactly what the agent wrote back to memory natively
                Ok((StatusCode::OK, res.return_data))
            }
            Err(e) => {
                let e_msg = format!("Trytet Mesh Invocation failed: {:?}", e);
                Err((StatusCode::BAD_GATEWAY, e_msg))
            }
        }
    } else {
        Err((StatusCode::NOT_FOUND, "No Trytet Ingress mapped to this path".to_string()))
    }
}

async fn handle_topology(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, axum::http::StatusCode> {
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
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Memory query failed: {}", e))),
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
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Inference failed: {}", e))),
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
        Ok(results) => {
            Ok((StatusCode::OK, Json(serde_json::json!({ "status": "success", "agents_booted": results.len(), "results": results }))))
        },
        Err(e) => {
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Orchestration Failed: {}", e)))
        }
    }
}
