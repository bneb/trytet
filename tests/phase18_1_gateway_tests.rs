use std::sync::Arc;
use tet_core::economy::VoucherManager;
use tet_core::engine::TetSandbox;
use tet_core::hive::HivePeers;
use tet_core::mesh::TetMesh;
use tet_core::models::*;
use tet_core::sandbox::*;

use std::collections::HashMap;
use tet_core::gateway::{GatewayRequest, SovereignGateway};
use tet_core::hive::dht::HiveDht;

#[tokio::test]
#[ignore]
async fn test_phase18_local_ingress_registration_and_execution() {
    let (mesh, rx) = TetMesh::new(1000, HivePeers::default());
    let voucher_mgr = Arc::new(VoucherManager::new("t1".to_string()));

    let peers = HivePeers::new();
    let wallet = Arc::new(tet_core::crypto::AgentWallet::load_or_create().unwrap());
    let dht = Arc::new(HiveDht::new(peers, wallet));
    let gateway = Arc::new(SovereignGateway::new(dht));

    let sandbox = Arc::new(
        WasmtimeSandbox::new_with_engine(
            mesh.clone(),
            voucher_mgr,
            false,
            "t1".to_string(),
            Arc::new(tet_core::inference::MockNeuralEngine::new()),
        )
        .unwrap()
        .with_gateway(gateway.clone()),
    );

    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), rx);

    let wat = r#"
    (module
        (import "trytet" "listen" (func $listen (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "/chat")
        (data (i32.const 10) "my_handler")
        (func (export "_start")
            (call $listen (i32.const 0) (i32.const 5) (i32.const 10) (i32.const 10))
            drop
        )
        (func (export "my_handler")
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat).unwrap();
    let toml_str = r#"
        [metadata]
        name = "gateway-agent"
        version = "1.0.0"
        description = "Test"
        author_pubkey = "anon"
        
        [constraints]
        fuel_limit = 10000000
        max_memory_pages = 100
        
        [permissions]
        can_egress = []
        can_persist = false
        can_teleport = false
    "#;
    let manifest = tet_core::models::manifest::AgentManifest::from_toml(toml_str).unwrap();

    let req = TetExecutionRequest {
        payload: Some(wasm_bytes),
        alias: Some("gateway-agent".to_string()),
        allocated_fuel: 1_000_000,
        max_memory_mb: 10,
        env: HashMap::new(),
        injected_files: HashMap::new(),
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        manifest: Some(manifest),
        egress_policy: None,
        target_function: None,
    };

    let result = sandbox.execute(req).await.unwrap();
    assert_eq!(result.status, ExecutionStatus::Success);

    // Verify it was registered in local routes
    let routes = gateway.local_routes.get("gateway-agent").unwrap();
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].path, "/chat");
    assert_eq!(routes[0].handler_func, "my_handler");

    // Mock Gateway Proxy Request
    let proxy_req = GatewayRequest {
        alias: "gateway-agent".to_string(),
        path: "/chat/room1".to_string(),
        method: "POST".to_string(),
        body: b"hello".to_vec(),
        headers: HashMap::new(),
    };

    let proxy_result = gateway.handle_request(proxy_req, sandbox.clone()).await;
    // Should be executed gracefully or return ExecutionFailed if fuel runs out/not snapshot fork.
    // Because we just test mapping, if status is ExecutionFailed(Success), it means it invoked my_handler but had no `rpc_response.json` out.
    // Wait, the handler literally returns nothing and does not populate vector or return data. So it should be Success.
    assert!(
        proxy_result.is_ok(),
        "Response should be Ok with empty bytes, got {:?}",
        proxy_result
    );
    // return_data parsing
    let returned = proxy_result.unwrap();
    assert_eq!(returned.len(), 0);
}
