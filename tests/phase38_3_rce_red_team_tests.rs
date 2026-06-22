use reqwest::Client;
use std::sync::Arc;
use tet_core::api::{router, AppState};
use tet_core::config::Config;
use tet_core::mesh::TetMesh;
use tet_core::registry::LocalRegistry;
use tet_core::sandbox::WasmtimeSandbox;

fn test_registry_dir() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("tet-p38-3-reg-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&p);
    p
}

fn test_config() -> Config {
    Config {
        registry_path: test_registry_dir(),
        base_tet_path: None,
        database_url: None,
        registry_url: None,
        registry_token: None,
        cors_origin: None,
        fly_region: "test".to_string(),
        trytet_cartridge_dir: test_registry_dir().to_string_lossy().to_string(),
        trytet_api_url: "http://localhost:3000".to_string(),
    }
}

async fn spawn_mock_api_server() -> String {
    let config = test_config();
    let registry = Arc::new(LocalRegistry::new(config.registry_path.clone()).unwrap());
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
        gateway: Arc::new(tet_core::gateway::Gateway::default()),
        ingress_routes: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        mcp_server: None,
        key_store: Arc::new(tet_core::auth::KeyStore::new()),
        config,
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

    format!("http://127.0.0.1:{}", actual_port)
}

#[tokio::test]
#[ignore = "Targets deleted playground Node.js benchmark endpoint — no longer relevant"]
async fn test_red_team_node_rce_vulnerability() {
    let url = spawn_mock_api_server().await;
    let client = Client::new();

    // The attacker tries to read the host's environment variables (e.g. DATABASE_URL)
    let malicious_snippet = "process.env.PATH";

    let res = client
        .post(format!("{}/v1/benchmark/node", url))
        .json(&serde_json::json!({
            "snippet": malicious_snippet,
            "timeout_ms": 1000
        }))
        .send()
        .await
        .expect("Failed to send request");

    let data: serde_json::Value = res.json().await.unwrap();

    let output = data["output"].as_str().unwrap_or("");
    let status = data["status"].as_str().unwrap_or("");

    // If the server is using `eval()`, `process` is globally available, and the output will contain the PATH.
    // If the server is using `vm.runInNewContext()`, `process` is undefined, returning a ReferenceError.

    println!("Status: {}", status);
    println!("Output: {}", output);

    assert!(
        status == "Error" && output.contains("process is not defined"),
        "CRITICAL VULNERABILITY: Node benchmark endpoint allowed Remote Code Execution (RCE) by evaluating `process`. Output: {}",
        output
    );
}
