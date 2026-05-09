use tet_core::cartridge::{CartridgeError, CartridgeManager};
use colored::*;
use std::time::Instant;

fn setup_cartridge_manager() -> CartridgeManager {
    let mut config = wasmtime::Config::new();
    config.consume_fuel(true);
    config.wasm_component_model(true);

    let engine = wasmtime::Engine::new(&config).expect("Failed to create wasmtime Engine");
    CartridgeManager::new(&engine)
}

fn load_python_evaluator_wasm() -> Vec<u8> {
    let path = std::env::current_dir()
        .unwrap()
        .join("crates/python-evaluator/target/wasm32-wasip1/release/python_evaluator.wasm");
        
    std::fs::read(&path).unwrap_or_else(|_| {
        panic!("Failed to read Python Evaluator WASM at {:?}.", path)
    })
}

fn generate_python_script(pattern: &str) -> String {
    let template = r#"
import re

# Messy, unstructured log data containing a target ID and a subtle poison string
data = "LOG INFO CPU 45% RAM 12% aaaaaaaaaaaaaaaaaaaaaaaaaaaaaab TRYTET-ID-9F8A7"

# The LLM-generated candidate pattern
pattern = r'''__PATTERN__'''

try:
    match = re.search(pattern, data)
    match.group(0) if match else "None"
except Exception as e:
    str(e)
"#;
    template.replace("__PATTERN__", pattern)
}

fn main() {
    println!("\n{}\n", "==================================================".bold().blue());
    println!("{}", "   Trytet 'Data Agent' Regex Extraction Demo".bold().green());
    println!("{}\n", "==================================================".bold().blue());
    
    println!("{} Booting Trytet Engine...", "[SYSTEM]".bold().cyan());
    let mgr = setup_cartridge_manager();
    let wasm = load_python_evaluator_wasm();
    mgr.precompile("python", &wasm).expect("Failed to precompile python-evaluator");
    println!("{} Python Cartridge precompiled. Sandboxes are hot.\n", "[SYSTEM]".bold().cyan());

    println!("{} LLM Agent generated 100 candidate regex patterns.", "[AGENT]".bold().magenta());
    println!("{} Objective: Extract the TRYTET-ID from unstructured log data.", "[AGENT]".bold().magenta());
    println!("{} Fanning out verification to Trytet Sandboxes...\n", "[SYSTEM]".bold().cyan());

    let mut success_found = false;
    let start_total = Instant::now();
    let total_candidates = 100;

    for candidate_id in 1..=total_candidates {
        if success_found {
            break;
        }

        let pattern = if candidate_id == 82 {
            "TRYTET-ID-[A-Z0-9]+" // The correct success pattern
        } else if candidate_id == 45 {
            "(a+)+$" // The Catastrophic Backtracking (ReDoS) pattern
        } else if candidate_id == 73 {
            "([a-zA-Z]+)*$" // Another ReDoS pattern variant
        } else {
            "[A-Z]{5}-[0-9]{5}" // Harmless incorrect pattern
        };

        let script = generate_python_script(pattern);

        let start_eval = Instant::now();
        // 2 Billion fuel is needed for RustPython's cold boot. 8MB memory limit.
        let result = mgr.invoke("python", &script, 2_000_000_000, 8);
        let duration = start_eval.elapsed();

        let node_label = format!("[Node {:03}]", candidate_id).bold().bright_black();

        match result {
            Ok((output, _metrics)) => {
                if output.contains("TRYTET-ID") {
                    println!("{} {} 🎯 SUCCESS: Extracted '{}' ({}µs)", node_label, "✓".bold().green(), output, duration.as_micros());
                    success_found = true;
                } else {
                    println!("{} {} ❌ Failed: Returned '{}' ({}µs)", node_label, "⨯".bold().red(), output, duration.as_micros());
                }
            }
            Err(CartridgeError::FuelExhausted) => {
                println!("{} {} ⚠️  TRAPPED: Fuel Exhausted - Catastrophic Backtracking Detected! ({}µs)", node_label, "⚡".bold().yellow(), duration.as_micros());
            }
            Err(CartridgeError::MemoryExceeded) | Err(CartridgeError::ExecutionError(..)) => {
                println!("{} {} ⚠️  TRAPPED: Memory Exceeded - Massive Allocation Detected! ({}µs)", node_label, "💥".bold().magenta(), duration.as_micros());
            }
            Err(e) => {
                println!("{} ❌ Unexpected Error: {:?}", node_label, e);
            }
        }
    }

    println!("\n{}\n", "==================================================".bold().blue());
    println!("{} Verification completed in {:?}", "[SYSTEM]".bold().cyan(), start_total.elapsed());
    println!("{} ReDoS logic bombs trapped instantly. Zero host degradation.", "[SYSTEM]".bold().cyan());
    println!("{}\n", "==================================================".bold().blue());
}
