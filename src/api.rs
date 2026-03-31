//! Axum API layer for the Tet engine.
//!
//! All routes are JSON-in, JSON-out. Error responses use structured
//! JSON payloads with `error` and `error_type` fields — never raw
//! stack traces or HTML error pages.

use crate::engine::TetSandbox;
use crate::models::{SnapshotResponse, TetExecutionRequest};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use std::sync::Arc;
use crate::sandbox::SnapshotPayload;

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
        .route("/health", axum::routing::get(handle_health))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

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
) -> impl IntoResponse {
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
) -> impl IntoResponse {
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
) -> impl IntoResponse {
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
) -> impl IntoResponse {
    match state.registry.pull(&tag) {
        Ok(Some(bytes)) => (StatusCode::OK, bytes).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
