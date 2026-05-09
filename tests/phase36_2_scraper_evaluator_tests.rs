//! Phase 36.2: Scraper Evaluator Cartridge — TDD Test Suite
//!
//! Validates the creation of an HTML Scraper Wasm Cartridge, which allows
//! AI agents to extract structured text from HTML reliably and securely
//! within the Trytet sandbox limits.

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

/// Helper: Load the pre-compiled Scraper Evaluator WebAssembly component.
fn load_scraper_evaluator_wasm() -> Vec<u8> {
    let path = std::env::current_dir()
        .unwrap()
        .join("crates/scraper-cartridge/target/wasm32-wasip1/release/scraper_cartridge.wasm");
        
    std::fs::read(&path).unwrap_or_else(|_| {
        panic!("Failed to read Scraper Evaluator WASM at {:?}. Please run `cargo component build --release` inside `crates/scraper-cartridge`.", path)
    })
}

// ===========================================================================
// Test 1: Successful Scraper Execution
// ===========================================================================

#[test]
fn test_scraper_evaluator_success() {
    let mgr = setup_cartridge_manager();
    let wasm = load_scraper_evaluator_wasm();
    
    mgr.precompile("scraper-cartridge", &wasm).expect("Failed to precompile scraper-cartridge");

    // Pass valid JSON payload to the scraper evaluator
    let payload = r#"{
        "html": "<html><body><main><p class=\"content\">Hello World</p><p class=\"content\">Second paragraph</p></main></body></html>",
        "selector": "main p.content"
    }"#;
    
    let result = mgr.invoke("scraper-cartridge", payload, 100_000_000, 512);
    
    // Assert: Successful execution
    let (output, metrics) = result.expect("Scraper Evaluator should successfully extract content");
    
    assert!(output.contains(r#"Hello World"#));
    assert!(output.contains(r#"Second paragraph"#));
    
    eprintln!(
        "Scraper Evaluation Success: output='{}', fuel_consumed={}, duration={}µs",
        output, metrics.fuel_consumed, metrics.duration_us
    );
}

// ===========================================================================
// Test 2: Scraper Execution Extract Attribute
// ===========================================================================

#[test]
fn test_scraper_evaluator_extract_attribute() {
    let mgr = setup_cartridge_manager();
    let wasm = load_scraper_evaluator_wasm();
    
    mgr.precompile("scraper-cartridge", &wasm).expect("Failed to precompile scraper-cartridge");

    let payload = r#"{
        "html": "<html><body><a href=\"https://trytet.io\">Link</a><a href=\"https://github.com/trytet\">GitHub</a></body></html>",
        "selector": "a",
        "extract_attribute": "href"
    }"#;
    
    let result = mgr.invoke("scraper-cartridge", payload, 100_000_000, 512);
    
    let (output, _) = result.expect("Scraper should extract hrefs");
    
    assert!(output.contains(r#""https://trytet.io""#));
    assert!(output.contains(r#""https://github.com/trytet""#));
}

// ===========================================================================
// Test 3: Large Document Fuel Limit Exhaustion
// ===========================================================================

#[test]
fn test_scraper_evaluator_fuel_exhaustion() {
    let mgr = setup_cartridge_manager();
    let wasm = load_scraper_evaluator_wasm();
    
    mgr.precompile("scraper-cartridge", &wasm).expect("Failed to precompile scraper-cartridge");

    // Generate a huge HTML document
    let huge_p = "<p>Nested</p>".repeat(20_000);
    let payload_str = format!(r#"{{"html": "<html><body><main>{}</main></body></html>", "selector": "p"}}"#, huge_p);
    
    // Provide very little fuel (100_000)
    let result = mgr.invoke("scraper-cartridge", &payload_str, 100_000, 512);

    match result {
        Ok(_) => panic!("Expected CartridgeError::FuelExhausted, got success"),
        Err(CartridgeError::FuelExhausted) => {
            eprintln!("Scraper Evaluator correctly trapped execution via fuel exhaustion.");
        }
        Err(other) => panic!("Expected CartridgeError::FuelExhausted, got {:?}", other),
    }
}
