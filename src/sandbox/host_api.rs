use wasmtime::Caller;
use crate::sandbox::sandbox_wasmtime::{TetState, validate_range, validate_range_mut};
use crate::engine::TetError;
use std::time::Instant;
use crate::models::{MeshCallRequest, ExecutionStatus};

pub fn register_host_functions(
    linker: &mut wasmtime::Linker<TetState>,
    source_alias: String
) -> Result<(), TetError> {
        // Phase 3: Custom Inter-Tet RPC Host Function
        linker.func_wrap_async(
            "trytet",
            "request_migration",
            |mut caller: Caller<'_, TetState>, (target_ptr, target_len): (i32, i32)| -> Box<dyn std::future::Future<Output = wasmtime::Result<()>> + Send + '_> {
                Box::new(async move {
                    let memory = match caller.get_export("memory") {
                        Some(wasmtime::Extern::Memory(m)) => m,
                        _ => return Err(wasmtime::Error::msg("No memory exported")),
                    };

                    let mem_slice = validate_range(&memory, &caller, target_ptr, target_len)?;
                    let target_node = String::from_utf8_lossy(mem_slice).to_string();

                    caller.data_mut().migration_requested = true;
                    caller.data_mut().migration_target = Some(target_node);
                    let res: wasmtime::Result<()> = Err(wasmtime::Error::msg("MIGRATION_REQUESTED"));
                    res
                })
            }
        ).map_err(|e| TetError::EngineError(format!("Linking request_migration failed: {e:#}")))?;

        // Phase 25.1: Autonomous Pricing Arbitration
        linker.func_wrap_async(
            "trytet",
            "seek_equilibrium",
            |mut caller: Caller<'_, TetState>, (): ()| -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
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
            }
        ).map_err(|e| TetError::EngineError(format!("Linking seek_equilibrium failed: {e:#}")))?;

        // Phase 18.1: Sovereign Gateway Listener
        linker
            .func_wrap(
                "trytet",
                "listen",
                |mut caller: Caller<'_, TetState>,
                 path_ptr: i32,
                 path_len: i32,
                 handler_ptr: i32,
                 handler_len: i32|
                 -> wasmtime::Result<i32> {
                    let memory = match caller.get_export("memory") {
                        Some(wasmtime::Extern::Memory(m)) => m,
                        _ => return Err(wasmtime::Error::msg("No memory exported")),
                    };

                    let path_slice = validate_range(&memory, &caller, path_ptr, path_len)?;
                    let path = String::from_utf8_lossy(path_slice).to_string();

                    let handler_slice = validate_range(&memory, &caller, handler_ptr, handler_len)?;
                    let handler = String::from_utf8_lossy(handler_slice).to_string();

                    let alias = caller.data().manifest.metadata.name.clone();
                    caller.data().gateway.register_route(alias, path, handler);

                    Ok(0)
                },
            )
            .map_err(|e| TetError::EngineError(format!("Linking listen failed: {e:#}")))?;

        linker
            .func_wrap_async(
                "trytet",
                "invoke",
                move |mut caller: Caller<'_, TetState>,
                      (
                    target_ptr,
                    target_len,
                    payload_ptr,
                    payload_len,
                    out_ptr,
                    out_len_ptr,
                    fuel,
                ): (i32, i32, i32, i32, i32, i32, i64)|
                      -> Box<
                    dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_,
                > {
                    let source_alias = source_alias.clone();
                    Box::new(async move {
                        // 1. Read pointers from Linear Memory
                        let memory = match caller.get_export("memory") {
                            Some(wasmtime::Extern::Memory(m)) => m,
                            _ => return Err(wasmtime::Error::msg("Memory Error")),
                        };

                        let target_alias =
                            validate_range(&memory, &caller, target_ptr, target_len)?;
                        let target_alias = String::from_utf8_lossy(target_alias).to_string();
                        let payload_bytes =
                            validate_range(&memory, &caller, payload_ptr, payload_len)?.to_vec();

                        let mesh = caller.data().mesh.clone();
                        let max_fuel = caller.get_fuel().unwrap_or(0);
                        let fuel_to_transfer = if (fuel as u64) > max_fuel {
                            max_fuel
                        } else {
                            fuel as u64
                        };

                        let call_req = MeshCallRequest {
                            target_alias: target_alias.clone(),
                            method: "invoke".to_string(), // MVP simplified
                            payload: payload_bytes,
                            fuel_to_transfer,
                            current_depth: caller.data().call_stack_depth,
                            target_function: None,
                        };

                        // Phase 7: Topology Observability Hook (enter)
                        let req_bytes = call_req.payload.len() as u64;
                        let start_ns = Instant::now();

                        // 2. Await the RPC call (this yields correctly to Tokio!)
                        let response = mesh.send_call(call_req).await;

                        // Phase 7: Topology Observability Hook (exit)
                        let elapsed_us = start_ns.elapsed().as_micros() as u64;
                        let mut is_error = false;
                        let mut res_bytes = 0_u64;

                        // 3. Process Result
                        let mut success_code = 0_i32;

                        match &response {
                            Ok(res) => {
                                res_bytes = res.return_data.len() as u64;
                            }
                            Err(_) => {
                                is_error = true;
                            }
                        }

                        // Flush the native telemetry hook into the memory mesh
                        mesh.record_telemetry(
                            source_alias,
                            target_alias,
                            req_bytes + res_bytes,
                            elapsed_us,
                            is_error,
                        )
                        .await;

                        match response {
                            Ok(res) => {
                                // Deduct the fuel the child actually burned
                                caller.data_mut().fuel_to_burn_from_parent += res.fuel_used;

                                let response_len = res.return_data.len() as i32;

                                // Re-borrow memory because caller was mutated above
                                let memory =
                                    caller.get_export("memory").unwrap().into_memory().unwrap();

                                let len_slice = validate_range(&memory, &caller, out_len_ptr, 4)?;
                                let mut len_buf = [0u8; 4];
                                len_buf.copy_from_slice(len_slice);
                                let guest_buffer_size = i32::from_le_bytes(len_buf);

                                if response_len > guest_buffer_size {
                                    let required_size = response_len.to_le_bytes();
                                    if let Ok(m) =
                                        validate_range_mut(&memory, &mut caller, out_len_ptr, 4)
                                    {
                                        m.copy_from_slice(&required_size);
                                    }
                                    success_code = 2_i32;
                                } else {
                                    let m = validate_range_mut(
                                        &memory,
                                        &mut caller,
                                        out_ptr,
                                        response_len,
                                    )?;
                                    m.copy_from_slice(&res.return_data);
                                    let written_size = response_len.to_le_bytes();
                                    if let Ok(m) =
                                        validate_range_mut(&memory, &mut caller, out_len_ptr, 4)
                                    {
                                        m.copy_from_slice(&written_size);
                                    }

                                    if res.status != ExecutionStatus::Success {
                                        success_code = 3_i32; // Child crashed or ran out of fuel
                                    }
                                }
                            }
                            Err(_) => {
                                success_code = 4_i32; // Mesh unreachable/resolution failed
                            }
                        }

                        // Burn the child's fuel from parent immediately
                        let to_burn = caller.data_mut().fuel_to_burn_from_parent;
                        caller.data_mut().fuel_to_burn_from_parent = 0;
                        if let Ok(current_fuel) = caller.get_fuel() {
                            if current_fuel >= to_burn {
                                let _ = caller.set_fuel(current_fuel - to_burn);
                            } else {
                                let _ = caller.set_fuel(0); // Exhaust parent perfectly
                            }
                        }

                        Ok(success_code)
                    })
                },
            )
            .map_err(|e| {
                TetError::EngineError(format!("Failed to register trytet::invoke: {e:#}"))
            })?;

        linker.func_wrap_async(
            "trytet",
            "fetch",
            move |mut caller: Caller<'_, TetState>, (url_ptr, url_len, method_ptr, method_len, body_ptr, body_len, out_ptr, out_len_ptr): (i32, i32, i32, i32, i32, i32, i32, i32)| -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let memory = match caller.get_export("memory") {
                        Some(wasmtime::Extern::Memory(m)) => m,
                        _ => return Err(wasmtime::Error::msg("Memory Error")),
                    };

                    let target_url = String::from_utf8_lossy(validate_range(&memory, &caller, url_ptr, url_len)?).to_string();
                    let req_method_str = String::from_utf8_lossy(validate_range(&memory, &caller, method_ptr, method_len)?).to_string();
                    let req_body = validate_range(&memory, &caller, body_ptr, body_len)?.to_vec();

                    // Apply Vector 1: PathJailer security
                    if !target_url.starts_with("http") {
                        let jailer = crate::sandbox::security::PathJailer::new(std::path::PathBuf::from("/vfs/Agent_Workspace_Root"));
                        if let Err(e) = jailer.safe_join(&target_url) {
                            return Err(wasmtime::Error::msg(e.to_string()));
                        }
                    }

                    let policy = caller.data().egress_policy.clone();
                    if let Some(p) = policy {
                        if p.require_https && !target_url.starts_with("https://") {
                            return Err(wasmtime::Error::msg("Security Violation: HTTPS strictly required"));
                        }
                        if let Ok(parsed_url) = reqwest::Url::parse(&target_url) {
                            if let Some(host) = parsed_url.host_str() {
                                if !p.allowed_domains.contains(&host.to_string()) {
                                    return Err(wasmtime::Error::msg(format!("Security Violation: Domain '{}' not in EgressAllowList", host)));
                                }
                            } else {
                                return Err(wasmtime::Error::msg("Security Violation: Target URL has no identifiable hostname"));
                            }
                        } else {
                            return Err(wasmtime::Error::msg("Security Violation: Unparseable URI"));
                        }
                    } else {
                        return Err(wasmtime::Error::msg("Security Violation: No EgressPolicy assigned to this Sandbox Execution"));
                    }

                    // Phase 15.1: Deterministic Abstract Metering (Pre-flight cost)
                    let c_base = 50_000_u64;
                    let c_unit = 10_u64;
                    let req_size = target_url.len() as u64 + req_method_str.len() as u64 + req_body.len() as u64;
                    let req_fuel = c_base + (req_size / 1024) * c_unit;

                    if let Ok(current_fuel) = caller.get_fuel() {
                        if current_fuel >= req_fuel {
                            let _ = caller.set_fuel(current_fuel - req_fuel);
                        } else {
                            let _ = caller.set_fuel(0);
                            return Ok(6); // out of fuel
                        }
                    }

                    let oracle_req = crate::oracle::OracleRequest {
                        url: target_url.clone(),
                        method: req_method_str.clone(),
                        body: req_body.clone(),
                    };

                    let oracle = caller.data().oracle.clone();
                    let cache_dir = caller.data().oracle_cache_dir.clone();

                    // Phase 17.1: Pre-flight Egress Quota Check
                    let quota_mgr = caller.data().quota_manager.clone();
                    let tenant_id = caller.data().tenant_id.clone();
                    let max_egress = caller.data().max_egress_bytes;
                    let header_overhead = crate::fortress::SovereignHeaders::header_overhead(
                        &caller.data().tet_id,
                        &caller.data().author_pubkey,
                    );
                    let pre_flight_bytes = req_size + header_overhead;

                    if quota_mgr.check_and_record(&tenant_id, pre_flight_bytes, max_egress).is_err() {
                        return Ok(8); // EgressQuotaExceeded
                    }

                    // Phase 17.1: Construct Sovereign Identity Headers
                    let sovereign_headers = crate::fortress::SovereignHeaders::inject(
                        &caller.data().tet_id,
                        &caller.data().author_pubkey,
                        &oracle.wallet,
                        &req_method_str,
                        &target_url,
                        &req_body,
                    );

                    let (status_code, returned_bytes) = match oracle.resolve_with_headers(oracle_req, &cache_dir, sovereign_headers).await {
                        Ok((s, b)) => (s, b),
                        Err(_) => (500, vec![]),
                    };

                    // Phase 17.1: Post-flight Egress Quota (response bytes)
                    let _ = quota_mgr.check_and_record(&tenant_id, returned_bytes.len() as u64, max_egress);

                    // Phase 15.1: Response Fuel cost (post-flight cost)
                    let res_fuel = (returned_bytes.len() as u64 / 1024) * c_unit;
                    if let Ok(current_fuel) = caller.get_fuel() {
                        if current_fuel >= res_fuel {
                            let _ = caller.set_fuel(current_fuel - res_fuel);
                        } else {
                            let _ = caller.set_fuel(0);
                            return Ok(6); // out of fuel
                        }
                    }

                    let success_code = if (200..400).contains(&status_code) { 0_i32 } else { 6_i32 };

                    if success_code == 0 {
                        let response_len = returned_bytes.len() as i32;
                        let len_slice = validate_range(&memory, &caller, out_len_ptr, 4)?;
                        let mut len_buf = [0u8; 4];
                        len_buf.copy_from_slice(len_slice);
                        let guest_buffer_size = i32::from_le_bytes(len_buf);

                        if response_len > guest_buffer_size {
                            let required_size = response_len.to_le_bytes();
                            if let Ok(m) = validate_range_mut(&memory, &mut caller, out_len_ptr, 4) {
                                m.copy_from_slice(&required_size);
                            }
                            return Ok(2_i32);
                        } else {
                            let m = validate_range_mut(&memory, &mut caller, out_ptr, response_len)?;
                            m.copy_from_slice(&returned_bytes);
                            let written_size = response_len.to_le_bytes();
                            if let Ok(m) = validate_range_mut(&memory, &mut caller, out_len_ptr, 4) {
                                m.copy_from_slice(&written_size);
                            }
                        }
                    }

                    Ok(success_code)
                })
            }
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::fetch: {e:#}")))?;

        linker.func_wrap_async(
            "trytet",
            "predict",
            |mut _caller: wasmtime::Caller<'_, TetState>, (_prompt_ptr, _prompt_len): (i32, i32)| -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let watchdog = crate::sandbox::security::Watchdog::new(std::time::Duration::from_millis(50));
                    
                    let iterations = 10;
                    for _ in 0..iterations {
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        
                        // Break sandbox constraint dynamically upon violation
                        if let Err(e) = watchdog.check() {
                            return Err(wasmtime::Error::msg(e.to_string()));
                        }
                    }

                    Ok(0)
                })
            }
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::predict: {e:#}")))?;

        // Phase 9: The Sovereign Memory
        linker.func_wrap_async(
            "trytet",
            "remember",
            |mut caller: Caller<'_, TetState>,
             (collection_ptr, collection_len, record_ptr, record_len): (i32, i32, i32, i32)|
             -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return Err(wasmtime::Error::msg("Memory error")),
                    };

                    let cb = validate_range(&memory, &caller, collection_ptr, collection_len)?.to_vec();
                    let rb = validate_range(&memory, &caller, record_ptr, record_len)?.to_vec();

                    if true {
                        if let Ok(collection_name) = String::from_utf8(cb) {
                            if let Ok(record) = serde_json::from_slice::<crate::memory::VectorRecord>(&rb) {

                                // Metric Fuel Adjusted Indexing Cost
                                let dim = record.vector.len() as u64;
                                let base_cost = 500;
                                let multiplier = 5;
                                let fuel_cost = base_cost + (dim * multiplier);

                                if let Ok(current_fuel) = caller.get_fuel() {
                                    if current_fuel >= fuel_cost {
                                        let _ = caller.set_fuel(current_fuel - fuel_cost);
                                    } else {
                                        let _ = caller.set_fuel(0);
                                        return Ok(5);
                                    }
                                }

                                let vfs = caller.data().vector_vfs.clone();
                                vfs.remember(&collection_name, record);
                                return Ok(0);
                            }
                        }
                    }
                    Ok(2)
                })
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::remember: {e:#}")))?;

        linker.func_wrap_async(
            "trytet",
            "recall",
            |mut caller: Caller<'_, TetState>,
             (query_ptr, query_len, out_ptr, out_len_ptr): (i32, i32, i32, i32)|
             -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return Err(wasmtime::Error::msg("Memory Error")),
                    };

                    let qb = validate_range(&memory, &caller, query_ptr, query_len)?.to_vec();

                    if true {
                        if let Ok(query) = serde_json::from_slice::<crate::memory::SearchQuery>(&qb) {

                            let dim = query.query_vector.len() as u64;
                            let search_cost = 100 + (dim * 2);

                            if let Ok(current_fuel) = caller.get_fuel() {
                                if current_fuel >= search_cost {
                                    let _ = caller.set_fuel(current_fuel - search_cost);
                                } else {
                                    let _ = caller.set_fuel(0);
                                    return Ok(5);
                                }
                            }

                            let vfs = caller.data().vector_vfs.clone();
                            let results = vfs.recall(&query);

                            if let Ok(response_json) = serde_json::to_vec(&results) {
                                let response_len = response_json.len() as i32;
                                let len_slice = validate_range(&memory, &caller, out_len_ptr, 4)?;
                                let mut len_buf = [0u8; 4];
                                len_buf.copy_from_slice(len_slice);
                                let guest_buffer_size = i32::from_le_bytes(len_buf);

                                if response_len > guest_buffer_size {
                                    let required_size = response_len.to_le_bytes();
                                    if let Ok(m) = validate_range_mut(&memory, &mut caller, out_len_ptr, 4) {
                                        m.copy_from_slice(&required_size);
                                    }
                                    return Ok(2);
                                } else {
                                    let m = validate_range_mut(&memory, &mut caller, out_ptr, response_len)?;
                                    m.copy_from_slice(&response_json);

                                    let written_size = response_len.to_le_bytes();
                                    if let Ok(m) = validate_range_mut(&memory, &mut caller, out_len_ptr, 4) {
                                        m.copy_from_slice(&written_size);
                                    }
                                    return Ok(0);
                                }
                            }
                        }
                    }
                    Ok(3) // Bad input
                })
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::recall: {e:#}")))?;

        // Phase 10: The Sovereign Inference — model_load
        linker.func_wrap_async(
            "trytet",
            "model_load",
            |mut caller: Caller<'_, TetState>,
             (alias_ptr, alias_len, path_ptr, path_len): (i32, i32, i32, i32)|
             -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return Err(wasmtime::Error::msg("Memory Error")),
                    };

                    let ab = validate_range(&memory, &caller, alias_ptr, alias_len)?.to_vec();
                    let pb = validate_range(&memory, &caller, path_ptr, path_len)?.to_vec();

                    if true {
                        if let (Ok(alias), Ok(path)) = (String::from_utf8(ab), String::from_utf8(pb)) {
                            // Deduct model load fuel cost
                            let load_cost = crate::inference::InferenceFuelCalculator::model_load_cost();
                            if let Ok(current_fuel) = caller.get_fuel() {
                                if current_fuel >= load_cost {
                                    let _ = caller.set_fuel(current_fuel - load_cost);
                                } else {
                                    let _ = caller.set_fuel(0);
                                    return Ok(5); // Out of fuel
                                }
                            }

                            let engine = caller.data().inference_engine.clone();
                            match engine.load_model(&alias, &path).await {
                                Ok(_) => return Ok(0), // Success
                                Err(_) => return Ok(3), // Load failed
                            }
                        }
                    }
                    Ok(2)
                })
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::model_load: {e:#}")))?;

        // Phase 15.2: The Sovereign Inference — model_predict (Oracle-Mediated)
        linker.func_wrap_async(
            "trytet",
            "model_predict",
            |mut caller: Caller<'_, TetState>,
             (request_ptr, request_len, out_ptr, out_len_ptr): (i32, i32, i32, i32)|
             -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return Err(wasmtime::Error::msg("Memory Error")),
                    };

                    let rb = validate_range(&memory, &caller, request_ptr, request_len)?.to_vec();

                    let request = match serde_json::from_slice::<crate::inference::InferenceRequest>(&rb) {
                        Ok(r) => r,
                        Err(_) => return Ok(3), // Bad input
                    };

                    // Phase 15.2: Context Overflow Check via ContextRouter
                    let model_proxy = caller.data().model_proxy.clone();
                    let context_limit = model_proxy.provider.context_limit(&request.model_alias);

                    // Estimate prompt tokens using 1.15x safety factor
                    let estimated_prompt_tokens = std::cmp::max(1, request.prompt.len().div_ceil(4));
                    let t_total = (estimated_prompt_tokens as f64 * 1.15).ceil() as usize;

                    if t_total > context_limit {
                        // Context overflow: return error code 7 to guest
                        return Ok(7);
                    }

                    // Phase 15.2: Build InferenceProxyRequest for Oracle-mediated flow
                    let proxy_req = crate::model_proxy::InferenceProxyRequest {
                        prompt: request.prompt.clone(),
                        model_id: request.model_alias.clone(),
                        temperature: request.temperature,
                        max_tokens: request.max_tokens,
                    };

                    let cache_dir = caller.data().oracle_cache_dir.clone();
                    let telemetry = caller.data().telemetry.clone();

                    // Phase 16.1: Emit InferenceStarted
                    telemetry.broadcast(crate::telemetry::HiveEvent::InferenceStarted {
                        tet_id: "guest".to_string(),
                        model_id: request.model_alias.clone(),
                        prompt_tokens_est: estimated_prompt_tokens as u32,
                        timestamp_us: crate::telemetry::now_us(),
                    });

                    // Phase 15.2: Resolve through ModelProxy (Oracle cache → Provider → Sign)
                    match model_proxy.predict(proxy_req, &cache_dir).await {
                        Ok(proxy_resp) => {
                            // Phase 15.2: Deterministic Token Billing
                            // Fuel = (InputTokens + OutputTokens) × C_TOKEN_WEIGHT + C_BASE_OVERHEAD
                            let fuel_cost = crate::model_proxy::ModelProxy::calculate_fuel(
                                proxy_resp.input_tokens,
                                proxy_resp.output_tokens,
                            );

                            if let Ok(current_fuel) = caller.get_fuel() {
                                if current_fuel >= fuel_cost {
                                    let _ = caller.set_fuel(current_fuel - fuel_cost);
                                } else {
                                    let _ = caller.set_fuel(0);
                                    return Ok(6); // Out of fuel
                                }
                            }

                            // Serialize the proxy response to guest memory
                            if let Ok(response_json) = serde_json::to_vec(&proxy_resp) {
                                let response_len = response_json.len() as i32;
                                let len_slice = validate_range(&memory, &caller, out_len_ptr, 4)?;

                                let mut len_buf = [0u8; 4];
                                len_buf.copy_from_slice(len_slice);
                                let guest_buffer_size = i32::from_le_bytes(len_buf);

                                if response_len > guest_buffer_size {
                                    let required_size = response_len.to_le_bytes();
                                    if let Ok(m) = validate_range_mut(&memory, &mut caller, out_len_ptr, 4) {
                                        m.copy_from_slice(&required_size);
                                    }
                                    return Ok(2); // Buffer too small
                                } else {
                                    let m = validate_range_mut(&memory, &mut caller, out_ptr, response_len)?;
                                    m.copy_from_slice(&response_json);

                                    let written_size = response_len.to_le_bytes();
                                    if let Ok(m) = validate_range_mut(&memory, &mut caller, out_len_ptr, 4) {
                                        m.copy_from_slice(&written_size);
                                    }
                                    return Ok(0); // Success
                                }
                            }

                            // Phase 16.1: Emit InferenceCompleted
                            telemetry.broadcast(crate::telemetry::HiveEvent::InferenceCompleted {
                                tet_id: "guest".to_string(),
                                model_id: request.model_alias.clone(),
                                input_tokens: proxy_resp.input_tokens,
                                output_tokens: proxy_resp.output_tokens,
                                fuel_cost,
                                cached: proxy_resp.cached,
                                timestamp_us: crate::telemetry::now_us(),
                            });

                            Ok(4) // Serialization failure
                        }
                        Err(_) => Ok(4), // Inference provider error
                    }
                })
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::model_predict: {e:#}")))?;

        linker.func_wrap_async(
            "trytet",
            "fork",
            |mut caller: Caller<'_, TetState>,
             (fuel_to_give, node_ptr, node_len): (i64, i32, i32)|
             -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let fuel_to_give = fuel_to_give as u64;
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return Err(wasmtime::Error::msg("Memory Error")),
                    };

                    let target_node = if node_len > 0 {
                        let rb = validate_range(&memory, &caller, node_ptr, node_len)?.to_vec();
                        Some(String::from_utf8_lossy(&rb).to_string())
                    } else {
                        None
                    };

                    let snapshot_bytes = memory.data(&caller).to_vec();

                    let manifest = caller.data().manifest.clone();
                    let alias_name = manifest.metadata.name.clone();
                    let max_memory_mb = manifest.constraints.max_memory_pages * 64 / 1024;
                    let egress_policy = caller.data().egress_policy.clone();
                    let mesh = caller.data().mesh.clone();

                    let max_fuel = caller.get_fuel().unwrap_or(0);
                    if fuel_to_give > max_fuel {
                        return Ok(5); // OUT OF FUEL
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

                    if let Some(_tn) = target_node {
                        // Normally we would route to the target node,
                        // but setting target node directly on req isn't natively supported yet.
                        // For MVP, we treat local requests similarly to networked ones via MeshWorker
                    }

                    let _ = mesh.send_fork(req).await;

                    Ok(0) // Return Child TetID success
                })
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::fork: {e:#}")))?;

        linker.func_wrap(
            "trytet",
            "suspend",
            |_caller: Caller<'_, TetState>| -> wasmtime::Result<()> {
                Err(wasmtime::Error::msg("TET_SUSPEND"))
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::suspend: {e:#}")))?;

        // Phase 22.1: The Autonomous Economy
        linker.func_wrap_async(
            "trytet",
            "pay",
            |mut caller: Caller<'_, TetState>,
             (target_ptr, target_len, amount): (i32, i32, i64)|
             -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let amount = amount as u64;
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return Err(wasmtime::Error::msg("Memory Error")),
                    };

                    let target_alias = {
                        let rb = validate_range(&memory, &caller, target_ptr, target_len)?.to_vec();
                        String::from_utf8_lossy(&rb).to_string()
                    };

                    let max_fuel = caller.get_fuel().unwrap_or(0);
                    if amount > max_fuel {
                        return Ok(5); // OUT OF FUEL Error Code
                    }
                    // Extract fuel locally before issuing transaction!
                    let _ = caller.set_fuel(max_fuel - amount);

                    let manifest = caller.data().manifest.clone();
                    let source_alias = manifest.metadata.name.clone();

                    let mesh = caller.data().mesh.clone();
                    // In a production system, we call try_p2p_fuel_transfer logic via MeshWorker or Hive command
                    // We broadcast this payment intent!

                    // Host-isolated Wallet Deterministic generation for the sender!
                    use sha2::Digest;
                    let mut hasher = sha2::Sha256::new();
                    hasher.update(source_alias.as_bytes());
                    let mut seed_a = [0u8; 32];
                    seed_a.copy_from_slice(&hasher.finalize()[..]);
                    let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed_a);
                    let pub_a = signing_key.verifying_key().to_bytes().to_vec();

                    let mut hasher2 = sha2::Sha256::new();
                    hasher2.update(target_alias.as_bytes());
                    let mut seed_b = [0u8; 32];
                    seed_b.copy_from_slice(&hasher2.finalize()[..]);
                    let pub_b = ed25519_dalek::SigningKey::from_bytes(&seed_b).verifying_key().to_bytes().to_vec();

                    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64;

                    let mut signed_data = Vec::new();
                    signed_data.extend_from_slice(&pub_a);
                    signed_data.extend_from_slice(&pub_b);
                    signed_data.extend_from_slice(&amount.to_be_bytes());
                    signed_data.extend_from_slice(&nonce.to_be_bytes());

                    use ed25519_dalek::Signer;
                    let sig = signing_key.sign(&signed_data).to_bytes().to_vec();

                    let tx = crate::economy::registry::FuelTransaction {
                        from: pub_a,
                        to: pub_b,
                        amount,
                        nonce,
                        signature: sig,
                    };

                    let pkt = crate::hive::HiveCommand::Economy(crate::hive::HiveEconomyCommand::TransferCredit(tx));
                    // Broadcast or Local processing:
                    // For test contexts, we directly mock or process via our gateway/network if available.
                    let _ = mesh.send_economy_packet(pkt).await;

                    Ok(0) // Success
                })
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::pay: {e:#}")))?;

        linker.func_wrap_async(
            "trytet",
            "bill",
            |mut caller: Caller<'_, TetState>,
             (source_ptr, source_len, amount): (i32, i32, i64)|
             -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let amount = amount as u64;
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return Err(wasmtime::Error::msg("Memory Error")),
                    };

                    let source_alias = {
                        let rb = validate_range(&memory, &caller, source_ptr, source_len)?.to_vec();
                        String::from_utf8_lossy(&rb).to_string()
                    };

                    let manifest = caller.data().manifest.clone();
                    let target_alias = manifest.metadata.name.clone();
                    let mesh = caller.data().mesh.clone();

                    let pkt = crate::hive::HiveCommand::Economy(crate::hive::HiveEconomyCommand::BillRequest {
                        source_alias,
                        target_alias,
                        amount,
                    });

                    let _ = mesh.send_economy_packet(pkt).await;
                    Ok(0)
                })
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::bill: {e:#}")))?;

        // Phase 23.1: The External Settlement Bridge
        linker.func_wrap_async(
            "trytet",
            "withdraw",
            |mut caller: Caller<'_, TetState>,
             (amount, addr_ptr, addr_len): (i64, i32, i32)|
             -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let amount = amount as u64;
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return Err(wasmtime::Error::msg("Memory Error")),
                    };

                    let target_address = {
                        let rb = validate_range(&memory, &caller, addr_ptr, addr_len)?.to_vec();
                        String::from_utf8_lossy(&rb).to_string()
                    };

                    // 1. Atomic Burn
                    let max_fuel = caller.get_fuel().unwrap_or(0);
                    if amount > max_fuel {
                        return Ok(5); // OUT OF FUEL
                    }
                    let _ = caller.set_fuel(max_fuel - amount);

                    let manifest = caller.data().manifest.clone();
                    let source_alias = manifest.metadata.name.clone();

                    let mesh = caller.data().mesh.clone();

                    // Generate host-isolated signature for BridgeIntent
                    use sha2::Digest;
                    let mut hasher = sha2::Sha256::new();
                    hasher.update(source_alias.as_bytes());
                    let mut seed = [0u8; 32];
                    seed.copy_from_slice(&hasher.finalize()[..]);
                    let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);

                    let mut signed_data = Vec::new();
                    signed_data.extend_from_slice(&amount.to_be_bytes());
                    signed_data.extend_from_slice(b"ETH"); // External Asset mapping statically for now
                    signed_data.extend_from_slice(target_address.as_bytes());

                    use ed25519_dalek::Signer;
                    let sig = signing_key.sign(&signed_data).to_bytes().to_vec();

                    let intent = crate::economy::bridge::BridgeIntent {
                        internal_fuel: amount,
                        external_asset: "ETH".to_string(),
                        target_address,
                        agent_signature: sig,
                    };

                    let pkt = crate::hive::HiveCommand::Economy(crate::hive::HiveEconomyCommand::WithdrawalPending(intent));

                    // 2-Phase Commit logic: if broadcasting fails, we rollback the Wasm fuel!
                    if mesh.send_economy_packet(pkt).await.is_err() {
                        let _ = caller.set_fuel(max_fuel); // Rollback
                        return Ok(6); // NETWORK DISCONNECT
                    }

                    Ok(0) // Success
                })
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::withdraw: {e:#}")))?;

        // Phase 24.1: Genesis Factory Lifecycle Hooks
        linker.func_wrap_async(
            "trytet",
            "reclaim",
            |mut caller: Caller<'_, TetState>,
             (child_ptr, child_len): (i32, i32)|
             -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let permissions = caller.data().manifest.permissions.clone();
                    if !permissions.is_genesis_factory {
                        return Ok(7); // ACCESS_DENIED
                    }

                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return Err(wasmtime::Error::msg("Memory Error")),
                    };

                    let child_id = {
                        let rb = validate_range(&memory, &caller, child_ptr, child_len)?.to_vec();
                        String::from_utf8_lossy(&rb).to_string()
                    };

                    let mesh = caller.data().mesh.clone();

                    if let Err(_) = mesh.send_reclaim(child_id).await {
                        return Ok(6); // DISCONNECT
                    }

                    Ok(0) // Success gracefully initiated!
                })
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::reclaim: {e:#}")))?;

        // Phase 33.1: Neuro-Symbolic Cartridge Substrate — invoke_component
        // Signature: (component_id_ptr, component_id_len, payload_ptr, payload_len, fuel: i64, out_ptr, out_len_ptr) -> i32
        // Return codes: 0=success, 1=fuel_exhausted, 2=buffer_too_small, 3=compilation_failed,
        //               4=interface_mismatch, 5=execution_error, 6=registry_error
        linker.func_wrap_async(
            "trytet",
            "invoke_component",
            |mut caller: Caller<'_, TetState>,
             (cid_ptr, cid_len, payload_ptr, payload_len, fuel, out_ptr, out_len_ptr): (i32, i32, i32, i32, i64, i32, i32)|
             -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return Err(wasmtime::Error::msg("Memory Error")),
                    };

                    // 1. Read component ID and payload from guest linear memory
                    let cid_bytes = validate_range(&memory, &caller, cid_ptr, cid_len)?.to_vec();
                    let component_id = String::from_utf8_lossy(&cid_bytes).to_string();

                    let payload_bytes = validate_range(&memory, &caller, payload_ptr, payload_len)?.to_vec();
                    let payload = String::from_utf8_lossy(&payload_bytes).to_string();

                    // 2. Deduct the fuel from the parent Agent
                    let fuel_to_give = fuel as u64;
                    let max_fuel = caller.get_fuel().unwrap_or(0);
                    if fuel_to_give > max_fuel {
                        return Ok(1); // FuelExhausted (parent can't afford it)
                    }
                    let _ = caller.set_fuel(max_fuel - fuel_to_give);

                    // 3. Invoke the Cartridge via the CartridgeManager
                    let cartridge_mgr = caller.data().cartridge_manager.clone();

                    let result = cartridge_mgr.invoke(
                        &component_id,
                        &payload,
                        fuel_to_give,
                        512, // Default max_memory_mb for cartridges
                    );

                    match result {
                        Ok((output, metrics)) => {
                            // Refund unused fuel to the parent
                            let refund = fuel_to_give.saturating_sub(metrics.fuel_consumed);
                            if refund > 0 {
                                if let Ok(current) = caller.get_fuel() {
                                    let _ = caller.set_fuel(current + refund);
                                }
                            }

                            let response_bytes = output.as_bytes();
                            let response_len = response_bytes.len() as i32;

                            // Re-borrow memory after mutation
                            let memory = caller.get_export("memory").unwrap().into_memory().unwrap();

                            // Check guest buffer size
                            let len_slice = validate_range(&memory, &caller, out_len_ptr, 4)?;
                            let mut len_buf = [0u8; 4];
                            len_buf.copy_from_slice(len_slice);
                            let guest_buffer_size = i32::from_le_bytes(len_buf);

                            if response_len > guest_buffer_size {
                                // Buffer too small — write required size
                                let required = response_len.to_le_bytes();
                                if let Ok(m) = validate_range_mut(&memory, &mut caller, out_len_ptr, 4) {
                                    m.copy_from_slice(&required);
                                }
                                return Ok(2); // BUFFER_TOO_SMALL
                            }

                            // Write response to guest buffer
                            let m = validate_range_mut(&memory, &mut caller, out_ptr, response_len)?;
                            m.copy_from_slice(response_bytes);
                            let written = response_len.to_le_bytes();
                            if let Ok(m) = validate_range_mut(&memory, &mut caller, out_len_ptr, 4) {
                                m.copy_from_slice(&written);
                            }

                            Ok(0) // SUCCESS
                        }
                        Err(crate::cartridge::CartridgeError::FuelExhausted) => {
                            // Cartridge burned all its fuel — no refund
                            Ok(1)
                        }
                        Err(crate::cartridge::CartridgeError::MemoryExceeded) => {
                            Ok(1) // Treat same as fuel exhaustion from parent's perspective
                        }
                        Err(crate::cartridge::CartridgeError::CompilationFailed(_)) => {
                            // Refund the pre-deducted fuel since the cartridge never ran
                            if let Ok(current) = caller.get_fuel() {
                                let _ = caller.set_fuel(current + fuel_to_give);
                            }
                            Ok(3)
                        }
                        Err(crate::cartridge::CartridgeError::InterfaceMismatch(_)) => {
                            if let Ok(current) = caller.get_fuel() {
                                let _ = caller.set_fuel(current + fuel_to_give);
                            }
                            Ok(4)
                        }
                        Err(crate::cartridge::CartridgeError::ExecutionError(_, fuel_consumed)) => {
                            let refund = fuel_to_give.saturating_sub(fuel_consumed);
                            if refund > 0 {
                                if let Ok(current) = caller.get_fuel() {
                                    let _ = caller.set_fuel(current + refund);
                                }
                            }
                            Ok(5)
                        }
                        Err(crate::cartridge::CartridgeError::RegistryError(_)) => {
                            if let Ok(current) = caller.get_fuel() {
                                let _ = caller.set_fuel(current + fuel_to_give);
                            }
                            Ok(6)
                        }
                    }
                })
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::invoke_component: {e:#}")))?;

    Ok(())
}
