//! `seek_equilibrium` — autonomous pricing arbitration via market.
use super::TetState;
use crate::engine::TetError;
use wasmtime::Caller;

pub fn register(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap_async(
            "trytet",
            "seek_equilibrium",
            |mut caller: Caller<'_, TetState>, (): ()| -> Box<
                dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_,
            > {
                Box::new(async move {
                    let market = caller.data().market_handle.clone();
                    let current_node = caller.data().tet_id.clone();
                    if let Some(best_bid) = market.find_best_arbitrage(&current_node) {
                        caller.data_mut().migration_requested = true;
                        caller.data_mut().migration_target = Some(best_bid.node_id);
                        return Ok(1);
                    }
                    Ok(0)
                })
            },
        )
        .map_err(|e| TetError::EngineError(format!("Linking seek_equilibrium failed: {e:#}")))?;
    Ok(())
}
