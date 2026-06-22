//! Server startup — extracted from main.rs so both `tet-core` and `tet serve` can use it.

use crate::api::{self, AppState};
use crate::config::Config;
use crate::sandbox::WasmtimeSandbox;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

/// Start the API server on 0.0.0.0:3000. Initialize tracing, sandbox, hive,
/// mesh worker, purger, and Axum router. Blocks until the server exits.
pub async fn start(config: Config) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .json()
        .init();

    tracing::info!("Tet Core Engine starting...");

    // Ensure persistent volume mounts exist
    std::fs::create_dir_all(&config.registry_path).ok();
    if let Some(ref base_path) = config.base_tet_path {
        std::fs::create_dir_all(base_path).ok();
    }
    if let Some(ref db_url) = config.database_url {
        if db_url.starts_with("sqlite://") {
            let path_str = db_url.replace("sqlite://", "");
            if let Some(parent) = std::path::Path::new(&path_str).parent() {
                std::fs::create_dir_all(parent).ok();
            }
        }
    }

    let hive_peers = crate::hive::HivePeers::new();
    let (mesh, call_rx) = crate::mesh::TetMesh::new(100, hive_peers.clone());

    let registry_client = if let Some(ref url) = config.registry_url {
        Some(Arc::new(crate::registry::oci::OciClient::new(
            url.clone(),
            config.registry_token.clone(),
        )))
    } else {
        None
    };

    let hive_server =
        crate::hive::HiveServer::new(hive_peers.clone(), registry_client.clone(), None);
    let mesh_clone = mesh.clone();

    let local_node_id = uuid::Uuid::new_v4().to_string();
    let voucher_manager = Arc::new(crate::economy::VoucherManager::new(local_node_id.clone()));

    let telemetry = Arc::new(crate::telemetry::TelemetryHub::default_capacity());

    let sandbox = WasmtimeSandbox::new(mesh, voucher_manager, false, local_node_id)
        // ^ require_payment is false until voucher acquisition is implemented
        .expect("Failed to initialize Wasmtime engine")
        .with_telemetry(telemetry.clone());
    tracing::info!("Wasmtime engine pre-warmed with async support and Wasm Fuel enabled");
    tracing::info!(
        "TelemetryHub initialized ({} subscriber slots)",
        telemetry.subscriber_count()
    );

    let sandbox = Arc::new(sandbox);

    // Spawn Hive P2P Server
    let hive_sandbox = sandbox.clone();
    tokio::spawn(async move {
        if let Err(e) = hive_server.start(2026, mesh_clone, hive_sandbox).await {
            tracing::error!("Hive Server failed: {}", e);
        }
    });

    // Spawn zero-residue purger
    crate::server::purge::spawn_purge_thread(config.registry_path.clone()).await;

    // Spawn the Tet-Mesh worker to route Inter-Tet RPC calls
    crate::mesh_worker::spawn_mesh_worker(sandbox.clone(), call_rx);
    tracing::info!("Tet-Mesh worker running to route inter-agent RPC calls");

    // Build the Axum router
    let registry = Arc::new(
        crate::registry::LocalRegistry::new(config.registry_path.clone()).expect("registry"),
    );
    let mcp_server = crate::mcp::server::McpServer::new(sandbox.clone());

    let key_store = Arc::new(crate::auth::KeyStore::new());
    if !key_store.has_keys() {
        let boot_key = key_store.create_key("boot-admin".into());
        tracing::warn!("══════════════════════════════════════════════════════════════");
        tracing::warn!("  NO API KEYS FOUND — Boot key generated:");
        tracing::warn!("  {}", boot_key);
        tracing::warn!("  Store this key securely. It will not be shown again.");
        tracing::warn!("  Create additional keys via POST /v1/auth/keys");
        tracing::warn!("══════════════════════════════════════════════════════════════");
    }

    let state = Arc::new(AppState {
        sandbox: sandbox.clone(),
        registry,
        registry_client,
        hive: Some(hive_peers),
        gateway: Arc::new(crate::gateway::Gateway::default()),
        ingress_routes: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        mcp_server: Some(mcp_server),
        key_store,
        config,
    });

    let app = api::router(state);

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
