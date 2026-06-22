//! `invoke_component` — neuro-symbolic cartridge invocation.
//!
//! Return codes: 0=success, 1=fuel_exhausted, 2=buffer_too_small,
//! 3=compilation_failed, 4=interface_mismatch, 5=execution_error, 6=registry_error.
use super::helpers::{get_memory, read_guest_bytes, write_response};
use super::TetState;
use crate::cartridge::CartridgeError;
use crate::engine::TetError;
use wasmtime::Caller;

pub fn register(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap_async(
            "trytet",
            "invoke_component",
            |mut caller: Caller<'_, TetState>,
             (cid_ptr, cid_len, payload_ptr, payload_len, fuel, out_ptr, out_len_ptr): (
                i32, i32, i32, i32, i64, i32, i32,
            )|
             -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let memory = get_memory(&mut caller)?;
                    let component_id = String::from_utf8_lossy(
                        &read_guest_bytes(&memory, &caller, cid_ptr, cid_len)?,
                    )
                    .to_string();
                    let payload = String::from_utf8_lossy(
                        &read_guest_bytes(&memory, &caller, payload_ptr, payload_len)?,
                    )
                    .to_string();

                    let fuel_to_give = fuel as u64;
                    let max_fuel = caller.get_fuel().unwrap_or(0);
                    if fuel_to_give > max_fuel {
                        return Ok(1);
                    }
                    let _ = caller.set_fuel(max_fuel - fuel_to_give);

                    let cartridge_mgr = caller.data().cartridge_manager.clone();
                    let result = cartridge_mgr.invoke(&component_id, &payload, fuel_to_give, 512);

                    match result {
                        Ok((output, metrics)) => {
                            refund_fuel(&mut caller, fuel_to_give, metrics.fuel_consumed);
                            let memory = get_memory(&mut caller)?;
                            write_response(&memory, &mut caller, out_ptr, out_len_ptr, output.as_bytes())
                        }
                        Err(CartridgeError::FuelExhausted | CartridgeError::MemoryExceeded) => Ok(1),
                        Err(CartridgeError::CompilationFailed(_)) => {
                            refund_fuel(&mut caller, fuel_to_give, 0);
                            Ok(3)
                        }
                        Err(CartridgeError::InterfaceMismatch(_)) => {
                            refund_fuel(&mut caller, fuel_to_give, 0);
                            Ok(4)
                        }
                        Err(CartridgeError::ExecutionError(_, fuel_consumed)) => {
                            refund_fuel(&mut caller, fuel_to_give, fuel_consumed);
                            Ok(5)
                        }
                        Err(CartridgeError::RegistryError(_)) => {
                            refund_fuel(&mut caller, fuel_to_give, 0);
                            Ok(6)
                        }
                    }
                })
            },
        )
        .map_err(|e| TetError::EngineError(format!("Failed to register trytet::invoke_component: {e:#}")))?;
    Ok(())
}

fn refund_fuel(caller: &mut Caller<'_, TetState>, given: u64, consumed: u64) {
    let refund = given.saturating_sub(consumed);
    if refund > 0 {
        if let Ok(current) = caller.get_fuel() {
            let _ = caller.set_fuel(current + refund);
        }
    }
}
