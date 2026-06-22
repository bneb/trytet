//! API handler functions — extracted from api.rs to keep file size under 400 lines.
//!
//! Each handler was previously defined inline in the monolithic `api.rs`.
//! They are imported and wired into the router by the parent module.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;

use crate::api::AppState;
use crate::models::{SnapshotResponse, TetExecutionRequest};
use crate::sandbox::SnapshotPayload;

// ---- Types ----

#[derive(serde::Serialize, serde::Deserialize)]
pub struct TopUpRequest {
    pub alias: String,
    pub voucher: crate::economy::FuelVoucher,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CartridgeInvokeRequest {
    pub cartridge_id: String,
    pub payload: String,
    pub fuel_limit: u64,
    pub memory_limit_mb: u64,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SandboxBenchmarkRequest {
    pub code: String,
    pub timeout_ms: u64,
}

// ---- Handler functions ----

pub async fn handle_topup(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<TopUpRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    let snapshot_id = match state.sandbox.snapshot(&payload.alias).await {
        Ok(id) => id,
        Err(_) => {
            return Err((
                StatusCode::NOT_FOUND,
                "Alias not active on this Node or has no suspended state".into(),
            ))
        }
    };
    let req = TetExecutionRequest {
        payload: None,
        alias: Some(payload.alias),
        env: std::collections::HashMap::new(),
        injected_files: std::collections::HashMap::new(),
        allocated_fuel: payload.voucher.fuel_limit,
        max_memory_mb: 64,
        parent_snapshot_id: Some(snapshot_id.clone()),
        call_depth: 0,
        voucher: Some(payload.voucher),
        manifest: None,
        egress_policy: None,
        target_function: None,
    };
    match state.sandbox.fork(&snapshot_id, req).await {
        Ok(result) => Ok((StatusCode::OK, Json(result))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to resuscitate top-up snapshot: {e}"),
        )),
    }
}

pub async fn handle_cartridge_invoke(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CartridgeInvokeRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    let (output, metrics, status) = match state
        .sandbox
        .send_mesh_call(crate::models::MeshCallRequest {
            target_alias: req.cartridge_id.clone(),
            method: "execute".into(),
            payload: req.payload.as_bytes().to_vec(),
            fuel_to_transfer: req.fuel_limit,
            current_depth: 0,
            target_function: Some("trytet:component/cartridge-v1#execute".into()),
        })
        .await
    {
        Ok(res) => (
            String::from_utf8_lossy(&res.return_data).to_string(),
            res.fuel_used,
            res.status,
        ),
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string(), "status": "Error"})),
            ))
        }
    };
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "output": output, "fuel_consumed": metrics, "status": status
        })),
    ))
}

/// Execute JavaScript in the Wasm sandbox and return benchmark metrics.
///
/// Uses the js-evaluator Wasm cartridge (fuel-metered, memory-capped)
/// instead of spawning a host Node.js process.
pub async fn handle_sandbox_benchmark(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SandboxBenchmarkRequest>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();

    let mcp =
        match state.mcp_server.as_ref() {
            Some(m) => m,
            None => return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"status": "Error", "error": "MCP server not initialized"})),
            )
                .into_response(),
        };

    // Build a JSON-RPC tools/call request and execute through the MCP server
    let rpc = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "tools/call",
        "params": {
            "name": "trytet_js_evaluator",
            "arguments": { "code": &req.code }
        }
    });

    let body = serde_json::to_vec(&rpc).unwrap_or_default();
    let response_bytes = mcp.handle_http_request(&body).await;
    let result: serde_json::Value = serde_json::from_slice(&response_bytes).unwrap_or_default();
    let duration_ms = start.elapsed().as_millis();

    // Extract content from MCP response
    let (output, is_error) = if let Some(content) = result["result"]["content"].as_array() {
        let text = content
            .first()
            .and_then(|c| c["text"].as_str())
            .unwrap_or("No output");
        (
            text.to_string(),
            result["result"]["isError"].as_bool().unwrap_or(false),
        )
    } else if let Some(err) = result["error"].as_object() {
        (
            err["message"]
                .as_str()
                .unwrap_or("Unknown error")
                .to_string(),
            true,
        )
    } else {
        ("Unknown response".into(), true)
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": if is_error { "Error" } else { "Success" },
            "duration_ms": duration_ms,
            "output": output,
            "sandbox": "wasm-fuel-metered"
        })),
    )
        .into_response()
}

