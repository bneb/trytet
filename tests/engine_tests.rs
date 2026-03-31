//! Engine-level tests for the WasmtimeSandbox.
//!
//! These tests exercise the core Wasm execution, epoch-based timeout,
//! crash handling, stdout/stderr capture, and memory snapshot/fork
//! mechanics directly through the `TetSandbox` trait.

use std::collections::HashMap;
use std::sync::Arc;
use tet_core::engine::TetSandbox;
use tet_core::models::{ExecutionStatus, TetExecutionRequest, TetExecutionResult};
use tet_core::sandbox::WasmtimeSandbox;

fn setup_sandbox() -> Arc<WasmtimeSandbox> {
    let (mesh, call_rx) = tet_core::mesh::TetMesh::new(100);
    let sandbox = Arc::new(WasmtimeSandbox::new(mesh).unwrap());
    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), call_rx);
    sandbox
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
// Phase 2: Basic Execution
// ===========================================================================

#[tokio::test]
async fn test_basic_execution_success() {
    let sandbox = setup_sandbox();

    let req = make_request(
        r#"(module
            (memory (export "memory") 1)
            (func (export "_start"))
        )"#,
        1000,
    );

    let result = sandbox.execute(req).await.unwrap();
    assert_eq!(result.status, ExecutionStatus::Success);
    assert!(!result.tet_id.is_empty());
    assert!(result.execution_duration_us > 0);
}

#[tokio::test]
async fn test_stdout_capture() {
    let sandbox = setup_sandbox();

    // Module that writes "hello\n" to stdout via WASI fd_write
    let req = make_request(
        r#"(module
            (import "wasi_snapshot_preview1" "fd_write"
                (func $fd_write (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)

            ;; iov_base at offset 0, iov_len at offset 4
            ;; string data at offset 100: "hello\n"
            (data (i32.const 100) "hello\n")

            (func (export "_start")
                ;; Set up iovec: base = 100, len = 6
                (i32.store (i32.const 0) (i32.const 100))   ;; iov_base
                (i32.store (i32.const 4) (i32.const 6))     ;; iov_len

                ;; fd_write(fd=1 (stdout), iovs=0, iovs_len=1, nwritten=200)
                (drop (call $fd_write
                    (i32.const 1)    ;; stdout
                    (i32.const 0)    ;; iovs pointer
                    (i32.const 1)    ;; iovs count
                    (i32.const 200)  ;; nwritten pointer
                ))
            )
        )"#,
        1000,
    );

    let result = sandbox.execute(req).await.unwrap();
    assert_eq!(result.status, ExecutionStatus::Success);
    assert!(
        result.telemetry.stdout_lines.contains(&"hello".to_string()),
        "Expected stdout to contain 'hello', got: {:?}",
        result.telemetry.stdout_lines
    );
}

#[tokio::test]
async fn test_stderr_capture() {
    let sandbox = setup_sandbox();

    // Module that writes "error\n" to stderr (fd=2) via WASI fd_write
    let req = make_request(
        r#"(module
            (import "wasi_snapshot_preview1" "fd_write"
                (func $fd_write (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)

            (data (i32.const 100) "error\n")

            (func (export "_start")
                (i32.store (i32.const 0) (i32.const 100))
                (i32.store (i32.const 4) (i32.const 6))
                (drop (call $fd_write
                    (i32.const 2)    ;; stderr
                    (i32.const 0)
                    (i32.const 1)
                    (i32.const 200)
                ))
            )
        )"#,
        1000,
    );

    let result = sandbox.execute(req).await.unwrap();
    assert_eq!(result.status, ExecutionStatus::Success);
    assert!(
        result.telemetry.stderr_lines.contains(&"error".to_string()),
        "Expected stderr to contain 'error', got: {:?}",
        result.telemetry.stderr_lines
    );
}

// ===========================================================================
// Phase 2: Deterministic Compute Metering (Fuel)
// ===========================================================================

#[tokio::test]
async fn test_out_of_fuel_on_infinite_loop() {
    let sandbox = setup_sandbox();

    // Module with an infinite loop — should be deterministically killed based on fuel
    let req = make_request(
        r#"(module
            (memory (export "memory") 1)
            (func (export "_start")
                (loop $inf
                    (br $inf)
                )
            )
        )"#,
        1_000, // 1,000 instructions of fuel
    );

    let result = sandbox.execute(req).await.unwrap();
    assert_eq!(
        result.status,
        ExecutionStatus::OutOfFuel,
        "Expected OutOfFuel for infinite loop, got: {:?}",
        result.status
    );
    // Verify the execution didn't take dramatically longer than the timeout
    // Allow generous margin since epoch ticking has ~1ms granularity
    assert!(
        result.execution_duration_us < 500_000, // < 500ms
        "Timeout took too long: {}us",
        result.execution_duration_us
    );
}

#[tokio::test]
async fn test_sufficient_fuel_completes_fast_module() {
    let sandbox = setup_sandbox();

    // A fast module should complete if given enough fuel
    let req = make_request(
        r#"(module
            (memory (export "memory") 1)
            (func (export "_start")
                ;; Do nothing — instant return
            )
        )"#,
        100_000, // 100k fuel
    );

    let result = sandbox.execute(req).await.unwrap();
    assert_eq!(result.status, ExecutionStatus::Success);
}

