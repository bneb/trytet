//! `predict` — watchdog-guarded inference placeholder.
use super::TetState;
use crate::engine::TetError;
use wasmtime::Caller;

pub fn register(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap_async(
            "trytet",
            "predict",
            |mut _caller: Caller<'_, TetState>, (_prompt_ptr, _prompt_len): (i32, i32)| -> Box<
                dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_,
            > {
                Box::new(async move {
                    let watchdog =
                        crate::sandbox::security::Watchdog::new(std::time::Duration::from_millis(50));
                    for _ in 0..10 {
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        watchdog.check().map_err(|e| wasmtime::Error::msg(e.to_string()))?;
                    }
                    Ok(0)
                })
            },
        )
        .map_err(|e| TetError::EngineError(format!("Failed to register trytet::predict: {e:#}")))?;
    Ok(())
}
