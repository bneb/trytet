//! # trytet-guest
//!
//! Guest-side SDK for building Trytet agents and cartridges.
//!
//! This crate provides safe Rust wrappers over the Trytet host functions,
//! allowing you to write agents that invoke other agents, call cartridges,
//! access the vector store, and interact with the mesh. All from inside
//! a Wasm sandbox.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use trytet_guest::*;
//!
//! #[no_mangle]
//! pub extern "C" fn run() {
//!     // Call a neuro-symbolic cartridge
//!     let result = invoke_component(
//!         "z3-solver",
//!         r#"{"meetings": 5, "constraints": ["no_conflicts"]}"#,
//!         1_000_000, // fuel budget
//!     );
//!
//!     match result {
//!         Ok(json) => print(&format!("Solver returned: {}", json)),
//!         Err(CartridgeResult::FuelExhausted) => print("Solver ran out of fuel"),
//!         Err(e) => print(&format!("Solver error: {:?}", e)),
//!     }
//! }
//! ```

// ---------------------------------------------------------------------------
// Host Function Imports (provided by the Trytet runtime)
// ---------------------------------------------------------------------------

#[link(wasm_import_module = "trytet")]
extern "C" {
    /// Write a string to the agent's stdout.
    fn log(ptr: i32, len: i32);

    /// Invoke a child agent by alias via the Tet Mesh.
    fn invoke(
        alias_ptr: i32,
        alias_len: i32,
        payload_ptr: i32,
        payload_len: i32,
        fuel: i64,
        out_ptr: i32,
        out_len: i32,
    ) -> i32;

    /// Invoke a Wasm Component (Cartridge) by content ID.
    fn invoke_component(
        cid_ptr: i32,
        cid_len: i32,
        payload_ptr: i32,
        payload_len: i32,
        fuel: i64,
        out_ptr: i32,
        out_len_ptr: i32,
    ) -> i32;

    /// Store a vector in the agent's semantic memory.
    fn remember(
        collection_ptr: i32,
        collection_len: i32,
        id_ptr: i32,
        id_len: i32,
        vector_ptr: i32,
        vector_len: i32,
    ) -> i32;

    /// Query the agent's semantic memory by vector similarity.
    fn recall(
        collection_ptr: i32,
        collection_len: i32,
        query_ptr: i32,
        query_len: i32,
        limit: i32,
        out_ptr: i32,
        out_len: i32,
    ) -> i32;
}

// ---------------------------------------------------------------------------
// Safe Wrappers
// ---------------------------------------------------------------------------

/// Print a message to the agent's stdout stream.
pub fn print(msg: &str) {
    unsafe {
        log(msg.as_ptr() as i32, msg.len() as i32);
    }
}

/// Result codes from a cartridge invocation.
#[derive(Debug, Clone, PartialEq)]
pub enum CartridgeResult {
    /// The cartridge exhausted its fuel budget.
    FuelExhausted,
    /// The output buffer was too small (contains required size).
    BufferTooSmall(usize),
    /// The component binary failed to compile.
    CompilationFailed,
    /// The component doesn't export the cartridge-v1 interface.
    InterfaceMismatch,
    /// The cartridge's execute function returned an error.
    ExecutionError,
    /// The component ID was not found in the registry.
    RegistryError,
    /// Unknown error code.
    Unknown(i32),
}

/// Invoke a cartridge by content ID.
///
/// The cartridge runs in a fuel-bounded sub-sandbox. On fuel exhaustion,
/// returns `Err(CartridgeResult::FuelExhausted)`. The calling agent is unaffected.
///
/// # Arguments
///
/// * `component_id` - Content-addressed ID of the pre-compiled cartridge
/// * `payload` - JSON input for the cartridge's `execute` function
/// * `fuel` - Maximum fuel budget for this invocation
///
/// # Returns
///
/// * `Ok(String)` - JSON result from the cartridge
/// * `Err(CartridgeResult)` - Structured error code
pub fn invoke_cartridge(
    component_id: &str,
    payload: &str,
    fuel: u64,
) -> Result<String, CartridgeResult> {
    // Allocate a 64KB output buffer on the stack
    let mut out_buf = vec![0u8; 65536];
    let mut out_len: i32 = out_buf.len() as i32;

    let code = unsafe {
        invoke_component(
            component_id.as_ptr() as i32,
            component_id.len() as i32,
            payload.as_ptr() as i32,
            payload.len() as i32,
            fuel as i64,
            out_buf.as_mut_ptr() as i32,
            &mut out_len as *mut i32 as i32,
        )
    };

    match code {
        0 => {
            let result = &out_buf[..out_len as usize];
            Ok(String::from_utf8_lossy(result).to_string())
        }
        1 => Err(CartridgeResult::FuelExhausted),
        2 => Err(CartridgeResult::BufferTooSmall(out_len as usize)),
        3 => Err(CartridgeResult::CompilationFailed),
        4 => Err(CartridgeResult::InterfaceMismatch),
        5 => Err(CartridgeResult::ExecutionError),
        6 => Err(CartridgeResult::RegistryError),
        other => Err(CartridgeResult::Unknown(other)),
    }
}

/// Invoke a child agent by alias via the Tet Mesh.
///
/// Inter-agent RPC. The child may live on a different node.
pub fn call_agent(
    alias: &str,
    payload: &[u8],
    fuel: u64,
) -> Result<Vec<u8>, i32> {
    let mut out_buf = vec![0u8; 65536];

    let code = unsafe {
        invoke(
            alias.as_ptr() as i32,
            alias.len() as i32,
            payload.as_ptr() as i32,
            payload.len() as i32,
            fuel as i64,
            out_buf.as_mut_ptr() as i32,
            out_buf.len() as i32,
        )
    };

    if code == 0 {
        Ok(out_buf)
    } else {
        Err(code)
    }
}