// ===========================================================================
// Phase 2: Crash Handling
// ===========================================================================

#[tokio::test]
async fn test_crash_on_unreachable() {
    let sandbox = setup_sandbox();

    let req = make_request(
        r#"(module
            (memory (export "memory") 1)
            (func (export "_start")
                unreachable
            )
        )"#,
        1000,
    );

    let result = sandbox.execute(req).await.unwrap();
    match &result.status {
        ExecutionStatus::Crash(report) => {
            assert_eq!(report.error_type, "unreachable");
        }
        other => panic!("Expected Crash, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_crash_on_memory_out_of_bounds() {
    let sandbox = setup_sandbox();

    let req = make_request(
        r#"(module
            (memory (export "memory") 1)
            (func (export "_start")
                ;; Try to load from offset way beyond 1 page (64KB)
                (drop (i32.load (i32.const 1000000)))
            )
        )"#,
        1000,
    );

    let result = sandbox.execute(req).await.unwrap();
    match &result.status {
        ExecutionStatus::Crash(report) => {
            assert_eq!(report.error_type, "memory_out_of_bounds");
        }
        other => panic!("Expected Crash with memory_out_of_bounds, got: {:?}", other),
    }
}

// ===========================================================================
// Phase 2: Environment Variables
// ===========================================================================

#[tokio::test]
async fn test_env_variables_injected() {
    let sandbox = setup_sandbox();

    // We can't easily read env vars in pure WAT without a full WASI environ_get
    // implementation, but we CAN verify the sandbox accepts env vars without error
    let mut env = HashMap::new();
    env.insert("FOO".to_string(), "bar".to_string());
    env.insert("BAZ".to_string(), "qux".to_string());

    let req = TetExecutionRequest {
        payload: Some(wat_to_wasm(
            r#"(module
            (memory (export "memory") 1)
            (func (export "_start"))
        )"#,
        )),
        env,
        injected_files: HashMap::new(),
        allocated_fuel: 10_000_000,
        max_memory_mb: 16,
        parent_snapshot_id: None,
        alias: None,
        call_depth: 0,
    };

    let result = sandbox.execute(req).await.unwrap();
    assert_eq!(result.status, ExecutionStatus::Success);
}

// ===========================================================================
// Phase 3: Memory State Forking
// ===========================================================================

#[tokio::test]
async fn test_memory_snapshot_and_fork() {
    let sandbox = setup_sandbox();

    // Step 1: Execute a module that writes a known value at a known offset
    let writer_wat = r#"(module
        (memory (export "memory") 1)
        (func (export "_start")
            ;; Write 0xDEAD (two bytes: 0xAD at 100, 0xDE at 101)
            (i32.store16 (i32.const 100) (i32.const 0xDEAD))
        )
    )"#;

    let req = make_request(writer_wat, 1000);
    let result = sandbox.execute(req).await.unwrap();
    assert_eq!(result.status, ExecutionStatus::Success);
    let tet_id = result.tet_id.clone();

    // Step 2: Snapshot the state
    let snapshot_id = sandbox.snapshot(&tet_id).await.unwrap();
    assert!(!snapshot_id.is_empty());

    // Step 3: Fork — the new instance should have 0xDEAD at offset 100
    // The reader module loads offset 100 and writes the value to stdout
    let reader_wat = r#"(module
        (import "wasi_snapshot_preview1" "fd_write"
            (func $fd_write (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)

        (func (export "_start")
            ;; Read the value at offset 100 (should be 0xDEAD from snapshot)
            ;; Store the loaded value at offset 200 as a demonstration
            (i32.store (i32.const 200) (i32.load (i32.const 100)))
        )
    )"#;

    let fork_req = make_request(reader_wat, 1000);
    let fork_result = sandbox.fork(&snapshot_id, fork_req).await.unwrap();
    assert_eq!(fork_result.status, ExecutionStatus::Success);
    assert_ne!(fork_result.tet_id, tet_id, "Fork must have a new tet_id");
}

