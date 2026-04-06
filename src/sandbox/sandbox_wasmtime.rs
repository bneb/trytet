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
use bincode::Options;

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use wasmtime::{Caller, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

pub const MAX_SNAPSHOT_SIZE: u64 = 50 * 1024 * 1024;
use wasmtime_wasi::p1::WasiP1Ctx;
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

use crate::sandbox::SnapshotPayload;

fn validate_range<'a>(
    memory: &'a wasmtime::Memory,
    caller: &'a wasmtime::Caller<'_, TetState>,
    ptr: i32,
    len: i32,
) -> wasmtime::Result<&'a [u8]> {
    let data = memory.data(caller);
    let start = (ptr as u32) as u64;
    let len_u64 = (len as u32) as u64;
    let end = start.saturating_add(len_u64);
    if end > data.len() as u64 {
        return Err(wasmtime::Error::msg("OOB Guest Memory Access"));
    }
    Ok(&data[(start as usize)..(end as usize)])
}
fn validate_range_mut<'a>(
    memory: &'a wasmtime::Memory,
    caller: &'a mut wasmtime::Caller<'_, TetState>,
    ptr: i32,
    len: i32,
) -> wasmtime::Result<&'a mut [u8]> {
    let data = memory.data_mut(caller);
    let start = (ptr as u32) as u64;
    let len_u64 = (len as u32) as u64;
    let end = start.saturating_add(len_u64);
    if end > data.len() as u64 {
        return Err(wasmtime::Error::msg("OOB Guest Memory Mut Access"));
    }
    Ok(&mut data[(start as usize)..(end as usize)])
}

pub fn production_engine_config() -> wasmtime::Config {
    let mut config = wasmtime::Config::new();
    config.consume_fuel(true);
    config.cranelift_opt_level(wasmtime::OptLevel::Speed);
    config.cranelift_nan_canonicalization(true);
    config
}

// ---------------------------------------------------------------------------
// Store State
// ---------------------------------------------------------------------------

struct TetState {
    wasi_p1: WasiP1Ctx,
    limits: StoreLimits,
    mesh: TetMesh,
    call_stack_depth: u32,
    fuel_to_burn_from_parent: u64,
    pub manifest: crate::models::manifest::AgentManifest,
    pub migration_requested: bool,
    pub migration_target: Option<String>,
    pub egress_policy: Option<crate::oracle::EgressPolicy>,
    pub vector_vfs: Arc<crate::memory::VectorVfs>,
    pub inference_engine: Arc<dyn crate::inference::NeuralEngine>,
    pub oracle: Arc<crate::oracle::MeshOracle>,
    pub oracle_cache_dir: std::path::PathBuf,
    pub model_proxy: Arc<crate::model_proxy::ModelProxy>,
    pub telemetry: Arc<crate::telemetry::TelemetryHub>,
    // Phase 17.1: Multi-Tenant Fortress
    pub tet_id: String,
    pub tenant_id: String,
    pub author_pubkey: String,
    pub quota_manager: Arc<crate::fortress::QuotaManager>,
    pub max_egress_bytes: u64,
    pub gateway: Arc<crate::gateway::SovereignGateway>,
    pub market_handle: Arc<crate::market::HiveMarket>,
}

// ---------------------------------------------------------------------------
// WasmtimeSandbox
// ---------------------------------------------------------------------------

pub struct WasmtimeSandbox {
    engine: Engine,
    snapshots: Arc<RwLock<HashMap<String, SnapshotPayload>>>,
    active_memories:
        Arc<RwLock<HashMap<String, (SnapshotPayload, crate::models::manifest::AgentManifest)>>>,
    pub mesh: TetMesh,
    pub voucher_manager: Arc<crate::economy::VoucherManager>,
    pub require_payment: bool,
    pub local_node_id: String,
    pub neural_engine: Arc<dyn crate::inference::NeuralEngine>,
    pub oracle: Arc<crate::oracle::MeshOracle>,
    pub model_proxy: Arc<crate::model_proxy::ModelProxy>,
    pub telemetry: Arc<crate::telemetry::TelemetryHub>,
    pub quota_manager: Arc<crate::fortress::QuotaManager>,
    pub gateway: Arc<crate::gateway::SovereignGateway>,
    pub market_handle: Arc<crate::market::HiveMarket>,
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
        let config = production_engine_config();

        let engine = Engine::new(&config).map_err(|e| TetError::EngineError(format!("{e:#}")))?;

        let oracle =
            Arc::new(crate::oracle::MeshOracle::new().map_err(|e| {
                TetError::EngineError(format!("Failed to initialize MeshOracle: {e}"))
            })?);

