//! Phase 33.1: Neuro-Symbolic Cartridge Substrate — TDD Test Suite
//!
//! Three red-to-green tests validating the CartridgeManager:
//! 1. "Z3 Logic Bomb"     — Fuel exhaustion isolation
//! 2. "Micro-Latency"     — Sub-millisecond spin-up overhead
//! 3. "Schema-Strict"     — String boundary integrity across Wasm boundary

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

/// Build a Component WAT with an infinite loop that burns all fuel.
///
/// This is a minimal component: the core function enters an infinite loop
/// immediately. The `canon lift` bridges it to the component model. When fuel
/// runs out, wasmtime traps with OutOfFuel.
///
/// Note: We use a function with no params/results to avoid the complexity
/// of the canonical ABI string handling in the loop test. The execute function
/// with strings is tested separately in the schema-strict test.
fn infinite_loop_component_wat() -> &'static str {
    // A component that exports a single function "execute" which loops forever.
    // We use minimal canonical ABI: the function takes a string (ptr+len) and
    // should return a result, but it loops before returning.
    r#"(component
        (core module $m
            (memory (export "memory") 1)

            (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32)
                ;; Simple bump allocator: return the alignment parameter as offset
                ;; This is fine for tests since we never actually allocate much
                i32.const 512
            )

            ;; The execute function enters an infinite loop before returning.
            ;; The canonical ABI will call this after lowering the string argument.
            (func (export "execute") (param i32 i32) (result i32)
                ;; Infinite loop — simulates Z3 solver hitting a logic bomb
                (loop $spin
                    (br $spin)
                )
                ;; unreachable — but the loop prevents us from getting here
                unreachable
            )
        )
        (core instance $i (instantiate $m))

        ;; Lift execute to component model: string -> result<string, string>
        (func $execute (param "input" string) (result (result string (error string)))
            (canon lift (core func $i "execute")
                (memory $i "memory")
                (realloc (func $i "cabi_realloc"))
            )
        )

        (export "execute" (func $execute))
    )"#
}

/// Build a trivial Component that echoes the input string back.
/// The execute function immediately returns Ok(input) by passing through
/// the input pointer/length.
fn echo_component_wat() -> &'static str {
    r#"(component
        (core module $m
            (memory (export "memory") 1)

            ;; Bump allocator at fixed offset 4096
            (global $bump (mut i32) (i32.const 4096))
            (func (export "cabi_realloc") (param $old_ptr i32) (param $old_size i32) (param $align i32) (param $new_size i32) (result i32)
                (local $ptr i32)
                (local.set $ptr (global.get $bump))
                (global.set $bump (i32.add (global.get $bump) (local.get $new_size)))
                (local.get $ptr)
            )

            ;; Echo: return the input string as Ok variant
            ;; Canonical ABI for result<string, string>:
            ;;   ret_area + 0: discriminant (0 = Ok, 1 = Err), i8
            ;;   ret_area + 4: string ptr, i32
            ;;   ret_area + 8: string len, i32
            (func (export "execute") (param $ptr i32) (param $len i32) (result i32)
                ;; Write return value at offset 2048
                ;; discriminant = 0 (Ok)
                (i32.store8 (i32.const 2048) (i32.const 0))
                ;; string ptr = input ptr (echo back)
                (i32.store (i32.const 2052) (local.get $ptr))
                ;; string len = input len
                (i32.store (i32.const 2056) (local.get $len))
                ;; return pointer to ret area
                (i32.const 2048)
            )
        )
        (core instance $i (instantiate $m))

        (func $execute (param "input" string) (result (result string (error string)))
            (canon lift (core func $i "execute")
                (memory $i "memory")
                (realloc (func $i "cabi_realloc"))
            )
        )

        (export "execute" (func $execute))
    )"#
}

