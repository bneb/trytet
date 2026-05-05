//! Neuro-Symbolic Cartridge Substrate (Phase 33.1)
//!
//! The `CartridgeManager` enables the Trytet Host to dynamically load, link,
//! and execute Wasm Components ("Cartridges") using the Wasm Component Model.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  LLM Agent (fuzzy plan)                                     │
//! │  "Schedule meetings avoiding conflicts using Z3 solver"     │
//! └───────────────────┬─────────────────────────────────────────┘
//!                     │ trytet::invoke_component("z3-solver", payload, fuel)
//! ┌───────────────────▼─────────────────────────────────────────┐
//! │  CartridgeManager                                           │
//! │  ┌──────────────┐ ┌──────────────┐ ┌────────────────────┐  │
//! │  │ Compiled     │ │ Child Store  │ │ WIT Interface      │  │
//! │  │ Cache        │ │ (fuel-bound) │ │ cartridge-v1       │  │
//! │  │ DashMap<CID> │ │ StoreLimits  │ │ execute(str)->r<>  │  │
//! │  └──────────────┘ └──────────────┘ └────────────────────┘  │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Lifecycle
//!
//! 1. **Registry Lookup**: The LLM requests `cartridge:z3-solver`
//! 2. **Snapshot Loading**: Host pulls `z3.wasm` from SovereignRegistry
//! 3. **Component Linking**: Host links Cartridge exports via WIT
//! 4. **Fuel-Bound Execution**: Cartridge runs in its own sub-sandbox
//! 5. **Instant Unmount**: Store dropped → $O(1)$ memory reclamation

use dashmap::DashMap;
use std::time::Instant;
use thiserror::Error;
use wasmtime::component::{Component, Linker, Val};
use wasmtime::{Engine, Store, StoreLimits, StoreLimitsBuilder};

// ---------------------------------------------------------------------------
// Error Types
// ---------------------------------------------------------------------------

/// Errors specific to Cartridge operations.
#[derive(Error, Debug)]
pub enum CartridgeError {
    /// The cartridge exhausted its allocated fuel budget.
    /// This is the expected outcome for runaway solvers (e.g., Z3 logic bomb).
    #[error("Cartridge fuel exhausted")]
    FuelExhausted,

    /// The cartridge exceeded its memory allocation.
    #[error("Cartridge memory limit exceeded")]
    MemoryExceeded,

    /// The Wasm binary failed to compile as a Component.
    #[error("Cartridge compilation failed: {0}")]
    CompilationFailed(String),

    /// The component does not export the required `cartridge-v1` interface.
    #[error("Cartridge interface mismatch: {0}")]
    InterfaceMismatch(String),

    /// The cartridge's `execute` function returned an error result.
    #[error("Cartridge execution error: {0}")]
    ExecutionError(String),

    /// Failed to resolve the cartridge from the registry.
    #[error("Cartridge registry error: {0}")]
    RegistryError(String),
}

// ---------------------------------------------------------------------------
// Cartridge Store State (minimal — cartridges are stateless)
// ---------------------------------------------------------------------------

/// The store-level state for a Cartridge execution.
///
/// Deliberately minimal: cartridges are stateless functional units.
/// All persistence is handled by the parent Agent's CoW VFS.
pub struct CartridgeState {
    limits: StoreLimits,
}

// ---------------------------------------------------------------------------
// CartridgeManager
// ---------------------------------------------------------------------------

/// The host-side manager for loading, caching, and executing Cartridges.
///
/// Thread-safe: all interior state is behind concurrent data structures.
/// A single `CartridgeManager` is shared across all Agents on a node.
pub struct CartridgeManager {
    /// Shared wasmtime Engine (same config as the parent sandbox).
    engine: Engine,

    /// Pre-compiled Component cache, keyed by content-addressed ID (CID).
    /// `Component::new()` is expensive (Cranelift compilation); we do it once.
    compiled_cache: DashMap<String, Component>,
}

impl CartridgeManager {
    /// Create a new CartridgeManager backed by the given Engine.
    ///
    /// The Engine should have `consume_fuel(true)` and `component_model(true)`
    /// in its config (both are set by `production_engine_config()`).
    pub fn new(engine: &Engine) -> Self {
        Self {
            engine: engine.clone(),
            compiled_cache: DashMap::new(),
        }
    }

    /// Pre-compile a component from raw bytes and cache it.
    ///
    /// This is the expensive path — Cranelift compiles the Wasm to native code.
    /// Subsequent `invoke()` calls for the same CID will hit the cache.
    pub fn precompile(&self, cid: &str, component_bytes: &[u8]) -> Result<(), CartridgeError> {
        let component = Component::new(&self.engine, component_bytes)
            .map_err(|e| CartridgeError::CompilationFailed(format!("{e:#}")))?;

        self.compiled_cache
            .insert(cid.to_string(), component);

        Ok(())
    }

    /// Check if a component is already cached.
    pub fn is_cached(&self, cid: &str) -> bool {
        self.compiled_cache.contains_key(cid)
    }

    /// Evict a component from the cache.
    pub fn evict(&self, cid: &str) {
        self.compiled_cache.remove(cid);
    }

