//! `bill` — request payment from another agent.
use super::helpers::{get_memory, read_guest_str};
use super::TetState;
use crate::engine::TetError;
use wasmtime::Caller;

pub fn register(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap_async(
            "trytet",
            "bill",
            |mut caller: Caller<'_, TetState>, (source_ptr, source_len, amount): (i32, i32, i64)| -> Box<
                dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_,
            > {
                Box::new(async move {
                    let amount = amount as u64;
                    let memory = get_memory(&mut caller)?;
                    let source_alias = read_guest_str(&memory, &caller, source_ptr, source_len)?;
                    let target_alias = caller.data().manifest.metadata.name.clone();

                    let pkt = crate::hive::HiveCommand::Economy(
                        crate::hive::HiveEconomyCommand::BillRequest {
                            source_alias,
                            target_alias,
                            amount,
                        },
                    );
                    let _ = caller.data().mesh.send_economy_packet(pkt).await;
                    Ok(0)
                })
            },
        )
        .map_err(|e| TetError::EngineError(format!("Failed to register trytet::bill: {e:#}")))?;
    Ok(())
}
