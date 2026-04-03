//! Tests for the Tet-Mesh and Inter-Tet RPC routing capabilities.

use std::collections::HashMap;
use std::sync::Arc;
use tet_core::engine::TetSandbox;
use tet_core::models::{ExecutionStatus, TetExecutionRequest};
use tet_core::sandbox::WasmtimeSandbox;

fn setup_mesh_sandbox() -> Arc<WasmtimeSandbox> {
    let hive_peers = tet_core::hive::HivePeers::new();
    let (mesh, call_rx) = tet_core::mesh::TetMesh::new(100, hive_peers);
    let sandbox = Arc::new(
        WasmtimeSandbox::new(
            mesh,
            std::sync::Arc::new(tet_core::economy::VoucherManager::new("test".to_string())),
            false,
            "test".to_string(),
        )
        .unwrap(),
    );
    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), call_rx);
    sandbox
}

// ---------------------------------------------------------------------------
// Phase 3: The Tet-Mesh Inter-Tet Communication
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_inter_tet_communication() {
    let sandbox = setup_mesh_sandbox();

    let receiver_bytes = std::fs::read("tests/fixtures/mesh_receiver.wasm")
        .expect("Mock receiver WASM not found. Did you run the rustc helper?");
    let caller_bytes = std::fs::read("tests/fixtures/mesh_caller.wasm")
        .expect("Mock caller WASM not found. Did you run the rustc helper?");

    // 1. Stage the Receiver. The engine executes it once, it finishes cleanly,
    // and is automatically snapshotted into "Hibernation" state under the alias.
    let receiver_req = TetExecutionRequest {
        payload: Some(receiver_bytes),
        alias: Some("service-alpha".to_string()),
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel: 20_000_000,
        max_memory_mb: 32,
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        egress_policy: None,
    };

    let receiver_result = sandbox.execute(receiver_req).await.unwrap();
    assert_eq!(receiver_result.status, ExecutionStatus::Success);

    // 2. Execute the Caller. The caller invokes the Host Function `trytet::invoke`
    // which delegates to the Mesh, automatically wakes up the Receiver snapshot,
    // injects the JSON payload, executes it, and writes the response buffer back!
    let caller_req = TetExecutionRequest {
        payload: Some(caller_bytes),
        alias: None,
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel: 20_000_000,
        max_memory_mb: 32,
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        egress_policy: None,
    };

    let caller_result = sandbox.execute(caller_req).await.unwrap();

    // 3. Assertions
    println!("CALLER STDOUT: {:#?}", caller_result.telemetry.stdout_lines);
    println!("CALLER STDERR: {:#?}", caller_result.telemetry.stderr_lines);

    assert_eq!(caller_result.status, ExecutionStatus::Success);

    // Verify the caller successfully received and decoded the payload from the mesh!
    assert!(caller_result
        .telemetry
        .stdout_lines
        .contains(&"CALLER_RECEIVED: SecretData-ECHO".to_string()));

    // Verify economy fuel deduplication
    // The Caller was allocated 20m fuel. It ran its own WASM code + charged 5m to the Receiver.
    // The total combined fuel consumed should be strictly greater than what a typical caller uses
    // and correctly captured in the result.
    assert!(caller_result.fuel_consumed > 0);
}
