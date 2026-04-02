//! WasmtimeSandbox — the concrete implementation of `TetSandbox`.
//!
//! This is the heart of the Tet engine. It manages:
//! - A pre-warmed `wasmtime::Engine` with `async_support` enabled.
//! - WASI p1 sandboxing with isolated `/workspace` VFS.
//! - Deterministic Wasm Fuel metering and custom RPC `trytet::invoke` host functions.
//! - Linear memory and VFS snapshot/fork execution graphs.

use crate::engine::{TetError, TetSandbox};
use crate::mesh::TetMesh;
use crate::models::{
    CrashReport, ExecutionStatus, MeshCallRequest, StructuredTelemetry, TetExecutionRequest,
    TetExecutionResult, TetMetadata,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use wasmtime::{Caller, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::p1::WasiP1Ctx;
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

use crate::sandbox::SnapshotPayload;

// ---------------------------------------------------------------------------
// Store State
// ---------------------------------------------------------------------------

struct TetState {
    wasi_p1: WasiP1Ctx,
    limits: StoreLimits,
    mesh: TetMesh,
    call_stack_depth: u32,
    fuel_to_burn_from_parent: u64, // Used to communicate back how much child spent
    pub migration_requested: bool,
    pub migration_target: Option<String>,
    pub egress_policy: Option<crate::oracle::EgressPolicy>,
    pub vector_vfs: Arc<crate::memory::VectorVfs>,
    pub inference_engine: Arc<dyn crate::inference::NeuralEngine>,
}

// ---------------------------------------------------------------------------
// WasmtimeSandbox
// ---------------------------------------------------------------------------

pub struct WasmtimeSandbox {
    engine: Engine,
    snapshots: Arc<RwLock<HashMap<String, SnapshotPayload>>>,
    active_memories: Arc<RwLock<HashMap<String, SnapshotPayload>>>,
    pub mesh: TetMesh,
    pub voucher_manager: Arc<crate::economy::VoucherManager>,
    pub require_payment: bool,
    pub local_node_id: String,
    pub neural_engine: Arc<dyn crate::inference::NeuralEngine>,
}

impl WasmtimeSandbox {
    pub fn new(
        mesh: TetMesh,
        voucher_manager: Arc<crate::economy::VoucherManager>,
        require_payment: bool,
        local_node_id: String,
    ) -> Result<Self, TetError> {
        Self::new_with_engine(
            mesh,
            voucher_manager,
            require_payment,
            local_node_id,
            Arc::new(crate::inference::MockNeuralEngine::new()),
        )
    }

    pub fn new_with_engine(
        mesh: TetMesh,
        voucher_manager: Arc<crate::economy::VoucherManager>,
        require_payment: bool,
        local_node_id: String,
        neural_engine: Arc<dyn crate::inference::NeuralEngine>,
    ) -> Result<Self, TetError> {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        config.cranelift_opt_level(wasmtime::OptLevel::Speed);

        let engine =
            Engine::new(&config).map_err(|e| TetError::EngineError(format!("{e:#}")))?;

        Ok(Self {
            engine,
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            active_memories: Arc::new(RwLock::new(HashMap::new())),
            mesh,
            voucher_manager,
            require_payment,
            local_node_id,
            neural_engine,
        })
    }

    fn capture_workspace(dir_path: &Path) -> HashMap<String, String> {
        let mut files = HashMap::new();
        if let Ok(entries) = fs::read_dir(dir_path) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_file() {
                        let path = entry.path();
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if let Ok(content) = fs::read_to_string(&path) {
                                files.insert(name.to_string(), content);
                            }
                        }
                    }
                }
            }
        }
        files
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_inner(
        engine: &Engine,
        mesh: &TetMesh,
        wasm_bytes: &[u8],
        req: &TetExecutionRequest,
        memory_to_restore: Option<&[u8]>,
        vfs_to_restore: Option<&[u8]>,
        vector_to_restore: Option<&[u8]>,
        inference_to_restore: Option<&[u8]>,
        neural_engine: Arc<dyn crate::inference::NeuralEngine>,
        call_depth: u32,
    ) -> Result<(TetExecutionResult, SnapshotPayload), TetError> {
        if call_depth > 5 {
            return Err(TetError::CallStackExhausted);
        }

        let start = Instant::now();
        let tet_id = uuid::Uuid::new_v4().to_string();

        let temp_dir = tempfile::tempdir()
            .map_err(|e| TetError::VfsError(format!("Failed to create isolated tempdir: {e}")))?;

        if let Some(tarball_bytes) = vfs_to_restore {
            let mut archive = tar::Archive::new(tarball_bytes);
            archive
                .unpack(temp_dir.path())
                .map_err(|e| TetError::VfsError(format!("Failed to unpack VFS archive: {e}")))?;
        }

        for (filename, content) in &req.injected_files {
            let safe_filename = Path::new(filename).file_name().unwrap_or_default();
            let file_path = temp_dir.path().join(safe_filename);
            fs::write(file_path, content)
                .map_err(|e| TetError::VfsError(format!("Failed to inject file '{filename}': {e}")))?;
        }

        let module = Module::new(engine, wasm_bytes)
            .map_err(|e| TetError::EngineError(format!("Module compilation failed: {e:#}")))?;

        let stdout_pipe = MemoryOutputPipe::new(1024 * 1024);
        let stderr_pipe = MemoryOutputPipe::new(1024 * 1024);

        let mut wasi_builder = WasiCtxBuilder::new();
        for (key, value) in &req.env {
            wasi_builder.env(key, value);
        }
        wasi_builder.stdout(stdout_pipe.clone());
        wasi_builder.stderr(stderr_pipe.clone());

        wasi_builder
            .preopened_dir(
                temp_dir.path(),
                "/workspace",
                DirPerms::all(),
                FilePerms::all(),
            )
            .map_err(|e| TetError::EngineError(format!("VFS mapping failed: {e}")))?;

        let wasi_p1_ctx = wasi_builder.build_p1();

        let limits = StoreLimitsBuilder::new()
            .memory_size(req.max_memory_mb as usize * 1024 * 1024)
            .instances(1)
            .tables(10)
            .memories(1)
            .build();

        let mut vector_vfs = crate::memory::VectorVfs::new();
        if let Some(v) = vector_to_restore {
            if let Ok(restored) = bincode::deserialize::<crate::memory::VectorVfs>(v) {
                restored.rebuild_all_indexes(); // Essential for instant-distance!
                vector_vfs = restored;
            }
        }

        // Phase 10: Restore inference sessions from snapshot (Context Replay)
        if let Some(inf_data) = inference_to_restore {
            if !inf_data.is_empty() {
                neural_engine.restore_sessions(inf_data).await;
            }
        }

        let state = TetState {
            wasi_p1: wasi_p1_ctx,
            limits,
            mesh: mesh.clone(),
            call_stack_depth: call_depth,
            fuel_to_burn_from_parent: 0,
            migration_requested: false,
            migration_target: None,
            egress_policy: req.egress_policy.clone(),
            vector_vfs: Arc::new(vector_vfs),
            inference_engine: neural_engine.clone(),
        };

        let mut store = Store::new(engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_fuel(req.allocated_fuel).unwrap();

        let mut linker: Linker<TetState> = Linker::new(engine);
        wasmtime_wasi::p1::add_to_linker_async(&mut linker, |state: &mut TetState| &mut state.wasi_p1)
            .map_err(|e| TetError::EngineError(format!("WASI linking failed: {e:#}")))?;

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
                    
                    let mem_slice = memory.data(&caller);
                    
                    let t_start = target_ptr as usize;
                    let t_end = t_start + target_len as usize;
                    if t_end > mem_slice.len() { return Err(wasmtime::Error::msg("Out of bounds")); }
                    
                    let target_node = String::from_utf8_lossy(&mem_slice[t_start..t_end]).to_string();
                    
                    caller.data_mut().migration_requested = true;
                    caller.data_mut().migration_target = Some(target_node);
                    let res: wasmtime::Result<()> = Err(wasmtime::Error::msg("MIGRATION_REQUESTED"));
                    res
                })
            }
        ).map_err(|e| TetError::EngineError(format!("Linking request_migration failed: {e:#}")))?;

        let source_alias = req.alias.clone().unwrap_or_else(|| "anonymous_tet".to_string());

        linker
            .func_wrap_async(
                "trytet",
                "invoke",
                move |mut caller: Caller<'_, TetState>, (target_ptr, target_len, payload_ptr, payload_len, out_ptr, out_len_ptr, fuel): (i32, i32, i32, i32, i32, i32, i64)| {
                    let source_alias = source_alias.clone();
                    Box::new(async move {
                        // 1. Read pointers from Linear Memory
                        let memory = match caller.get_export("memory") {
                            Some(wasmtime::Extern::Memory(m)) => m,
                            _ => return 1_i32, // Memory error code
                        };

                        let mem_slice = memory.data(&caller);
                        
                        // Extract target alias
                        let t_start = target_ptr as usize;
                        let t_end = t_start + target_len as usize;
                        if t_end > mem_slice.len() { return 1_i32; }
                        let target_alias = String::from_utf8_lossy(&mem_slice[t_start..t_end]).to_string();

                        // Extract payload
                        let p_start = payload_ptr as usize;
                        let p_end = p_start + payload_len as usize;
                        if p_end > mem_slice.len() { return 1_i32; }
                        let payload_bytes = mem_slice[p_start..p_end].to_vec();

                        let mesh = caller.data().mesh.clone();
                        let max_fuel = caller.get_fuel().unwrap_or(0);
                        let fuel_to_transfer = if (fuel as u64) > max_fuel { max_fuel } else { fuel as u64 };

                        let call_req = MeshCallRequest {
                            target_alias: target_alias.clone(),
                            method: "invoke".to_string(), // MVP simplified
                            payload: payload_bytes,
                            fuel_to_transfer,
                            current_depth: caller.data().call_stack_depth,
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
                        mesh.record_telemetry(source_alias, target_alias, req_bytes + res_bytes, elapsed_us, is_error).await;

                        match response {
                            Ok(res) => {
                                // Deduct the fuel the child actually burned
                                caller.data_mut().fuel_to_burn_from_parent += res.fuel_used;

                                let response_len = res.return_data.len() as i32;
                                
                                // Re-borrow memory because caller was mutated above
                                let memory = caller.get_export("memory").unwrap().into_memory().unwrap();

                                // Check if guest buffer is large enough
                                let length_ptr_start = out_len_ptr as usize;
                                let mut len_buf = [0u8; 4];
                                len_buf.copy_from_slice(&memory.data(&caller)[length_ptr_start..length_ptr_start+4]);
                                let guest_buffer_size = i32::from_le_bytes(len_buf);

                                if response_len > guest_buffer_size {
                                    // Too small! Inform the guest of the required size.
                                    let required_size = response_len.to_le_bytes();
                                    memory.data_mut(&mut caller)[length_ptr_start..length_ptr_start+4].copy_from_slice(&required_size);
                                    success_code = 2_i32; // Buffer too small code
                                } else {
                                    // Valid! Copy the data into the guest.
                                    let o_start = out_ptr as usize;
                                    memory.data_mut(&mut caller)[o_start..o_start + response_len as usize].copy_from_slice(&res.return_data);
                                    
                                    let written_size = response_len.to_le_bytes();
                                    memory.data_mut(&mut caller)[length_ptr_start..length_ptr_start+4].copy_from_slice(&written_size);
                                    
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

                        success_code
                    })
                },
            )
            .map_err(|e| TetError::EngineError(format!("Failed to register trytet::invoke: {e:#}")))?;

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        linker.func_wrap_async(
            "trytet",
            "fetch",
            move |mut caller: Caller<'_, TetState>, (url_ptr, url_len, method_ptr, method_len, body_ptr, body_len, out_ptr, out_len_ptr): (i32, i32, i32, i32, i32, i32, i32, i32)| {
                let http_client = http_client.clone();
                Box::new(async move {
                    let memory = match caller.get_export("memory") {
                        Some(wasmtime::Extern::Memory(m)) => m,
                        _ => return Ok(1_i32),
                    };

                    let mem_slice = memory.data(&caller);
                    
                    let url_start = url_ptr as usize;
                    let url_end = url_start + url_len as usize;
                    if url_end > mem_slice.len() { return Ok(1_i32); }
                    let target_url = String::from_utf8_lossy(&mem_slice[url_start..url_end]).to_string();

                    let m_start = method_ptr as usize;
                    let m_end = m_start + method_len as usize;
                    if m_end > mem_slice.len() { return Ok(1_i32); }
                    let req_method_str = String::from_utf8_lossy(&mem_slice[m_start..m_end]).to_string();

                    let b_start = body_ptr as usize;
                    let b_end = b_start + body_len as usize;
                    if b_end > mem_slice.len() { return Ok(1_i32); }
                    let req_body = mem_slice[b_start..b_end].to_vec();

                    // Security/Filtering
                    let policy = caller.data().egress_policy.clone();
                    if let Some(p) = policy {
                        if p.require_https && !target_url.starts_with("https://") {
                            return Err(wasmtime::Error::msg("Security Violation: HTTPS strictly required"));
                        }
                        
                        // Parse host
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

                    let method = match req_method_str.as_str() {
                        "POST" => reqwest::Method::POST,
                        "PUT" => reqwest::Method::PUT,
                        "DELETE" => reqwest::Method::DELETE,
                        _ => reqwest::Method::GET,
                    };

                    // Execute HTTP Request
                    let response = http_client.request(method, &target_url)
                        .body(req_body.clone())
                        .send()
                        .await;

                    let mut returned_bytes = Vec::new();
                    let success_code = match response {
                        Ok(res) => {
                            if let Ok(bytes) = res.bytes().await {
                                returned_bytes = bytes.to_vec();
                                0_i32
                            } else {
                                6_i32 // Body read failed
                            }
                        }
                        Err(_) => {
                            6_i32 // Legacy network error
                        }
                    };

                    let total_network_bytes = req_body.len() as u64 + returned_bytes.len() as u64;
                    let fuel_tax = total_network_bytes * 10;
                    
                    if let Ok(current_fuel) = caller.get_fuel() {
                        if current_fuel >= fuel_tax {
                            let _ = caller.set_fuel(current_fuel - fuel_tax);
                        } else {
                            let _ = caller.set_fuel(0); // Trap out-of-fuel quickly
                        }
                    }

                    // Write response to linear memory if success
                    if success_code == 0 {
                        let response_len = returned_bytes.len() as i32;
                        let length_ptr_start = out_len_ptr as usize;
                        let mut len_buf = [0u8; 4];
                        len_buf.copy_from_slice(&memory.data(&caller)[length_ptr_start..length_ptr_start+4]);
                        let guest_buffer_size = i32::from_le_bytes(len_buf);

                        if response_len > guest_buffer_size {
                            let required_size = response_len.to_le_bytes();
                            memory.data_mut(&mut caller)[length_ptr_start..length_ptr_start+4].copy_from_slice(&required_size);
                            return Ok(2_i32); // Buffer too small code
                        } else {
                            let o_start = out_ptr as usize;
                            memory.data_mut(&mut caller)[o_start..o_start + response_len as usize].copy_from_slice(&returned_bytes);
                            
                            let written_size = response_len.to_le_bytes();
                            memory.data_mut(&mut caller)[length_ptr_start..length_ptr_start+4].copy_from_slice(&written_size);
                        }
                    }
                    
                    Ok(success_code)
                })
            }
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::fetch: {e:#}")))?;

        // Phase 9: The Sovereign Memory
        linker.func_wrap_async(
            "trytet",
            "remember",
            |mut caller: Caller<'_, TetState>,
             (collection_ptr, collection_len, record_ptr, record_len): (i32, i32, i32, i32)|
             -> Box<dyn std::future::Future<Output = i32> + Send + '_> {
                Box::new(async move {
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return 1, // Memory error
                    };

                    let collection_bytes = {
                        let start = collection_ptr as usize;
                        let end = start + collection_len as usize;
                        memory.data(&caller).get(start..end).map(|b| b.to_vec())
                    };
                    
                    let record_bytes = {
                        let start = record_ptr as usize;
                        let end = start + record_len as usize;
                        memory.data(&caller).get(start..end).map(|b| b.to_vec())
                    };

                    if let (Some(cb), Some(rb)) = (collection_bytes, record_bytes) {
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
                                        return 5; // Out of fuel / traps naturally
                                    }
                                }
                                
                                let vfs = caller.data().vector_vfs.clone();
                                vfs.remember(&collection_name, record);
                                return 0; // Success
                            }
                        }
                    }
                    2 // Parse error
                })
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::remember: {e:#}")))?;

        linker.func_wrap_async(
            "trytet",
            "recall",
            |mut caller: Caller<'_, TetState>,
             (query_ptr, query_len, out_ptr, out_len_ptr): (i32, i32, i32, i32)|
             -> Box<dyn std::future::Future<Output = i32> + Send + '_> {
                Box::new(async move {
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return 1,
                    };

                    let query_bytes = {
                        let start = query_ptr as usize;
                        let end = start + query_len as usize;
                        memory.data(&caller).get(start..end).map(|b| b.to_vec())
                    };

                    if let Some(qb) = query_bytes {
                        if let Ok(query) = serde_json::from_slice::<crate::memory::SearchQuery>(&qb) {
                            
                            let dim = query.query_vector.len() as u64;
                            let search_cost = 100 + (dim * 2);
                            
                            if let Ok(current_fuel) = caller.get_fuel() {
                                if current_fuel >= search_cost {
                                    let _ = caller.set_fuel(current_fuel - search_cost);
                                } else {
                                    let _ = caller.set_fuel(0);
                                    return 5;
                                }
                            }
                            
                            let vfs = caller.data().vector_vfs.clone();
                            let results = vfs.recall(&query);
                            
                            if let Ok(response_json) = serde_json::to_vec(&results) {
                                let response_len = response_json.len() as i32;
                                let length_ptr_start = out_len_ptr as usize;
                                
                                let mut len_buf = [0u8; 4];
                                len_buf.copy_from_slice(&memory.data(&caller)[length_ptr_start..length_ptr_start+4]);
                                let guest_buffer_size = i32::from_le_bytes(len_buf);

                                if response_len > guest_buffer_size {
                                    let required_size = response_len.to_le_bytes();
                                    memory.data_mut(&mut caller)[length_ptr_start..length_ptr_start+4].copy_from_slice(&required_size);
                                    return 2; // Buffer too small
                                } else {
                                    let o_start = out_ptr as usize;
                                    memory.data_mut(&mut caller)[o_start..o_start + response_len as usize].copy_from_slice(&response_json);
                                    
                                    let written_size = response_len.to_le_bytes();
                                    memory.data_mut(&mut caller)[length_ptr_start..length_ptr_start+4].copy_from_slice(&written_size);
                                    return 0; // Success
                                }
                            }
                        }
                    }
                    3 // Bad input
                })
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::recall: {e:#}")))?;

        // Phase 10: The Sovereign Inference — model_load
        linker.func_wrap_async(
            "trytet",
            "model_load",
            |mut caller: Caller<'_, TetState>,
             (alias_ptr, alias_len, path_ptr, path_len): (i32, i32, i32, i32)|
             -> Box<dyn std::future::Future<Output = i32> + Send + '_> {
                Box::new(async move {
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return 1,
                    };

                    let alias_bytes = {
                        let start = alias_ptr as usize;
                        let end = start + alias_len as usize;
                        memory.data(&caller).get(start..end).map(|b| b.to_vec())
                    };

                    let path_bytes = {
                        let start = path_ptr as usize;
                        let end = start + path_len as usize;
                        memory.data(&caller).get(start..end).map(|b| b.to_vec())
                    };

                    if let (Some(ab), Some(pb)) = (alias_bytes, path_bytes) {
                        if let (Ok(alias), Ok(path)) = (String::from_utf8(ab), String::from_utf8(pb)) {
                            // Deduct model load fuel cost
                            let load_cost = crate::inference::InferenceFuelCalculator::model_load_cost();
                            if let Ok(current_fuel) = caller.get_fuel() {
                                if current_fuel >= load_cost {
                                    let _ = caller.set_fuel(current_fuel - load_cost);
                                } else {
                                    let _ = caller.set_fuel(0);
                                    return 5; // Out of fuel
                                }
                            }

                            let engine = caller.data().inference_engine.clone();
                            match engine.load_model(&alias, &path).await {
                                Ok(_) => return 0, // Success
                                Err(_) => return 3, // Load failed
                            }
                        }
                    }
                    2 // Parse error
                })
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::model_load: {e:#}")))?;

        // Phase 10: The Sovereign Inference — model_predict
        linker.func_wrap_async(
            "trytet",
            "model_predict",
            |mut caller: Caller<'_, TetState>,
             (request_ptr, request_len, out_ptr, out_len_ptr): (i32, i32, i32, i32)|
             -> Box<dyn std::future::Future<Output = i32> + Send + '_> {
                Box::new(async move {
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return 1,
                    };

                    let request_bytes = {
                        let start = request_ptr as usize;
                        let end = start + request_len as usize;
                        memory.data(&caller).get(start..end).map(|b| b.to_vec())
                    };

                    if let Some(rb) = request_bytes {
                        if let Ok(request) = serde_json::from_slice::<crate::inference::InferenceRequest>(&rb) {
                            let engine = caller.data().inference_engine.clone();
                            let current_fuel = caller.get_fuel().unwrap_or(0);

                            match engine.predict(&request, current_fuel).await {
                                Ok(response) => {
                                    // Burn fuel proportional to tokens
                                    let fuel_cost = response.fuel_burned;
                                    if let Ok(fuel) = caller.get_fuel() {
                                        if fuel >= fuel_cost {
                                            let _ = caller.set_fuel(fuel - fuel_cost);
                                        } else {
                                            let _ = caller.set_fuel(0);
                                            // Fallthrough to write OutOfFuel trap_reason to Wasm memory
                                        }
                                    }

                                    // Write response back to Wasm linear memory
                                    if let Ok(response_json) = serde_json::to_vec(&response) {
                                        let response_len = response_json.len() as i32;
                                        let length_ptr_start = out_len_ptr as usize;

                                        let mut len_buf = [0u8; 4];
                                        len_buf.copy_from_slice(&memory.data(&caller)[length_ptr_start..length_ptr_start+4]);
                                        let guest_buffer_size = i32::from_le_bytes(len_buf);

                                        if response_len > guest_buffer_size {
                                            let required_size = response_len.to_le_bytes();
                                            memory.data_mut(&mut caller)[length_ptr_start..length_ptr_start+4].copy_from_slice(&required_size);
                                            return 2; // Buffer too small
                                        } else {
                                            let o_start = out_ptr as usize;
                                            memory.data_mut(&mut caller)[o_start..o_start + response_len as usize].copy_from_slice(&response_json);

                                            let written_size = response_len.to_le_bytes();
                                            memory.data_mut(&mut caller)[length_ptr_start..length_ptr_start+4].copy_from_slice(&written_size);
                                            return 0; // Success
                                        }
                                    }
                                }
                                Err(_) => return 4, // Inference failed
                            }
                        }
                    }
                    3 // Bad input
                })
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::model_predict: {e:#}")))?;

        let instance = linker
            .instantiate_async(&mut store, &module)
            .await
            .map_err(|e| TetError::EngineError(format!("Instantiation failed: {e:#}")))?;

        if let Some(snapshot_bytes) = memory_to_restore {
            if let Some(memory) = instance.get_memory(&mut store, "memory") {
                let current_size = memory.data_size(&store);
                if snapshot_bytes.len() > current_size {
                    let pages_needed =
                        (snapshot_bytes.len() - current_size).div_ceil(65536) as u64;
                    memory.grow(&mut store, pages_needed).map_err(|e| {
                        TetError::EngineError(format!("Memory grow for fork failed: {e:#}"))
                    })?;
                }
                let dest = memory.data_mut(&mut store);
                dest[..snapshot_bytes.len()].copy_from_slice(snapshot_bytes);
            }
        }

        let run_result = match instance.get_typed_func::<(), ()>(&mut store, "_start") {
            Ok(start_fn) => start_fn.call_async(&mut store, ()).await,
            Err(_) => Ok(()), // empty default export
        };

        let status = match run_result {
            Ok(()) => ExecutionStatus::Success,
            Err(trap) => {
                if store.data().migration_requested {
                    ExecutionStatus::Migrated
                } else {
                    classify_trap(&trap)
                }
            }
        };

        let fuel_consumed = req.allocated_fuel - store.get_fuel().unwrap_or(0);

        let memory_snapshot = instance
            .get_memory(&mut store, "memory")
            .map(|mem| mem.data(&store).to_vec())
            .unwrap_or_default();
        let memory_used_kb = (memory_snapshot.len() / 1024) as u64;

        let mut archive_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut archive_bytes);
            let _ = builder.append_dir_all(".", temp_dir.path());
            let _ = builder.into_inner();
        }

        let mutated_files = Self::capture_workspace(temp_dir.path());

        let stdout_bytes = stdout_pipe.contents();
        let stderr_bytes = stderr_pipe.contents();
        let stdout_str = String::from_utf8_lossy(&stdout_bytes);
        let stderr_str = String::from_utf8_lossy(&stderr_bytes);
        let stdout_lines = if stdout_str.is_empty() { vec![] } else { stdout_str.lines().map(String::from).collect() };
        let stderr_lines = if stderr_str.is_empty() { vec![] } else { stderr_str.lines().map(String::from).collect() };

        let vector_vfs = store.data().vector_vfs.clone();
        let vector_idx = bincode::serialize(&vector_vfs).unwrap_or_default();

        // Phase 10: Serialize inference sessions for snapshot
        let inference_state = store.data().inference_engine.serialize_sessions().await;

        let result = TetExecutionResult {
            tet_id: tet_id.clone(),
            status,
            telemetry: StructuredTelemetry {
                stdout_lines,
                stderr_lines,
                memory_used_kb,
            },
            execution_duration_us: start.elapsed().as_micros() as u64,
            fuel_consumed,
            mutated_files,
            migrated_to: store.data().migration_target.clone(),
        };

        let payload = SnapshotPayload {
            memory_bytes: memory_snapshot,
            wasm_bytes: wasm_bytes.to_vec(),
            fs_tarball: archive_bytes,
            vector_idx,
            inference_state,
        };

        // Auto-register in the Registry if alias provided
        if let Some(alias) = &req.alias {
            let metadata = TetMetadata {
                tet_id: tet_id.clone(),
                is_hibernating: false, // Will be set to true later
                snapshot_id: None,
                wasm_bytes: Some(wasm_bytes.to_vec()),
            };
            mesh.register(alias.clone(), metadata).await;
        }

        Ok((result, payload))
    }
}

// ---------------------------------------------------------------------------
// TetSandbox Implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl TetSandbox for WasmtimeSandbox {
    async fn execute(&self, mut req: TetExecutionRequest) -> Result<TetExecutionResult, TetError> {
        // --- 🔴 Phase 6: Economic Pre-Flight Guard 🔴 ---
        if self.require_payment {
            let violation = if let Some(voucher) = &req.voucher {
                match self.voucher_manager.verify_and_claim(voucher) {
                    Ok(_) => {
                        // Enforce mathematically bound fuel limits
                        req.allocated_fuel = voucher.fuel_limit;
                        None
                    }
                    Err(e) => Some(format!("Invalid Fuel Voucher: {}", e)),
                }
            } else {
                Some("Missing Fuel Voucher".to_string())
            };

            if let Some(msg) = violation {
                let tet_id = req.alias.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                return Ok(TetExecutionResult {
                    tet_id,
                    status: crate::models::ExecutionStatus::Crash(crate::models::CrashReport {
                        error_type: "EconomicViolation".to_string(),
                        instruction_offset: None,
                        message: msg,
                    }),
                    telemetry: crate::models::StructuredTelemetry {
                        stdout_lines: vec![],
                        stderr_lines: vec![],
                        memory_used_kb: 0,
                    },
                    execution_duration_us: 0,
                    fuel_consumed: 0,
                    mutated_files: std::collections::HashMap::new(),
                    migrated_to: None,
                });
            }
        }

        let parent_snapshot = if let Some(ref snap_id) = req.parent_snapshot_id {
            let store = self.snapshots.read().await;
            Some(
                store
                    .get(snap_id)
                    .ok_or_else(|| TetError::SnapshotNotFound(snap_id.clone()))?
                    .clone(),
            )
        } else {
            None
        };

        let wasm_bytes = match (&req.payload, &parent_snapshot) {
            (Some(bytes), _) => bytes.clone(),
            (None, Some(parent)) => parent.wasm_bytes.clone(),
            (None, None) => return Err(TetError::EngineError("No Wasm payload provided".into())),
        };

        let (mem_to_restore, vfs_to_restore, vec_to_restore, inf_to_restore) = match &parent_snapshot {
            Some(p) => (Some(p.memory_bytes.as_slice()), Some(p.fs_tarball.as_slice()), Some(p.vector_idx.as_slice()), Some(p.inference_state.as_slice())),
            None => (None, None, None, None),
        };

        // Pure async call — natively yields to Tokio via wasmtime async support!
        let (result, snapshot_payload) = Self::execute_inner(
            &self.engine,
            &self.mesh,
            &wasm_bytes,
            &req,
            mem_to_restore,
            vfs_to_restore,
            vec_to_restore,
            inf_to_restore,
            self.neural_engine.clone(),
            req.call_depth,
        ).await?;

        // Store active memory and auto-snapshot it
        self.active_memories
            .write()
            .await
            .insert(result.tet_id.clone(), snapshot_payload.clone());

        let snapshot_id = uuid::Uuid::new_v4().to_string();
        self.snapshots.write().await.insert(snapshot_id.clone(), snapshot_payload);

        // Update the registry to point the alias to the snapshot (hibernation)
        if let Some(alias) = &req.alias {
            let metadata = TetMetadata {
                tet_id: result.tet_id.clone(),
                is_hibernating: true,
                snapshot_id: Some(snapshot_id.clone()),
                wasm_bytes: Some(wasm_bytes.clone()),
            };
            self.mesh.register(alias.clone(), metadata).await;
        }

        Ok(result)
    }

    async fn snapshot(&self, id_or_alias: &str) -> Result<String, TetError> {
        let active = self.active_memories.read().await;
        
        // 1. First try direct look-up
        let mut payload_opt = active.get(id_or_alias).cloned();
        
        // 2. If not found, try resolving via TetMesh Registry
        if payload_opt.is_none() {
            if let Some(target_meta) = self.mesh.resolve(id_or_alias).await {
                payload_opt = active.get(&target_meta.tet_id).cloned();
            }
        }
        
        let payload = payload_opt.ok_or_else(|| TetError::SnapshotNotFound(id_or_alias.to_string()))?;
        drop(active);

        let snapshot_id = uuid::Uuid::new_v4().to_string();
        self.snapshots
            .write()
            .await
            .insert(snapshot_id.clone(), payload);

        Ok(snapshot_id)
    }

    async fn export_snapshot(&self, snapshot_id: &str) -> Result<SnapshotPayload, TetError> {
        let store = self.snapshots.read().await;
        let payload = store
            .get(snapshot_id)
            .ok_or_else(|| TetError::SnapshotNotFound(snapshot_id.to_string()))?
            .clone();
        Ok(payload)
    }

    async fn import_snapshot(&self, payload: SnapshotPayload) -> Result<String, TetError> {
        let snapshot_id = uuid::Uuid::new_v4().to_string();
        self.snapshots.write().await.insert(snapshot_id.clone(), payload);
        Ok(snapshot_id)
    }

    async fn query_memory(&self, alias: &str, query: crate::memory::SearchQuery) -> Result<Vec<crate::memory::SearchResult>, TetError> {
        let active = self.active_memories.read().await;
        
        let mut payload_opt = active.get(alias).cloned();
        
        if payload_opt.is_none() {
            if let Some(target_meta) = self.mesh.resolve(alias).await {
                payload_opt = active.get(&target_meta.tet_id).cloned();
            }
        }
        
        let payload = payload_opt.ok_or_else(|| TetError::SnapshotNotFound(alias.to_string()))?;
        drop(active);
        
        if payload.vector_idx.is_empty() {
            return Ok(Vec::new());
        }

        let vector_vfs: crate::memory::VectorVfs = bincode::deserialize(&payload.vector_idx)
            .map_err(|e| TetError::EngineError(format!("Failed to deserialize VectorVfs: {}", e)))?;
            
        vector_vfs.rebuild_all_indexes();
        Ok(vector_vfs.recall(&query))
    }

    async fn infer(&self, _alias: &str, request: crate::inference::InferenceRequest, fuel_limit: u64) -> Result<crate::inference::InferenceResponse, TetError> {
        self.neural_engine.predict(&request, fuel_limit).await
            .map_err(TetError::InferenceError)
    }

    async fn fork(
        &self,
        snapshot_id: &str,
        mut req: TetExecutionRequest,
    ) -> Result<TetExecutionResult, TetError> {
        req.parent_snapshot_id = Some(snapshot_id.to_string());
        self.execute(req).await
    }

    async fn get_topology(&self) -> Vec<crate::models::TopologyEdge> {
        self.mesh.get_topology().await
    }

    async fn send_mesh_call(
        &self,
        req: crate::models::MeshCallRequest,
    ) -> Result<crate::models::MeshCallResponse, TetError> {
        self.mesh.send_call(req).await
            .map_err(|e| TetError::EngineError(format!("Mesh Invocation Failed: {}", e)))
    }
}

