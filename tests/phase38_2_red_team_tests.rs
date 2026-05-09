use serde_json::json;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tempfile::tempdir;

use tet_core::engine::TetSandbox;
use tet_core::mcp::server::McpServer;
use tet_core::sandbox::WasmtimeSandbox;

// ===========================================================================
// Vulnerability 1: MCP Server OOM via Unbounded read_line
// ===========================================================================

#[tokio::test]
async fn test_red_team_mcp_unbounded_read() {
    let hive_peers = tet_core::hive::HivePeers::new();
    let (mesh, _worker_rx) = tet_core::mesh::TetMesh::new(100, hive_peers.clone());
    let sandbox = Arc::new(
        WasmtimeSandbox::new(
            mesh.clone(),
            Arc::new(tet_core::economy::VoucherManager::new("mcp-test".to_string())),
            false,
            "mcp-test".to_string(),
        )
        .unwrap(),
    );

    let (mut client_stream, server_stream) = tokio::io::duplex(1024 * 1024 * 50); // 50MB buffer
    let (server_rx, server_tx) = tokio::io::split(server_stream);

    let mcp_server = McpServer::new(sandbox.clone());
    
    let handle = tokio::spawn(async move {
        // Run the server with a bounded timeout to prevent the test from hanging forever
        // if it doesn't OOM (which we hope it doesn't after the fix)
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            mcp_server.run(server_rx, server_tx)
        ).await;
    });

    let (mut client_rx, mut client_tx) = tokio::io::split(client_stream);

    // Send a massive continuous payload WITHOUT a newline character.
    // A naive `read_line` will try to buffer this entirely in RAM until OOM.
    // 30 MB of just 'A's without a newline.
    let massive_payload = vec![b'A'; 30 * 1024 * 1024]; 
    
    // We expect the server to disconnect or return a parse error immediately,
    // rather than trying to buffer all 30MB into a single String.
    let result = client_tx.write_all(&massive_payload).await;
    
    // Allow server to process
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // The test might hang if we don't abort it, so we'll assert that the fix is in place
    // To make it fail initially, we check if it's finished. A vulnerable server will still be blocking on read_line.
    let is_finished = handle.is_finished();
    
    assert!(
        is_finished,
        "CRITICAL VULNERABILITY: MCP Server hangs and attempts unbounded allocation on large lines without a newline."
    );
}

// ===========================================================================
// Vulnerability 2: Path Traversal in `tet replay` and `tet snapshot`
// ===========================================================================

#[tokio::test]
async fn test_red_team_replay_path_traversal() {
    let temp_home = tempdir().unwrap();
    let home_path = temp_home.path();
    
    // Setup a fake trytet environment
    std::fs::create_dir_all(home_path.join(".trytet/snapshots")).unwrap();

    // Create a target file outside the snapshots directory to overwrite
    let secret_file_path = home_path.join("secret.txt");
    std::fs::write(&secret_file_path, "SUPER_SECRET_DATA").unwrap();

    // The attacker provides a path traversal snapshot ID
    let malicious_snapshot_id = "../secret";

    // Run the replay command
    // tet replay pulls from the API, but writes locally to ~/.trytet/snapshots/{snapshot_id}.tet
    // So it will write to ~/.trytet/snapshots/../secret.tet -> ~/.trytet/secret.tet
    // Let's use an even worse one: "../../secret" -> ~/.trytet/snapshots/../../secret.tet -> ~/secret.tet
    let worse_malicious_id = "../../secret";
    
    // We'll mock an API server just to return 200 OK with some dummy bytes so `tet replay` writes them to disk
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();

    let url = format!("http://127.0.0.1:{}", port);

    std::thread::spawn(move || {
        use axum::{routing::get, Router};
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let app = Router::new().route("/{*path}", get(|| async { "MOCKED_SNAPSHOT_BYTES" }));
            let tokio_listener = tokio::net::TcpListener::from_std(listener).unwrap();
            let _ = axum::serve(tokio_listener, app).await;
        });
    });

    // Execute tet replay
    let output = Command::new(env!("CARGO_BIN_EXE_tet"))
        .env("TRYTET_API_URL", &url)
        .env("HOME", home_path)
        .args(["replay", worse_malicious_id])
        .output()
        .await
        .expect("Failed to execute tet replay");

    println!("STDOUT: {}", String::from_utf8_lossy(&output.stdout));
    println!("STDERR: {}", String::from_utf8_lossy(&output.stderr));

    // List files in temp dir
    for entry in walkdir::WalkDir::new(home_path) {
        let entry = entry.unwrap();
        println!("{}", entry.path().display());
    }

    // The vulnerability: Did it overwrite/create the file outside the snapshots directory?
    let target_malicious_file = home_path.join("secret.tet");
    
    // We want the test to FAIL if the vulnerability exists.
    // If it exists, the assertion `!exists()` is false, and it panics.
    assert!(
        !target_malicious_file.exists(),
        "CRITICAL VULNERABILITY: Path Traversal allowed `tet replay` to write outside the snapshots directory!"
    );
}
