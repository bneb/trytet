//! `suspend` — halt execution for later resurrection.
use super::TetState;
use crate::engine::TetError;
use wasmtime::Caller;

pub fn register(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap(
            "trytet",
            "suspend",
            |_caller: Caller<'_, TetState>| -> wasmtime::Result<()> {
                Err(wasmtime::Error::msg("TET_SUSPEND"))
            },
        )
        .map_err(|e| TetError::EngineError(format!("Failed to register trytet::suspend: {e:#}")))?;
    Ok(())
}
