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
    let hive_peers = tet_core::hive::HivePeers::new();
    let (mesh, call_rx) = tet_core::mesh::TetMesh::new(10, hive_peers.clone());
    let sandbox = Arc::new(
        WasmtimeSandbox::new(
            mesh,
            std::sync::Arc::new(tet_core::economy::VoucherManager::new("test".to_string())),
            false,
            "test".to_string(),
        )
        .expect("Failed to create sandbox"),
    );
    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), call_rx);

    let state = Arc::new(AppState {
        sandbox,
        registry: Arc::new(tet_core::registry::LocalRegistry::new().unwrap()),
        registry_client: None,
        hive: Some(hive_peers),
        gateway: Arc::new(tet_core::gateway::SovereignGateway::default()),
        ingress_routes: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
    });
    api::router(state)
}

fn test_app_with_engine(neural_engine: Arc<dyn tet_core::inference::NeuralEngine>) -> axum::Router {
    let hive_peers = tet_core::hive::HivePeers::new();
    let (mesh, call_rx) = tet_core::mesh::TetMesh::new(10, hive_peers.clone());
    let sandbox = Arc::new(
        tet_core::sandbox::WasmtimeSandbox::new_with_engine(
            mesh,
            std::sync::Arc::new(tet_core::economy::VoucherManager::new(
                "test-node".to_string(),
            )),
            false, // Mock payment system by default
            "test-node".to_string(),
            neural_engine,
        )
        .expect("Failed to create sandbox"),
    );
    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), call_rx);

    let state = Arc::new(AppState {
        sandbox,
        registry: Arc::new(tet_core::registry::LocalRegistry::new().unwrap()),
        registry_client: None,
        hive: Some(hive_peers),
        gateway: Arc::new(tet_core::gateway::SovereignGateway::default()),
        ingress_routes: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
    });
    api::router(state)
}

/// Helper: builds a secure Sovereign Test App requiring payment constraints.
fn test_app_sovereign(node_id: &str) -> axum::Router {
    let hive_peers = tet_core::hive::HivePeers::new();
    let (mesh, call_rx) = tet_core::mesh::TetMesh::new(10, hive_peers.clone());
    let sandbox = Arc::new(
        WasmtimeSandbox::new(
            mesh,
            std::sync::Arc::new(tet_core::economy::VoucherManager::new(node_id.to_string())),
            true, // REQUIRE PAYMENT
            node_id.to_string(),
        )
        .expect("Failed to create sandbox"),
    );
    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), call_rx);

    let state = Arc::new(AppState {
        sandbox,
        registry: Arc::new(tet_core::registry::LocalRegistry::new().unwrap()),
        registry_client: None,
        hive: Some(hive_peers),
        gateway: Arc::new(tet_core::gateway::SovereignGateway::default()),
        ingress_routes: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
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
        voucher: None,
        egress_policy: None,
        target_function: None,
        manifest: None,
    }
}

// ===========================================================================
// Phase 1: API Routing & Validation
// ===========================================================================

