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
    CrashReport, ExecutionStatus, StructuredTelemetry, TetExecutionRequest,
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
use wasmtime::{Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

pub const MAX_SNAPSHOT_SIZE: u64 = 50 * 1024 * 1024;
use wasmtime_wasi::p1::WasiP1Ctx;
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

use crate::sandbox::SnapshotPayload;

pub(crate) fn validate_range<'a>(
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
pub(crate) fn validate_range_mut<'a>(
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

pub struct TetState {
    pub wasi_p1: WasiP1Ctx,
    pub limits: StoreLimits,
    pub mesh: TetMesh,
    pub call_stack_depth: u32,
    pub fuel_to_burn_from_parent: u64,
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
    // Phase 33.1: Neuro-Symbolic Cartridge Substrate
    pub cartridge_manager: Arc<crate::cartridge::CartridgeManager>,
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
    // Phase 33.1: Neuro-Symbolic Cartridge Substrate
    pub cartridge_manager: Arc<crate::cartridge::CartridgeManager>,
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

        // Phase 33.1: Create CartridgeManager sharing the same Engine
        let cartridge_manager = Arc::new(crate::cartridge::CartridgeManager::new(&engine));

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
            cartridge_manager,
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
            self.cartridge_manager.clone(),
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
        cartridge_manager: Arc<crate::cartridge::CartridgeManager>,
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
        tokio::fs::create_dir_all(&oracle_cache_dir).await
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
            // Phase 33.1: Neuro-Symbolic Cartridge Substrate
            cartridge_manager,
        };

        let mut store = Store::new(engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_fuel(req.allocated_fuel).unwrap();

        let mut linker: Linker<TetState> = Linker::new(engine);
        wasmtime_wasi::p1::add_to_linker_async(&mut linker, |state: &mut TetState| {
            &mut state.wasi_p1
        })
        .map_err(|e| TetError::EngineError(format!("WASI linking failed: {e:#}")))?;

        let source_alias = req
            .alias
            .clone()
            .unwrap_or_else(|| "anonymous_tet".to_string());

        crate::sandbox::host_api::register_host_functions(&mut linker, source_alias)?;

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
            ExecutionStatus::Suspended => "Suspended".to_string(),
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
            self.cartridge_manager.clone(),
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

        // Remove old snapshot entry to prevent time-travel bugs
        if let Some(parent_id) = &req.parent_snapshot_id {
            if let Some(alias) = &req.alias {
                self.mesh.remove_by_snapshot(alias, parent_id).await;
            }
        }

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

    if message.contains("TET_SUSPEND") {
        return ExecutionStatus::Suspended;
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
