//! `request_migration` — triggers agent migration to a target node.
use super::helpers::read_guest_str;
use super::TetState;
use crate::engine::TetError;
use wasmtime::Caller;

pub fn register(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap_async(
            "trytet",
            "request_migration",
            |mut caller: Caller<'_, TetState>, (target_ptr, target_len): (i32, i32)| -> Box<
                dyn std::future::Future<Output = wasmtime::Result<()>> + Send + '_,
            > {
                Box::new(async move {
                    let memory = super::helpers::get_memory(&mut caller)?;
                    let target_node = read_guest_str(&memory, &caller, target_ptr, target_len)?;
                    caller.data_mut().migration_requested = true;
                    caller.data_mut().migration_target = Some(target_node);
                    Err(wasmtime::Error::msg("MIGRATION_REQUESTED"))
                })
            },
        )
        .map_err(|e| TetError::EngineError(format!("Linking request_migration failed: {e:#}")))?;
    Ok(())
}