#[tokio::test]
#[ignore]
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
        voucher: None,
        egress_policy: None,
        target_function: None,
        manifest: None,
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
        voucher: None,
        egress_policy: None,
        target_function: None,
        manifest: None,
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
        voucher: None,
        egress_policy: None,
        target_function: None,
        manifest: None,
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
    let hive_peers = tet_core::hive::HivePeers::new();
    let (mesh, call_rx) = tet_core::mesh::TetMesh::new(10, hive_peers.clone());
    let sandbox = Arc::new(
        WasmtimeSandbox::new(
            mesh,
            std::sync::Arc::new(tet_core::economy::VoucherManager::new("test".to_string())),
            false,
            "test".to_string(),
        )
        .expect("Failed to create sandbox"),
    );
    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), call_rx);

    let state = Arc::new(AppState {
        sandbox,
        registry: Arc::new(tet_core::registry::LocalRegistry::new().unwrap()),
        registry_client: None,
        hive: Some(hive_peers),
        gateway: Arc::new(tet_core::gateway::SovereignGateway::default()),
        ingress_routes: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
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
                .uri(format!("/v1/tet/snapshot/{}", tet_id))
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
        voucher: None,
        egress_policy: None,
        target_function: None,
        manifest: None,
    };

    let app = api::router(state.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(format!("/v1/tet/fork/{}", snap_result.snapshot_id))
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

// ===========================================================================
// Phase 6: The Sovereign Gateway (Economic Routing)
// ===========================================================================

#[tokio::test]
async fn test_phase_11_economic_rejection() {
    let app = test_app_sovereign("node_alpha");
    let mut req = make_request("(module (func (export \"_start\")))", 100);
    // Explicitly do not attach a voucher.
    req.voucher = None;

    let axum_req = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&req).unwrap()))
        .unwrap();

    let res = app.oneshot(axum_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK); // Executes and bounces back safely

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let result: TetExecutionResult = serde_json::from_slice(&body).unwrap();

    // Assert strictly crashes
    match result.status {
        tet_core::models::ExecutionStatus::Crash(report) => {
            assert_eq!(report.error_type, "EconomicViolation");
            assert!(report.message.contains("Missing Fuel Voucher"));
        }
        _ => panic!("Expected Economic Crash, got {:?}", result.status),
    }
}