pub async fn handle_execute(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TetExecutionRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if req.payload.is_none() && req.parent_snapshot_id.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({"error": "payload is required when not forking", "error_type": "bad_request"}),
            ),
        ));
    }
    if let Some(ref p) = req.payload {
        if p.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"error": "payload must not be empty", "error_type": "bad_request"}),
                ),
            ));
        }
    }
    if req.allocated_fuel == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({"error": "allocated_fuel must be greater than 0", "error_type": "bad_request"}),
            ),
        ));
    }
    if req.max_memory_mb == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({"error": "max_memory_mb must be greater than 0", "error_type": "bad_request"}),
            ),
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
            Json(serde_json::json!({"error": e.to_string(), "error_type": "engine_error"})),
        )),
    }
}

pub async fn handle_snapshot(
    State(state): State<Arc<AppState>>,
    Path(tet_id): Path<String>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    match state.sandbox.snapshot(&tet_id).await {
        Ok(snapshot_id) => Ok((StatusCode::OK, Json(SnapshotResponse { snapshot_id }))),
        Err(e) => Err((
            StatusCode::from_u16(e.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(serde_json::json!({"error": e.to_string(), "error_type": "snapshot_error"})),
        )),
    }
}

pub async fn handle_fork(
    State(state): State<Arc<AppState>>,
    Path(snapshot_id): Path<String>,
    Json(req): Json<TetExecutionRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    match state.sandbox.fork(&snapshot_id, req).await {
        Ok(result) => Ok((StatusCode::OK, Json(result))),
        Err(e) => Err((
            StatusCode::from_u16(e.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(serde_json::json!({"error": e.to_string(), "error_type": "fork_error"})),
        )),
    }
}

pub async fn handle_export(
    State(state): State<Arc<AppState>>,
    Path(snapshot_id): Path<String>,
) -> Response {
    match state.sandbox.export_snapshot(&snapshot_id).await {
        Ok(payload) => (StatusCode::OK, Json(payload)).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Snapshot not found"})),
        )
            .into_response(),
    }
}

pub async fn handle_import(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SnapshotPayload>,
) -> Response {
    match state.sandbox.import_snapshot(payload).await {
        Ok(snapshot_id) => (
            StatusCode::OK,
            Json(serde_json::json!({"snapshot_id": snapshot_id})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn handle_registry_push(
    State(state): State<Arc<AppState>>,
    Path(tag): Path<String>,
    bytes: axum::body::Bytes,
) -> Response {
    match state.registry.push(&tag, &bytes) {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn handle_registry_pull(
    State(state): State<Arc<AppState>>,
    Path(tag): Path<String>,
) -> Response {
    match state.registry.pull(&tag) {
        Ok(Some(bytes)) => (StatusCode::OK, bytes).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn handle_hive_peers(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if let Some(hive) = &state.hive {
        let peers = hive.list_peers().await;
        Ok(Json(
            serde_json::json!({"status": "success", "peers": peers}),
        ))
    } else {
        Err((
            StatusCode::NOT_IMPLEMENTED,
            Json(
                serde_json::json!({"error": "Hive Networking is not enabled on this node", "error_type": "not_implemented"}),
            ),
        ))
    }
}

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
    let hive = state.hive.as_ref().ok_or_else(|| {
        (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "Hive Networking is not enabled on this node", "error_type": "not_implemented"})))
    })?;
    let target_node = hive.get_peer(target_node_id).await.ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("Target node {} not found in local peer list", target_node_id), "error_type": "bad_request"})))
    })?;
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
            "receipt": {"agent_id": receipt.agent_id, "target": receipt.target_address, "bytes": receipt.bytes_transferred}
        }))),
        Err(e) => {
            let status = match e {
                crate::teleport::TeleportError::PermissionDenied => StatusCode::FORBIDDEN,
                crate::teleport::TeleportError::TargetError(_) => StatusCode::BAD_GATEWAY,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            Err((
                status,
                Json(serde_json::json!({"error": e.to_string(), "error_type": "teleport_error"})),
            ))
        }
    }
}

