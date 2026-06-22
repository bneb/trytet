use crate::builder::{BuildError, TetArtifact};
use crate::mesh::TetMesh;
use crate::models::{TetExecutionRequest, TetExecutionResult};
use crate::sandbox::WasmtimeSandbox;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("Integrity error: {0}")]
    SecurityViolation(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Engine error: {0}")]
    Engine(String),
    #[error("Builder error: {0}")]
    Builder(#[from] BuildError),
}

pub struct ActiveAgent {
    pub result: TetExecutionResult,
}

pub struct ResurrectionContext {
    pub artifact: TetArtifact,
    pub node_workspace: PathBuf,
}

impl ResurrectionContext {
    pub async fn boot(self, fuel_override: Option<u64>) -> Result<ActiveAgent, RuntimeError> {
        let vfs_path = self.node_workspace.join("vfs");
        fs::create_dir_all(&vfs_path)?;

        // 2. VFS Decompression (Zstd)
        let decompressed_tarball = match zstd::stream::decode_all(&self.artifact.vfs_zstd[..]) {
            Ok(d) => d,
            Err(e) => {
                return Err(RuntimeError::SecurityViolation(format!(
                    "VFS decompression failed: {}",
                    e
                )))
            }
        };

        let fuel = fuel_override.unwrap_or(self.artifact.manifest.constraints.fuel_limit);

        // We evaluate requested permissions into EgressPolicy
        let policy = if !self.artifact.manifest.permissions.can_egress.is_empty() {
            Some(crate::oracle::EgressPolicy {
                allowed_domains: self.artifact.manifest.permissions.can_egress.clone(),
                require_https: true,
                max_daily_bytes: 1_000_000,
            })
        } else {
            None
        };

        let request = TetExecutionRequest {
            payload: Some(self.artifact.blueprint_wasm.clone()),
            alias: Some(self.artifact.manifest.metadata.name.clone()),
            env: std::collections::HashMap::new(),
            injected_files: std::collections::HashMap::new(),
            allocated_fuel: fuel,
            max_memory_mb: (self.artifact.manifest.constraints.max_memory_pages * 65536
                / 1024
                / 1024),
            parent_snapshot_id: None,
            call_depth: 0,
            voucher: None,
            manifest: Some(self.artifact.manifest.clone()),
            egress_policy: policy,
            target_function: None,
        };

        // 3. Wasmtime Provisioning
        let mesh = TetMesh::new(1000, crate::hive::HivePeers::default()).0;
        let voucher_manager = Arc::new(crate::economy::VoucherManager::new("local".into()));
        let sandbox = WasmtimeSandbox::new(mesh, voucher_manager, false, "local_node".into())
            .map_err(|e| RuntimeError::Engine(e.to_string()))?;

        // 5. Start Execution
        let result = sandbox
            .boot_artifact(
                &self.artifact.blueprint_wasm,
                &request,
                Some(&decompressed_tarball),
            )
            .await
            .map_err(|e| RuntimeError::Engine(e.to_string()))?;

        // 4. Map VFS to node_workspace/vfs
        // As a post-execution materialization to satisfy host-side inspection:
        for (filename, content) in &result.0.mutated_files {
            let p = vfs_path.join(filename);
            fs::write(p, content)?;
        }

        Ok(ActiveAgent { result: result.0 })
    }
}