#[tokio::test]
async fn test_phase_12_ransom_test() {
    // 1. Setup cryptographics
    use ed25519_dalek::SigningKey;
    use rand_core::OsRng;
    use signature::Signer;
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let pub_key = signing_key.verifying_key();
    let agent_id_hex = hex::encode(pub_key.as_bytes());
    let node_id = "node_alpha".to_string();

    let app = test_app_sovereign(&node_id);

    // Infinite loop module
    let wat = r#"
    (module
        (func $start
            (loop $spin
                br $spin
            )
        )
        (export "_start" (func $start))
    )
    "#;

    let mut req = make_request(wat, 5); // very small fuel allocation!
    req.alias = Some("kidnapped_agent".to_string());

    // Assemble the cryptographic signed voucher
    let mut signed_data = Vec::new();
    signed_data.extend_from_slice(agent_id_hex.as_bytes());
    signed_data.extend_from_slice(node_id.as_bytes());
    let fuel_limit: u64 = 5;
    let expiry_timestamp: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 1000;
    let nonce = "init_nonce_1".to_string();

    signed_data.extend_from_slice(&fuel_limit.to_be_bytes());
    signed_data.extend_from_slice(&expiry_timestamp.to_be_bytes());
    signed_data.extend_from_slice(nonce.as_bytes());

    let signature_bytes = signing_key.sign(&signed_data).to_vec();

    req.voucher = Some(tet_core::economy::FuelVoucher {
        agent_id: agent_id_hex.clone(),
        provider_id: node_id.clone(),
        fuel_limit,
        expiry_timestamp,
        nonce,
        signature: signature_bytes,
    });

    let axum_req = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&req).unwrap()))
        .unwrap();

    // 2. Execute! (It should immediately trap OutOfFuel)
    let res = app.clone().oneshot(axum_req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let result: TetExecutionResult = serde_json::from_slice(&body).unwrap();

    assert_eq!(result.status, tet_core::models::ExecutionStatus::OutOfFuel);

    // 3. Issue the Top-Up Ransom!
    let topup_fuel_limit: u64 = 1_000_000;
    let topup_nonce = "topup_nonce_2".to_string();

    let mut topup_signed_data = Vec::new();
    topup_signed_data.extend_from_slice(agent_id_hex.as_bytes());
    topup_signed_data.extend_from_slice(node_id.as_bytes());
    topup_signed_data.extend_from_slice(&topup_fuel_limit.to_be_bytes());
    topup_signed_data.extend_from_slice(&expiry_timestamp.to_be_bytes());
    topup_signed_data.extend_from_slice(topup_nonce.as_bytes());

    let topup_signature = signing_key.sign(&topup_signed_data).to_vec();

    let topup_req = tet_core::api::TopUpRequest {
        alias: "kidnapped_agent".to_string(),
        voucher: tet_core::economy::FuelVoucher {
            agent_id: agent_id_hex,
            provider_id: node_id,
            fuel_limit: topup_fuel_limit,
            expiry_timestamp,
            nonce: topup_nonce,
            signature: topup_signature,
        },
    };

    let axum_topup = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/topup")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&topup_req).unwrap(),
        ))
        .unwrap();

    let topup_res = app.oneshot(axum_topup).await.unwrap();
    assert_eq!(topup_res.status(), StatusCode::OK);

    let topup_body = topup_res.into_body().collect().await.unwrap().to_bytes();
    let topup_result: TetExecutionResult = serde_json::from_slice(&topup_body).unwrap();

    assert_eq!(
        topup_result.status,
        tet_core::models::ExecutionStatus::OutOfFuel
    );
    assert!(
        topup_result.fuel_consumed > 100_000,
        "Should have burned the top-up fuel limit!"
    );
}
#[tokio::test]
async fn test_phase_13_swarm_topographer() {
    let app = test_app();

    // Module importing `trytet::invoke` and making a multi-agent RPC
    let wat = r#"
    (module
        (import "trytet" "invoke" (func $invoke (param i32 i32 i32 i32 i32 i32 i64) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "AgentTarget")
        (data (i32.const 20) "SwarmPing!")
        
        (func $start
            ;; call invoke(target=0:11, payload=20:10, out=50, out_len=100, fuel=10)
            (call $invoke
                (i32.const 0)  ;; target_ptr
                (i32.const 11) ;; target_len
                (i32.const 20) ;; payload_ptr
                (i32.const 10) ;; payload_len
                (i32.const 50) ;; out_ptr
                (i32.const 100) ;; out_len_ptr
                (i64.const 10) ;; fuel
            )
            drop
        )
        (export "_start" (func $start))
    )
    "#;

    let mut req = make_request(wat, 5_000_000);
    req.alias = Some("AgentSource".to_string());

    let axum_req = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&req).unwrap()))
        .unwrap();

    // 1. Execute agent that pings the target
    let res = app.clone().oneshot(axum_req).await.unwrap();
    assert_eq!(res.status(), 200);

    // 2. Query the Native Topology
    let topo_req = axum::http::Request::builder()
        .method("GET")
        .uri("/v1/topology")
        .body(axum::body::Body::empty())
        .unwrap();

    let topo_res = app.clone().oneshot(topo_req).await.unwrap();
    assert_eq!(topo_res.status(), 200);

    let body = topo_res.into_body().collect().await.unwrap().to_bytes();
    let edges: Vec<tet_core::models::TopologyEdge> = serde_json::from_slice(&body).unwrap();

    assert_eq!(edges.len(), 1, "Expected exactly 1 telemetry edge!");
    let edge = &edges[0];

    assert_eq!(edge.source, "AgentSource");
    assert_eq!(edge.target, "AgentTarget");
    assert_eq!(edge.call_count, 1);
    assert_eq!(edge.total_bytes, 10);
    assert!(edge.total_latency_us > 0);
    assert!(edge.last_seen_ns > 0);
}