    /// The hot path: instantiate a cached Component, call `execute(input)`,
    /// return the result string.
    ///
    /// Each invocation gets its own `Store` with independent fuel and memory
    /// limits. When the Store is dropped, all guest memory is reclaimed in
    /// $O(1)$ time.
    ///
    /// # Returns
    ///
    /// - `Ok(json_string)` — the cartridge's successful result
    /// - `Err(FuelExhausted)` — the cartridge hit its fuel limit
    /// - `Err(MemoryExceeded)` — the cartridge hit its memory limit
    /// - `Err(ExecutionError)` — the cartridge's `execute` returned `Err`
    /// - `Err(InterfaceMismatch)` — the component doesn't export `cartridge-v1`
    pub fn invoke(
        &self,
        component_id: &str,
        payload: &str,
        fuel: u64,
        max_memory_mb: u32,
    ) -> Result<(String, InvocationMetrics), CartridgeError> {
        let start = Instant::now();

        // 1. Resolve the pre-compiled component from cache
        let component = self
            .compiled_cache
            .get(component_id)
            .ok_or_else(|| {
                CartridgeError::RegistryError(format!(
                    "Component '{}' not found in compiled cache",
                    component_id
                ))
            })?;

        // 2. Create a fresh child Store with the specified fuel and memory limits
        let limits = StoreLimitsBuilder::new()
            .memory_size(max_memory_mb as usize * 1024 * 1024)
            .instances(1)
            .tables(10)
            .memories(10)
            .build();

        let state = CartridgeState { limits };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.limits);
        store
            .set_fuel(fuel)
            .map_err(|e| CartridgeError::ExecutionError(format!("Failed to set fuel: {e}")))?;

        // 3. Create a component Linker (empty — cartridges have no imports)
        let linker: Linker<CartridgeState> = Linker::new(&self.engine);

        // 4. Instantiate the component
        let instance = linker
            .instantiate(&mut store, &component)
            .map_err(|e| classify_cartridge_trap(&e))?;

        // 5. Look up the exported `execute` function via the cartridge-v1 interface
        //    The WIT export path is: trytet:component/cartridge-v1.execute
        let execute_func = instance
            .get_func(&mut store, "trytet:component/cartridge-v1#execute")
            .or_else(|| {
                // Fallback: try the flattened export name
                instance.get_func(&mut store, "execute")
            })
            .ok_or_else(|| {
                CartridgeError::InterfaceMismatch(
                    "Component does not export 'execute' function from cartridge-v1 interface"
                        .to_string(),
                )
            })?;

        // 6. Call execute(input) -> result<string, string>
        //    In the Component Model, result<string, string> is represented as a variant:
        //    - Ok(string) = (0, string_ptr)
        //    - Err(string) = (1, string_ptr)
        let mut results = vec![Val::Bool(false)]; // placeholder, will be overwritten
        let params = vec![Val::String(payload.into())];

        let call_result = execute_func.call(&mut store, &params, &mut results);
        let fuel_remaining = store.get_fuel().unwrap_or(0);
        let fuel_consumed = fuel.saturating_sub(fuel_remaining);

        let instantiation_to_result = start.elapsed();

        match call_result {
            Ok(()) => {
                // post_return is handled automatically by wasmtime's component Func::call

                // Parse the result<string, string>
                // Val::Result contains Result<Option<Box<Val>>, Option<Box<Val>>>
                match &results[0] {
                    Val::Result(Ok(Some(inner))) => {
                        if let Val::String(s) = inner.as_ref() {
                            let output = s.to_string();
                            Ok((
                                output,
                                InvocationMetrics {
                                    fuel_consumed,
                                    duration_us: instantiation_to_result.as_micros() as u64,
                                },
                            ))
                        } else {
                            Err(CartridgeError::ExecutionError(format!(
                                "Expected string in Ok variant, got: {:?}",
                                inner
                            )))
                        }
                    }
                    Val::Result(Ok(None)) => Err(CartridgeError::ExecutionError(
                        "Cartridge returned Ok with no value".to_string(),
                    )),
                    Val::Result(Err(Some(inner))) => {
                        if let Val::String(e) = inner.as_ref() {
                            Err(CartridgeError::ExecutionError(e.to_string()))
                        } else {
                            Err(CartridgeError::ExecutionError(format!(
                                "Cartridge returned error: {:?}",
                                inner
                            )))
                        }
                    }
                    Val::Result(Err(None)) => Err(CartridgeError::ExecutionError(
                        "Cartridge returned Err with no value".to_string(),
                    )),
                    Val::String(s) => {
                        // Some component models flatten result<string, string> to string
                        Ok((
                            s.to_string(),
                            InvocationMetrics {
                                fuel_consumed,
                                duration_us: instantiation_to_result.as_micros() as u64,
                            },
                        ))
                    }
                    other => Err(CartridgeError::InterfaceMismatch(format!(
                        "Expected result<string, string>, got: {:?}",
                        other
                    ))),
                }
            }
            Err(e) => Err(classify_cartridge_trap(&e)),
        }
    }
}

// ---------------------------------------------------------------------------
// Invocation Metrics
// ---------------------------------------------------------------------------

/// Performance metrics from a single Cartridge invocation.
#[derive(Debug, Clone)]
pub struct InvocationMetrics {
    /// Fuel consumed by the cartridge execution.
    pub fuel_consumed: u64,
    /// Wall-clock duration in microseconds.
    pub duration_us: u64,
}

// ---------------------------------------------------------------------------
// Trap Classification
// ---------------------------------------------------------------------------

/// Classify a wasmtime error into the appropriate CartridgeError.
fn classify_cartridge_trap(error: &wasmtime::Error) -> CartridgeError {
    let message = format!("{error:#}");

    if message.contains("out of fuel")
        || message.contains("fuel consumed")
        || message.contains("all fuel consumed")
    {
        return CartridgeError::FuelExhausted;
    }

    if message.contains("memory")
        && (message.contains("limit") || message.contains("maximum") || message.contains("grow"))
    {
        return CartridgeError::MemoryExceeded;
    }

    CartridgeError::ExecutionError(message)
}