        let model_proxy = Arc::new(crate::model_proxy::ModelProxy::new(
            Arc::new(crate::model_proxy::MockInferenceProvider::new()),
            oracle.clone(),
        ));

        Ok(Self {
            engine,
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            active_memories: Arc::new(RwLock::new(HashMap::new())),
            mesh,
            voucher_manager,
            require_payment,
            local_node_id: local_node_id.clone(),
            neural_engine,
            oracle,
            model_proxy,
            telemetry: Arc::new(crate::telemetry::TelemetryHub::noop()),
            quota_manager: Arc::new(crate::fortress::QuotaManager::new()),
            gateway: Arc::new(crate::gateway::SovereignGateway::default()),
            market_handle: Arc::new(crate::market::HiveMarket::new(local_node_id)),
        })
    }

    pub fn with_gateway(mut self, gateway: Arc<crate::gateway::SovereignGateway>) -> Self {
        self.gateway = gateway;
        self
    }

    /// Set the telemetry hub on this sandbox (opt-in).
    pub fn with_telemetry(mut self, hub: Arc<crate::telemetry::TelemetryHub>) -> Self {
        self.telemetry = hub;
        self
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

    pub async fn boot_artifact(
        &self,
        wasm_bytes: &[u8],
        req: &TetExecutionRequest,
        vfs_tarball: Option<&[u8]>,
    ) -> Result<(TetExecutionResult, crate::sandbox::SnapshotPayload), TetError> {
        Self::execute_inner(
            &self.engine,
            &self.mesh,
            wasm_bytes,
            req,
            None,
            vfs_tarball,
            None,
            None,
            req.call_depth,
            self.voucher_manager.clone(),
            self.neural_engine.clone(),
            self.oracle.clone(),
            self.model_proxy.clone(),
            self.telemetry.clone(),
            self.quota_manager.clone(),
            self.gateway.clone(),
            self.market_handle.clone(),
        )
        .await
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
        call_depth: u32,
        _voucher_manager: Arc<crate::economy::VoucherManager>,
        neural_engine: Arc<dyn crate::inference::NeuralEngine>,
        oracle: Arc<crate::oracle::MeshOracle>,
        model_proxy: Arc<crate::model_proxy::ModelProxy>,
        telemetry: Arc<crate::telemetry::TelemetryHub>,
        quota_manager: Arc<crate::fortress::QuotaManager>,
        gateway: Arc<crate::gateway::SovereignGateway>,
        market_handle: Arc<crate::market::HiveMarket>,
    ) -> Result<(TetExecutionResult, SnapshotPayload), TetError> {
        if call_depth > 5 {
            return Err(TetError::CallStackExhausted);
        }

        let start = Instant::now();
        let tet_id = uuid::Uuid::new_v4().to_string();

        // Phase 16.1: Emit AgentBooted telemetry
        telemetry.broadcast(crate::telemetry::HiveEvent::AgentBooted {
            tet_id: tet_id.clone(),
            alias: req.alias.clone(),
            fuel_limit: req.allocated_fuel,
            memory_limit_mb: req.max_memory_mb,
            timestamp_us: crate::telemetry::now_us(),
        });
        let temp_dir = tempfile::tempdir()
            .map_err(|e| TetError::VfsError(format!("Failed to create isolated tempdir: {e}")))?;

        if let Some(tarball_bytes) = vfs_to_restore {
            let path_buf = temp_dir.path().to_path_buf();
            let tarball_vec = tarball_bytes.to_vec();
            tokio::task::spawn_blocking(move || {
                let mut archive = tar::Archive::new(&tarball_vec[..]);
                archive.unpack(&path_buf)
            })
            .await
            .map_err(|e| TetError::VfsError(format!("Tarball join error: {e}")))?
            .map_err(|e| TetError::VfsError(format!("Failed to unpack VFS archive: {e}")))?;
        }

        for (filename, content) in &req.injected_files {
            let safe_filename = Path::new(filename).file_name().unwrap_or_default();
            let file_path = temp_dir.path().join(safe_filename);
            fs::write(file_path, content).map_err(|e| {
                TetError::VfsError(format!("Failed to inject file '{filename}': {e}"))
            })?;
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

        // Phase 17.1: Derive tenant-namespaced Oracle cache dir
        let author_pubkey = req
            .manifest
            .as_ref()
            .and_then(|m| m.metadata.author_pubkey.as_deref())
            .unwrap_or("UNKNOWN");
        let tenant_id = crate::fortress::TenantNamespace::tenant_id(Some(author_pubkey));
        let max_egress_bytes = req
            .manifest
            .as_ref()
            .map(|m| m.constraints.max_egress_bytes)
            .unwrap_or(1_000_000);

        let oracle_cache_dir = crate::fortress::TenantNamespace::derive_cache_dir(
            temp_dir.path(),
            Some(author_pubkey),
        );
        fs::create_dir_all(&oracle_cache_dir)
            .map_err(|e| TetError::VfsError(format!("Oracle cache setup failed: {e}")))?;

        wasi_builder
            .preopened_dir(
                &oracle_cache_dir,
                "/vfs/oracle_cache",
                DirPerms::all(),
                FilePerms::all(),
            )
            .map_err(|e| TetError::EngineError(format!("VFS Oracle Cache mapping failed: {e}")))?;

        let wasi_p1_ctx = wasi_builder.build_p1();

        let limits = StoreLimitsBuilder::new()
            .memory_size(req.max_memory_mb as usize * 1024 * 1024)
            .instances(1)
            .tables(10)
            .memories(1)
            .build();

        let mut vector_vfs = crate::memory::VectorVfs::new();
        if let Some(v) = vector_to_restore {
            if let Ok(mut restored) = bincode::options()
                .with_limit(MAX_SNAPSHOT_SIZE)
                .deserialize::<crate::memory::VectorVfs>(v)
            {
                restored.rebuild_all_indexes(); // Essential for instant-distance!
                restored.start_background_worker();
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
            manifest: req.manifest.clone().unwrap_or_else(|| {
                crate::models::manifest::AgentManifest {
                    metadata: crate::models::manifest::Metadata {
                        name: req.alias.clone().unwrap_or_default(),
                        version: "0.0.0".to_string(),
                        author_pubkey: Some("UNKNOWN".to_string()),
                    },
                    constraints: crate::models::manifest::ResourceConstraints {
                        max_memory_pages: 1024,
                        fuel_limit: 1_000_000,
                        max_egress_bytes: 1_000_000,
                    },
                    permissions: crate::models::manifest::CapabilityPolicy {
                        can_egress: vec![],
                        can_persist: false,
                        can_teleport: false,
                        is_genesis_factory: false,
                        can_fork: false,
                    },
                }
            }),
            migration_requested: false,
            migration_target: None,
            egress_policy: req.manifest.as_ref().map(|m| crate::oracle::EgressPolicy {
                allowed_domains: m.permissions.can_egress.clone(),
                max_daily_bytes: 1_000_000,
                require_https: false,
            }),
            vector_vfs: Arc::new(vector_vfs),
            inference_engine: neural_engine.clone(),
            oracle: oracle.clone(),
            oracle_cache_dir,
            model_proxy: model_proxy.clone(),
            telemetry: telemetry.clone(),
            // Phase 17.1: Multi-Tenant Fortress
            tet_id: tet_id.clone(),
            tenant_id: tenant_id.clone(),
            author_pubkey: author_pubkey.to_string(),
            quota_manager: quota_manager.clone(),
            max_egress_bytes,
            gateway: gateway.clone(),
            market_handle: market_handle.clone(),
        };

        let mut store = Store::new(engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_fuel(req.allocated_fuel).unwrap();

        let mut linker: Linker<TetState> = Linker::new(engine);
        wasmtime_wasi::p1::add_to_linker_async(&mut linker, |state: &mut TetState| {
            &mut state.wasi_p1
        })
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

        let source_alias = req
            .alias
            .clone()
            .unwrap_or_else(|| "anonymous_tet".to_string());

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

                    let pkt = crate::hive::HiveCommand::TransferCredit(tx);
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

                    let pkt = crate::hive::HiveCommand::BillRequest {
                        source_alias,
                        target_alias,
                        amount,
                    };

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

                    let pkt = crate::hive::HiveCommand::WithdrawalPending(intent);

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

                    if mesh.send_reclaim(child_id).await.is_err() {
                        return Ok(6); // DISCONNECT
                    }

                    Ok(0) // Success gracefully initiated!
                })
            },
        ).map_err(|e| TetError::EngineError(format!("Failed to register trytet::reclaim: {e:#}")))?;

        let instance = linker
            .instantiate_async(&mut store, &module)
            .await
            .map_err(|e| TetError::EngineError(format!("Instantiation failed: {e:#}")))?;

        if let Some(snapshot_bytes) = memory_to_restore {
            if let Some(memory) = instance.get_memory(&mut store, "memory") {
                let current_size = memory.data_size(&store);
                if snapshot_bytes.len() > current_size {
                    let pages_needed = (snapshot_bytes.len() - current_size).div_ceil(65536) as u64;
                    memory.grow(&mut store, pages_needed).map_err(|e| {
                        TetError::EngineError(format!("Memory grow for fork failed: {e:#}"))
                    })?;
                }
                let dest = memory.data_mut(&mut store);
                dest[..snapshot_bytes.len()].copy_from_slice(snapshot_bytes);
            }
        }

        let run_result = if let Some(ref func_name) = req.target_function {
            match instance.get_typed_func::<(), ()>(&mut store, func_name) {
                Ok(start_fn) => start_fn.call_async(&mut store, ()).await,
                Err(_) => Err(wasmtime::Error::msg(format!(
                    "Target function '{}' not found or invalid signature",
                    func_name
                ))),
            }
        } else {
            match instance.get_typed_func::<(), ()>(&mut store, "_start") {
                Ok(start_fn) => start_fn.call_async(&mut store, ()).await,
                Err(_) => Ok(()), // empty default export
            }
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

        let mutated_files = Self::capture_workspace(temp_dir.path());

        let stdout_bytes = stdout_pipe.contents();
        let stderr_bytes = stderr_pipe.contents();
        let stdout_str = String::from_utf8_lossy(&stdout_bytes);
        let stderr_str = String::from_utf8_lossy(&stderr_bytes);
        let stdout_lines = if stdout_str.is_empty() {
            vec![]
        } else {
            stdout_str.lines().map(String::from).collect()
        };
        let stderr_lines = if stderr_str.is_empty() {
            vec![]
        } else {
            stderr_str.lines().map(String::from).collect()
        };

        let vector_vfs = store.data().vector_vfs.clone();
        let path_buf = temp_dir.path().to_path_buf();
        let (archive_bytes, vector_idx) = tokio::task::spawn_blocking(move || {
            let mut archive_bytes = Vec::new();
            {
                let mut builder = tar::Builder::new(&mut archive_bytes);
                let _ = builder.append_dir_all(".", path_buf);
                let _ = builder.into_inner();
            }
            let vector_idx = bincode::options()
                .with_limit(MAX_SNAPSHOT_SIZE)
                .serialize(&*vector_vfs)
                .unwrap_or_default();
            (archive_bytes, vector_idx)
        })
        .await
        .map_err(|e| TetError::EngineError(e.to_string()))?;

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

        // Phase 16.1: Emit AgentCompleted telemetry
        let status_str = match &result.status {
            ExecutionStatus::Success => "Success".to_string(),
            ExecutionStatus::OutOfFuel => "OutOfFuel".to_string(),
            ExecutionStatus::MemoryExceeded => "MemoryExceeded".to_string(),
            ExecutionStatus::Crash(r) => format!("Crash({})", r.error_type),
            ExecutionStatus::Migrated => "Migrated".to_string(),
        };
        telemetry.broadcast(crate::telemetry::HiveEvent::AgentCompleted {
            tet_id: tet_id.clone(),
            alias: req.alias.clone(),
            status: status_str,
            fuel_consumed,
            fuel_limit: req.allocated_fuel,
            memory_used_kb,
            duration_us: start.elapsed().as_micros() as u64,
            timestamp_us: crate::telemetry::now_us(),
        });

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
                let tet_id = req
                    .alias
                    .clone()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
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

        let (mem_to_restore, vfs_to_restore, vec_to_restore, inf_to_restore) =
            match &parent_snapshot {
                Some(p) => (
                    Some(p.memory_bytes.as_slice()),
                    Some(p.fs_tarball.as_slice()),
                    Some(p.vector_idx.as_slice()),
                    Some(p.inference_state.as_slice()),
                ),
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
            req.call_depth,
            self.voucher_manager.clone(),
            self.neural_engine.clone(),
            self.oracle.clone(),
            self.model_proxy.clone(),
            self.telemetry.clone(),
            self.quota_manager.clone(),
            self.gateway.clone(),
            self.market_handle.clone(),
        )
        .await?;

        // Store active memory and auto-snapshot it
        self.active_memories.write().await.insert(
            result.tet_id.clone(),
            (
                snapshot_payload.clone(),
                req.manifest
                    .clone()
                    .unwrap_or_else(|| crate::models::manifest::AgentManifest {
                        metadata: crate::models::manifest::Metadata {
                            name: req.alias.clone().unwrap_or_default(),
                            version: "0.0.0".to_string(),
                            author_pubkey: Some("UNKNOWN".to_string()),
                        },
                        constraints: crate::models::manifest::ResourceConstraints {
                            max_memory_pages: 1024,
                            fuel_limit: 1_000_000,
                            max_egress_bytes: 1_000_000,
                        },
                        permissions: crate::models::manifest::CapabilityPolicy {
                            can_egress: vec![],
                            can_persist: false,
                            can_teleport: false,
                            is_genesis_factory: false,
                            can_fork: false,
                        },
                    }),
            ),
        );

        let snapshot_id = uuid::Uuid::new_v4().to_string();
        self.snapshots
            .write()
            .await
            .insert(snapshot_id.clone(), snapshot_payload);

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
        let mut tuple_opt = active.get(id_or_alias).cloned();

        // 2. If not found, try resolving via TetMesh Registry
        if tuple_opt.is_none() {
            if let Some(target_meta) = self.mesh.resolve(id_or_alias).await {
                tuple_opt = active.get(&target_meta.tet_id).cloned();
            }
        }

        let (payload, _) =
            tuple_opt.ok_or_else(|| TetError::SnapshotNotFound(id_or_alias.to_string()))?;
        drop(active);

        let snapshot_id = uuid::Uuid::new_v4().to_string();
        self.snapshots
            .write()
            .await
            .insert(snapshot_id.clone(), payload);

        Ok(snapshot_id)
    }

    async fn export_manifest(
        &self,
        id_or_alias: &str,
    ) -> Result<crate::models::manifest::AgentManifest, TetError> {
        let active = self.active_memories.read().await;
        let mut tuple_opt = active.get(id_or_alias).cloned();
        if tuple_opt.is_none() {
            if let Some(target_meta) = self.mesh.resolve(id_or_alias).await {
                tuple_opt = active.get(&target_meta.tet_id).cloned();
            }
        }
        let (_, manifest) =
            tuple_opt.ok_or_else(|| TetError::SnapshotNotFound(id_or_alias.to_string()))?;
        Ok(manifest)
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
        self.snapshots
            .write()
            .await
            .insert(snapshot_id.clone(), payload);
        Ok(snapshot_id)
    }

    async fn query_memory(
        &self,
        alias: &str,
        query: crate::memory::SearchQuery,
    ) -> Result<Vec<crate::memory::SearchResult>, TetError> {
        let active = self.active_memories.read().await;

        let mut payload_opt = active.get(alias).cloned();

        if payload_opt.is_none() {
            if let Some(target_meta) = self.mesh.resolve(alias).await {
                payload_opt = active.get(&target_meta.tet_id).cloned();
            }
        }

        let (payload, _) =
            payload_opt.ok_or_else(|| TetError::SnapshotNotFound(alias.to_string()))?;
        drop(active);

        if payload.vector_idx.is_empty() {
            return Ok(Vec::new());
        }

        let vector_vfs: crate::memory::VectorVfs = bincode::deserialize(&payload.vector_idx)
            .map_err(|e| {
                TetError::EngineError(format!("Failed to deserialize VectorVfs: {}", e))
            })?;

        vector_vfs.rebuild_all_indexes();
        Ok(vector_vfs.recall(&query))
    }

    async fn infer(
        &self,
        _alias: &str,
        request: crate::inference::InferenceRequest,
        fuel_limit: u64,
    ) -> Result<crate::inference::InferenceResponse, TetError> {
        self.neural_engine
            .predict(&request, fuel_limit)
            .await
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
        self.mesh
            .send_call(req)
            .await
            .map_err(|e| TetError::MeshError(e.to_string()))
    }

    async fn resolve_local(&self, alias: &str) -> Option<crate::models::TetMetadata> {
        self.mesh.resolve_local(alias).await
    }

    async fn deregister(&self, alias: &str) {
        self.mesh.deregister(alias).await;
    }

    async fn publish_dht_route(
        &self,
        alias: &str,
        target_ip: &str,
        signature: &str,
    ) -> Result<(), String> {
        self.gateway
            .dht
            .update_route(alias, target_ip, signature)
            .await
            .map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Trap Classification
// ---------------------------------------------------------------------------

fn classify_trap(error: &wasmtime::Error) -> ExecutionStatus {
    let message = format!("{error:#}");

    if message.contains("out of fuel")
        || message.contains("fuel consumed")
        || message.contains("epoch")
        || message.contains("interrupt")
    {
        return ExecutionStatus::OutOfFuel;
    }

    if message.contains("memory") && (message.contains("limit") || message.contains("maximum")) {
        return ExecutionStatus::MemoryExceeded;
    }

    if message.contains("proc_exit")
        && (message.contains("exit status 0") || message.contains("with code 0"))
    {
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
