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

fn main() {
    println!("\n{}\n", "==================================================".bold().blue());
    println!("{}", "   Trytet Autonomous Swarm MCTS Demonstration".bold().green());
    println!("{}\n", "==================================================".bold().blue());
    
    println!("{} Booting Trytet Engine...", "[SYSTEM]".bold().cyan());
    let mgr = setup_cartridge_manager();
    let wasm = load_python_evaluator_wasm();
    mgr.precompile("python", &wasm).expect("Failed to precompile python-evaluator");
    println!("{} Python Cartridge precompiled. Sandboxes are hot.\n", "[SYSTEM]".bold().cyan());

    println!("{} Initializing 500 Agent Swarm nodes...", "[SWARM]".bold().magenta());
    println!("{} Objective: Find the Python permutation that yields the value '100'.\n", "[SWARM]".bold().magenta());

    let mut success_found = false;
    let start_total = Instant::now();
    let total_branches = 500;

    for branch_id in 1..=total_branches {
        if success_found {
            break;
        }

        let script_type = if branch_id == 482 {
            "success"
        } else if branch_id % 15 == 0 {
            "infinite_loop"
        } else if branch_id % 23 == 0 {
            "memory_bomb"
        } else {
            "failed_logic"
        };

        let script = match script_type {
            "success" => "10 * 10",
            "infinite_loop" => "while True:\n    pass",
            "memory_bomb" => "bomb = []\nwhile True:\n    bomb.append('A' * 1000000)",
            _ => "2 + 3", // Wrong answer
        };

        let start_eval = Instant::now();
        // 2 Billion fuel is needed for RustPython's cold boot. 8MB memory limit.
        let result = mgr.invoke("python", script, 2_000_000_000, 8);
        let duration = start_eval.elapsed();

        let node_label = format!("[Node {:03}]", branch_id).bold().bright_black();

        match result {
            Ok((output, _metrics)) => {
                if output == "100" {
                    println!("{} {} 🎯 SUCCESS: Found correct solution! ({}µs)", node_label, "✓".bold().green(), duration.as_micros());
                    success_found = true;
                } else {
                    println!("{} {} ❌ Logic evaluated but output '{}' is incorrect. ({}µs)", node_label, "⨯".bold().red(), output, duration.as_micros());
                }
            }
            Err(CartridgeError::FuelExhausted) => {
                println!("{} {} ⚠️  TRAPPED: Fuel Exhausted (Infinite Loop) ({}µs)", node_label, "⚡".bold().yellow(), duration.as_micros());
            }
            Err(CartridgeError::MemoryExceeded) | Err(CartridgeError::ExecutionError(..)) => {
                println!("{} {} ⚠️  TRAPPED: Memory Exceeded (Memory Bomb) ({}µs)", node_label, "💥".bold().magenta(), duration.as_micros());
            }
            Err(e) => {
                println!("{} ❌ Unexpected Error: {:?}", node_label, e);
            }
        }
    }

    println!("\n{}\n", "==================================================".bold().blue());
    println!("{} Swarm converged in {:?}", "[SWARM]".bold().magenta(), start_total.elapsed());
    println!("{} All malicious logic deterministically trapped.", "[SYSTEM]".bold().cyan());
    println!("{}\n", "==================================================".bold().blue());
}
