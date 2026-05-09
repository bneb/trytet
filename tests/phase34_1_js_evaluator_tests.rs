//! Phase 34.1: JS Evaluator Cartridge — TDD Test Suite
//!
//! Validates the "Code-Generation Bottleneck" pivot. Ensures that Trytet
//! can evaluate untrusted JavaScript via the `js-evaluator` Cartridge
//! without crashing the host, specifically testing fuel/memory limits on loops.

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

/// Helper: Load the pre-compiled JS Evaluator WebAssembly component.
/// In a standard red-to-green workflow, this requires building `crates/js-evaluator`
/// using `cargo component build --release --target wasm32-wasi`.
fn load_js_evaluator_wasm() -> Vec<u8> {
    // Note: If you encounter an error here during `cargo test`, ensure you have compiled
    // the js-evaluator crate to a wasm32-wasip1 component first:
    // `cd crates/js-evaluator && cargo component build --release`
    
    // For this test suite, we assume the binary is placed here by the build process.
    let path = std::env::current_dir()
        .unwrap()
        .join("crates/js-evaluator/target/wasm32-wasip1/release/js_evaluator.wasm");
        
    std::fs::read(&path).unwrap_or_else(|_| {
        panic!("Failed to read JS Evaluator WASM at {:?}. Please run `cargo component build --release` inside `crates/js-evaluator`.", path)
    })
}

// ===========================================================================
// Test 1: Successful Code Evaluation
// ===========================================================================

#[test]
fn test_js_evaluator_success() {
    let mgr = setup_cartridge_manager();
    let wasm = load_js_evaluator_wasm();
    
    mgr.precompile("js-evaluator", &wasm).expect("Failed to precompile js-evaluator");

    // Pass valid JavaScript code to the evaluator
    let js_code = "Math.PI * 2";
    
    let result = mgr.invoke("js-evaluator", js_code, 10_000_000, 512);
    
    // Assert: Successful evaluation
    let (output, metrics) = result.expect("JS Evaluator should successfully execute simple math");
    
    // Assert: Output is mathematically correct (approximate)
    assert!(output.starts_with("6.283185307179586"));
    
    eprintln!(
        "JS Evaluation Success: output='{}', fuel_consumed={}, duration={}µs",
        output, metrics.fuel_consumed, metrics.duration_us
    );
}

// ===========================================================================
// Test 2: Fuel Exhaustion Isolation (Infinite Loop)
// ===========================================================================

#[test]
fn test_js_evaluator_fuel_exhaustion() {
    let mgr = setup_cartridge_manager();
    let wasm = load_js_evaluator_wasm();
    
    mgr.precompile("js-evaluator", &wasm).expect("Failed to precompile js-evaluator");

    // Pass a malicious infinite loop that would hang a Firecracker VM timeout
    let untrusted_js = "while(true) { let x = 1; }";
    
    // Invoke with a strict fuel budget
    let result = mgr.invoke("js-evaluator", untrusted_js, 1_000_000, 512);

    // Assert: Must trap immediately with FuelExhausted, returning control to Host
    match result {
        Err(CartridgeError::FuelExhausted) => {
            // PASS: The Host trapped the malicious agent code
            eprintln!("JS Evaluator correctly trapped infinite loop via fuel exhaustion.");
        }
        other => panic!(
            "Expected CartridgeError::FuelExhausted, got: {:?}",
            other
        ),
    }
}

// ===========================================================================
// Test 3: Memory Exhaustion (Array Bomb)
// ===========================================================================

#[test]
fn test_js_evaluator_memory_exhaustion() {
    let mgr = setup_cartridge_manager();
    let wasm = load_js_evaluator_wasm();
    
    mgr.precompile("js-evaluator", &wasm).expect("Failed to precompile js-evaluator");

    // Pass a malicious script attempting to allocate massive amounts of memory
    let untrusted_js = "let bomb = []; while(true) { bomb.push('A'.repeat(1000000)); }";
    
    // Invoke with plenty of fuel, but a strict 8MB memory limit
    let result = mgr.invoke("js-evaluator", untrusted_js, 5_000_000_000, 8);

    // Assert: Must trap due to memory allocation failure
    match result {
        Err(CartridgeError::MemoryExceeded) | Err(CartridgeError::ExecutionError(..)) => {
            // Depending on how Boa engine handles allocation failures in Wasm,
            // it may either hit the Wasmtime memory limit directly, or throw an internal error.
            // Both mean the host survived safely.
            eprintln!("JS Evaluator correctly trapped memory bomb.");
        }
        Ok(_) => panic!("Expected JS evaluator to trap on memory bomb, but it succeeded!"),
        Err(e) => panic!("Expected MemoryExceeded or ExecutionError, got: {:?}", e),
    }
}
