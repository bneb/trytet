//! Tet Core Engine — Main Entrypoint
//!
//! Initializes the pre-warmed Wasmtime engine, spawns the epoch ticker,
//! wires the Axum router, and binds to 0.0.0.0:3000.

use std::sync::Arc;
use tet_core::api::{self, AppState};
use tet_core::sandbox::WasmtimeSandbox;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // -- Structured JSON logging ------------------------------------------------
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    tracing::info!("Tet Core Engine starting...");

    // -- Initialize the pre-warmed sandbox, Hive Peers, and Tet-Mesh -------------------------
    let hive_peers = tet_core::hive::HivePeers::new();
    let (mesh, call_rx) = tet_core::mesh::TetMesh::new(100, hive_peers.clone());
    
    let hive_server = tet_core::hive::HiveServer::new(hive_peers.clone());
    let mesh_clone = mesh.clone();

    let local_node_id = uuid::Uuid::new_v4().to_string(); // In a real node, load from config
    let voucher_manager = Arc::new(tet_core::economy::VoucherManager::new(local_node_id.clone()));
    let sandbox = WasmtimeSandbox::new(mesh, voucher_manager, true, local_node_id).expect("Failed to initialize Wasmtime engine");
    tracing::info!("Wasmtime engine pre-warmed with async support and Wasm Fuel enabled");

    let sandbox = Arc::new(sandbox);
    
    // Spawn Hive P2P Server
    let hive_sandbox = sandbox.clone();
    tokio::spawn(async move {
        if let Err(e) = hive_server.start(2026, mesh_clone, hive_sandbox).await {
            tracing::error!("Hive Server failed: {}", e);
        }
    });

    // -- Spawn the Tet-Mesh worker to route Inter-Tet RPC calls -----------------
    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), call_rx);
    tracing::info!("Tet-Mesh worker running to route inter-agent RPC calls");

    // -- Build the Axum router --------------------------------------------------
    let registry = Arc::new(tet_core::registry::LocalRegistry::new().unwrap());
    let state = Arc::new(AppState {
        sandbox: sandbox.clone(),
        registry,
        hive: Some(hive_peers),
        ingress_routes: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
    });

    let app = api::router(state);

    // -- Bind and serve ---------------------------------------------------------
    let addr = "0.0.0.0:3000";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Tet Core Engine listening on {}", addr);
    tracing::info!("  POST /v1/tet/execute       — Execute a Wasm payload");
    tracing::info!("  POST /v1/tet/snapshot/{{id}} — Snapshot execution state");
    tracing::info!("  POST /v1/tet/fork/{{id}}     — Fork from snapshot");
    tracing::info!("  GET  /health               — Health check");

    axum::serve(listener, app).await?;

    Ok(())
}
