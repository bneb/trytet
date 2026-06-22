//! `remember` and `recall` — vector memory operations.
use super::helpers::{get_memory, read_guest_bytes, write_response};
use super::TetState;
use crate::engine::TetError;
use wasmtime::Caller;

pub fn register_remember(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap_async(
            "trytet",
            "remember",
            |mut caller: Caller<'_, TetState>,
             (collection_ptr, collection_len, record_ptr, record_len): (i32, i32, i32, i32)|
             -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let memory = get_memory(&mut caller)?;
                    let collection_bytes = read_guest_bytes(&memory, &caller, collection_ptr, collection_len)?;
                    let record_bytes = read_guest_bytes(&memory, &caller, record_ptr, record_len)?;

                    let collection_name = String::from_utf8(collection_bytes).map_err(|_| wasmtime::Error::msg("Invalid UTF-8"))?;
                    let record: crate::memory::VectorRecord =
                        serde_json::from_slice(&record_bytes).map_err(|_| wasmtime::Error::msg("Invalid record JSON"))?;

                    let dim = record.vector.len() as u64;
                    let fuel_cost = 500 + (dim * 5);
                    deduct_fuel(&mut caller, fuel_cost)?;

                    caller.data().vector_vfs.remember(&collection_name, record);
                    Ok(0)
                })
            },
        )
        .map_err(|e| TetError::EngineError(format!("Failed to register trytet::remember: {e:#}")))?;
    Ok(())
}

pub fn register_recall(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap_async(
            "trytet",
            "recall",
            |mut caller: Caller<'_, TetState>,
             (query_ptr, query_len, out_ptr, out_len_ptr): (i32, i32, i32, i32)|
             -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let memory = get_memory(&mut caller)?;
                    let query_bytes = read_guest_bytes(&memory, &caller, query_ptr, query_len)?;
                    let query: crate::memory::SearchQuery =
                        serde_json::from_slice(&query_bytes).map_err(|_| wasmtime::Error::msg("Invalid query JSON"))?;

                    let dim = query.query_vector.len() as u64;
                    let search_cost = 100 + (dim * 2);
                    deduct_fuel(&mut caller, search_cost)?;

                    let results = caller.data().vector_vfs.recall(&query);
                    let response_json = serde_json::to_vec(&results)
                        .map_err(|_| wasmtime::Error::msg("Serialization error"))?;

                    let memory = get_memory(&mut caller)?;
                    write_response(&memory, &mut caller, out_ptr, out_len_ptr, &response_json)
                })
            },
        )
        .map_err(|e| TetError::EngineError(format!("Failed to register trytet::recall: {e:#}")))?;
    Ok(())
}

fn deduct_fuel(caller: &mut Caller<'_, TetState>, amount: u64) -> wasmtime::Result<()> {
    let current = caller.get_fuel().unwrap_or(0);
    if current < amount {
        let _ = caller.set_fuel(0);
        return Ok(());
    }
    let _ = caller.set_fuel(current - amount);
    Ok(())
}
