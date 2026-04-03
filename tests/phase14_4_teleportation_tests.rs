use tet_core::models::manifest::{AgentManifest, Metadata, ResourceConstraints, CapabilityPolicy};
use tet_core::models::{TetExecutionRequest, ExecutionStatus};
use tet_core::sandbox::WasmtimeSandbox;
use tet_core::hive::{HivePeers, HiveServer, HiveClient, HiveCommand};
use tet_core::teleport::{TeleportRequest, TeleportError};
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::test]
async fn test_teleport_permission_denied() {
    let _ = tracing_subscriber::fmt::try_init();
    let peers = HivePeers::new();
    let (mesh, _rx) = tet_core::mesh::TetMesh::new(1024, peers.clone());
    let node_id = "test-node-1".to_string();
    let voucher_manager = Arc::new(tet_core::economy::VoucherManager::new(node_id.clone()));
    let sandbox = Arc::new(WasmtimeSandbox::new(mesh, voucher_manager, false, node_id).unwrap());
    
    let manifest = AgentManifest {
        metadata: Metadata {
            name: "no-teleport-agent".to_string(),
            version: "1.0.0".to_string(),
            author_pubkey: None,
        },
        constraints: ResourceConstraints {
            max_memory_pages: 16,
            fuel_limit: 1000000,
        },
        permissions: CapabilityPolicy {
            can_egress: vec![],
            can_persist: false,
            can_teleport: false, // DENIED
        },
    };

    let wasm_bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    let req = TetExecutionRequest {
        payload: Some(wasm_bytes.clone()),
        alias: Some("no-teleport-agent".to_string()),
        env: std::collections::HashMap::new(),
        injected_files: std::collections::HashMap::new(),
        allocated_fuel: 1000000,
        max_memory_mb: 64,
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        manifest: Some(manifest),
        egress_policy: None,
    };

    use tet_core::engine::TetSandbox;
    let _ = sandbox.execute(req).await.expect("Sandbox execution failed");

    let teleport_req = TeleportRequest {
        agent_id: "no-teleport-agent".to_string(),
        target_address: "127.0.0.1:9999".to_string(),
        use_registry: false,
    };

    let result = teleport_req.execute(sandbox.clone() as Arc<dyn TetSandbox>, None).await;
    dbg!(&result);
    assert!(matches!(result, Err(TeleportError::PermissionDenied)));
}

#[tokio::test]
async fn test_teleport_atomic_handoff() {
    // 1. Start Target Hive Node
    let target_peers = HivePeers::new();
    let (target_mesh, _target_rx) = tet_core::mesh::TetMesh::new(1024, target_peers.clone());
    let target_node_id = "target-node".to_string();
    let target_voucher_manager = Arc::new(tet_core::economy::VoucherManager::new(target_node_id.clone()));
    let target_sandbox = Arc::new(WasmtimeSandbox::new(target_mesh, target_voucher_manager, false, target_node_id).unwrap());
    let target_server = HiveServer::new(target_peers);
    
    // Find free port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_port = listener.local_addr().unwrap().port();
    drop(listener);

    target_server.start(target_port, target_sandbox.mesh.clone(), target_sandbox.clone()).await.unwrap();

    // 2. Start Source Node and Counter Agent
    let source_peers = HivePeers::new();
    let (source_mesh, _source_rx) = tet_core::mesh::TetMesh::new(1024, source_peers.clone());
    let source_node_id = "source-node".to_string();
    let source_voucher_manager = Arc::new(tet_core::economy::VoucherManager::new(source_node_id.clone()));
    let source_sandbox = Arc::new(WasmtimeSandbox::new(source_mesh, source_voucher_manager, false, source_node_id).unwrap());
    
    let manifest = AgentManifest {
        metadata: Metadata {
            name: "counter-agent".to_string(),
            version: "1.0.0".to_string(),
            author_pubkey: None,
        },
        constraints: ResourceConstraints {
            max_memory_pages: 16,
            fuel_limit: 1000000,
        },
        permissions: CapabilityPolicy {
            can_egress: vec![],
            can_persist: false,
            can_teleport: true, // ALLOWED
        },
    };

    let wasm_bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    let req = TetExecutionRequest {
        payload: Some(wasm_bytes.clone()),
        alias: Some("counter-agent".to_string()),
        env: std::collections::HashMap::new(),
        injected_files: std::collections::HashMap::new(),
        allocated_fuel: 1000000,
        max_memory_mb: 64,
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        manifest: Some(manifest),
        egress_policy: None,
    };

    use tet_core::engine::TetSandbox;
    let _ = source_sandbox.execute(req).await.expect("Source execution failed");

    // 3. Teleport!
    let teleport_req = TeleportRequest {
        agent_id: "counter-agent".to_string(),
        target_address: format!("127.0.0.1:{}", target_port),
        use_registry: false,
    };

    let receipt = teleport_req.execute(source_sandbox.clone() as Arc<dyn TetSandbox>, None).await.expect("Teleport failed");
    assert!(receipt.bytes_transferred > 0);

    // 4. Verify resurrection on target node
    // Wait a bit for the async task to finish resurrection
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    
    let resolved: Option<tet_core::models::TetMetadata> = target_sandbox.resolve_local("counter-agent").await;
    assert!(resolved.is_some(), "Agent should exist on target node");
    
    // 5. Verify purge on source node
    let resolved_source: Option<tet_core::models::TetMetadata> = source_sandbox.resolve_local("counter-agent").await;
    assert!(resolved_source.is_none(), "Agent should be purged from source node");
}
