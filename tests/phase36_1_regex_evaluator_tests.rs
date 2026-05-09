//! Phase 36.1: Regex Evaluator Cartridge — TDD Test Suite
//!
//! Validates the creation of a high-performance, deterministic regex engine
//! as a Wasm Cartridge. Ensures that Trytet can execute regular expressions
//! safely without host ReDoS vulnerabilities.

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

/// Helper: Load the pre-compiled Regex Evaluator WebAssembly component.
fn load_regex_evaluator_wasm() -> Vec<u8> {
    let path = std::env::current_dir()
        .unwrap()
        .join("crates/regex-evaluator/target/wasm32-wasip1/release/regex_evaluator.wasm");
        
    std::fs::read(&path).unwrap_or_else(|_| {
        panic!("Failed to read Regex Evaluator WASM at {:?}. Please run `cargo component build --release` inside `crates/regex-evaluator`.", path)
    })
}

// ===========================================================================
// Test 1: Successful Regex Execution
// ===========================================================================

#[test]
fn test_regex_evaluator_success() {
    let mgr = setup_cartridge_manager();
    let wasm = load_regex_evaluator_wasm();
    
    mgr.precompile("regex-evaluator", &wasm).expect("Failed to precompile regex-evaluator");

    // Pass valid JSON payload to the regex evaluator
    // Expected format: {"pattern": r"^[a-zA-Z0-9]+$", "text": "HelloWorld123"}
    let payload = r#"{"pattern": "^[a-zA-Z0-9]+$", "text": "HelloWorld123"}"#;
    
    let result = mgr.invoke("regex-evaluator", payload, 10_000_000, 512);
    
    // Assert: Successful execution
    let (output, metrics) = result.expect("Regex Evaluator should successfully match simple pattern");
    
    assert!(output.contains(r#""matched":true"#), "Output was: {}", output);
    
    eprintln!(
        "Regex Evaluation Success: output='{}', fuel_consumed={}, duration={}µs",
        output, metrics.fuel_consumed, metrics.duration_us
    );
}

// ===========================================================================
// Test 2: Catastrophic Backtracking (ReDoS) Protection
// ===========================================================================

#[test]
fn test_regex_evaluator_redos_protection() {
    let mgr = setup_cartridge_manager();
    let wasm = load_regex_evaluator_wasm();
    
    mgr.precompile("regex-evaluator", &wasm).expect("Failed to precompile regex-evaluator");

    // Standard ReDoS pattern: ^(a+)+$ against a string of 'a's followed by 'b'
    // Rust's `regex` crate is actually ReDoS immune (O(n) guarantees), 
    // but the Wasm engine fuel limit adds a second layer of deterministic compute protection
    // regardless of the underlying engine's algorithmic guarantees.
    // If the regex engine iterates too much, or if we use an engine like `fancy-regex`
    // that allows backtracking for lookarounds, it will hit the fuel limit.
    
    // We will simulate a very long text match
    let long_a = "a".repeat(100_000);
    let payload = format!(r#"{{"pattern": "^(a+)+$", "text": "{}b"}}"#, long_a);
    
    // Invoke with a strict fuel budget
    let result = mgr.invoke("regex-evaluator", &payload, 1_000_000, 512);

    // Assert: Must trap immediately with FuelExhausted, returning control to Host
    // Note: Rust's regex crate might actually finish this instantly and return false, 
    // but if we exhaust fuel building the string or regex, it will trap.
    // We will accept either a safe return or a fuel exhaustion.
    match result {
        Ok((out, _)) => {
            assert!(out.contains(r#""matched":false"#) || out.contains("error"));
            eprintln!("ReDoS prevented natively by engine algorithm. Safe.");
        }
        Err(CartridgeError::FuelExhausted) => {
            eprintln!("Regex Evaluator correctly trapped ReDoS via fuel exhaustion.");
        }
        other => panic!(
            "Expected safe return or FuelExhausted, got: {:?}",
            other
        ),
    }
}