#[tokio::test]
async fn test_phase_14_secure_egress() {
    let app = test_app();

    // Module importing `trytet::fetch`
    let wat = r#"
    (module
        (import "trytet" "fetch" (func $fetch (param i32 i32 i32 i32 i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "https://google.com")
        (data (i32.const 30) "GET")
        
        (func $start
            ;; call fetch(url=0:18, method=30:3, body=0:0, out=100, out_len=150)
            (call $fetch
                (i32.const 0)  ;; url_ptr
                (i32.const 18) ;; url_len
                (i32.const 30) ;; method_ptr
                (i32.const 3)  ;; method_len
                (i32.const 0)  ;; body_ptr
                (i32.const 0)  ;; body_len
                (i32.const 100) ;; out_ptr
                (i32.const 150) ;; out_len_ptr
            )
            drop
        )
        (export "_start" (func $start))
    )
    "#;

    // 1. First execution WITHOUT google.com in egress_policy
    let mut req = make_request(wat, 5_000_000);
    req.egress_policy = Some(tet_core::oracle::EgressPolicy {
        allowed_domains: vec!["trytet.com".to_string()],
        max_daily_bytes: 1024 * 1024,
        require_https: true,
    });

    let axum_req = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&req).unwrap()))
        .unwrap();

    let res = app.clone().oneshot(axum_req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let result: tet_core::models::TetExecutionResult = serde_json::from_slice(&body).unwrap();

    // Assert 1: Domain was not in allow-list, so it should have Crashed with SecurityViolation!
    match &result.status {
        tet_core::models::ExecutionStatus::Crash(cr) => {
            assert_eq!(cr.error_type, "security_violation");
        }
        _ => panic!(
            "Expected Crash(security_violation), got: {:?}",
            result.status
        ),
    }
}

#[tokio::test]
#[ignore]
async fn test_phase_15_legacy_bridge() {
    let app = test_app();

    // Register the ingress route
    let route = tet_core::oracle::IngressRoute {
        public_path: "/v1/chat".to_string(),
        target_alias: "oracle-agent".to_string(),
        method_filter: vec!["POST".to_string()],
    };

    let axum_req = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/ingress/register")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&route).unwrap()))
        .unwrap();

    let res = app.clone().oneshot(axum_req).await.unwrap();
    assert_eq!(res.status(), 200);

    // Target Agent WASM: Echos the input back to host.
    let wat = r#"
    (module
        (memory (export "memory") 1)
        (func $start
            ;; In a real scenario it would extract RPC payload and process. We just return Success.
            nop
        )
        (export "_start" (func $start))
    )
    "#;

    // Deploy it
    let mut req = make_request(wat, 5_000_000);
    req.alias = Some("oracle-agent".to_string());
    let axum_execute = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&req).unwrap()))
        .unwrap();
    let _ = app.clone().oneshot(axum_execute).await.unwrap();

    // Now call the proxy endpoint!
    let axum_proxy = axum::http::Request::builder()
        .method("POST")
        .uri("/ingress/v1/chat") // Note: /ingress prefix is forced by router!
        .header("content-type", "application/json")
        .body(axum::body::Body::from("hello agent!"))
        .unwrap();

    let res = app.clone().oneshot(axum_proxy).await.unwrap();
    assert_eq!(
        res.status(),
        200,
        "Should successfully proxy to active mesh worker"
    );
}

