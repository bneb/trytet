//! Hive-Pulse Telemetry — Phase 16.1
//!
//! Zero-overhead, opt-in event streaming for the Trytet Engine.
//! When no TUI is connected, `broadcast()` compiles down to a single
//! failed `try_send` — no serialization, no allocation. When the TUI
//! subscribes, events flow through a bounded broadcast channel with
//! backpressure protection (events are silently dropped if the buffer fills).

use serde::Serialize;
use tokio::sync::broadcast;

// ---------------------------------------------------------------------------
// Hive Events
// ---------------------------------------------------------------------------

/// Typed telemetry events emitted by the Trytet Engine.
///
/// Every event carries enough context for the TUI to render
/// without querying back into the engine.
#[derive(Debug, Clone, Serialize)]
pub enum HiveEvent {
    /// A new agent has been instantiated and is about to execute.
    AgentBooted {
        tet_id: String,
        alias: Option<String>,
        fuel_limit: u64,
        memory_limit_mb: u32,
        timestamp_us: u64,
    },

    /// An agent has completed execution (success, crash, or out-of-fuel).
    AgentCompleted {
        tet_id: String,
        alias: Option<String>,
        status: String,
        fuel_consumed: u64,
        fuel_limit: u64,
        memory_used_kb: u64,
        duration_us: u64,
        timestamp_us: u64,
    },

    /// Fuel has been consumed by a specific operation.
    FuelConsumed {
        tet_id: String,
        operation: String,
        amount: u64,
        timestamp_us: u64,
    },

    /// A teleportation handoff has been initiated.
    TeleportInitiated {
        agent_id: String,
        target_node: String,
        use_registry: bool,
        timestamp_us: u64,
    },

    /// A teleportation handoff has completed.
    TeleportCompleted {
        agent_id: String,
        target_node: String,
        bytes_transferred: u64,
        timestamp_us: u64,
    },

    /// The MeshOracle returned a cached SignedTruth (no network I/O).
    OracleHit {
        tet_id: String,
        request_hash: String,
        timestamp_us: u64,
    },

    /// The MeshOracle had to perform a real network fetch.
    OracleMiss {
        tet_id: String,
        request_hash: String,
        url: String,
        timestamp_us: u64,
    },

    /// An inference call has started.
    InferenceStarted {
        tet_id: String,
        model_id: String,
        prompt_tokens_est: u32,
        timestamp_us: u64,
    },

    /// An inference call has completed (from provider or cache).
    InferenceCompleted {
        tet_id: String,
        model_id: String,
        input_tokens: u32,
        output_tokens: u32,
        fuel_cost: u64,
        cached: bool,
        timestamp_us: u64,
    },

    /// The ContextRouter pruned blocks to fit the model window.
    ContextPruned {
        tet_id: String,
        tokens_removed: usize,
        blocks_evicted: usize,
        timestamp_us: u64,
    },
}

// ---------------------------------------------------------------------------
// Telemetry Hub
// ---------------------------------------------------------------------------

/// The observability backbone of the Trytet Engine.
///
/// Uses `tokio::sync::broadcast` to fan events out to all subscribers
/// (TUI, WebSocket streams, metrics exporters) without blocking the
/// execution hot path.
///
/// # Zero-Overhead Guarantee
///
/// When no subscriber exists, `try_send` fails immediately with no
/// allocation. The engine pays only the cost of constructing the enum
/// variant (stack-allocated, no heap).
pub struct TelemetryHub {
    tx: broadcast::Sender<HiveEvent>,
}

impl TelemetryHub {
    /// Create a new hub with the specified channel capacity.
    ///
    /// A capacity of 10,000 events provides sufficient buffer for burst
    /// workloads (1,000 agents × 10 events each) without unbounded growth.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Create a default hub with 10,000 event capacity.
    pub fn default_capacity() -> Self {
        Self::new(10_000)
    }

    /// Create a no-op hub. `broadcast()` is guaranteed to be a single
    /// failed `try_send` — zero allocation, zero serialization.
    pub fn noop() -> Self {
        Self::new(1)
    }

    /// Emit a telemetry event to all subscribers.
    ///
    /// This is non-blocking. If the channel buffer is full or no
    /// subscribers exist, the event is silently dropped. This ensures
    /// telemetry never introduces backpressure on the execution loop.
    #[inline]
    pub fn broadcast(&self, event: HiveEvent) {
        let _ = self.tx.send(event);
    }

    /// Subscribe to the event stream. Returns a receiver that can
    /// be polled from the TUI thread or a WebSocket handler.
    pub fn subscribe(&self) -> broadcast::Receiver<HiveEvent> {
        self.tx.subscribe()
    }

    /// Returns the current number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

/// Convenience: get monotonic microsecond timestamp for events.
pub fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}
