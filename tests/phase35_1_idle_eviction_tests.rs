use std::sync::Arc;
use tet_core::engine::TetSandbox;
use tet_core::mesh::TetMesh;
use tet_core::models::{ExecutionStatus, TetExecutionRequest};
use tet_core::sandbox::WasmtimeSandbox;
use tet_core::economy::VoucherManager;
use tet_core::hive::HivePeers;

#[tokio::test]
async fn test_phase35_idle_eviction_and_resume() {
    let peers = HivePeers::new();
    let (mesh, _rx) = TetMesh::new(100, peers);
    let vm = Arc::new(VoucherManager::new("test".to_string()));
    let sandbox = Arc::new(
        WasmtimeSandbox::new(mesh.clone(), vm, false, "test_node".to_string()).unwrap(),
    );

    // 1. Build a Wasm module that writes some state, calls `trytet::suspend`, then upon resume finishes state.
    // We use a global variable to track if we've resumed.
    let wat = r#"
    (module
        (import "trytet" "suspend" (func $suspend))
        (memory (export "memory") 1)
        (global $resumed (mut i32) (i32.const 0))
        
        (func (export "_start")
            ;; Check if we are resuming (global should be 0 on first boot)
            ;; But wait, globals are NOT preserved in snapshot by default (only linear memory is "Git for RAM").
            ;; So we must use linear memory to track state!
            
            (if (i32.eq (i32.load (i32.const 0)) (i32.const 42))
                (then
                    ;; Resumed! Write 84 to memory and return cleanly.
                    (i32.store (i32.const 4) (i32.const 84))
                )
                (else
                    ;; First run. Write 42 to memory.
                    (i32.store (i32.const 0) (i32.const 42))
                    
                    ;; Call suspend! This should evict the agent from RAM.
                    (call $suspend)
                    
                    ;; Unreachable, because suspend should trap/exit.
                    unreachable
                )
            )
        )
    )
    "#;
    
    let wasm_bytes = wat::parse_str(wat).unwrap();

    let manifest = tet_core::models::manifest::AgentManifest {
        metadata: tet_core::models::manifest::Metadata {
            name: "idle-evict-agent".to_string(),
            version: "1.0".to_string(),
            author_pubkey: None,
        },
        constraints: tet_core::models::manifest::ResourceConstraints {
            max_memory_pages: 10,
            fuel_limit: 100000,
            max_egress_bytes: 0,
        },
        permissions: tet_core::models::manifest::CapabilityPolicy {
            can_egress: vec![],
            can_persist: true,
            can_teleport: true,
            is_genesis_factory: false,
            can_fork: false,
        },
    };

    let req = TetExecutionRequest {
        payload: Some(wasm_bytes.clone()),
        alias: Some("idle-evict-agent".to_string()),
        allocated_fuel: 50000,
        max_memory_mb: 10,
        env: Default::default(),
        injected_files: Default::default(),
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        egress_policy: None,
        target_function: None,
        manifest: Some(manifest.clone()),
    };

    // 2. Execute First Phase
    let (res1, snapshot1) = sandbox.boot_artifact(&wasm_bytes, &req, None).await.unwrap();
    
    // Assert status is Suspended
    assert_eq!(res1.status, ExecutionStatus::Suspended);
    
    // Validate memory was preserved up to the suspend call (contains 42 at index 0)
    let memory = &snapshot1.memory_bytes;
    let mut val_bytes = [0u8; 4];
    val_bytes.copy_from_slice(&memory[0..4]);
    assert_eq!(i32::from_le_bytes(val_bytes), 42);

    // 3. Resume Execution from Snapshot
    // The VFS and Linear memory are restored.
    // The engine re-invokes _start.
    
    let snap_id = sandbox.import_snapshot(snapshot1).await.unwrap();
    let req2 = TetExecutionRequest {
        payload: None,
        alias: Some("idle-evict-agent".to_string()),
        allocated_fuel: 50000,
        max_memory_mb: 10,
        env: Default::default(),
        injected_files: Default::default(),
        parent_snapshot_id: Some(snap_id),
        call_depth: 0,
        voucher: None,
        egress_policy: None,
        target_function: None,
        manifest: Some(manifest.clone()),
    };

    // Use fork, which imports parent snapshot
    let res2 = sandbox.execute(req2).await.unwrap();

    // Assert it finished successfully
    assert_eq!(res2.status, ExecutionStatus::Success);

    // To verify memory mutation on resume, we can inspect the active snapshot in registry
    let final_snap_id = sandbox.snapshot(&res2.tet_id).await.unwrap();
    let final_payload = sandbox.export_snapshot(&final_snap_id).await.unwrap();
    let mut val2_bytes = [0u8; 4];
    val2_bytes.copy_from_slice(&final_payload.memory_bytes[4..8]);
    assert_eq!(i32::from_le_bytes(val2_bytes), 84);
}