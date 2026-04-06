use tet_core::sandbox::security::{PathJailer, SecurityError, Watchdog};
use tet_core::sandbox::WasmtimeSandbox;
use tet_core::mesh::TetMesh;
use tet_core::economy::VoucherManager;
use tet_core::hive::HivePeers;
use tet_core::models::TetExecutionRequest;
use std::sync::Arc;
use std::path::PathBuf;
use std::time::Duration;

#[tokio::test]
async fn test_path_traversal_jail() {
    let jailer = PathJailer::new(PathBuf::from("/vfs/Agent_Workspace_Root"));
    assert!(
        matches!(jailer.safe_join("../../../etc/shadow"), Err(SecurityError::PathTraversalAttempt)),
        "PathJailer MUST strictly reject direct relative prefix combinations."
    );
    assert!(
        matches!(jailer.safe_join("malicious.txt\0.exe"), Err(SecurityError::PathTraversalAttempt)),
        "PathJailer MUST strictly reject null bytes."
    );
    let valid_path = jailer.safe_join("valid_config.json").unwrap();
    assert!(valid_path.to_string_lossy().contains("Agent_Workspace_Root"), "Path MUST remain jailed.");
}

#[tokio::test]
async fn test_memory_wrap_escape() {
    let (mesh, _rx) = TetMesh::new(10, HivePeers::new());
    let voucher = Arc::new(VoucherManager::new("test".to_string()));
    let sandbox = WasmtimeSandbox::new(mesh, voucher, false, "system".into()).unwrap();

    let wat = r#"
        (module
            (import "trytet" "fetch" (func $fetch (param i32 i32 i32 i32 i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "run_exploit")
                ;; u32::MAX - 1 (which is -2 in i32)
                i32.const -2
                i32.const 100
                i32.const 0
                i32.const 0
                i32.const 0
                i32.const 0
                i32.const 0
                i32.const 0
                call $fetch
                drop
            )
        )
    "#;

    let wasm_bytes = wat::parse_str(wat).unwrap();
    let req = TetExecutionRequest {
        payload: Some(wasm_bytes.clone()),
        alias: Some("test".into()),
        env: Default::default(),
        injected_files: Default::default(),
        allocated_fuel: 1_000_000_000,
        max_memory_mb: 100,
        parent_snapshot_id: None,
        target_function: Some("run_exploit".to_string()),
        call_depth: 0,
        egress_policy: None,
        manifest: None,
        voucher: None,
    };

    let (result, _) = sandbox.boot_artifact(&wasm_bytes, &req, None).await.unwrap();

    match result.status {
        tet_core::models::ExecutionStatus::Crash(rpt) => {
            assert!(rpt.message.contains("OOB Guest Memory"), "Did not receive proper OOB Trap: {:?}", rpt.message);
        },
        _ => panic!("Memory wrap vulnerability exploited! Wasm accessed host memory."),
    }
}

#[tokio::test]
async fn test_inference_dos_preemption() {
    let (mesh, _rx) = TetMesh::new(10, HivePeers::new());
    let voucher = Arc::new(VoucherManager::new("test".to_string()));
    let sandbox = WasmtimeSandbox::new(mesh, voucher, false, "system".into()).unwrap();

    let wat = r#"
        (module
            (import "trytet" "predict" (func $predict (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "run_exploit")
                i32.const 0
                i32.const 10
                call $predict
                drop
            )
        )
    "#;

    let wasm_bytes = wat::parse_str(wat).unwrap();
    let req = TetExecutionRequest {
        payload: Some(wasm_bytes.clone()),
        alias: Some("test".into()),
        env: Default::default(),
        injected_files: Default::default(),
        allocated_fuel: 1_000_000_000,
        max_memory_mb: 100,
        parent_snapshot_id: None,
        target_function: Some("run_exploit".to_string()),
        call_depth: 0,
        egress_policy: None,
        manifest: None,
        voucher: None,
    };

    let (result, _) = sandbox.boot_artifact(&wasm_bytes, &req, None).await.unwrap();
    
    // Check it ended in a crash
    match result.status {
        tet_core::models::ExecutionStatus::Crash(rpt) => {
            assert!(rpt.message.contains("Resource Exhaustion Attempt Detected"), "Must correctly relay SecurityError: {}", rpt.message);
        },
        _ => panic!("GPU/inference watchdog did NOT snap on large timeouts! Expected Crash trap."),
    }
}
