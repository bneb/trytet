//! Interpreter-level / VFS tests for the Tet engine.
//!
//! These tests exercise the "Git for Disk" ephemeral Virtual Filesystem (VFS)
//! by running a mock Rust->WASI interpreter that reads and writes to `/workspace`.

use std::collections::HashMap;
use tet_core::engine::TetSandbox;
use tet_core::models::{ExecutionStatus, TetExecutionRequest};
use tet_core::sandbox::WasmtimeSandbox;

// ---------------------------------------------------------------------------
// Phase 2: Ephemeral VFS ("Git for Disk")
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_vfs_injection_and_extraction() {
    let (mesh, call_rx) = tet_core::mesh::TetMesh::new(10);
    let sandbox = std::sync::Arc::new(WasmtimeSandbox::new(mesh).unwrap());
    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), call_rx);

    let wasm_bytes = std::fs::read("tests/fixtures/mock_interpreter.wasm")
        .expect("Mock interpreter WASM not found. Did you run rustc?");

    let mut injected_files = HashMap::new();
    injected_files.insert("main.txt".to_string(), "hello world".to_string());

    let req = TetExecutionRequest {
        payload: Some(wasm_bytes.clone()),
        env: HashMap::new(),
        injected_files,
        allocated_fuel: 50_000_000, // Sufficient fuel for a Rust WASI binary
        max_memory_mb: 64,          // Rust needs a bit more memory
        parent_snapshot_id: None,
        alias: None,
        call_depth: 0,
    };

    let result = sandbox.execute(req).await.unwrap();

    println!("STDOUT: {:#?}", result.telemetry.stdout_lines);
    println!("STDERR: {:#?}", result.telemetry.stderr_lines);
    println!("CRASH: {:?}", result.status);

    // The mock interpreter reads main.txt and writes out.txt
    assert_eq!(result.status, ExecutionStatus::Success);

    // Stdout tracking
    assert!(result.telemetry.stdout_lines.contains(&"Read: hello world".to_string()));
    assert!(result.telemetry.stdout_lines.contains(&"Success".to_string()));

    // VFS Extraction tracking ("mutated_files")
    let out_txt = result.mutated_files.get("out.txt").expect("out.txt should have been created");
    assert_eq!(out_txt, "hello world-MODIFIED");
    
    // Check that original was also preserved in the VFS results
    let main_txt = result.mutated_files.get("main.txt").unwrap();
    assert_eq!(main_txt, "hello world");

    // Phase 2 part B: Snapshot the VFS and Fork
    let tet_id = result.tet_id;
    let snapshot_id = sandbox.snapshot(&tet_id).await.unwrap();

    // Now fork, but supply a new injected file to overwrite main.txt
    let mut fork_injected = HashMap::new();
    fork_injected.insert("main.txt".to_string(), "forked data".to_string());

    let fork_req = TetExecutionRequest {
        payload: None, // Use parent's binary
        env: HashMap::new(),
        injected_files: fork_injected,
        allocated_fuel: 50_000_000,
        max_memory_mb: 64,
        parent_snapshot_id: Some(snapshot_id),
        alias: None,
        call_depth: 0,
    };

    let fork_result = sandbox.fork(&fork_req.parent_snapshot_id.as_ref().unwrap(), fork_req.clone()).await.unwrap();

    println!("FORK STDOUT: {:#?}", fork_result.telemetry.stdout_lines);
    println!("FORK STDERR: {:#?}", fork_result.telemetry.stderr_lines);
    println!("FORK CRASH: {:?}", fork_result.status);

    // The fork reinstantiates the Wasm module with the parent's completed memory.
    // For a Rust WASI binary, calling `_start` a second time triggers a libc 
    // `unreachable` panic due to re-entrancy guards. This is expected since our 
    // MVP "Git for RAM" only captures linear memory, not the execution stack.
    if let ExecutionStatus::Crash(report) = &fork_result.status {
        assert_eq!(report.error_type, "unreachable");
    } else {
        panic!("Expected Crash(unreachable) due to libc re-entry guard, got {:?}", fork_result.status);
    }

    // However, Git-for-Disk STILL works!
    // The engine unpacks the parent's VFS tarball BEFORE execution, applies injections,
    // and copies `mutated_files` back OUT even if execution trapped.
    
    // 1. Prove VFS Tarball Extraction: The parent's `out.txt` should be restored natively.
    let fork_out_txt = fork_result.mutated_files.get("out.txt").unwrap();
    assert_eq!(fork_out_txt, "hello world-MODIFIED");

    // 2. Prove File Injection inside Fork overrode the tarballed `main.txt`
    let fork_main_txt = fork_result.mutated_files.get("main.txt").unwrap();
    assert_eq!(fork_main_txt, "forked data");
}