pub async fn handle_ws_stream(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| async move { handle_socket(socket, state).await })
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
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

pub async fn handle_ingress_register(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<crate::oracle::IngressRoute>,
) -> Result<impl IntoResponse, axum::http::StatusCode> {
    let mut map = state.ingress_routes.write().await;
    map.insert(payload.public_path.clone(), payload.clone());
    Ok((StatusCode::OK, Json(payload)))
}

pub async fn handle_ingress_proxy(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
    method: axum::http::Method,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, impl IntoResponse> {
    let mut parts = path.splitn(2, '/');
    let alias = parts.next().unwrap_or_default().to_string();
    let subpath = parts.next().unwrap_or_default().to_string();
    let formatted_path = format!("/{}", subpath);
    let mut header_map = std::collections::HashMap::new();
    for (name, value) in headers.iter() {
        if let Ok(v) = value.to_str() {
            header_map.insert(name.as_str().into(), v.into());
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
            "No Trytet Ingress mapped to this path".into(),
        )),
        Err(e) => Err((StatusCode::BAD_GATEWAY, format!("Gateway Error: {}", e))),
    }
}

pub async fn handle_topology(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, axum::http::StatusCode> {
    let edges = state.sandbox.get_topology().await;
    Ok(Json(edges))
}

pub async fn handle_memory_query(
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

pub async fn handle_infer(
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
    use crate::studio::StudioOrchestrator;
    let orchestrator = match StudioOrchestrator::new(&body) {
        Ok(o) => o,
        Err(e) => return Err((StatusCode::BAD_REQUEST, format!("Invalid Manifest: {}", e))),
    };
    match orchestrator.up(state.sandbox.clone()).await {
        Ok(results) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "success", "agents_booted": results.len(), "results": results
            })),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Orchestration Failed: {}", e),
        )),
    }
}

pub async fn handle_northstar_metrics(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    let report = tokio::task::spawn_blocking(crate::benchmarks::run_full_suite)
        .await
        .unwrap_or_default();
    (StatusCode::OK, Json(report))
}

pub async fn handle_mcp_http(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<impl IntoResponse, impl IntoResponse> {
    let mcp = state.mcp_server.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "MCP server not initialized".to_string(),
        )
    })?;
    let response_bytes = mcp.handle_http_request(body.as_bytes()).await;
    let result: Result<_, (StatusCode, String)> = Ok((
        StatusCode::OK,
        [("content-type", "application/json")],
        response_bytes,
    ));
    result
}

// ---------------------------------------------------------------------------
// Auth handlers
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub struct CreateKeyRequest {
    pub label: String,
}

/// Create a new API key. Returns the raw key — shown only once.
pub async fn handle_create_key(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateKeyRequest>,
) -> impl IntoResponse {
    let raw = state.key_store.create_key(payload.label);
    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "message": "API key created. Store it securely — it will not be shown again.",
            "key": raw
        })),
    )
}

/// List active API keys (prefix and usage only — never the raw key).
pub async fn handle_list_keys(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let keys: Vec<_> = state.key_store.list().into_iter().map(|(prefix, count, label)| {
        serde_json::json!({"prefix": prefix, "invocations": count, "label": label})
    }).collect();
    (StatusCode::OK, Json(serde_json::json!({"keys": keys})))
}

/// Revoke an API key by its prefix.
pub async fn handle_revoke_key(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(prefix): axum::extract::Path<String>,
) -> impl IntoResponse {
    if state.key_store.revoke(&prefix) {
        (
            StatusCode::OK,
            Json(serde_json::json!({"status": "revoked", "prefix": prefix})),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "key not found", "prefix": prefix})),
        )
    }
}