#[tokio::test]
#[ignore]
async fn test_phase_16_semantic_persistence() {
    let app = test_app();

    let wat1 = r#"
    (module
        (import "trytet" "remember" (func $remember (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "default")
        (data (i32.const 100) "{\"id\":\"fact_1\",\"vector\":[0.80,0.20],\"metadata\":{\"key\":\"val\"}}")
        (func $start
            (call $remember (i32.const 0) (i32.const 7) (i32.const 100) (i32.const 61))
            (if (i32.eq (i32.const 0))
                (then)
                (else (unreachable))
            )
        )
        (export "_start" (func $start))
    )
    "#;

    // 1. Execute parent
    let mut req1 = make_request(wat1, 5_000_000);
    req1.alias = Some("thinker".to_string());

    let axum_execute = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&req1).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(axum_execute).await.unwrap();
    assert_eq!(res.status(), 200, "Remembering execution failed");

    // 2. Snapshot the Tet
    let snap_req = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/snapshot/thinker")
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(snap_req).await.unwrap();
    let snap_res: tet_core::models::SnapshotResponse = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    let parent_snap = snap_res.snapshot_id;

    // 3. Fork into new agent
    let fork_req = tet_core::models::TetExecutionRequest {
        payload: None, // Uses Parent's WASM inherently
        alias: Some("forked-thinker".to_string()),
        env: std::collections::HashMap::new(),
        injected_files: std::collections::HashMap::new(),
        allocated_fuel: 5_000_000,
        max_memory_mb: 64,
        parent_snapshot_id: Some(parent_snap),
        voucher: None,
        call_depth: 0,
        egress_policy: None,
        target_function: None,
        manifest: None,
    };

    let axum_fork = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&fork_req).unwrap(),
        ))
        .unwrap();
    let res = app.clone().oneshot(axum_fork).await.unwrap();
    assert_eq!(res.status(), 200, "Fork execution failed");

    // 4. Query the Forked agent's memory using our endpoint
    let query_payload = tet_core::memory::SearchQuery {
        collection: "default".to_string(),
        query_vector: vec![0.85, 0.15],
        limit: 1,
        min_score: 0.1, // Relaxed score
    };

    let axum_query = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/memory/forked-thinker")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&query_payload).unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(axum_query).await.unwrap();
    assert_eq!(res.status(), 200, "Memory query failed");

    let results: Vec<tet_core::memory::SearchResult> = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        results.len(),
        1,
        "Expected to find 1 matched semantic result from the snapshot"
    );
    assert_eq!(results[0].id, "fact_1", "Expected right matching vector ID");
}

// ===========================================================================
// Phase 10: The Sovereign Inference
// ===========================================================================

#[tokio::test]
async fn test_phase_17_sovereign_inference() {
    let app = test_app();

    // WAT module that:
    // 1. Calls trytet::model_load("default", "/mock/path")
    // 2. Calls trytet::model_predict with a JSON InferenceRequest asking "What is 2+2?"
    // 3. The response is written to Wasm memory at offset 2048 with length at offset 4096
    let wat = r#"
    (module
        (import "trytet" "model_load" (func $model_load (param i32 i32 i32 i32) (result i32)))
        (import "trytet" "model_predict" (func $model_predict (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 2)
        ;; model alias "default" at offset 0
        (data (i32.const 0) "default")
        ;; model path at offset 16
        (data (i32.const 16) "/mock/model.gguf")
        ;; InferenceRequest JSON at offset 100
        (data (i32.const 100) "{\"model_alias\":\"default\",\"prompt\":\"What is 2+2?\",\"temperature\":0.0,\"max_tokens\":64,\"stop_sequences\":[]}")
        ;; Buffer size (4 bytes LE) at offset 4096 = 8192 (0x00002000)
        (data (i32.const 4096) "\00\20\00\00")
        (func $start
            ;; 1. Load the model: model_load("default"=7, "/mock/model.gguf"=16)
            (call $model_load (i32.const 0) (i32.const 7) (i32.const 16) (i32.const 16))
            drop
            ;; 2. Predict: request at 100, len ~100, output buffer at 2048, length ptr at 4096
            (call $model_predict (i32.const 100) (i32.const 100) (i32.const 2048) (i32.const 4096))
            drop
        )
        (export "_start" (func $start))
    )
    "#;

    // Execute the agent
    let mut req = make_request(wat, 5_000_000);
    req.alias = Some("brainy-agent".to_string());

    let axum_execute = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&req).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(axum_execute).await.unwrap();
    assert_eq!(res.status(), 200, "Inference execution request failed");

    let result: TetExecutionResult = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        result.status,
        tet_core::models::ExecutionStatus::Success,
        "Agent should succeed: {:?}",
        result.telemetry
    );

    // Fuel should have been consumed (model_load=10000 + inference tokens)
    assert!(
        result.fuel_consumed > 10_000,
        "Should have burned fuel for model load + inference, got: {}",
        result.fuel_consumed
    );

    // Now query the inference endpoint directly via the API
    let infer_request = tet_core::inference::InferenceRequest {
        model_alias: "default".to_string(),
        prompt: "What is 2+2?".to_string(),
        temperature: 0.0,
        max_tokens: 64,
        stop_sequences: Vec::new(),
        session_id: None,
        deterministic_seed: 42,
    };

    let axum_infer = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/infer/brainy-agent")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&infer_request).unwrap(),
        ))
        .unwrap();
    let res = app.clone().oneshot(axum_infer).await.unwrap();
    assert_eq!(res.status(), 200, "API inference endpoint failed");

    let response: tet_core::inference::InferenceResponse = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();

    // The MockNeuralEngine returns "The answer is 4." for prompts containing "2+2"
    assert!(
        response.text.contains("4"),
        "Response should contain '4', got: {}",
        response.text
    );
    assert!(
        response.tokens_generated > 0,
        "Should have generated tokens"
    );
    assert!(response.fuel_burned > 0, "Should have burned fuel");

    // Verify fuel formula: (prompt_tokens * W_IN) + (generated_tokens * W_OUT)
    let expected_fuel = tet_core::inference::InferenceFuelCalculator::calculate(
        response.prompt_tokens,
        response.tokens_generated,
    );
    assert_eq!(
        response.fuel_burned, expected_fuel,
        "Fuel should match the deterministic formula"
    );
}

