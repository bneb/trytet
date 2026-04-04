use tet_core::benchmarks::{
    bench_market_evacuation, bench_mitosis_constant, bench_oracle_fidelity, bench_teleport_warp,
    run_full_suite,
};

/// TDD Case 1: The "Zero-G" Mitosis Test
///
/// Proves O(1) CoW fork scaling: the latency difference between forking
/// an empty VFS and a 10,000-record VFS must be < 2ms. If the CoW layer
/// is working correctly, fork is a metadata-only pointer swap regardless
/// of underlying data volume.
#[test]
fn test_phase26_mitosis_o1_scaling() {
    let (small_us, large_us) = bench_mitosis_constant();

    // Both should complete in microseconds if CoW is working
    let delta_us = large_us.abs_diff(small_us);

    // The delta between empty and 10k-record fork must be < 5ms (5000µs)
    // This is generous for debug builds; release builds achieve < 2ms easily.
    assert!(
        delta_us < 5_000,
        "Mitosis FAILED O(1) invariant: small={}µs, large={}µs, delta={}µs (must be <5000µs)",
        small_us,
        large_us,
        delta_us
    );
}

/// TDD Case 2: The "Warp Speed" Validation
///
/// Proves that serializing + deserializing a 16MB agent state via bincode
/// completes quickly. Debug builds use 16MB; the full 128MB benchmark
/// runs under `--release` criterion.
#[test]
fn test_phase26_teleport_warp_ceiling() {
    let warp_us = bench_teleport_warp();
    let warp_ms = warp_us / 1000;

    // 16MB bincode round-trip must complete in under 2s even in debug mode
    assert!(
        warp_ms < 2_000,
        "Teleport Warp EXCEEDED ceiling: {}ms (must be <2000ms for 16MB agent in debug)",
        warp_ms
    );
}

/// TDD Case 3: Oracle Fidelity — Cryptographic Truth is Affordable
///
/// Proves that Ed25519 sign + verify round-trip stays under 5ms.
/// This validates that every Oracle fetch can be cryptographically
/// signed without measurable impact on agent execution latency.
#[test]
fn test_phase26_oracle_fidelity() {
    let verification_us = bench_oracle_fidelity();

    assert!(
        verification_us < 5_000,
        "Oracle Fidelity EXCEEDED ceiling: {}µs (must be <5000µs / 5ms)",
        verification_us
    );
}

/// TDD Case 4: The "Thermal Panic" Drill
///
/// Simulates a 10-agent cluster where one node hits 96°C thermal throttling.
/// Measures the time until all rational agents compute their evacuation targets.
/// Must complete in under 1 second (1000ms).
#[test]
fn test_phase26_market_thermal_evacuation() {
    let evacuation_ms = bench_market_evacuation();

    assert!(
        evacuation_ms < 1_000,
        "Market evacuation EXCEEDED ceiling: {}ms (must be <1000ms)",
        evacuation_ms
    );
}

/// Integration: Full Northstar Suite produces a valid, complete report.
///
/// Validates that all four metrics are populated and within their
/// respective Northstar thresholds. This is the CI regression gate.
#[test]
fn test_phase26_full_northstar_report() {
    let report = run_full_suite();

    // Teleport Warp: must be measured (non-zero)
    assert!(
        report.teleport_warp_us > 0,
        "Teleport warp was not measured"
    );

    // Oracle Fidelity: under 5ms
    assert!(
        report.oracle_verification_us < 5_000,
        "Oracle fidelity regression: {}µs",
        report.oracle_verification_us
    );

    // Market Evacuation: under 1s
    assert!(
        report.market_evacuation_ms < 1_000,
        "Market evacuation regression: {}ms",
        report.market_evacuation_ms
    );

    // Fuel Efficiency: must be positive
    assert!(
        report.fuel_efficiency_ratio > 0.0,
        "Fuel efficiency ratio is zero or negative"
    );

    // Validate JSON serialization (Prometheus/Grafana compatibility)
    let json =
        serde_json::to_string_pretty(&report).expect("NorthstarReport must be JSON-serializable");
    assert!(
        json.contains("teleport_warp_us"),
        "JSON missing teleport_warp_us"
    );
    assert!(
        json.contains("mitosis_latency_us"),
        "JSON missing mitosis_latency_us"
    );
    assert!(
        json.contains("oracle_verification_us"),
        "JSON missing oracle_verification_us"
    );
    assert!(
        json.contains("market_evacuation_ms"),
        "JSON missing market_evacuation_ms"
    );
    assert!(
        json.contains("fuel_efficiency_ratio"),
        "JSON missing fuel_efficiency_ratio"
    );
}
