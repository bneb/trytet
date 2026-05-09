use anyhow::Result;
use std::sync::Arc;
use tokio::io;
use tet_core::sandbox::WasmtimeSandbox;
use tet_core::mcp::server::McpServer;

pub async fn mcp_cmd() -> Result<()> {
    let hive_peers = tet_core::hive::HivePeers::new();
    let (mesh, mut worker_rx) = tet_core::mesh::TetMesh::new(100, hive_peers.clone());
    let sandbox = Arc::new(
        WasmtimeSandbox::new(
            mesh.clone(),
            Arc::new(tet_core::economy::VoucherManager::new("mcp-server".to_string())),
            false,
            "mcp-server".to_string(),
        )
        .unwrap(),
    );

    let sandbox_clone = sandbox.clone();
    tet_core::mesh_worker::spawn_mesh_worker(sandbox_clone, worker_rx);

    // Precompile known cartridges so they are ready to be invoked
    let cartridges = ["js-evaluator", "python-evaluator", "regex-evaluator", "scraper-cartridge", "jmespath-cartridge", "sat-cartridge"];
    let base_dir = std::env::current_dir().unwrap().join("crates");
    
    for c in cartridges {
        let name = c.replace("-", "_");
        let wasm_path = base_dir.join(c).join("target/wasm32-wasip1/release").join(format!("{}.wasm", name));
        if let Ok(wasm) = std::fs::read(&wasm_path) {
            let _ = sandbox.cartridge_manager.precompile(c, &wasm);
        }
    }

    let mcp_server = McpServer::new(sandbox.clone());
    
    let stdin = io::stdin();
    let stdout = io::stdout();

    mcp_server.run(stdin, stdout).await?;

    Ok(())
}