#[tokio::test]
async fn test_phase_18_teleported_thought() {
    let app = test_app();

    // Step 1: Boot an agent that loads a model and runs inference
    let wat = r#"
    (module
        (import "trytet" "model_load" (func $model_load (param i32 i32 i32 i32) (result i32)))
        (import "trytet" "model_predict" (func $model_predict (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 2)
        (data (i32.const 0) "default")
        (data (i32.const 16) "/mock/model.gguf")
        (data (i32.const 100) "{\"model_alias\":\"default\",\"prompt\":\"Hello, tell me about\",\"temperature\":0.7,\"max_tokens\":32,\"stop_sequences\":[]}")
        (data (i32.const 4096) "\00\20\00\00")
        (func $start
            (call $model_load (i32.const 0) (i32.const 7) (i32.const 16) (i32.const 16))
            drop
            (call $model_predict (i32.const 100) (i32.const 113) (i32.const 2048) (i32.const 4096))
            drop
        )
        (export "_start" (func $start))
    )
    "#;

    let mut req = make_request(wat, 5_000_000);
    req.alias = Some("thought-agent".to_string());

    let axum_execute = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&req).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(axum_execute).await.unwrap();
    assert_eq!(res.status(), 200);

    // Step 2: Snapshot the agent (captures inference_state)
    let snap_req = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/snapshot/thought-agent")
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(snap_req).await.unwrap();
    assert_eq!(res.status(), 200);
    let snap_res: SnapshotResponse = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();

    // Verify the snapshot contains inference_state
    let export_req = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/v1/tet/export/{}", snap_res.snapshot_id))
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(export_req).await.unwrap();
    assert_eq!(res.status(), 200);
    let payload: tet_core::sandbox::SnapshotPayload = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        !payload.inference_state.is_empty(),
        "Snapshot should contain serialized inference sessions"
    );

    // Step 3: Fork into a new agent (simulating teleportation)
    let fork_req = TetExecutionRequest {
        payload: None,
        alias: Some("teleported-thought".to_string()),
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel: 5_000_000,
        max_memory_mb: 64,
        parent_snapshot_id: Some(snap_res.snapshot_id),
        voucher: None,
        call_depth: 0,
        egress_policy: None,
        target_function: None,
        manifest: None,
    };

    let axum_fork = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/execute")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&fork_req).unwrap(),
        ))
        .unwrap();
    let res = app.clone().oneshot(axum_fork).await.unwrap();
    assert_eq!(res.status(), 200, "Fork/teleport execution failed");

    // Step 4: Query inference on the teleported agent — the model should still work
    let infer_request = tet_core::inference::InferenceRequest {
        model_alias: "default".to_string(),
        prompt: "What is 2+2?".to_string(),
        temperature: 0.0,
        max_tokens: 32,
        stop_sequences: Vec::new(),
        session_id: None,
        deterministic_seed: 42,
    };

    let axum_infer = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/infer/teleported-thought")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&infer_request).unwrap(),
        ))
        .unwrap();
    let res = app.clone().oneshot(axum_infer).await.unwrap();
    assert_eq!(res.status(), 200, "Teleported inference should succeed");

    let response: tet_core::inference::InferenceResponse = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        response.text.contains("4"),
        "Teleported agent should still produce correct inference"
    );
    assert!(
        response.fuel_burned > 0,
        "Teleported inference should consume fuel"
    );
}

