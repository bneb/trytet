//! Phase 36.3: JMESPath Evaluator Cartridge — TDD Test Suite
//!
//! Validates the creation of a JMESPath Cartridge, which allows AI agents
//! to safely filter and extract data from massive JSON documents.

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

/// Helper: Load the pre-compiled JMESPath Evaluator WebAssembly component.
fn load_jmespath_evaluator_wasm() -> Vec<u8> {
    let path = std::env::current_dir()
        .unwrap()
        .join("crates/jmespath-cartridge/target/wasm32-wasip1/release/jmespath_cartridge.wasm");
        
    std::fs::read(&path).unwrap_or_else(|_| {
        panic!("Failed to read JMESPath Evaluator WASM at {:?}. Please run `cargo component build --release` inside `crates/jmespath-cartridge`.", path)
    })
}

// ===========================================================================
// Test 1: Successful JMESPath Execution
// ===========================================================================

#[test]
fn test_jmespath_evaluator_success() {
    let mgr = setup_cartridge_manager();
    let wasm = load_jmespath_evaluator_wasm();
    
    mgr.precompile("jmespath-cartridge", &wasm).expect("Failed to precompile jmespath-cartridge");

    // Pass valid JSON payload to the jmespath evaluator
    let payload = r#"{
        "json": "{\"locations\": [{\"name\": \"Seattle\", \"state\": \"WA\"}, {\"name\": \"New York\", \"state\": \"NY\"}, {\"name\": \"Bellevue\", \"state\": \"WA\"}]}",
        "expression": "locations[?state == 'WA'].name | sort(@) | join(', ', @)"
    }"#;
    
    let result = mgr.invoke("jmespath-cartridge", payload, 10_000_000, 512);
    
    // Assert: Successful execution
    let (output, metrics) = result.expect("JMESPath Evaluator should successfully filter JSON");
    
    assert!(output.contains(r#"\"Bellevue, Seattle\""#), "Output was: {}", output);
    
    eprintln!(
        "JMESPath Evaluation Success: output='{}', fuel_consumed={}, duration={}µs",
        output, metrics.fuel_consumed, metrics.duration_us
    );
}

// ===========================================================================
// Test 2: JMESPath Execution Memory/Fuel Protection
// ===========================================================================

#[test]
fn test_jmespath_evaluator_resource_limits() {
    let mgr = setup_cartridge_manager();
    let wasm = load_jmespath_evaluator_wasm();
    
    mgr.precompile("jmespath-cartridge", &wasm).expect("Failed to precompile jmespath-cartridge");

    // Build a huge JSON array
    let json_array = format!("[{}]", vec![r#"{"value": 1}"#; 50000].join(","));
    let payload = format!(
        r#"{{"json": {}, "expression": "[*].value"}}"#,
        serde_json::to_string(&json_array).unwrap()
    );
    
    // Provide very little fuel (100_000)
    let result = mgr.invoke("jmespath-cartridge", &payload, 100_000, 512);
    
    match result {
        Ok(_) => panic!("Expected CartridgeError::FuelExhausted, got success"),
        Err(CartridgeError::FuelExhausted) => {
            eprintln!("JMESPath Evaluator correctly trapped execution via fuel exhaustion.");
        }
        Err(CartridgeError::MemoryExceeded) => {
            eprintln!("JMESPath Evaluator correctly trapped execution via memory exhaustion.");
        }
        Err(other) => panic!("Expected resource exhaustion, got {:?}", other),
    }
}
