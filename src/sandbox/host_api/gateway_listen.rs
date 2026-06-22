//! `listen` — register a gateway route for HTTP ingress.
use super::helpers::{get_memory, read_guest_str};
use super::TetState;
use crate::engine::TetError;
use wasmtime::Caller;

pub fn register(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap(
            "trytet",
            "listen",
            |mut caller: Caller<'_, TetState>,
             path_ptr: i32,
             path_len: i32,
             handler_ptr: i32,
             handler_len: i32|
             -> wasmtime::Result<i32> {
                let memory = get_memory(&mut caller)?;
                let path = read_guest_str(&memory, &caller, path_ptr, path_len)?;
                let handler = read_guest_str(&memory, &caller, handler_ptr, handler_len)?;
                let alias = caller.data().manifest.metadata.name.clone();
                caller.data().gateway.register_route(alias, path, handler);
                Ok(0)
            },
        )
        .map_err(|e| TetError::EngineError(format!("Linking listen failed: {e:#}")))?;
    Ok(())
}
