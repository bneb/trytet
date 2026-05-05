//! Demo harness for the neuro-symbolic workflow.
//!
//! 1. Load Z3 stub, invoke with scheduling constraints, get SAT result
//! 2. Load logic bomb, invoke, verify FuelExhausted trap
//! 3. Re-invoke Z3 stub, verify host survived
//!
//! Run: cargo test --test demo_neuro_symbolic -- --nocapture

use tet_core::cartridge::{CartridgeError, CartridgeManager};

fn setup() -> CartridgeManager {
    let mut config = wasmtime::Config::new();
    config.consume_fuel(true);
    config.wasm_component_model(true);
    let engine = wasmtime::Engine::new(&config).expect("Engine init");
    CartridgeManager::new(&engine)
}

#[test]
fn demo_neuro_symbolic_workflow() {
    let mgr = setup();

    eprintln!("\n  TRYTET NEURO-SYMBOLIC DEMO\n");

    // 1. Load and invoke Z3 stub
    let z3_wat = include_str!("../demos/neuro_symbolic/z3_stub.wat");
    let z3_wasm = wat::parse_str(z3_wat).expect("parse z3_stub.wat");
    mgr.precompile("z3-solver", &z3_wasm).expect("precompile z3");

    eprintln!("  [1/5] z3-solver compiled");

    let constraints = r#"{"meetings":5,"week":"2026-W19","constraints":["no_conflicts","room_capacity"]}"#;
    let start = std::time::Instant::now();
    let (result, metrics) = mgr
        .invoke("z3-solver", constraints, 10_000_000, 512)
        .expect("z3 invoke");
    let elapsed = start.elapsed();

    let parsed: serde_json::Value = serde_json::from_str(&result).expect("valid json");
    assert_eq!(parsed["status"], "sat");
    assert_eq!(parsed["model"]["conflicts"], 0);
    let schedule = parsed["model"]["schedule"].as_array().unwrap();
    assert_eq!(schedule.len(), 5);

    eprintln!("  [2/5] SAT: {} meetings, 0 conflicts ({}us, {} fuel)",
        schedule.len(), elapsed.as_micros(), metrics.fuel_consumed);

    // 2. Load and invoke logic bomb
    let bomb_wat = include_str!("../demos/neuro_symbolic/logic_bomb.wat");
    let bomb_wasm = wat::parse_str(bomb_wat).expect("parse logic_bomb.wat");
    mgr.precompile("logic-bomb", &bomb_wasm).expect("precompile bomb");

    eprintln!("  [3/5] logic-bomb compiled, invoking with fuel=100000");

    let bomb_start = std::time::Instant::now();
    let bomb_result = mgr.invoke("logic-bomb", "{\"formula\":\"unsat\"}", 100_000, 512);
    let bomb_elapsed = bomb_start.elapsed();

    match &bomb_result {
        Err(CartridgeError::FuelExhausted) => {
            eprintln!("  [4/5] FuelExhausted in {}us", bomb_elapsed.as_micros());
        }
        other => panic!("expected FuelExhausted, got: {:?}", other),
    }

    // 3. Verify host survived
    let (result2, metrics2) = mgr
        .invoke("z3-solver", constraints, 10_000_000, 512)
        .expect("z3 must still work after bomb");

    let parsed2: serde_json::Value = serde_json::from_str(&result2).unwrap();
    assert_eq!(parsed2["status"], "sat");

    eprintln!("  [5/5] z3-solver still operational ({}us, {} fuel)\n",
        metrics2.duration_us, metrics2.fuel_consumed);
}