/// Build a Component that returns a complex JSON schedule string.
/// Tests string allocation/deallocation across the Wasm boundary.
fn json_schedule_component_wat() -> &'static str {
    r#"(component
        (core module $m
            (memory (export "memory") 1)

            ;; Pre-baked JSON response at a known offset (starts at byte 8192)
            (data (i32.const 8192)
                "{\"schedule\":[{\"time\":\"09:00\",\"event\":\"standup\"},{\"time\":\"14:00\",\"event\":\"review\"}],\"solver\":\"z3\",\"status\":\"sat\"}"
            )

            ;; Bump allocator at offset 4096
            (global $bump (mut i32) (i32.const 4096))
            (func (export "cabi_realloc") (param $old_ptr i32) (param $old_size i32) (param $align i32) (param $new_size i32) (result i32)
                (local $ptr i32)
                (local.set $ptr (global.get $bump))
                (global.set $bump (i32.add (global.get $bump) (local.get $new_size)))
                (local.get $ptr)
            )

            (func (export "execute") (param $ptr i32) (param $len i32) (result i32)
                ;; Return a fixed JSON string as Ok
                ;; ret area at offset 2048
                ;; discriminant = 0 (Ok)
                (i32.store8 (i32.const 2048) (i32.const 0))
                ;; string ptr = 8192 (where our JSON data lives)
                (i32.store (i32.const 2052) (i32.const 8192))
                ;; string len = 105
                (i32.store (i32.const 2056) (i32.const 105))
                ;; return pointer to ret area
                (i32.const 2048)
            )
        )
        (core instance $i (instantiate $m))

        (func $execute (param "input" string) (result (result string (error string)))
            (canon lift (core func $i "execute")
                (memory $i "memory")
                (realloc (func $i "cabi_realloc"))
            )
        )

        (export "execute" (func $execute))
    )"#
}

// ===========================================================================
// Test 1: "Z3 Logic Bomb" — Fuel Exhaustion Isolation
// ===========================================================================

#[test]
fn test_cartridge_fuel_exhaustion_returns_error() {
    let mgr = setup_cartridge_manager();

    // 1. Pre-compile the infinite loop cartridge
    let wat = infinite_loop_component_wat();
    let wasm = wat::parse_str(wat).expect("Failed to parse infinite loop component WAT");
    mgr.precompile("logic-bomb", &wasm)
        .expect("Failed to precompile logic bomb cartridge");

    // 2. Invoke with a low fuel limit (100,000 units)
    let result = mgr.invoke("logic-bomb", "{\"query\": \"unsolvable\"}", 100_000, 512);

    // 3. Assert: Must return FuelExhausted, NOT hang
    match result {
        Err(CartridgeError::FuelExhausted) => {
            // PASS: The Host correctly trapped the runaway solver
        }
        other => panic!(
            "Expected CartridgeError::FuelExhausted, got: {:?}",
            other
        ),
    }
}

#[test]
fn test_cartridge_fuel_exhaustion_host_survives() {
    let mgr = setup_cartridge_manager();

    // 1. Pre-compile both cartridges
    let bomb_wat = infinite_loop_component_wat();
    let bomb_wasm = wat::parse_str(bomb_wat).expect("Failed to parse bomb WAT");
    mgr.precompile("bomb", &bomb_wasm).unwrap();

    let echo_wat = echo_component_wat();
    let echo_wasm = wat::parse_str(echo_wat).expect("Failed to parse echo WAT");
    mgr.precompile("echo", &echo_wasm).unwrap();

    // 2. Detonate the logic bomb
    let bomb_result = mgr.invoke("bomb", "{}", 100_000, 512);
    assert!(matches!(bomb_result, Err(CartridgeError::FuelExhausted)));

    // 3. The host must still be alive — invoke a well-behaved cartridge
    let echo_result = mgr.invoke("echo", "hello", 1_000_000, 512);
    assert!(
        echo_result.is_ok(),
        "CartridgeManager must remain operational after a fuel exhaustion trap. Got: {:?}",
        echo_result
    );
}

// ===========================================================================
// Test 2: "Micro-Latency Spin-up" — Performance
// ===========================================================================

#[test]
fn test_cartridge_micro_latency_spinup() {
    let mgr = setup_cartridge_manager();

    // 1. Pre-compile the echo cartridge
    let echo_wat = echo_component_wat();
    let echo_wasm = wat::parse_str(echo_wat).expect("Failed to parse echo WAT");
    mgr.precompile("echo-bench", &echo_wasm).unwrap();

    // 2. Warm-up run (JIT compilation may happen on first call)
    let _ = mgr.invoke("echo-bench", "warmup", 1_000_000, 512);

    // 3. Measure 100 invocations
    let mut durations_us = Vec::with_capacity(100);
    for i in 0..100 {
        let start = std::time::Instant::now();
        let result = mgr.invoke("echo-bench", &format!("test-{}", i), 1_000_000, 512);
        let elapsed_us = start.elapsed().as_micros() as u64;

        assert!(result.is_ok(), "Echo cartridge should succeed: {:?}", result);
        durations_us.push(elapsed_us);
    }

    // 4. Compute p99
    durations_us.sort();
    let p50 = durations_us[49];
    let p99 = durations_us[98];
    let max = durations_us[99];
    let mean = durations_us.iter().sum::<u64>() / durations_us.len() as u64;

    eprintln!(
        "Cartridge Spin-up Latency (100 invocations):\n  mean={}µs  p50={}µs  p99={}µs  max={}µs",
        mean, p50, p99, max
    );

    // Assert: p99 must be < 500µs (0.5ms) — targeting 100µs Northstar
    assert!(
        p99 < 500,
        "Cartridge spin-up p99 exceeded 0.5ms target: p99={}µs",
        p99
    );
}

