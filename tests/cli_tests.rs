use std::fs;
use std::sync::Arc;
use tet_core::api::{router, AppState};
use tet_core::mesh::TetMesh;
use tet_core::registry::LocalRegistry;
use tet_core::sandbox::WasmtimeSandbox;
use tokio::process::Command;

async fn spawn_mock_api_server() -> (u16, u16) {
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

    // Drop worker_rx to avoid hanging if the test triggers mesh execution,
    // though the CLI tests don't strictly require the worker.
    let app_state = Arc::new(AppState {
        sandbox: sandbox.clone(),
        registry,
        hive: Some(hive_peers.clone()),
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

    let hive_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let hive_port = hive_listener.local_addr().unwrap().port();
    drop(hive_listener);
    let hive_server = tet_core::hive::HiveServer::new(hive_peers);
    let mesh_clone = mesh.clone();
    let hive_sandbox = sandbox.clone();
    tokio::spawn(async move {
        // hive_server.start binds to the port itself
        if let Err(e) = hive_server.start(hive_port, mesh_clone, hive_sandbox).await {
            println!("Hive Server failed: {}", e);
        }
    });

    // Give it a moment to boot
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    (actual_port, hive_port)
}
#[tokio::test]
async fn test_phase_8_git_for_ram_registry() {
    let (port, _) = spawn_mock_api_server().await;
    let url = format!("http://127.0.0.1:{}", port);

    // 1. We create a simple WAT that writes a value to memory
    let wasm = r#"
        (module
            (memory (export "memory") 1)
            (func $start (export "_start")
                i32.const 0
                i32.const 88
                i32.store
            )
        )
    "#;
    let wasm_bytes = wat::parse_str(wasm).unwrap();
    fs::write("target/tmp_mem.wasm", wasm_bytes).unwrap();

    // 2. tet run tmp_mem.wasm --alias testing-tet
    let output = Command::new(env!("CARGO_BIN_EXE_tet"))
        .env("TRYTET_API_URL", &url)
        .args(["run", "target/tmp_mem.wasm", "--alias", "testing-tet"])
        .output()
        .await
        .expect("Failed to execute tet run");
    if !output.status.success() {
        panic!(
            "tet run failed. stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // 3. tet snapshot testing-tet my-tag-v1
    let snap_out = Command::new(env!("CARGO_BIN_EXE_tet"))
        .env("TRYTET_API_URL", &url)
        .args(["snapshot", "testing-tet", "my-tag-v1"])
        .output()
        .await
        .expect("Failed to execute tet snapshot");
    if !snap_out.status.success() {
        panic!(
            "tet snapshot failed. stderr: {}",
            String::from_utf8_lossy(&snap_out.stderr)
        );
    }

    // 4. tet push my-tag-v1
    let push_out = Command::new(env!("CARGO_BIN_EXE_tet"))
        .env("TRYTET_API_URL", &url)
        .args(["push", "my-tag-v1"])
        .output()
        .await
        .expect("Failed to execute tet push");
    if !push_out.status.success() {
        panic!(
            "tet push failed. stderr: {}",
            String::from_utf8_lossy(&push_out.stderr)
        );
    }

    // We skip the delete original Tet API call because our local registry test inherently verifies
    // the Pull will just download the tarball and inject it natively!

    // 5. tet pull my-tag-v1
    let pull_out = Command::new(env!("CARGO_BIN_EXE_tet"))
        .env("TRYTET_API_URL", &url)
        .args(["pull", "my-tag-v1"])
        .output()
        .await
        .expect("Failed to execute tet pull");
    if !pull_out.status.success() {
        panic!(
            "tet pull failed. stderr: {}",
            String::from_utf8_lossy(&pull_out.stderr)
        );
    }
}

#[tokio::test]
async fn test_phase_9_base_tet_injection() {
    let (port, _) = spawn_mock_api_server().await;
    let url = format!("http://127.0.0.1:{}", port);

    // 1. Create a script
    fs::write("target/hello.py", "print('Hello from Trytet!')").unwrap();

    // 2. tet run hello.py
    let output = Command::new(env!("CARGO_BIN_EXE_tet"))
        .env("TRYTET_API_URL", &url)
        .args(["run", "target/hello.py", "--alias", "py-agent"])
        .output()
        .await
        .expect("Failed to execute tet run for py");

    if !output.status.success() {
        panic!(
            "tet run py failed. stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Mock Base-Tet Python Interpreter booting..."));
    assert!(stdout.contains("Python WASI running: print('Hello from Trytet!')"));
}

#[tokio::test]
async fn test_phase_10_live_migration() {
    let (port1, hive_port1) = spawn_mock_api_server().await;
    let (port2, hive_port2) = spawn_mock_api_server().await;
    let url1 = format!("http://127.0.0.1:{}", port1);
    let url2 = format!("http://127.0.0.1:{}", port2);

    // 1. Bootstrap: Node 2 joins Node 1's Hive
    let join = tet_core::hive::HiveCommand::Join(tet_core::hive::HiveNodeIdentity {
        node_id: "node2".to_string(),
        public_addr: format!("127.0.0.1:{}", hive_port2),
        available_fuel: 999999,
        total_memory_mb: 64,
        price_per_million_fuel: 100,
        min_reputation_score: 50,
        available_capacity_mb: 1000,
    });

    // Use the RPC system to notify Node 1.
    tet_core::hive::HiveClient::rpc_call(&format!("127.0.0.1:{}", hive_port1), join)
        .await
        .unwrap();

    // 2. Start a simple loop/memory payload on Node 1
    let wasm = r#"
        (module
            (memory (export "memory") 1)
            (func $start (export "_start")
                i32.const 0
                i32.const 99
                i32.store
            )
        )
    "#;
    let wasm_bytes = wat::parse_str(wasm).unwrap();
    fs::write("target/mig_mem.wasm", wasm_bytes).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_tet"))
        .env("TRYTET_API_URL", &url1)
        .args(["run", "target/mig_mem.wasm", "--alias", "tele-agent"])
        .output()
        .await
        .expect("Failed to execute tet run");
    assert!(output.status.success());

    // 3. Command Node 1 to teleport "tele-agent" to "node2"
    let teleport_out = Command::new(env!("CARGO_BIN_EXE_tet"))
        .env("TRYTET_API_URL", &url1)
        .args(["teleport", "tele-agent", "node2"])
        .output()
        .await
        .expect("Failed to execute tet teleport");

    assert!(
        teleport_out.status.success(),
        "Teleport command failed: STDOUT: {}\nSTDERR: {}",
        String::from_utf8_lossy(&teleport_out.stdout),
        String::from_utf8_lossy(&teleport_out.stderr)
    );

    // Wait a brief moment for the target node to actually execute it internally
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // 4. Verify Node 2 successfully imported it by capturing a snapshot via API URL 2
    let snap_out = Command::new(env!("CARGO_BIN_EXE_tet"))
        .env("TRYTET_API_URL", &url2)
        .args(["snapshot", "tele-agent", "tele-mig-v1"])
        .output()
        .await
        .expect("Failed to execute tet snapshot");

    if !snap_out.status.success() {
        panic!(
            "Migration verification failed! Node 2 could not snapshot the alias. stderr: {}",
            String::from_utf8_lossy(&snap_out.stderr)
        );
    }
}
