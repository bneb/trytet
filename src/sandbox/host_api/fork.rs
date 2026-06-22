//! `fork` — clone the current agent's state into a new child.
use super::helpers::{get_memory, read_guest_bytes};
use super::TetState;
use crate::engine::TetError;
use wasmtime::Caller;

pub fn register(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap_async(
            "trytet",
            "fork",
            |mut caller: Caller<'_, TetState>, (fuel_to_give, node_ptr, node_len): (i64, i32, i32)| -> Box<
                dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_,
            > {
                Box::new(async move {
                    let fuel_to_give = fuel_to_give as u64;
                    let memory = get_memory(&mut caller)?;
                    let _target_node = if node_len > 0 {
                        Some(String::from_utf8_lossy(
                            &read_guest_bytes(&memory, &caller, node_ptr, node_len)?,
                        )
                        .to_string())
                    } else {
                        None
                    };

                    let snapshot_bytes = memory.data(&caller).to_vec();
                    let manifest = caller.data().manifest.clone();
                    let alias_name = manifest.metadata.name.clone();
                    let max_memory_mb = manifest.constraints.max_memory_pages * 64 / 1024;
                    let egress_policy = caller.data().egress_policy.clone();

                    let max_fuel = caller.get_fuel().unwrap_or(0);
                    if fuel_to_give > max_fuel {
                        return Ok(5);
                    }
                    let _ = caller.set_fuel(max_fuel - fuel_to_give);

                    let req = crate::models::TetExecutionRequest {
                        payload: Some(snapshot_bytes),
                        alias: Some(alias_name),
                        allocated_fuel: fuel_to_give,
                        max_memory_mb,
                        env: std::collections::HashMap::new(),
                        injected_files: std::collections::HashMap::new(),
                        parent_snapshot_id: None,
                        call_depth: 0,
                        voucher: None,
                        manifest: Some(manifest),
                        egress_policy,
                        target_function: None,
                    };

                    let _ = caller.data().mesh.send_fork(req).await;
                    Ok(0)
                })
            },
        )
        .map_err(|e| TetError::EngineError(format!("Failed to register trytet::fork: {e:#}")))?;
    Ok(())
}
