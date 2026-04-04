//! Northstar Benchmarking Suite — Phase 26.1
//!
//! High-fidelity performance instrumentation proving the Sovereign Delta:
//! the measurable gap between "Dumb Infrastructure" and "Living Intelligence."
//!
//! All measurements use `std::time::Instant` for nanosecond-precision monotonic
//! timestamps to eliminate wall-clock drift and NTP jitter from results.

use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::crypto::AgentWallet;
use crate::market::{HiveMarket, MarketBid};
use crate::memory::{VectorRecord, NUM_SHARDS};
use crate::shards::LayeredVectorStore;

// ---------------------------------------------------------------------------
// Northstar Report — The Exportable Metrics Payload
// ---------------------------------------------------------------------------

/// The canonical Northstar performance report.
///
/// Every field is a discrete, independently measurable metric that maps
/// to one of the four "Story" pillars in the Trytet pitch deck.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct NorthstarReport {
    /// Teleport Warp (µs): Time from `request_migration` to first instruction
    /// on the target node. Target: < 200ms for a 128MB agent.
    pub teleport_warp_us: u64,

    /// Mitosis Constant (µs): Time to execute `trytet::fork` relative to VFS
    /// size. Target: O(1) — forking 1GB must equal forking 1MB (< 15ms).
    pub mitosis_latency_us: u64,

    /// Oracle Fidelity (µs): Latency overhead of Ed25519 signature verification
    /// on every Oracle fetch. Target: < 5ms per verification.
    pub oracle_verification_us: u64,

    /// Market Convergence (ms): Time for 90% of agents to evacuate a "Hot Node"
    /// via `seek_equilibrium`. Target: < 800ms for a 10-agent cluster.
    pub market_evacuation_ms: u64,

    /// Fuel Efficiency Ratio: (Agent_Work / Total_Fuel_Burned).
    /// Higher is better. Measures overhead of the sandbox itself.
    pub fuel_efficiency_ratio: f32,
}

// ---------------------------------------------------------------------------
// Individual Benchmark Runners
// ---------------------------------------------------------------------------

/// Measures the CoW VFS fork latency to prove O(1) scaling.
///
/// Uses `LayeredVectorStore` directly (no Tokio runtime required) to isolate
/// the pure CoW clone cost. The architecture means forking is a metadata-only
/// pointer swap: physical VFS size has zero impact on fork time.
pub fn bench_mitosis_constant() -> (u64, u64) {
    // Small VFS: empty store — measure baseline fork cost
    let small_store = LayeredVectorStore::new();
    let start_small = Instant::now();
    let _small_clone = small_store.clone();
    let small_us = start_small.elapsed().as_micros() as u64;

    // Large VFS: pre-populate active layer shards with 10,000 synthetic records
    let large_store = LayeredVectorStore::new();
    if let Some(shards) = &large_store.active_layer.memory_shards {
        use ahash::AHasher;
        use std::hash::{Hash, Hasher};
        use std::sync::Arc;

        for i in 0..10_000 {
            let record = VectorRecord {
                id: format!("rec_{}", i),
                vector: vec![0.5; 64],
                metadata: std::collections::HashMap::new(),
            };
            let collection_name = "benchmark";
            let mut hasher = AHasher::default();
            collection_name.hash(&mut hasher);
            let idx = (hasher.finish() as usize) % NUM_SHARDS;

            let shard = &shards[idx];
            let col = shard
                .collections
                .entry(collection_name.to_string())
                .or_insert_with(|| Arc::new(crate::memory::VectorCollection::default()));
            col.tier1.records.insert(record.id.clone(), record);
        }
    }

    // Now fork the large store (CoW clone — should be O(1))
    let start_large = Instant::now();
    let _forked = large_store.clone();
    let large_us = start_large.elapsed().as_micros() as u64;

    (small_us, large_us)
}

/// Measures Ed25519 signature + verification round-trip latency.
///
/// This proves that cryptographic truth is affordable: the Oracle can
/// sign and verify every fetch without measurable impact on agent execution.
pub fn bench_oracle_fidelity() -> u64 {
    let wallet = AgentWallet::load_or_create().expect("Wallet init failed");

    // Simulate a realistic Oracle payload (4KB response body)
    let payload: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();

    let start = Instant::now();

    // Sign
    let signature_hex = wallet.sign_manifest(&payload);

    // Verify
    let pubkey_hex = wallet.public_key_hex();
    let _valid = AgentWallet::verify_signature(&pubkey_hex, &payload, &signature_hex);

    start.elapsed().as_micros() as u64
}

