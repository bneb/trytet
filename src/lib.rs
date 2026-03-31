//! The Tet Core Engine library.
//!
//! This crate implements a sub-millisecond, hyper-ephemeral Wasm execution
//! substrate designed for Agentic AI workflows. The atomic unit is the
//! Branchable Tet Sandbox.
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

pub mod api;
pub mod crypto;
pub mod engine;
pub mod mesh;
pub mod mesh_worker;
pub mod models;
pub mod registry;
pub mod sandbox;
