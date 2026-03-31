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

    // -- Initialize the pre-warmed sandbox and Tet-Mesh -------------------------
    let (mesh, call_rx) = tet_core::mesh::TetMesh::new(100);
    let sandbox = WasmtimeSandbox::new(mesh).expect("Failed to initialize Wasmtime engine");
    tracing::info!("Wasmtime engine pre-warmed with async support and Wasm Fuel enabled");

    let sandbox = Arc::new(sandbox);

    // -- Spawn the Tet-Mesh worker to route Inter-Tet RPC calls -----------------
    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), call_rx);
    tracing::info!("Tet-Mesh worker running to route inter-agent RPC calls");

    // -- Build the Axum router --------------------------------------------------
    let state = Arc::new(AppState {
        sandbox: sandbox.clone(),
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