// ---------------------------------------------------------------------------
// Trap Classification
// ---------------------------------------------------------------------------

fn classify_trap(error: &wasmtime::Error) -> ExecutionStatus {
    let message = format!("{error:#}");

    if message.contains("out of fuel") || message.contains("fuel consumed") || message.contains("epoch") || message.contains("interrupt") {
        return ExecutionStatus::OutOfFuel;
    }

    if message.contains("memory") && (message.contains("limit") || message.contains("maximum")) {
        return ExecutionStatus::MemoryExceeded;
    }

    if message.contains("proc_exit") && (message.contains("exit status 0") || message.contains("with code 0")) {
        return ExecutionStatus::Success;
    }

    let error_type = if message.contains("unreachable") {
        "unreachable".to_string()
    } else if message.contains("Security Violation") {
        "security_violation".to_string()
    } else if message.contains("out of bounds") {
        "memory_out_of_bounds".to_string()
    } else if message.contains("divide") || message.contains("division") {
        "integer_divide_by_zero".to_string()
    } else if message.contains("indirect call") {
        "indirect_call_type_mismatch".to_string()
    } else if message.contains("stack overflow") {
        "stack_overflow".to_string()
    } else if message.contains("null") {
        "null_reference".to_string()
    } else {
        "unknown_trap".to_string()
    };

    ExecutionStatus::Crash(CrashReport {
        error_type,
        instruction_offset: extract_instruction_offset(&message),
        message: message.clone(),
    })
}

fn extract_instruction_offset(message: &str) -> Option<usize> {
    if let Some(idx) = message.find("offset ") {
        let after = &message[idx + 7..];
        let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        num_str.parse().ok()
    } else {
        None
    }
}
