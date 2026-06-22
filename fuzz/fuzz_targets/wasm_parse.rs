#![no_main]

use libfuzzer_sys::fuzz_target;
use wasmtime::{Config, Engine, Module, OptLevel};

// Fuzz target: feed arbitrary bytes to wasmtime module parsing.
// Goal: ensure that `Module::new` never panics or exhibits undefined behavior
// when given attacker-controlled (or otherwise malformed) WASM bytes.
fuzz_target!(|data: &[u8]| {
    // Use the same engine config that Tet uses in production.
    let mut config = Config::new();
    config.consume_fuel(true);
    config.cranelift_opt_level(OptLevel::Speed);

    // Engine creation is infallible with a valid config, but handle gracefully.
    let engine = match Engine::new(&config) {
        Ok(e) => e,
        Err(_) => return,
    };

    // Module::new validates the WASM binary header, sections, and types.
    // It should return an Err for any malformed input — never panic.
    let _ = Module::new(&engine, data);
});
