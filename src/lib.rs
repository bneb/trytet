//! The Tet Core Engine library.
//!
//! WebAssembly execution substrate with fuel-bounded sandboxing for AI agent tool use.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │  Axum API Layer (api.rs)                                │
//! │  POST /v1/tet/execute   POST /v1/tet/snapshot/{tet_id}  │
//! └───────────────────────┬─────────────────────────────────┘
//!                         │ Arc<dyn TetSandbox>
//! ┌───────────────────────▼─────────────────────────────────┐
//! │  WasmtimeSandbox (sandbox.rs)                           │
//! │  ┌──────────┐ ┌──────────┐ ┌──────────────────────┐    │
//! │  │ Engine   │ │ Epoch    │ │ Snapshot Store        │    │
//! │  │ (shared) │ │ Ticker   │ │ RwLock<HashMap>       │    │
//! │  └──────────┘ └──────────┘ └──────────────────────┘    │
//! └─────────────────────────────────────────────────────────┘
//! ```

#[cfg(not(target_arch = "wasm32"))]
pub mod api;
#[cfg(not(target_arch = "wasm32"))]
pub mod auth;
#[cfg(not(target_arch = "wasm32"))]
pub mod benchmarks;
pub mod builder;
#[cfg(not(target_arch = "wasm32"))]
pub mod cartridge;
pub mod config;
pub mod consensus;
#[cfg(not(target_arch = "wasm32"))]
pub mod crypto;
pub mod economy;
pub mod engine;
#[cfg(not(target_arch = "wasm32"))]
pub mod fortress;
#[cfg(not(target_arch = "wasm32"))]
pub mod gateway;
#[cfg(not(target_arch = "wasm32"))]
pub mod hive;
pub mod inference;
#[cfg(not(target_arch = "wasm32"))]
pub mod llama_engine;
pub mod market;
pub mod mcp;
pub mod memory;
#[cfg(not(target_arch = "wasm32"))]
pub mod mesh;
#[cfg(not(target_arch = "wasm32"))]
pub mod mesh_worker;
#[cfg(not(target_arch = "wasm32"))]
pub mod model_proxy;
pub mod models;
#[cfg(not(target_arch = "wasm32"))]
pub mod network;
pub mod oracle;
#[cfg(not(target_arch = "wasm32"))]
pub mod registry;
pub mod resurrection;
pub mod runtime;
pub mod sandbox;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
pub mod shards;
#[cfg(not(target_arch = "wasm32"))]
pub mod studio;
#[cfg(not(target_arch = "wasm32"))]
pub mod telemetry;
pub mod teleport;
