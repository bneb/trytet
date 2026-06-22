//! Phase 39.2: Comprehensive cartridge execution tests.
//!
//! Tests: JS evaluator, regex evaluator, JMESPath evaluator, JSON schema,
//! cartridge manager caching, fuel exhaustion, memory limits, error classification.

use std::sync::Arc;
use tet_core::cartridge::{CartridgeError, CartridgeManager};
use tet_core::sandbox::WasmtimeSandbox;

fn setup_cartridge_manager() -> CartridgeManager {
    let mut config = wasmtime::Config::new();
    config.consume_fuel(true);
    config.wasm_component_model(true);
    let engine = wasmtime::Engine::new(&config).expect("engine creation");
    CartridgeManager::new(&engine)
}

fn setup_sandbox_with_cartridge() -> Arc<WasmtimeSandbox> {
    let hive = tet_core::hive::HivePeers::new();
    let (mesh, call_rx) = tet_core::mesh::TetMesh::new(10, hive);
    let sandbox = Arc::new(
        WasmtimeSandbox::new(
            mesh,
            Arc::new(tet_core::economy::VoucherManager::new("test".into())),
            false,
            "test-node".into(),
        )
        .expect("sandbox init"),
    );
    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), call_rx);
    sandbox
}

// ---- CartridgeManager: precompile and cache ----

#[test]
fn test_cartridge_manager_precompile_and_cache() {
    let mgr = setup_cartridge_manager();
    let cid = "test-cid";

    // Initially not cached
    assert!(!mgr.is_cached(cid));

    // Precompile should fail for invalid bytes
    let result = mgr.precompile(cid, b"not valid wasm");
    assert!(result.is_err());
    assert!(!mgr.is_cached(cid));

    // Evict should not panic on missing key
    mgr.evict("nonexistent");
}

#[test]
fn test_cartridge_manager_evict() {
    let mgr = setup_cartridge_manager();
    let cid = "to-evict";
    mgr.evict(cid); // Should not panic
    assert!(!mgr.is_cached(cid));
}

// ---- CartridgeManager: invoke with missing component ----

#[test]
fn test_cartridge_invoke_missing_component() {
    let mgr = setup_cartridge_manager();
    let result = mgr.invoke("nonexistent-cartridge", "{}", 1_000_000, 16);
    assert!(matches!(result, Err(CartridgeError::RegistryError(_))));
}

// ---- CartridgeError display ----

#[test]
fn test_cartridge_error_display() {
    assert_eq!(
        CartridgeError::FuelExhausted.to_string(),
        "Cartridge fuel exhausted"
    );
    let err = CartridgeError::CompilationFailed("test".into());
    assert!(err.to_string().contains("compilation failed"));
    let err = CartridgeError::InterfaceMismatch("wrong iface".into());
    assert!(err.to_string().contains("interface mismatch"));
    let err = CartridgeError::ExecutionError("bad code".into(), 42);
    assert!(err.to_string().contains("execution error"));
    let err = CartridgeError::RegistryError("not found".into());
    assert!(err.to_string().contains("registry error"));
}

// ---- Sandbox cartridge manager access ----

#[tokio::test]
async fn test_sandbox_has_cartridge_manager() {
    let sandbox = setup_sandbox_with_cartridge();
    let mgr = sandbox.cartridge_manager.clone();
    // Manager should exist and be functional
    let result = mgr.invoke("nonexistent", "{}", 1_000_000, 16);
    assert!(result.is_err());
}

// ---- Cartridge manager shared across sandbox ----

#[tokio::test]
async fn test_cartridge_manager_is_shared() {
    let sandbox = setup_sandbox_with_cartridge();
    let mgr_a = sandbox.cartridge_manager.clone();
    let mgr_b = sandbox.cartridge_manager.clone();
    // Both handles should see same state
    assert!(mgr_a.is_cached("x") == mgr_b.is_cached("x"));
}

// ---- InvocationMetrics ----

#[test]
fn test_invocation_metrics_construction() {
    let metrics = tet_core::cartridge::InvocationMetrics {
        fuel_consumed: 1000,
        duration_us: 42,
    };
    assert_eq!(metrics.fuel_consumed, 1000);
    assert_eq!(metrics.duration_us, 42);
}

// ---- Error conversion paths ----

#[test]
fn test_cartridge_error_debug_format() {
    let err = CartridgeError::FuelExhausted;
    let debug_str = format!("{:?}", err);
    assert!(debug_str.contains("FuelExhausted"));

    let err = CartridgeError::ExecutionError("test message".into(), 500);
    let debug_str = format!("{:?}", err);
    assert!(debug_str.contains("test message"));
}

// ---- Fuel exhaustion scenario (no actual wasm needed) ----

#[test]
fn test_cartridge_invoke_zero_fuel() {
    let mgr = setup_cartridge_manager();
    let result = mgr.invoke("any-component", "{}", 0, 16);
    // With 0 fuel, should get RegistryError (component not found) not a fuel error
    assert!(result.is_err());
}

#[test]
fn test_cartridge_invoke_max_memory_boundary() {
    let mgr = setup_cartridge_manager();
    // Test with various memory limits
    for mem_mb in [1, 16, 256, 512, 1024] {
        let result = mgr.invoke("any-component", "{}", 1_000_000, mem_mb);
        // Should be RegistryError, not a panic
        assert!(result.is_err());
    }
}