/// Measures Market Convergence: how fast agents detect thermal pressure
/// and select evacuation targets via the arbitrage engine.
///
/// Simulates a 10-node cluster where one node hits thermal throttling.
/// Measures time until all rational agents on the hot node would have
/// computed their migration targets.
pub fn bench_market_evacuation() -> u64 {
    let hot_market = HiveMarket::new("HotNode".to_string());

    // Simulate thermal crisis on the local node
    hot_market
        .local_vitals
        .thermal_pressure
        .store(96, std::sync::atomic::Ordering::Relaxed);

    // Register 9 cool neighbors
    for i in 0..9 {
        hot_market.process_bid(MarketBid {
            node_id: format!("CoolNode_{}", i),
            fuel_multiplier: 0.8 + (i as f32) * 0.02, // 0.8 to 0.96
            available_capacity_mb: 2048,
            thermal_score: 35 + (i as u8),
            timestamp_us: crate::telemetry::now_us(),
        });
    }

    // Simulate 10 agents independently querying for arbitrage
    let start = Instant::now();

    let mut evacuated = 0u32;
    for _agent in 0..10 {
        if let Some(_target) = hot_market.find_best_arbitrage(&"HotNode".to_string()) {
            evacuated += 1;
        }
    }

    let elapsed_ms = start.elapsed().as_millis() as u64;

    // Sanity: all 10 agents should have found an escape route
    assert!(
        evacuated >= 9,
        "Market convergence failure: only {}/10 agents evacuated",
        evacuated
    );

    elapsed_ms
}

/// Measures teleport serialization throughput for a synthetic stateful agent.
///
/// Uses a 16MB payload in debug mode (sufficient to validate the bincode
/// pipeline). The full 128MB benchmark runs under `--release` via criterion.
/// Network RTT is excluded — this captures only the serialization delta.
pub fn bench_teleport_warp() -> u64 {
    let payload = crate::sandbox::SnapshotPayload {
        wasm_bytes: vec![0u8; 1024 * 1024],      // 1MB Wasm
        memory_bytes: vec![0u8; 8 * 1024 * 1024],    // 8MB Heap
        fs_tarball: vec![0u8; 4 * 1024 * 1024],      // 4MB VFS
        vector_idx: vec![0u8; 2 * 1024 * 1024],      // 2MB Vectors
        inference_state: vec![0u8; 1024 * 1024], // 1MB KV Cache
    };
    // Total: ~16MB (debug-safe)

    let start = Instant::now();

    let encoded = bincode::serialize(&payload).expect("bincode encode failed");
    let _decoded: crate::sandbox::SnapshotPayload =
        bincode::deserialize(&encoded).expect("bincode decode failed");

    start.elapsed().as_micros() as u64
}

// ---------------------------------------------------------------------------
// Full Suite Runner
// ---------------------------------------------------------------------------

/// Executes the complete Northstar Benchmarking Suite and produces a
/// JSON-serializable report.
///
/// This function is designed to be called from:
/// 1. The `/v1/swarm/metrics` API endpoint (for Grafana/Prometheus scraping)
/// 2. The `tet-tui` dashboard (for live operator visibility)
/// 3. Unit tests (for CI regression detection)
pub fn run_full_suite() -> NorthstarReport {
    let (mitosis_small, mitosis_large) = bench_mitosis_constant();
    let oracle_us = bench_oracle_fidelity();
    let evacuation_ms = bench_market_evacuation();
    let teleport_us = bench_teleport_warp();

    // Fuel efficiency: ratio of "useful work" to total overhead.
    // In this synthetic benchmark, we measure the mitosis overhead ratio.
    let fuel_efficiency = if mitosis_large > 0 {
        (mitosis_small as f32) / (mitosis_large as f32)
    } else {
        1.0
    };

    NorthstarReport {
        teleport_warp_us: teleport_us,
        mitosis_latency_us: mitosis_large,
        oracle_verification_us: oracle_us,
        market_evacuation_ms: evacuation_ms,
        fuel_efficiency_ratio: fuel_efficiency,
    }
}
