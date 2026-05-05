//! Core trait definition for the Tet execution sandbox.
//!
//! This module defines the `TetSandbox` trait — the abstraction boundary
//! between the API layer and the Wasm execution engine. All implementations
//! must be `Send + Sync` for safe sharing across Axum handler tasks.

use crate::models::{TetExecutionRequest, TetExecutionResult};
use async_trait::async_trait;

/// The core abstraction for a Tet execution environment.
///
/// Implementations of this trait manage the full lifecycle of a Wasm
/// sandbox: instantiation, execution, state capture, and forking.
///
/// # Concurrency
///
/// All methods take `&self` — implementations must handle internal
/// synchronization (e.g., via `RwLock` for the snapshot store).
#[async_trait]
pub trait TetSandbox: Send + Sync {
    /// Instantiates a new micro-environment and executes the payload.
    ///
    /// If `req.parent_snapshot_id` is `Some`, the engine will load the
    /// referenced memory snapshot into the new instance before calling `_start`.
    ///
    /// # Returns
    /// - `Ok(TetExecutionResult)` on successful execution (including timeouts and crashes)
    /// - `Err(TetError)` only for infrastructure-level failures (engine init, missing snapshots)
    async fn execute(&self, req: TetExecutionRequest) -> Result<TetExecutionResult, TetError>;

    /// Freezes the current execution state of a Tet and returns a snapshot reference ID.
    ///
    /// The snapshot captures the Wasm linear memory at the point of last execution.
    /// It does NOT capture globals, table state, or the instruction pointer —
    /// this is "Git for RAM", not a full process checkpoint.
    ///
    /// # Returns
    /// - `Ok(snapshot_id)` — the ID to use in `fork()` or `parent_snapshot_id`
    /// - `Err(TetError::SnapshotNotFound)` if the `tet_id` has no recorded memory state
    async fn snapshot(&self, tet_id: &str) -> Result<String, TetError>;

    /// Export a snapshot payload from the engine's memory.
    async fn export_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<crate::sandbox::SnapshotPayload, TetError>;

    /// Phase 14: Export the manifest of an active or hibernating agent.
    async fn export_manifest(
        &self,
        id_or_alias: &str,
    ) -> Result<crate::models::manifest::AgentManifest, TetError>;

    /// Import a snapshot payload directly into the engine's memory, returning the snapshot ID.
    async fn import_snapshot(
        &self,
        payload: crate::sandbox::SnapshotPayload,
    ) -> Result<String, TetError>;

    /// Phase 9: Query the active Vector-VFS semantic memory of a running/hibernating Tet.
    async fn query_memory(
        &self,
        alias: &str,
        query: crate::memory::SearchQuery,
    ) -> Result<Vec<crate::memory::SearchResult>, TetError>;

    /// Phase 10: Perform neural inference on a Tet's loaded model.
    async fn infer(
        &self,
        alias: &str,
        request: crate::inference::InferenceRequest,
        fuel_limit: u64,
    ) -> Result<crate::inference::InferenceResponse, TetError>;

    /// Forks a previously snapshotted environment into a new, independent Tet instance.
    ///
    /// This is the core "undo" primitive: an agent can snapshot a known-good state,
    /// then fork it N ways to test N different hypotheses without paying the setup
    /// cost N times.
    ///
    /// # Semantics
    /// 1. The snapshot's linear memory bytes are loaded into the new instance
    /// 2. `_start` is invoked — the module runs from the beginning but with pre-loaded memory
    /// 3. The result is a fully independent `TetExecutionResult` with a new `tet_id`
    async fn fork(
        &self,
        snapshot_id: &str,
        req: TetExecutionRequest,
    ) -> Result<TetExecutionResult, TetError>;

    /// Extract the live Swarm Topology edge metrics from the Mesh.
    async fn get_topology(&self) -> Vec<crate::models::TopologyEdge>;

    /// Bridge a direct Call to the native Test-Mesh.
    async fn send_mesh_call(
        &self,
        req: crate::models::MeshCallRequest,
    ) -> Result<crate::models::MeshCallResponse, TetError>;

    /// Phase 14: Mesh discovery.
    async fn resolve_local(&self, alias: &str) -> Option<crate::models::TetMetadata>;

    /// Phase 14: Mesh cleanup.
    async fn deregister(&self, alias: &str);

    /// Phase 18.1: Publish a DHT update
    async fn publish_dht_route(
        &self,
        alias: &str,
        target_ip: &str,
        signature: &str,
    ) -> Result<(), String>;
}

/// Errors that can occur at the engine infrastructure level.
///
/// These are distinct from `ExecutionStatus` — a timeout or crash is a *valid*
/// execution result (the engine worked correctly), while these errors indicate
/// the engine itself failed to perform the operation.
#[derive(Debug, thiserror::Error)]
pub enum TetError {
    /// The Wasm engine failed to initialize, compile a module, or instantiate.
    #[error("Engine instantiation failed: {0}")]
    EngineError(String),

    /// A snapshot or fork referenced an ID that doesn't exist in the store.
    #[error("Snapshot not found for ID: {0}")]
    SnapshotNotFound(String),

    /// A WASI capability was requested that violates the zero-trust policy.
    #[error("Security or capability violation: {0}")]
    SecurityViolation(String),

    /// A virtual filesystem (VFS) operation failed (e.g., IO error, tarball extraction).
    #[error("VFS error: {0}")]
    VfsError(String),

    /// Missing or failed resolution in the Tet-Mesh.
    #[error("Tet-Mesh error: {0}")]
    MeshError(String),

    /// Call stack depth exceeded limit (infinite regression protection).
    #[error("Call stack exhausted")]
    CallStackExhausted,

    /// Neural inference operation failed.
    #[error("Inference error: {0}")]
    InferenceError(String),

    /// Phase 33.1: Cartridge (Component Model) execution failed.
    #[error("Cartridge error: {0}")]
    CartridgeError(String),
}

impl TetError {
    /// Maps this error to an HTTP status code for the API layer.
    pub fn status_code(&self) -> u16 {
        match self {
            TetError::EngineError(_) => 500,
            TetError::SnapshotNotFound(_) => 404,
            TetError::SecurityViolation(_) => 403,
            TetError::VfsError(_) => 500,
            TetError::MeshError(_) => 502,       // Bad Gateway pattern
            TetError::CallStackExhausted => 429, // Too many requests/depth
            TetError::InferenceError(_) => 500,
            TetError::CartridgeError(_) => 422,  // Unprocessable Entity
        }
    }
}

// Implement IntoResponse for ergonomic error handling in Axum handlers.
#[cfg(not(target_arch = "wasm32"))]
impl axum::response::IntoResponse for TetError {
    fn into_response(self) -> axum::response::Response {
        let status = axum::http::StatusCode::from_u16(self.status_code())
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);

        let body = serde_json::json!({
            "error": self.to_string(),
            "error_type": match &self {
                TetError::EngineError(_) => "engine_error",
                TetError::SnapshotNotFound(_) => "snapshot_not_found",
                TetError::SecurityViolation(_) => "security_violation",
                TetError::VfsError(_) => "vfs_error",
                TetError::MeshError(_) => "mesh_error",
                TetError::CallStackExhausted => "call_stack_exhausted",
                TetError::InferenceError(_) => "inference_error",
                TetError::CartridgeError(_) => "cartridge_error",
            }
        });

        (status, axum::Json(body)).into_response()
    }
}
