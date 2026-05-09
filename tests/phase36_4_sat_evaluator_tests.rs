//! Phase 36.4: SAT Solver Cartridge — TDD Test Suite
//!
//! Validates the creation of a formal verification Cartridge capable of
//! evaluating complex SAT constraints in pure Rust, fully bounded by Trytet's
//! deterministic fuel limits.

use tet_core::cartridge::{CartridgeError, CartridgeManager};

/// Helper: build a CartridgeManager with the production engine config.
fn setup_cartridge_manager() -> CartridgeManager {
    let mut config = wasmtime::Config::new();
    config.consume_fuel(true);
    config.wasm_component_model(true);

    let engine =
        wasmtime::Engine::new(&config).expect("Failed to create wasmtime Engine for cartridges");

    CartridgeManager::new(&engine)
}

/// Helper: Load the pre-compiled SAT Evaluator WebAssembly component.
fn load_sat_evaluator_wasm() -> Vec<u8> {
    let path = std::env::current_dir()
        .unwrap()
        .join("crates/sat-cartridge/target/wasm32-wasip1/release/sat_cartridge.wasm");
        
    std::fs::read(&path).unwrap_or_else(|_| {
        panic!("Failed to read SAT Evaluator WASM at {:?}. Please run `cargo component build --release` inside `crates/sat-cartridge`.", path)
    })
}

// ===========================================================================
// Test 1: Successful Satisfiability Evaluation
// ===========================================================================

#[test]
fn test_sat_evaluator_success() {
    let mgr = setup_cartridge_manager();
    let wasm = load_sat_evaluator_wasm();
    
    mgr.precompile("sat-cartridge", &wasm).expect("Failed to precompile sat-cartridge");

    // A simple, valid SAT problem in DIMACS CNF format
    // Variables: 1, 2. Clauses: (1 OR 2), (-1 OR -2), (1 OR -2)
    // This is satisfiable (e.g. 1=true, 2=false)
    let payload = r#"{
        "dimacs": "p cnf 2 3\n1 2 0\n-1 -2 0\n1 -2 0\n"
    }"#;
    
    let result = mgr.invoke("sat-cartridge", payload, 50_000_000, 512);
    
    let (output, metrics) = result.expect("SAT Evaluator should successfully verify the proof");
    
    assert!(output.contains(r#""satisfiable":true"#), "Output was: {}", output);
    
    eprintln!(
        "SAT Evaluation Success: output='{}', fuel_consumed={}, duration={}µs",
        output, metrics.fuel_consumed, metrics.duration_us
    );
}

// ===========================================================================
// Test 2: Unsatisfiable Logic Rejection
// ===========================================================================

#[test]
fn test_sat_evaluator_unsat() {
    let mgr = setup_cartridge_manager();
    let wasm = load_sat_evaluator_wasm();
    
    mgr.precompile("sat-cartridge", &wasm).expect("Failed to precompile sat-cartridge");

    // An unsatisfiable logic problem: (1), (-1)
    let payload = r#"{
        "dimacs": "p cnf 1 2\n1 0\n-1 0\n"
    }"#;
    
    let result = mgr.invoke("sat-cartridge", payload, 50_000_000, 512);
    
    let (output, _) = result.expect("SAT Evaluator should return SAT result, not crash");
    
    assert!(output.contains(r#""satisfiable":false"#), "Output was: {}", output);
}

// ===========================================================================
// Test 3: Resource Limits (Fuel Exhaustion during Exponential Search)
// ===========================================================================

#[test]
fn test_sat_evaluator_resource_limits() {
    let mgr = setup_cartridge_manager();
    let wasm = load_sat_evaluator_wasm();
    
    mgr.precompile("sat-cartridge", &wasm).expect("Failed to precompile sat-cartridge");

    // A very complex combinatorial explosion in DIMACS CNF.
    // To trigger a fuel limit quickly without taking minutes, we provide a tiny fuel budget to a valid but slightly complex problem.
    let payload = r#"{
        "dimacs": "p cnf 5 10\n1 2 -3 0\n-1 -2 3 0\n2 3 -4 0\n-2 -3 4 0\n3 4 -5 0\n-3 -4 5 0\n1 5 0\n-1 -5 0\n2 -5 0\n-2 4 0\n"
    }"#;
    
    // Provide very little fuel (10,000 instructions)
    let result = mgr.invoke("sat-cartridge", payload, 10_000, 512);
    
    match result {
        Ok(_) => panic!("Expected CartridgeError::FuelExhausted, got success"),
        Err(CartridgeError::FuelExhausted) => {
            eprintln!("SAT Evaluator correctly trapped exponential execution via fuel exhaustion.");
        }
        Err(CartridgeError::MemoryExceeded) => {
            eprintln!("SAT Evaluator correctly trapped execution via memory exhaustion.");
        }
        Err(other) => panic!("Expected resource exhaustion, got {:?}", other),
    }
}
