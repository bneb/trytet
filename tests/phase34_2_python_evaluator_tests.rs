#![cfg(all(not(coverage), not(target_os = "macos")))]
//! Phase 34.2: Python Evaluator Cartridge — TDD Test Suite
//! Skipped on macOS due to rustpython-vm platform incompatibility (CLOCK_PROCESS_CPUTIME_ID).
//!
//! Validates the Trytet engine's ability to run untrusted Python via the `python-evaluator` Cartridge.
//! Ensures memory isolation and fuel exhaustion traps function correctly inside the Wasm Component Model.

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

/// Helper: Load the pre-compiled Python Evaluator WebAssembly component.
fn load_python_evaluator_wasm() -> Option<Vec<u8>> {
    let candidates = &[
        "crates/python-evaluator/target/wasm32-wasip1/release/python_evaluator.wasm",
        "dist/cartridges/python_evaluator.wasm",
    ];
    for path_str in candidates {
        let path = std::env::current_dir().unwrap().join(path_str);
        if let Ok(wasm) = std::fs::read(&path) {
            return Some(wasm);
        }
    }
    eprintln!("Python evaluator .wasm not found, skipping test");
    None
}

// ===========================================================================
// Test 1: Successful Code Evaluation
// ===========================================================================

#[test]
fn test_python_evaluator_success() {
    let wasm = match load_python_evaluator_wasm() {
        Some(w) => w,
        None => return,
    };
    let mgr = setup_cartridge_manager();

    mgr.precompile("python-evaluator", &wasm)
        .expect("Failed to precompile python-evaluator");

    // Pass valid Python code to the evaluator
    let python_code = "2 + 2";

    let result = mgr.invoke("python-evaluator", python_code, 2_000_000_000, 512);

    // Assert: Successful evaluation
    let (output, metrics) =
        result.expect("Python Evaluator should successfully execute simple math");

    // Assert: Output is correct
    assert_eq!(output, "4");

    eprintln!(
        "Python Evaluation Success: output='{}', fuel_consumed={}, duration={}µs",
        output, metrics.fuel_consumed, metrics.duration_us
    );
}

// ===========================================================================
// Test 2: Fuel Exhaustion Isolation (Infinite Loop)
// ===========================================================================

#[test]
fn test_python_evaluator_fuel_exhaustion() {
    let wasm = match load_python_evaluator_wasm() {
        Some(w) => w,
        None => return,
    };
    let mgr = setup_cartridge_manager();

    mgr.precompile("python-evaluator", &wasm)
        .expect("Failed to precompile python-evaluator");

    // Pass a malicious infinite loop
    let untrusted_py = "while True:\n    pass";

    // Invoke with a strict fuel budget
    let result = mgr.invoke("python-evaluator", untrusted_py, 1_000_000, 512);

    // Assert: Must trap immediately with FuelExhausted
    match result {
        Err(CartridgeError::FuelExhausted) => {
            eprintln!("Python Evaluator correctly trapped infinite loop via fuel exhaustion.");
        }
        other => panic!("Expected CartridgeError::FuelExhausted, got: {:?}", other),
    }
}

// ===========================================================================
// Test 3: Memory Exhaustion (Array Bomb)
// ===========================================================================

#[test]
fn test_python_evaluator_memory_exhaustion() {
    let wasm = match load_python_evaluator_wasm() {
        Some(w) => w,
        None => return,
    };
    let mgr = setup_cartridge_manager();

    mgr.precompile("python-evaluator", &wasm)
        .expect("Failed to precompile python-evaluator");

    // Pass a malicious script attempting to allocate massive amounts of memory
    let untrusted_py = "bomb = []\nwhile True:\n    bomb.append('A' * 1000000)";

    // Invoke with plenty of fuel, but a strict 8MB memory limit
    let result = mgr.invoke("python-evaluator", untrusted_py, 5_000_000_000, 8);

    // Assert: Must trap due to memory allocation failure
    match result {
        Err(CartridgeError::MemoryExceeded) | Err(CartridgeError::ExecutionError(..)) => {
            eprintln!("Python Evaluator correctly trapped memory bomb.");
        }
        Ok(_) => panic!("Expected Python evaluator to trap on memory bomb, but it succeeded!"),
        Err(e) => panic!("Expected MemoryExceeded or ExecutionError, got: {:?}", e),
    }
}
