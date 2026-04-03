use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use axum::{
    routing::{get, put, post},
    Router, Json, extract::{Path, State},
    http::StatusCode,
    body::Bytes,
};
use tet_core::registry::oci::{OciClient, OciManifest};
use tet_core::sandbox::{WasmtimeSandbox};
use tet_core::engine::TetSandbox;
use tet_core::hive::{HivePeers, HiveServer};
use tet_core::mesh::TetMesh;
use tet_core::economy::VoucherManager;
use tet_core::models::TetExecutionRequest;
use std::collections::HashMap;

#[derive(Clone, Default)]
struct MockRegistry {
    blobs: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    manifests: Arc<Mutex<HashMap<String, OciManifest>>>,
}

async fn handle_post_blob(
    State(_reg): State<MockRegistry>,
    Path((name,)): Path<(String,)>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let location = format!("/v2/{}/blobs/uploads/?id=123", name);
    (StatusCode::ACCEPTED, [(axum::http::header::LOCATION, location)]).into_response()
}

async fn handle_head_blob(
    State(reg): State<MockRegistry>,
    Path((_name, digest)): Path<(String, String)>,
) -> StatusCode {
    if reg.blobs.lock().await.contains_key(&digest) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn handle_put_blob(
    State(reg): State<MockRegistry>,
    Path((_name,)): Path<(String,)>,
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
    body: Bytes,
) -> StatusCode {
    if let Some(digest) = query.get("digest") {
        reg.blobs.lock().await.insert(digest.clone(), body.to_vec());
        StatusCode::CREATED
    } else {
        StatusCode::BAD_REQUEST
    }
}

async fn handle_put_manifest(
    State(reg): State<MockRegistry>,
    Path((name, tag)): Path<(String, String)>,
    Json(manifest): Json<OciManifest>,
) -> StatusCode {
    let reference = format!("{}:{}", name, tag);
    reg.manifests.lock().await.insert(reference, manifest);
    StatusCode::CREATED
}

async fn handle_get_manifest(
    State(reg): State<MockRegistry>,
    Path((name, tag)): Path<(String, String)>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let reference = format!("{}:{}", name, tag);
    if let Some(manifest) = reg.manifests.lock().await.get(&reference) {
        Json(manifest.clone()).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn handle_get_blob(
    State(reg): State<MockRegistry>,
    Path((_name, digest)): Path<(String, String)>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    if let Some(blob) = reg.blobs.lock().await.get(&digest) {
        blob.clone().into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

fn create_mock_registry() -> (Router, MockRegistry) {
    let state = MockRegistry::default();
    let app = Router::new()
        .route("/v2/{name}/blobs/{digest}", get(handle_get_blob).head(handle_head_blob))
        .route("/v2/{name}/blobs/uploads/", post(handle_post_blob))
        .route("/v2/{name}/manifests/{tag}", get(handle_get_manifest).put(handle_put_manifest))
        // Put blob upload fallback using /uploads/
        .route("/v2/{name}/blobs/uploads/", put(handle_put_blob))
        .with_state(state.clone());
    (app, state)
}

async fn spawn_mock_registry() -> String {
    let (app, _) = create_mock_registry();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}", addr)
}

#[tokio::test]
async fn test_mediated_lazarus() {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    // 1. Start Mock Registry
    let reg_url = spawn_mock_registry().await;
    let oci_client = Arc::new(OciClient::new(reg_url, None));

    // 2. Setup Network
    let hive_peers = HivePeers::new();
    let port = 8999;
    
    // Disable economic guard
    std::env::set_var("TET_DISABLE_ECONOMIC_GUARD", "1");

    let (mesh, call_rx) = TetMesh::new(10, hive_peers.clone());
    let voucher_manager = Arc::new(VoucherManager::new("target_node".to_string()));
    let target_sandbox = Arc::new(WasmtimeSandbox::new(mesh.clone(), voucher_manager, false, "target_node".to_string()).unwrap());

    // Start Target Hive Server with OciClient
    let target_server = HiveServer::new(hive_peers.clone(), Some(oci_client.clone()), None);
    let target_sandbox_clone = target_sandbox.clone();
    let target_mesh = mesh.clone();
    tokio::spawn(async move {
        target_server.start(port, target_mesh, target_sandbox_clone).await.unwrap();
    });
    tet_core::mesh_worker::spawn_mesh_worker(target_sandbox.clone(), call_rx);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // 3. Setup Source Node
    let (source_mesh, _) = TetMesh::new(10, hive_peers.clone());
    let source_sandbox = Arc::new(WasmtimeSandbox::new(source_mesh.clone(), Arc::new(VoucherManager::new("source".to_string())), false, "source".to_string()).unwrap());

    // 4. Create Agent on Source
    let wasm_bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    use tet_core::models::manifest::{AgentManifest, Metadata, ResourceConstraints, CapabilityPolicy};
    let manifest = AgentManifest {
        metadata: Metadata {
            name: "agent-gamma".to_string(),
            version: "1.0".to_string(),
            author_pubkey: None,
        },
        constraints: ResourceConstraints {
            max_memory_pages: 16,
            fuel_limit: 1000000,
        },
        permissions: CapabilityPolicy {
            can_egress: vec![],
            can_persist: false,
            can_teleport: true,
        },
    };

    let req = TetExecutionRequest {
        alias: Some("agent-gamma".to_string()),
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
    };
    source_sandbox.execute(req).await.unwrap();

    // 5. Teleport Request using Registry
    let teleport_req = tet_core::teleport::TeleportRequest {
        agent_id: "agent-gamma".to_string(),
        target_address: format!("127.0.0.1:{}", port),
        use_registry: true,
    };

    let receipt = teleport_req.execute(source_sandbox.clone() as Arc<dyn TetSandbox>, Some(oci_client)).await.unwrap();
    
    assert_eq!(receipt.target_address, format!("127.0.0.1:{}", port));

    // Wait for target to pull and resurrect
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    // 6. Verify Source is purged, Target is active
    assert!(source_sandbox.resolve_local("agent-gamma").await.is_none());
    assert!(target_sandbox.resolve_local("agent-gamma").await.is_some());
}

#[tokio::test]
async fn test_split_brain_prevention() {
    let _ = tracing_subscriber::fmt::try_init();

    // Setup Network
    let hive_peers = HivePeers::new();
    let port = 8999;
    std::env::set_var("TET_DISABLE_ECONOMIC_GUARD", "1");

    let (source_mesh, _) = TetMesh::new(10, hive_peers.clone());
    let source_sandbox = Arc::new(WasmtimeSandbox::new(source_mesh.clone(), Arc::new(VoucherManager::new("source".to_string())), false, "source".to_string()).unwrap());

    // Create Agent on Source
    let wasm_bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    use tet_core::models::manifest::{AgentManifest, Metadata, ResourceConstraints, CapabilityPolicy};
    let manifest = AgentManifest {
        metadata: Metadata {
            name: "split-brain-agent".to_string(),
            version: "1.0".to_string(),
            author_pubkey: None,
        },
        constraints: ResourceConstraints {
            max_memory_pages: 16,
            fuel_limit: 1000000,
        },
        permissions: CapabilityPolicy {
            can_egress: vec![],
            can_persist: false,
            can_teleport: true,
        },
    };

    let req = TetExecutionRequest {
        alias: Some("split-brain-agent".to_string()),
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
    };
    source_sandbox.execute(req).await.unwrap();

    // Teleport Request to a target that is offline (timeout simulation)
    let teleport_req = tet_core::teleport::TeleportRequest {
        agent_id: "split-brain-agent".to_string(),
        target_address: "127.0.0.1:12345".to_string(), // port 12345 is offline
        use_registry: true,
    };

    let reg_url = spawn_mock_registry().await;
    let oci_client = Arc::new(OciClient::new(reg_url, None));

    let result = teleport_req.execute(source_sandbox.clone() as Arc<dyn TetSandbox>, Some(oci_client)).await;
    assert!(result.is_err());

    // Verify Source still retains the active agent after network failure
    assert!(source_sandbox.resolve_local("split-brain-agent").await.is_some());
}
