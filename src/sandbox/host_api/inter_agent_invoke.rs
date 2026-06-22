//! `invoke` — inter-agent RPC via the Tet mesh.
use super::helpers::{get_memory, read_guest_bytes, read_guest_str, write_response};
use super::TetState;
use crate::engine::TetError;
use crate::models::{ExecutionStatus, MeshCallRequest};
use std::time::Instant;
use wasmtime::Caller;

pub fn register(
    linker: &mut wasmtime::Linker<TetState>,
    source_alias: &str,
) -> Result<(), TetError> {
    let source_alias = source_alias.to_string();
    linker
        .func_wrap_async(
            "trytet",
            "invoke",
            move |mut caller: Caller<'_, TetState>,
                  (target_ptr, target_len, payload_ptr, payload_len, out_ptr, out_len_ptr, fuel): (
                i32, i32, i32, i32, i32, i32, i64,
            )|
                  -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                let source_alias = source_alias.clone();
                Box::new(async move {
                    let memory = get_memory(&mut caller)?;
                    let target_alias = read_guest_str(&memory, &caller, target_ptr, target_len)?;
                    let payload_bytes = read_guest_bytes(&memory, &caller, payload_ptr, payload_len)?;

                    let max_fuel = caller.get_fuel().unwrap_or(0);
                    let fuel_to_transfer = if (fuel as u64) > max_fuel { max_fuel } else { fuel as u64 };

                    let call_req = MeshCallRequest {
                        target_alias: target_alias.clone(),
                        method: "invoke".to_string(),
                        payload: payload_bytes,
                        fuel_to_transfer,
                        current_depth: caller.data().call_stack_depth,
                        target_function: None,
                    };

                    let req_bytes = call_req.payload.len() as u64;
                    let start_ns = Instant::now();
                    let mesh = caller.data().mesh.clone();
                    let response = mesh.send_call(call_req).await;
                    let elapsed_us = start_ns.elapsed().as_micros() as u64;

                    let res_bytes = response.as_ref().map(|r| r.return_data.len() as u64).unwrap_or(0);
                    let is_error = response.is_err();
                    mesh.record_telemetry(source_alias, target_alias, req_bytes + res_bytes, elapsed_us, is_error).await;

                    match response {
                        Ok(res) => {
                            caller.data_mut().fuel_to_burn_from_parent += res.fuel_used;
                            let memory = get_memory(&mut caller)?;
                            let code = write_response(&memory, &mut caller, out_ptr, out_len_ptr, &res.return_data)?;
                            let success = if res.status == ExecutionStatus::Success { 0 } else { 3 };
                            burn_fuel(&mut caller);
                            Ok(if code == 0 { success } else { code })
                        }
                        Err(_) => Ok(4),
                    }
                })
            },
        )
        .map_err(|e| TetError::EngineError(format!("Failed to register trytet::invoke: {e:#}")))?;
    Ok(())
}

fn burn_fuel(caller: &mut Caller<'_, TetState>) {
    let to_burn = caller.data_mut().fuel_to_burn_from_parent;
    caller.data_mut().fuel_to_burn_from_parent = 0;
    if let Ok(current) = caller.get_fuel() {
        let _ = caller.set_fuel(current.saturating_sub(to_burn));
    }
}
