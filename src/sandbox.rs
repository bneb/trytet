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
use tokio::sync::RwLock;
use wasmtime::{Caller, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::p1::WasiP1Ctx;
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

// ---------------------------------------------------------------------------
// Store State
// ---------------------------------------------------------------------------

struct TetState {
    wasi_p1: WasiP1Ctx,
    limits: StoreLimits,
    mesh: TetMesh,
    call_stack_depth: u32,
    fuel_to_burn_from_parent: u64, // Used to communicate back how much child spent
}

// ---------------------------------------------------------------------------
// Snapshot Payload
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct SnapshotPayload {
    pub memory_bytes: Vec<u8>,
    pub wasm_bytes: Vec<u8>,
    pub fs_tarball: Vec<u8>,
}

// ---------------------------------------------------------------------------
// WasmtimeSandbox
// ---------------------------------------------------------------------------

pub struct WasmtimeSandbox {
    engine: Engine,
    snapshots: Arc<RwLock<HashMap<String, SnapshotPayload>>>,
    active_memories: Arc<RwLock<HashMap<String, SnapshotPayload>>>,
    pub mesh: TetMesh,
}

impl WasmtimeSandbox {
    pub fn new(mesh: TetMesh) -> Result<Self, TetError> {
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

    async fn execute_inner(
        engine: &Engine,
        mesh: &TetMesh,
        wasm_bytes: &[u8],
        req: &TetExecutionRequest,
        memory_to_restore: Option<&[u8]>,
        vfs_to_restore: Option<&[u8]>,
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

        let state = TetState {
            wasi_p1: wasi_p1_ctx,
            limits,
            mesh: mesh.clone(),
            call_stack_depth: call_depth,
            fuel_to_burn_from_parent: 0,
        };

        let mut store = Store::new(engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_fuel(req.allocated_fuel).unwrap();

        let mut linker: Linker<TetState> = Linker::new(engine);
        wasmtime_wasi::p1::add_to_linker_async(&mut linker, |state: &mut TetState| &mut state.wasi_p1)
            .map_err(|e| TetError::EngineError(format!("WASI linking failed: {e:#}")))?;

        // Phase 3: Custom Inter-Tet RPC Host Function
        linker
            .func_wrap_async(
                "trytet",
                "invoke",
                |mut caller: Caller<'_, TetState>, (target_ptr, target_len, payload_ptr, payload_len, out_ptr, out_len_ptr, fuel): (i32, i32, i32, i32, i32, i32, i64)| {
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
                            target_alias,
                            method: "invoke".to_string(), // MVP simplified
                            payload: payload_bytes,
                            fuel_to_transfer,
                            current_depth: caller.data().call_stack_depth,
                        };

                        // 2. Await the RPC call (this yields correctly to Tokio!)
                        let response = mesh.send_call(call_req).await;

                        // 3. Process Result
                        let mut success_code = 0_i32;

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

        let instance = linker
            .instantiate_async(&mut store, &module)
            .await
            .map_err(|e| TetError::EngineError(format!("Instantiation failed: {e:#}")))?;

        if let Some(snapshot_bytes) = memory_to_restore {
            if let Some(memory) = instance.get_memory(&mut store, "memory") {
                let current_size = memory.data_size(&store);
                if snapshot_bytes.len() > current_size {
                    let pages_needed =
                        ((snapshot_bytes.len() - current_size + 65535) / 65536) as u64;
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
            Err(trap) => classify_trap(&trap),
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
        };

        let payload = SnapshotPayload {
            memory_bytes: memory_snapshot,
            wasm_bytes: wasm_bytes.to_vec(),
            fs_tarball: archive_bytes,
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
    async fn execute(&self, req: TetExecutionRequest) -> Result<TetExecutionResult, TetError> {
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

        let (mem_to_restore, vfs_to_restore) = match &parent_snapshot {
            Some(p) => (Some(p.memory_bytes.as_slice()), Some(p.fs_tarball.as_slice())),
            None => (None, None),
        };

        // Pure async call — natively yields to Tokio via wasmtime async support!
        let (result, snapshot_payload) = Self::execute_inner(
            &self.engine,
            &self.mesh,
            &wasm_bytes,
            &req,
            mem_to_restore,
            vfs_to_restore,
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

    async fn snapshot(&self, tet_id: &str) -> Result<String, TetError> {
        let active = self.active_memories.read().await;
        let payload = active
            .get(tet_id)
            .ok_or_else(|| TetError::SnapshotNotFound(tet_id.to_string()))?
            .clone();
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

    async fn fork(
        &self,
        snapshot_id: &str,
        mut req: TetExecutionRequest,
    ) -> Result<TetExecutionResult, TetError> {
        req.parent_snapshot_id = Some(snapshot_id.to_string());
        self.execute(req).await
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

    if message.contains("proc_exit") {
        if message.contains("exit status 0") || message.contains("with code 0") {
            return ExecutionStatus::Success;
        }
    }

    let error_type = if message.contains("unreachable") {
        "unreachable".to_string()
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
