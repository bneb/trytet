use anyhow::Result;
use serde_json::json;
use std::sync::Arc;
use tet_core::mcp::server::McpServer;
use tet_core::sandbox::WasmtimeSandbox;
use tokio::io;

pub async fn mcp_cmd(list_tools: bool) -> Result<()> {
    let hive_peers = tet_core::hive::HivePeers::new();
    let (mesh, worker_rx) = tet_core::mesh::TetMesh::new(100, hive_peers.clone());
    let sandbox = Arc::new(
        WasmtimeSandbox::new(
            mesh.clone(),
            Arc::new(tet_core::economy::VoucherManager::new(
                "mcp-server".to_string(),
            )),
            false,
            "mcp-server".to_string(),
        )
        .expect("failed to initialize sandbox"),
    );

    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), worker_rx);

    let mcp_server = McpServer::new(sandbox.clone());

    if list_tools {
        let req = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}});
        let body = serde_json::to_vec(&req)?;
        let resp = mcp_server.handle_http_request(&body).await;
        let parsed: serde_json::Value = serde_json::from_slice(&resp)?;
        if let Some(tools) = parsed["result"]["tools"].as_array() {
            for tool in tools {
                println!(
                    "  {} — {}",
                    tool["name"].as_str().unwrap_or("?"),
                    tool["description"].as_str().unwrap_or("")
                );
            }
            println!("  ({} tools)", tools.len());
        }
        return Ok(());
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    mcp_server.run(stdin, stdout).await?;
    Ok(())
}