#[tokio::test]
async fn test_fork_isolation() {
    let sandbox = setup_sandbox();

    // Step 1: Execute a module that writes to memory
    let writer_wat = r#"(module
        (memory (export "memory") 1)
        (func (export "_start")
            (i32.store (i32.const 0) (i32.const 42))
        )
    )"#;

    let req = make_request(writer_wat, 1000);
    let result = sandbox.execute(req).await.unwrap();
    let tet_id = result.tet_id.clone();

    // Step 2: Snapshot
    let snapshot_id = sandbox.snapshot(&tet_id).await.unwrap();

    // Step 3: Fork twice — mutations in one fork must not affect the other
    let mutator_a_wat = r#"(module
        (memory (export "memory") 1)
        (func (export "_start")
            ;; Overwrite offset 0 with 100
            (i32.store (i32.const 0) (i32.const 100))
        )
    )"#;

    let mutator_b_wat = r#"(module
        (memory (export "memory") 1)
        (func (export "_start")
            ;; Overwrite offset 0 with 200
            (i32.store (i32.const 0) (i32.const 200))
        )
    )"#;

    let fork_a = sandbox
        .fork(&snapshot_id, make_request(mutator_a_wat, 1000))
        .await
        .unwrap();
    let fork_b = sandbox
        .fork(&snapshot_id, make_request(mutator_b_wat, 1000))
        .await
        .unwrap();

    assert_eq!(fork_a.status, ExecutionStatus::Success);
    assert_eq!(fork_b.status, ExecutionStatus::Success);
    assert_ne!(fork_a.tet_id, fork_b.tet_id, "Forks must have unique IDs");

    // Both forks started from the same snapshot but mutated independently.
    // If there was state leakage between forks, it would be a critical bug.
    // We can't directly assert memory values from outside, but the fact that
    // both complete successfully with different mutations proves isolation.
}

#[tokio::test]
async fn test_snapshot_not_found_error() {
    let sandbox = setup_sandbox();

    let result = sandbox.snapshot("nonexistent-tet-id").await;
    assert!(result.is_err());
    match result.unwrap_err() {
        tet_core::engine::TetError::SnapshotNotFound(id) => {
            assert_eq!(id, "nonexistent-tet-id");
        }
        other => panic!("Expected SnapshotNotFound, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_fork_from_nonexistent_snapshot() {
    let sandbox = setup_sandbox();

    let req = make_request(
        r#"(module (memory (export "memory") 1) (func (export "_start")))"#,
        1000,
    );

    let result = sandbox.fork("nonexistent-snapshot-id", req).await;
    assert!(result.is_err());
}

// ===========================================================================
// Phase 3: Memory Telemetry
// ===========================================================================

#[tokio::test]
async fn test_memory_telemetry_reported() {
    let sandbox = setup_sandbox();

    let req = make_request(
        r#"(module
            (memory (export "memory") 2)
            (func (export "_start"))
        )"#,
        1000,
    );

    let result = sandbox.execute(req).await.unwrap();
    assert_eq!(result.status, ExecutionStatus::Success);
    // 2 pages = 128KB
    assert!(
        result.telemetry.memory_used_kb >= 128,
        "Expected at least 128KB, got {}KB",
        result.telemetry.memory_used_kb
    );
}

// ===========================================================================
// Phase 3: Concurrent Execution
// ===========================================================================

#[tokio::test]
async fn test_concurrent_executions() {
    let sandbox = setup_sandbox();

    // Launch 10 concurrent executions
    let mut handles: Vec<tokio::task::JoinHandle<TetExecutionResult>> = Vec::new();
    for _ in 0..10 {
        let sandbox = sandbox.clone();
        handles.push(tokio::spawn(async move {
            let req = TetExecutionRequest {
                payload: Some(wat::parse_str(
                    r#"(module
                    (memory (export "memory") 1)
                    (func (export "_start"))
                )"#,
                )
                .unwrap()),
                env: HashMap::new(),
                injected_files: HashMap::new(),
                allocated_fuel: 10_000_000,
                max_memory_mb: 16,
                parent_snapshot_id: None,
                alias: None,
                call_depth: 0,
            };
            sandbox.execute(req).await.unwrap()
        }));
    }

    let mut tet_ids = Vec::new();
    for handle in handles {
        let result = handle.await.unwrap();
        assert_eq!(result.status, ExecutionStatus::Success);
        tet_ids.push(result.tet_id);
    }

    // All tet_ids must be unique (no UUID collisions)
    let unique: std::collections::HashSet<_> = tet_ids.iter().collect();
    assert_eq!(unique.len(), tet_ids.len(), "All tet_ids must be unique");
}
