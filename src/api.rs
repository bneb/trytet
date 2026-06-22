//! Axum API layer for the Tet engine.
//!
//! All routes are JSON-in, JSON-out. Error responses use structured
//! JSON payloads with `error` and `error_type` fields — never raw
//! stack traces or HTML error pages.
//!
//! Handler functions are in `handlers/all.rs` to keep this file under 400 lines.

use axum::{extract::DefaultBodyLimit, middleware, routing::post, Router};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

pub mod console;
pub mod context;
mod handlers;
pub use handlers::all::{CartridgeInvokeRequest, SandboxBenchmarkRequest, TopUpRequest};

use crate::auth::KeyStore;
use crate::config::Config;
use crate::mcp::server::McpServer;

// ---------------------------------------------------------------------------
// Application State
// ---------------------------------------------------------------------------

/// Shared application state injected into every Axum handler.
pub struct AppState {
    pub sandbox: Arc<dyn crate::engine::TetSandbox>,
    pub registry: Arc<dyn crate::registry::Registry>,
    pub registry_client: Option<Arc<crate::registry::oci::OciClient>>,
    pub hive: Option<crate::hive::HivePeers>,
    pub gateway: Arc<crate::gateway::Gateway>,
    pub ingress_routes: Arc<RwLock<HashMap<String, crate::oracle::IngressRoute>>>,
    pub mcp_server: Option<McpServer>,
    pub key_store: Arc<KeyStore>,
    pub config: Config,
}

// ---------------------------------------------------------------------------
// Router Construction
// ---------------------------------------------------------------------------

/// Builds the Axum router with all Tet API routes.
///
/// `/v1/*` routes are protected by API key authentication.
/// `/health`, `/console`, `/v1/mcp`, and `/ingress/*` are public.
pub fn router(state: Arc<AppState>) -> Router {
    use handlers::all::*;

    let key_store = state.key_store.clone();

    // Protected routes (require API key)
    let protected = Router::new()
        .route("/v1/tet/execute", post(handle_execute))
        .route("/v1/tet/snapshot/{tet_id}", post(handle_snapshot))
        .route("/v1/tet/fork/{snapshot_id}", post(handle_fork))
        .route(
            "/v1/tet/export/{snapshot_id}",
            axum::routing::get(handle_export),
        )
        .route("/v1/tet/import", post(handle_import))
        .route("/v1/registry/push/{tag}", post(handle_registry_push))
        .route(
            "/v1/registry/pull/{tag}",
            axum::routing::get(handle_registry_pull),
        )
        .route("/v1/hive/peers", axum::routing::get(handle_hive_peers))
        .route("/v1/tet/teleport/{alias}", post(handle_teleport))
        .route("/v1/swarm/stream", axum::routing::get(handle_ws_stream))
        .route("/v1/tet/topup", post(handle_topup))
        .route("/v1/cartridge/invoke", post(handle_cartridge_invoke))
        .route("/v1/benchmark/sandbox", post(handle_sandbox_benchmark))
        .route("/v1/ingress/register", post(handle_ingress_register))
        .route("/v1/tet/memory/{alias}", post(handle_memory_query))
        .route("/v1/tet/infer/{alias}", post(handle_infer))
        .route("/v1/topology", axum::routing::get(handle_topology))
        .route("/v1/swarm/up", post(handle_swarm_up))
        .route(
            "/v1/swarm/metrics",
            axum::routing::get(handle_northstar_metrics),
        )
        .route("/v1/auth/keys", axum::routing::get(handle_list_keys))
        .route("/v1/auth/keys", post(handle_create_key))
        .route(
            "/v1/auth/keys/{prefix}",
            axum::routing::delete(handle_revoke_key),
        )
        .route_layer(middleware::from_fn_with_state(
            key_store,
            crate::auth::require_api_key,
        ))
        .with_state(state.clone());

    // Public routes (no auth required)
    let public = Router::new()
        .route(
            "/health",
            axum::routing::get(crate::server::health::handle_health),
        )
        .route("/console", axum::routing::get(console::serve_console_page))
        .route("/v1/mcp", post(handle_mcp_http))
        .route("/ingress/{*path}", axum::routing::any(handle_ingress_proxy))
        .with_state(state.clone());

    // CORS: configurable via CORS_ORIGIN env var, defaults to permissive for local dev
    let cors = state
        .config
        .cors_origin
        .clone()
        .map(|origin| {
            CorsLayer::new()
                .allow_origin(tower_http::cors::AllowOrigin::exact(
                    origin.parse().expect("invalid CORS_ORIGIN env var"),
                ))
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::DELETE,
                ])
                .allow_headers([
                    axum::http::header::CONTENT_TYPE,
                    axum::http::header::AUTHORIZATION,
                ])
        })
        .unwrap_or_else(CorsLayer::permissive);

    Router::new()
        .merge(public)
        .merge(protected)
        .layer(cors)
        .layer(DefaultBodyLimit::max(1024 * 1024 * 50)) // 50MB limit
}
