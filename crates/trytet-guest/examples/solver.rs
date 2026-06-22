//! Solver Agent: Neuro-Symbolic Scheduling
//!
//! Agent calls a Z3 cartridge, handles the result or the failure.
//!
//! Build: cargo build --target wasm32-wasip1 --release
//! Run:   tet up target/wasm32-wasip1/release/solver.wasm --fuel 50000000

use trytet_guest::{invoke_cartridge, print, CartridgeResult};

#[no_mangle]
pub extern "C" fn _start() {
    print("Trytet Scheduling Agent");
    print("=======================");

    let constraints = r#"{
        "meetings": 5,
        "week": "2026-W19",
        "constraints": ["no_conflicts", "room_capacity", "lunch_break"],
        "rooms": ["A1", "B3", "C2"],
        "blocked": [{"day": "Fri", "reason": "company offsite"}]
    }"#;

    print("\n[agent] Sending constraints to z3-solver...");

    match invoke_cartridge("z3-solver", constraints, 1_000_000) {
        Ok(result) => {
            print(&format!("[agent] Result: {}", result));

            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&result) {
                if parsed["status"] == "sat" {
                    print("\n[agent] Schedule is satisfiable:");
                    if let Some(schedule) = parsed["model"]["schedule"].as_array() {
                        for m in schedule {
                            print(&format!(
                                "  {} {} | {} (Room {})",
                                m["day"].as_str().unwrap_or("?"),
                                m["time"].as_str().unwrap_or("?"),
                                m["event"].as_str().unwrap_or("?"),
                                m["room"].as_str().unwrap_or("?"),
                            ));
                        }
                    }
                } else {
                    print("[agent] Unsatisfiable. Relaxing constraints...");
                }
            }
        }
        Err(CartridgeResult::FuelExhausted) => {
            print("[agent] Solver exhausted fuel. Falling back to heuristic.");
        }
        Err(CartridgeResult::RegistryError) => {
            print("[agent] z3-solver not found. Run: tet cartridge load z3-solver <path>");
        }
        Err(e) => {
            print(&format!("[agent] Error: {:?}", e));
        }
    }

    print("\n[agent] Done.");
}
