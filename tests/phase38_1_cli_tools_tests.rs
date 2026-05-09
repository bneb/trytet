use std::fs;
use std::sync::Arc;
use tet_core::api::{router, AppState};
use tet_core::mesh::TetMesh;
use tet_core::registry::LocalRegistry;
use tet_core::sandbox::WasmtimeSandbox;
use tokio::process::Command;
use tempfile::tempdir;
use bincode::Options;

async fn spawn_mock_api_server() -> u16 {
    let registry = Arc::new(LocalRegistry::new().unwrap());
    let hive_peers = tet_core::hive::HivePeers::new();
    let (mesh, _worker_rx) = TetMesh::new(100, hive_peers.clone());
    let sandbox = Arc::new(
        WasmtimeSandbox::new(
            mesh.clone(),
            std::sync::Arc::new(tet_core::economy::VoucherManager::new("test".to_string())),
            false,
            "test".to_string(),
        )
        .unwrap(),
    );

    let app_state = Arc::new(AppState {
        sandbox: sandbox.clone(),
        registry,
        registry_client: None,
        hive: Some(hive_peers.clone()),
        gateway: Arc::new(tet_core::gateway::SovereignGateway::default()),
        ingress_routes: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
    });
    let app = router(app_state);

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let actual_port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let tokio_listener = tokio::net::TcpListener::from_std(listener).unwrap();
            let _ = axum::serve(tokio_listener, app).await;
        });
    });

    actual_port
}

// ===========================================================================
// Test 1: BYOL Build Pipeline (`tet build`)
// ===========================================================================

#[tokio::test]
async fn test_phase38_byol_build_pipeline() {
    let temp_dir = tempdir().unwrap();
    let js_path = temp_dir.path().join("agent.js");
    fs::write(&js_path, "console.log('Hello from BYOL');").unwrap();
    
    let out_tet_path = temp_dir.path().join("output_agent.tet");

    let output = Command::new(env!("CARGO_BIN_EXE_tet"))
        .args([
            "build",
            js_path.to_str().unwrap(),
            "-o",
            out_tet_path.to_str().unwrap(),
        ])
        .output()
        .await
        .expect("Failed to execute tet build");

    assert!(
        output.status.success(),
        "Build command failed: STDOUT: {}\nSTDERR: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(out_tet_path.exists());

    // Verify it deserializes
    let tet_bytes = fs::read(&out_tet_path).unwrap();
    let parsed: tet_core::models::TetExecutionRequest = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .allow_trailing_bytes()
        .deserialize(&tet_bytes)
        .expect("Generated .tet artifact is not a valid TetExecutionRequest");

    assert_eq!(parsed.alias.unwrap(), "byol-agent");
    assert!(parsed.injected_files.contains_key("agent.js"));
    assert_eq!(parsed.injected_files.get("agent.js").unwrap(), "console.log('Hello from BYOL');");
}

// ===========================================================================
// Test 2: Time-Travel Replay Debugger (`tet replay`)
// ===========================================================================

#[tokio::test]
async fn test_phase38_replay_debugger() {
    let port = spawn_mock_api_server().await;
    let url = format!("http://127.0.0.1:{}", port);

    let temp_home = tempdir().unwrap();

    // 1. Execute a fast basic module to get a snapshot
    let wasm = r#"
        (module
            (memory (export "memory") 1)
            (func $start (export "_start")
                i32.const 0
                i32.const 42
                i32.store
            )
        )
    "#;
    let wasm_bytes = wat::parse_str(wasm).unwrap();
    let wasm_path = temp_home.path().join("test.wasm");
    fs::write(&wasm_path, wasm_bytes).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_tet"))
        .env("TRYTET_API_URL", &url)
        .env("HOME", temp_home.path())
        .args(["run", wasm_path.to_str().unwrap(), "--alias", "replay-test"])
        .output()
        .await
        .expect("Failed to execute tet run");
    
    assert!(output.status.success());

    // 2. Create a snapshot directly via API to capture the exact snapshot_id
    let req_client = reqwest::Client::new();
    let snap_res = req_client
        .post(&format!("{}/v1/tet/snapshot/replay-test", url))
        .send()
        .await
        .expect("Failed to call snapshot API");
        
    assert!(snap_res.status().is_success());
    let snap_data: serde_json::Value = snap_res.json().await.unwrap();
    let snapshot_id = snap_data["snapshot_id"].as_str().unwrap().to_string();

    // 3. Replay the snapshot
    let replay_out = Command::new(env!("CARGO_BIN_EXE_tet"))
        .env("TRYTET_API_URL", &url)
        .env("HOME", temp_home.path())
        .args(["replay", &snapshot_id])
        .output()
        .await
        .expect("Failed to execute tet replay");

    assert!(
        replay_out.status.success(),
        "Replay command failed: STDOUT: {}\nSTDERR: {}",
        String::from_utf8_lossy(&replay_out.stdout),
        String::from_utf8_lossy(&replay_out.stderr)
    );

    let replay_logs = String::from_utf8_lossy(&replay_out.stdout);
    assert!(replay_logs.contains("Time-Travel Debugger Initialized"));
    assert!(replay_logs.contains("Status: Success"));
}