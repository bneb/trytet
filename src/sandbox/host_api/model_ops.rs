//! `model_load` and `model_predict` — inference model lifecycle.
use super::helpers::{get_memory, read_guest_bytes, read_guest_str, write_response};
use super::TetState;
use crate::engine::TetError;
use wasmtime::Caller;

pub fn register_model_load(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap_async(
            "trytet",
            "model_load",
            |mut caller: Caller<'_, TetState>,
             (alias_ptr, alias_len, path_ptr, path_len): (i32, i32, i32, i32)|
             -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let memory = get_memory(&mut caller)?;
                    let alias = read_guest_str(&memory, &caller, alias_ptr, alias_len)?;
                    let path = read_guest_str(&memory, &caller, path_ptr, path_len)?;

                    let load_cost = crate::inference::InferenceFuelCalculator::model_load_cost();
                    deduct_fuel(&mut caller, load_cost)?;

                    let engine = caller.data().inference_engine.clone();
                    match engine.load_model(&alias, &path).await {
                        Ok(_) => Ok(0),
                        Err(_) => Ok(3),
                    }
                })
            },
        )
        .map_err(|e| TetError::EngineError(format!("Failed to register trytet::model_load: {e:#}")))?;
    Ok(())
}

pub fn register_model_predict(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap_async(
            "trytet",
            "model_predict",
            |mut caller: Caller<'_, TetState>,
             (request_ptr, request_len, out_ptr, out_len_ptr): (i32, i32, i32, i32)|
             -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let memory = get_memory(&mut caller)?;
                    let request_bytes = read_guest_bytes(&memory, &caller, request_ptr, request_len)?;
                    let request: crate::inference::InferenceRequest =
                        match serde_json::from_slice(&request_bytes) {
                            Ok(r) => r,
                            Err(_) => return Ok(3),
                        };

                    let model_proxy = caller.data().model_proxy.clone();
                    let context_limit = model_proxy.provider.context_limit(&request.model_alias);
                    let estimated_tokens = std::cmp::max(1, request.prompt.len().div_ceil(4));
                    let t_total = (estimated_tokens as f64 * 1.15).ceil() as usize;

                    if t_total > context_limit {
                        return Ok(7);
                    }

                    let proxy_req = crate::model_proxy::InferenceProxyRequest {
                        prompt: request.prompt.clone(),
                        model_id: request.model_alias.clone(),
                        temperature: request.temperature,
                        max_tokens: request.max_tokens,
                    };

                    let cache_dir = caller.data().oracle_cache_dir.clone();
                    let telemetry = caller.data().telemetry.clone();
                    telemetry.broadcast(crate::telemetry::HiveEvent::InferenceStarted {
                        tet_id: "guest".to_string(),
                        model_id: request.model_alias.clone(),
                        prompt_tokens_est: estimated_tokens as u32,
                        timestamp_us: crate::telemetry::now_us(),
                    });

                    match model_proxy.predict(proxy_req, &cache_dir).await {
                        Ok(proxy_resp) => {
                            let fuel_cost = crate::model_proxy::ModelProxy::calculate_fuel(
                                proxy_resp.input_tokens,
                                proxy_resp.output_tokens,
                            );
                            deduct_fuel(&mut caller, fuel_cost)?;

                            match serde_json::to_vec(&proxy_resp) {
                                Ok(json) => {
                                    let memory = get_memory(&mut caller)?;
                                    let code = write_response(&memory, &mut caller, out_ptr, out_len_ptr, &json)?;
                                    telemetry.broadcast(crate::telemetry::HiveEvent::InferenceCompleted {
                                        tet_id: "guest".to_string(),
                                        model_id: request.model_alias,
                                        input_tokens: proxy_resp.input_tokens,
                                        output_tokens: proxy_resp.output_tokens,
                                        fuel_cost,
                                        cached: proxy_resp.cached,
                                        timestamp_us: crate::telemetry::now_us(),
                                    });
                                    Ok(code)
                                }
                                Err(_) => Ok(4),
                            }
                        }
                        Err(_) => Ok(4),
                    }
                })
            },
        )
        .map_err(|e| TetError::EngineError(format!("Failed to register trytet::model_predict: {e:#}")))?;
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