// ===========================================================================
// Test 3: "Schema-Strict Return" — String Boundary Integrity
// ===========================================================================

#[test]
fn test_cartridge_schema_strict_return() {
    let mgr = setup_cartridge_manager();

    // 1. Pre-compile the JSON schedule cartridge
    let json_wat = json_schedule_component_wat();
    let json_wasm = wat::parse_str(json_wat).expect("Failed to parse JSON schedule WAT");
    mgr.precompile("schedule-solver", &json_wasm)
        .expect("Failed to precompile schedule cartridge");

    // 2. Invoke with a problem specification
    let result = mgr.invoke(
        "schedule-solver",
        "{\"constraints\": [\"no conflicts\"]}",
        10_000_000,
        512,
    );

    // 3. Assert: Successful invocation
    let (output, metrics) = result.expect("Schedule cartridge should return successfully");

    // 4. Assert: String is valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&output).expect("Cartridge output must be valid JSON");

    // 5. Assert: JSON structure is correct
    assert!(parsed.get("schedule").is_some(), "Missing 'schedule' key");
    assert!(parsed.get("solver").is_some(), "Missing 'solver' key");
    assert!(parsed.get("status").is_some(), "Missing 'status' key");

    let schedule = parsed["schedule"].as_array().expect("'schedule' must be an array");
    assert_eq!(schedule.len(), 2, "Expected 2 schedule entries");
    assert_eq!(schedule[0]["event"], "standup");
    assert_eq!(schedule[1]["event"], "review");

    // 6. Assert: No null bytes in the string (memory corruption indicator)
    assert!(
        !output.contains('\0'),
        "Output string contains null bytes — possible memory corruption"
    );

    // 7. Assert: Fuel was consumed
    assert!(
        metrics.fuel_consumed > 0,
        "Fuel consumed must be > 0, got: {}",
        metrics.fuel_consumed
    );

    eprintln!(
        "Schema-Strict Return: output_len={} fuel_consumed={} duration={}µs",
        output.len(),
        metrics.fuel_consumed,
        metrics.duration_us
    );
}

// ===========================================================================
// Supplementary: Cache Management Tests
// ===========================================================================

#[test]
fn test_cartridge_not_found_returns_registry_error() {
    let mgr = setup_cartridge_manager();

    let result = mgr.invoke("nonexistent-cartridge", "{}", 1_000_000, 512);
    match result {
        Err(CartridgeError::RegistryError(_)) => {
            // PASS: Correctly reports missing cartridge
        }
        other => panic!(
            "Expected CartridgeError::RegistryError, got: {:?}",
            other
        ),
    }
}

#[test]
fn test_cartridge_cache_eviction() {
    let mgr = setup_cartridge_manager();

    let echo_wat = echo_component_wat();
    let echo_wasm = wat::parse_str(echo_wat).unwrap();
    mgr.precompile("evictable", &echo_wasm).unwrap();

    assert!(mgr.is_cached("evictable"));

    mgr.evict("evictable");

    assert!(!mgr.is_cached("evictable"));

    // Invoking after eviction should fail with RegistryError
    let result = mgr.invoke("evictable", "test", 1_000_000, 512);
    assert!(matches!(result, Err(CartridgeError::RegistryError(_))));
}

#[test]
fn test_cartridge_precompile_invalid_wasm() {
    let mgr = setup_cartridge_manager();

    let result = mgr.precompile("invalid", b"this is not wasm");
    match result {
        Err(CartridgeError::CompilationFailed(_)) => {
            // PASS: Invalid bytes correctly rejected
        }
        other => panic!(
            "Expected CartridgeError::CompilationFailed, got: {:?}",
            other
        ),
    }
}
