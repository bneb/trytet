//! `reclaim` — genesis factory reclaims a child agent.
use super::helpers::{get_memory, read_guest_str};
use super::TetState;
use crate::engine::TetError;
use wasmtime::Caller;

pub fn register(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap_async(
            "trytet",
            "reclaim",
            |mut caller: Caller<'_, TetState>, (child_ptr, child_len): (i32, i32)| -> Box<
                dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_,
            > {
                Box::new(async move {
                    if !caller.data().manifest.permissions.is_genesis_factory {
                        return Ok(7);
                    }
                    let memory = get_memory(&mut caller)?;
                    let child_id = read_guest_str(&memory, &caller, child_ptr, child_len)?;
                    let mesh = caller.data().mesh.clone();
                    if mesh.send_reclaim(child_id).await.is_err() {
                        return Ok(6);
                    }
                    Ok(0)
                })
            },
        )
        .map_err(|e| TetError::EngineError(format!("Failed to register trytet::reclaim: {e:#}")))?;
    Ok(())
}
