//! Host function registration for the Trytet Wasm sandbox.
//!
//! Each module registers a single host function. The top-level
//! `register_all` wires everything into the linker.

mod cartridge_invoke;
mod economy_bill;
mod economy_pay;
mod economy_withdraw;
mod equilibrium;
mod fetch;
mod fork;
mod gateway_listen;
mod helpers;
mod inter_agent_invoke;
mod memory_ops;
mod migration;
mod model_ops;
mod predict;
mod reclaim;
mod suspend;

use crate::engine::TetError;
use crate::sandbox::sandbox_wasmtime::TetState;

/// Public entry point — register every Trytet host function on the given linker.
/// This is the function called by `sandbox_wasmtime.rs`.
pub fn register_host_functions(
    linker: &mut wasmtime::Linker<TetState>,
    source_alias: String,
) -> Result<(), TetError> {
    register_all(linker, source_alias)
}

/// Register every Trytet host function on the given linker.
fn register_all(
    linker: &mut wasmtime::Linker<TetState>,
    source_alias: String,
) -> Result<(), TetError> {
    migration::register(linker)?;
    equilibrium::register(linker)?;
    gateway_listen::register(linker)?;
    inter_agent_invoke::register(linker, &source_alias)?;
    fetch::register(linker)?;
    predict::register(linker)?;
    memory_ops::register_remember(linker)?;
    memory_ops::register_recall(linker)?;
    model_ops::register_model_load(linker)?;
    model_ops::register_model_predict(linker)?;
    fork::register(linker)?;
    suspend::register(linker)?;
    economy_pay::register(linker)?;
    economy_bill::register(linker)?;
    economy_withdraw::register(linker)?;
    reclaim::register(linker)?;
    cartridge_invoke::register(linker)?;
    Ok(())
}