/// Helper function to asynchronously download the model and display progress.
async fn download_test_model(url: &str, dest_path: &std::path::Path) -> anyhow::Result<()> {
    use futures_util::StreamExt;
    use std::io::Write;

    if dest_path.exists() {
        println!("Model already exists at {:?}", dest_path);
        return Ok(());
    }

    println!("Downloading test model Qwen2.5 0.5B (350MB)...");
    let response = reqwest::get(url).await?;
    let total_size = response.content_length().unwrap_or(0);

    let mut file = std::fs::File::create(dest_path)?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    let pb = indicatif::ProgressBar::new(total_size);
    pb.set_style(indicatif::ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .unwrap()
        .progress_chars("#>-"));

    while let Some(item) = stream.next().await {
        let chunk = item?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message("Download complete!");
    Ok(())
}

#[tokio::test]
#[ignore = "Downloads 350MB model and runs real ML inference"]
async fn test_phase_17_real_smoke() {
    let _ = tracing_subscriber::fmt::try_init();

    // 1. Download Model
    let model_url = "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf";
    let model_dir = std::path::PathBuf::from("/tmp/trytet_models");
    std::fs::create_dir_all(&model_dir).unwrap();
    let model_path = model_dir.join("qwen2.5-0.5b-instruct-q4_k_m.gguf");

    download_test_model(model_url, &model_path).await.unwrap();

    // 2. Initialize Real LlamaEngine
    println!("Initializing LlamaCppEngine and loading model...");
    let engine = std::sync::Arc::new(tet_core::llama_engine::LlamaCppEngine::new());
    tet_core::inference::NeuralEngine::load_model(&*engine, "qwen", model_path.to_str().unwrap())
        .await
        .unwrap();

    // 3. Prepare App
    let app = test_app_with_engine(engine.clone());

    // 4. Test inference directly through the Engine HTTP API
    let infer_request = tet_core::inference::InferenceRequest {
        model_alias: "qwen".to_string(),
        prompt: "The capital of France is ".to_string(),
        temperature: 0.0,
        max_tokens: 10,
        stop_sequences: Vec::new(),
        session_id: None,
        deterministic_seed: 42,
    };

    let axum_infer = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/tet/infer/qwen")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&infer_request).unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(axum_infer).await.unwrap();
    assert_eq!(res.status(), 200, "Execute failed");

    let inf_res: tet_core::inference::InferenceResponse = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();

    println!("Neural Output:");
    println!("Prompt: The capital of France is ");
    println!("Response: {}", inf_res.text);
    println!("Fuel Burned: {}", inf_res.fuel_burned);
    println!("Generated Tokens: {}", inf_res.tokens_generated);

    assert!(!inf_res.text.is_empty(), "Model should generate some text");
    assert!(inf_res.fuel_burned > 0, "Engine did not register fuel burn");
}
