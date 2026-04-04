use axum::{routing::get, Router};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tet_core::engine::TetSandbox;
use tet_core::hive::HivePeers;
use tet_core::mesh::TetMesh;
use tet_core::models::manifest::{AgentManifest, CapabilityPolicy, Metadata, ResourceConstraints};
use tet_core::models::{ExecutionStatus, TetExecutionRequest};
use tet_core::sandbox::WasmtimeSandbox;
use tokio::net::TcpListener;

// Helper to compile a small WAT into WebAssembly that uses the trytet::fetch.
// It will write the URL, Method, and Body directly from memory.
fn compile_fetch_wat(url: &str) -> Vec<u8> {
    let url_len = url.len();
    format!(
        r#"
(module
  (import "trytet" "fetch" (func $fetch (param i32 i32 i32 i32 i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "{url}")
  (data (i32.const 1024) "GET")
  (data (i32.const 2048) "")
  (func (export "_start")
    (call $fetch
      (i32.const 0)    ;; url_ptr
      (i32.const {url_len}) ;; url_len
      (i32.const 1024) ;; method_ptr
      (i32.const 3)    ;; method_len
      (i32.const 2048) ;; body_ptr
      (i32.const 0)    ;; body_len
      (i32.const 4096) ;; out_ptr
      (i32.const 4092) ;; out_len_ptr
    )
    (drop)
  )
)

"#
    )
    .into_bytes()
}

// ----------------------------------------------------
// Mock Network Server
// ----------------------------------------------------

async fn mock_server_app() -> Router {
    Router::new()
        .route(
            "/random",
            get(|| async {
                let num = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64;
                num.to_string()
            }),
        )
        .route(
            "/jitter",
            get(|| async {
                let lag = (SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
                    % 190)
                    + 10;
                tokio::time::sleep(Duration::from_millis(lag as u64)).await;
                "jittered_success"
            }),
        )
}

async fn run_mock_server() -> String {
    let app = mock_server_app().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}", addr)
}

// ----------------------------------------------------
// TDD Case 1: Permission Firewall
// ----------------------------------------------------
#[tokio::test]
async fn test_permission_firewall() {
    let (mesh, _rx) = TetMesh::new(10, HivePeers::new());
    let voucher_mgr = Arc::new(tet_core::economy::VoucherManager::new("t1".to_string()));
    let sandbox =
        Arc::new(WasmtimeSandbox::new(mesh, voucher_mgr, false, "t1".to_string()).unwrap());

    // Allow only api.openai.com
    let manifest = AgentManifest {
        metadata: Metadata {
            name: "test".to_string(),
            version: "1".to_string(),
            author_pubkey: None,
        },
        constraints: ResourceConstraints {
            max_memory_pages: 10,
            fuel_limit: 10_000_000,
            max_egress_bytes: 1_000_000,
        },
        permissions: CapabilityPolicy {
            can_egress: vec!["api.openai.com".to_string()],
            can_persist: false,
            can_teleport: false,
            is_genesis_factory: false,
            can_fork: false,
        },
    };

    let server_url = run_mock_server().await;
    let wat = compile_fetch_wat(&format!("{server_url}/random"));
    let wasm_bytes = wat::parse_bytes(&wat).unwrap().into_owned();

    let req = TetExecutionRequest {
        alias: None,
        payload: Some(wasm_bytes),
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel: 5_000_000,
        max_memory_mb: 64,
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        manifest: Some(manifest),
        egress_policy: None,
        target_function: None,
    };

    let result = sandbox.execute(req).await;
    assert!(result.is_ok());
    let res = result.unwrap();

    // Status Code 6 indicates trap/failure/security violation in fetch.
    match res.status {
        ExecutionStatus::Crash(_) => (),
        _ => panic!("Expected crash but got {:?}", res.status),
    }
}

// ----------------------------------------------------
// TDD Case 2: Oracle Replay
// ----------------------------------------------------
#[tokio::test]
async fn test_oracle_replay() {
    let server_url = run_mock_server().await;
    let _host = server_url.replace("http://", "");

    let (mesh_a, _rx) = TetMesh::new(10, HivePeers::new());
    let v_mgr = Arc::new(tet_core::economy::VoucherManager::new("t1".to_string()));
    let sandbox_a =
        Arc::new(WasmtimeSandbox::new(mesh_a, v_mgr.clone(), false, "t1".to_string()).unwrap());

    let (mesh_b, _rx) = TetMesh::new(10, HivePeers::new());
    let sandbox_b = Arc::new(WasmtimeSandbox::new(mesh_b, v_mgr, false, "t2".to_string()).unwrap());

    let manifest = AgentManifest {
        metadata: Metadata {
            name: "test_replay".to_string(),
            version: "1".to_string(),
            author_pubkey: None,
        },
        constraints: ResourceConstraints {
            max_memory_pages: 10,
            fuel_limit: 10_000_000,
            max_egress_bytes: 1_000_000,
        },
        permissions: CapabilityPolicy {
            can_egress: vec!["127.0.0.1".to_string()],
            can_persist: false,
            can_teleport: false,
            is_genesis_factory: false,
            can_fork: false,
        },
    };

    let wat = compile_fetch_wat(&format!("{server_url}/random"));
    let wasm_bytes = wat::parse_bytes(&wat).unwrap().into_owned();

    // Node A Execution
    let req_a = TetExecutionRequest {
        alias: Some("test_replay".to_string()),
        payload: Some(wasm_bytes.clone()),
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel: 5_000_000,
        max_memory_mb: 64,
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        manifest: Some(manifest.clone()),
        egress_policy: None,
        target_function: None,
    };

    let res_a = sandbox_a.execute(req_a).await.unwrap();
    let fuel_a = res_a.fuel_consumed;

    // Snapshot from A
    let snapshot_id = sandbox_a.snapshot("test_replay").await.unwrap();
    let payload_a = sandbox_a.export_snapshot(&snapshot_id).await.unwrap();

    let snapshot_b_id = sandbox_b.import_snapshot(payload_a).await.unwrap();

    // Resume to Node B using the exact snapshot from Node A
    let req_b = TetExecutionRequest {
        alias: None,
        payload: Some(wasm_bytes.clone()),
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel: 5_000_000,
        max_memory_mb: 64,
        parent_snapshot_id: Some(snapshot_b_id),
        call_depth: 0,
        voucher: None,
        manifest: Some(manifest.clone()),
        egress_policy: None,
        target_function: None,
    };

    let res_b = sandbox_b.execute(req_b).await.unwrap();

    let fuel_b = res_b.fuel_consumed;

    assert_eq!(res_a.status, ExecutionStatus::Success);
    assert_eq!(res_b.status, ExecutionStatus::Success);
    assert_eq!(fuel_a, fuel_b);
}

// ----------------------------------------------------
// TDD Case 3: Fuel Jitter
// ----------------------------------------------------
#[tokio::test]
async fn test_fuel_jitter() {
    let server_url = run_mock_server().await;
    let _host = server_url.replace("http://", "");

    let (mesh, _rx) = TetMesh::new(10, HivePeers::new());
    let v_mgr = Arc::new(tet_core::economy::VoucherManager::new("t1".to_string()));
    let sandbox = Arc::new(WasmtimeSandbox::new(mesh, v_mgr, false, "t1".to_string()).unwrap());

    let manifest = AgentManifest {
        metadata: Metadata {
            name: "test".to_string(),
            version: "1".to_string(),
            author_pubkey: None,
        },
        constraints: ResourceConstraints {
            max_memory_pages: 10,
            fuel_limit: 10_000_000,
            max_egress_bytes: 1_000_000,
        },
        permissions: CapabilityPolicy {
            can_egress: vec!["127.0.0.1".to_string()],
            can_persist: false,
            can_teleport: false,
            is_genesis_factory: false,
            can_fork: false,
        },
    };

    let wat = compile_fetch_wat(&format!("{server_url}/jitter"));
    let wasm_bytes = wat::parse_bytes(&wat).unwrap().into_owned();

    let mut fixed_fuel = 0;

    for i in 0..10 {
        let req = TetExecutionRequest {
            alias: None,
            payload: Some(wasm_bytes.clone()),
            env: HashMap::new(),
            injected_files: HashMap::new(),
            allocated_fuel: 5_000_000,
            max_memory_mb: 64,
            parent_snapshot_id: None,
            call_depth: 0,
            voucher: None,
            manifest: Some(manifest.clone()),
            egress_policy: None,
            target_function: None,
        };
        let res = sandbox.execute(req).await.unwrap();

        if i == 0 {
            fixed_fuel = res.fuel_consumed;
        } else {
            assert_eq!(
                res.fuel_consumed, fixed_fuel,
                "Fuel consumption must be perfectly deterministic despite web latency!"
            );
        }
    }
}
