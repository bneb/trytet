//! API-level integration tests for the Tet engine.
//!
//! These tests exercise the full stack: HTTP request → Axum handler →
//! WasmtimeSandbox → Wasmtime → structured JSON response.
//!
//! All Wasm modules are compiled inline from WAT (WebAssembly Text Format)
//! using the `wat` crate — no external .wasm files needed.

use axum::http::StatusCode;
use http_body_util::BodyExt;
use std::collections::HashMap;
use std::sync::Arc;
use tet_core::api::{self, AppState};
use tet_core::models::{SnapshotResponse, TetExecutionRequest, TetExecutionResult};
use tet_core::sandbox::WasmtimeSandbox;
use tower::ServiceExt;

/// Helper: builds a test Axum app with a real WasmtimeSandbox.
fn test_app() -> axum::Router {
    let (mesh, call_rx) = tet_core::mesh::TetMesh::new(10);
    let sandbox = Arc::new(WasmtimeSandbox::new(mesh).expect("Failed to create sandbox"));
    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), call_rx);

    let state = Arc::new(AppState {
        sandbox,
    });
    api::router(state)
}

/// Helper: compiles WAT to Wasm bytes.
fn wat_to_wasm(wat: &str) -> Vec<u8> {
    wat::parse_str(wat).expect("Invalid WAT")
}

/// Helper: builds a TetExecutionRequest from WAT source.
fn make_request(wat: &str, allocated_fuel: u64) -> TetExecutionRequest {
    TetExecutionRequest {
        payload: Some(wat_to_wasm(wat)),
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel,
        max_memory_mb: 16,
        parent_snapshot_id: None,
        alias: None,
        call_depth: 0,
    }
}

// ===========================================================================
// Phase 1: API Routing & Validation
// ===========================================================================

#[tokio::test]
async fn test_health_check() {
    let app = test_app();

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "operational");
    assert_eq!(json["engine"], "tet-core");
}

#[tokio::test]
async fn test_execute_returns_200_on_valid_wasm() {
    let app = test_app();

    // Minimal valid Wasm module — just defines memory and returns from _start
    let req = make_request(
        r#"(module
            (memory (export "memory") 1)
            (func (export "_start"))
        )"#,
        1000,
    );

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/tet/execute")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(serde_json::to_vec(&req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: TetExecutionResult = serde_json::from_slice(&body).unwrap();

    assert_eq!(result.status, tet_core::models::ExecutionStatus::Success);
    assert!(!result.tet_id.is_empty());
    assert!(result.execution_duration_us > 0);
}

#[tokio::test]
async fn test_execute_returns_400_on_empty_payload() {
    let app = test_app();

    let req = TetExecutionRequest {
        payload: Some(vec![]),
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel: 10_000_000,
        max_memory_mb: 16,
        parent_snapshot_id: None,
        alias: None,
        call_depth: 0,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/tet/execute")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(serde_json::to_vec(&req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_execute_returns_400_on_zero_fuel() {
    let app = test_app();

    let req = TetExecutionRequest {
        payload: Some(wat_to_wasm(r#"(module (func (export "_start")))"#)),
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel: 0,
        max_memory_mb: 16,
        parent_snapshot_id: None,
        alias: None,
        call_depth: 0,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/tet/execute")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(serde_json::to_vec(&req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_execute_returns_error_on_invalid_wasm() {
    let app = test_app();

    let req = TetExecutionRequest {
        payload: Some(vec![0x00, 0x61, 0x73, 0x6d, 0xFF, 0xFF]), // Invalid magic
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel: 10_000_000,
        max_memory_mb: 16,
        parent_snapshot_id: None,
        alias: None,
        call_depth: 0,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/tet/execute")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(serde_json::to_vec(&req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Invalid wasm → engine error → 500
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_snapshot_returns_404_for_unknown_tet() {
    let app = test_app();

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/tet/snapshot/nonexistent-tet-id")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ===========================================================================
// Phase 1b: Full Snapshot + Fork API Workflow
// ===========================================================================

#[tokio::test]
async fn test_execute_then_snapshot_then_fork_workflow() {
    let (mesh, call_rx) = tet_core::mesh::TetMesh::new(10);
    let sandbox = Arc::new(WasmtimeSandbox::new(mesh).expect("Failed to create sandbox"));
    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), call_rx);

    let state = Arc::new(AppState {
        sandbox,
    });

    // Step 1: Execute a module that writes to memory
    let wat = r#"(module
        (memory (export "memory") 1)
        (func (export "_start")
            ;; Write 0x42 at memory offset 100
            (i32.store8 (i32.const 100) (i32.const 0x42))
        )
    )"#;

    let req = make_request(wat, 1000);
    let app = api::router(state.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/tet/execute")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(serde_json::to_vec(&req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let exec_result: TetExecutionResult = serde_json::from_slice(&body).unwrap();
    let tet_id = exec_result.tet_id;

    // Step 2: Snapshot the execution
    let app = api::router(state.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(&format!("/v1/tet/snapshot/{}", tet_id))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let snap_result: SnapshotResponse = serde_json::from_slice(&body).unwrap();
    assert!(!snap_result.snapshot_id.is_empty());

    // Step 3: Fork from the snapshot
    // In Phase 2, we can omit the payload if we are forking
    let fork_req = TetExecutionRequest {
        payload: None,
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel: 100000,
        max_memory_mb: 16,
        parent_snapshot_id: Some(snap_result.snapshot_id.clone()),
        alias: None,
        call_depth: 0,
    };
    
    let app = api::router(state.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(&format!("/v1/tet/fork/{}", snap_result.snapshot_id))
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&fork_req).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let fork_result: TetExecutionResult = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        fork_result.status,
        tet_core::models::ExecutionStatus::Success
    );
    // Fork should have a different tet_id
    assert_ne!(fork_result.tet_id, tet_id);
}
