//! Domain models for the Tet execution substrate.
//!
//! These types define the strict API contract between the agentic caller
//! and the Tet engine. Every field is designed for machine consumption —
//! no raw terminal dumps, no ambiguous status strings.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod manifest;

fn default_fuel() -> u64 {
    50_000_000
}

/// A request to instantiate and execute a Tet sandbox.
///
/// The payload is a raw WebAssembly binary (not base64 — raw bytes serialized
/// as a JSON array of u8). If forking, the payload can be omitted to reuse
/// the parent snapshot's binary.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TetExecutionRequest {
    /// The raw WebAssembly binary payload (.wasm bytes).
    /// Omit this when forking to reuse the parent's binary.
    pub payload: Option<Vec<u8>>,

    /// Registry name for discovery in the Tet-Mesh.
    pub alias: Option<String>,

    /// Environment variables injected into the WASI context.
    /// These are the *only* external inputs the sandbox receives.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Files to inject into the `/workspace` virtual filesystem before execution.
    #[serde(default)]
    pub injected_files: HashMap<String, String>,

    /// Maximum deterministic fuel (Wasm instructions) allowed.
    #[serde(default = "default_fuel")]
    pub allocated_fuel: u64,

    /// Maximum memory allocation in megabytes.
    /// Enforced via wasmtime's `StoreLimiter`.
    pub max_memory_mb: u32,

    /// Optional: If provided, fork from this existing memory state ID.
    /// The snapshot's linear memory bytes will be written into the new
    /// instance before `_start` is invoked.
    /// Optional: If provided, fork from this existing memory state ID.
    pub parent_snapshot_id: Option<String>,

    /// The exported Wasm function to execute. If None, uses `_start`.
    #[serde(default)]
    pub target_function: Option<String>,

    /// Internal property to prevent infinite recursion.
    #[serde(default)]
    pub call_depth: u32,

    /// Optional presentation of a pre-paid compute authorization.
    #[serde(default)]
    pub voucher: Option<crate::economy::FuelVoucher>,
    pub manifest: Option<crate::models::manifest::AgentManifest>,

    /// The Sovereign Oracle networking egress policy governing outbound external HTTP requests.
    #[serde(default)]
    pub egress_policy: Option<crate::oracle::EgressPolicy>,
}

/// A Swarm Telemetry Edge, mapping the Four Golden Metrics between
/// natively executing Tet agents inside the memory sandbox via eBPF-inspired topology tracing.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TopologyEdge {
    pub source: String,
    pub target: String,
    pub call_count: u64,
    pub error_count: u64,
    pub total_latency_us: u64,
    pub total_bytes: u64,
    pub last_seen_ns: u64, // Monotonic time/Epoch time
}

/// The structured result of a Tet execution.
///
/// Designed to be cheaply parseable by an LLM — no raw stack traces,
/// no ambiguous exit codes. Every field is typed and meaningful.
#[derive(Debug, Serialize, Deserialize)]
pub struct TetExecutionResult {
    /// Unique identifier for this Tet execution instance.
    pub tet_id: String,

    /// The terminal state of the execution.
    pub status: ExecutionStatus,

    /// Structured telemetry — stdout/stderr split into lines,
    /// memory usage reported in KB.
    pub telemetry: StructuredTelemetry,

    /// Wall-clock execution duration in microseconds.
    /// Measured from instantiation to teardown.
    pub execution_duration_us: u64,

    /// The exact deterministic compute used for billing.
    pub fuel_consumed: u64,

    /// The state of the `/workspace` filesystem after execution finishes.
    pub mutated_files: HashMap<String, String>,

    /// If the status is Migrated, this contains the requested target node ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migrated_to: Option<String>,
}

/// The terminal state of a Tet execution.
///
/// This is an enum, not a string — agents can pattern-match on it
/// without parsing. `Crash` carries a structured report.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum ExecutionStatus {
    /// The Wasm module exited cleanly (proc_exit(0) or _start returned).
    Success,

    /// The deterministic fuel budget was exhausted.
    OutOfFuel,

    /// The module attempted to grow memory beyond `max_memory_mb`.
    MemoryExceeded,

    /// The module trapped (unreachable, div-by-zero, OOB memory access, etc).
    /// The `CrashReport` contains structured diagnostics.
    Crash(CrashReport),

    /// The execution requested a migration and was safely ejected.
    Migrated,

    /// The agent yielded execution to wait for an external event.
    /// Memory state is snapshotted and evicted from RAM.
    Suspended,
}

/// Structured crash diagnostics for LLM consumption.
///
/// An agent receiving this can immediately reason about the failure mode
/// without parsing a raw stack trace.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct CrashReport {
    /// The category of the trap (e.g., "unreachable", "memory_out_of_bounds").
    pub error_type: String,

    /// The Wasm instruction offset where the trap occurred, if available.
    pub instruction_offset: Option<usize>,

    /// A human-readable (and LLM-readable) description of the crash.
    pub message: String,
}

/// Telemetry captured from the sandbox's stdout/stderr pipes.
///
/// Agents don't read raw terminals. We intercept all output and
/// return it as structured, line-split data.
#[derive(Debug, Serialize, Deserialize)]
pub struct StructuredTelemetry {
    /// Lines written to stdout, split by newline.
    pub stdout_lines: Vec<String>,

    /// Lines written to stderr, split by newline.
    pub stderr_lines: Vec<String>,

    /// Peak memory usage of the Wasm linear memory in kilobytes.
    pub memory_used_kb: u64,
}

/// Response from a snapshot operation.
#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotResponse {
    /// The unique ID of the created snapshot. Use this as
    /// `parent_snapshot_id` in a subsequent `TetExecutionRequest` to fork.
    pub snapshot_id: String,
}

// ---------------------------------------------------------------------------
// Tet-Mesh Domain Models
// ---------------------------------------------------------------------------

/// A remote procedure call inside the Tet-Mesh.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MeshCallRequest {
    pub target_alias: String,
    pub method: String,
    pub payload: Vec<u8>,
    pub fuel_to_transfer: u64,
    pub current_depth: u32,
    #[serde(default)]
    pub target_function: Option<String>,
}

/// The result returned from an inter-tet RPC.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MeshCallResponse {
    pub status: ExecutionStatus,
    pub return_data: Vec<u8>,
    pub fuel_used: u64,
}

/// Information tracked by the Tet-Mesh registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TetMetadata {
    pub tet_id: String,
    pub is_hibernating: bool,
    /// If hibernating, what snapshot represents its frozen state?
    pub snapshot_id: Option<String>,
    /// We cache the WASM bytes so the mesh can boot stateless fresh instances cleanly.
    pub wasm_bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TetManifest {
    pub name: String,
    pub version: String,
    pub created_at: u64,
    pub author_pubkey: String, // Ed25519 for Agentic Trust
    pub hashes: TetHashes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TetHashes {
    pub wasm_hash: String,
    pub memory_hash: String,
    pub vfs_hash: String,
}
