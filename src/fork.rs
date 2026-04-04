use crate::sandbox::WasmtimeSandbox;
use crate::models::{TetExecutionRequest, TetExecutionResult, MeshCallRequest};
use crate::sandbox::SnapshotPayload;
use std::sync::Arc;
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum ForkError {
    #[error("Engine error: {0}")]
    Engine(String),
    #[error("Network error: {0}")]
    Network(String),
}

pub struct ForkRequest {
    pub parent_id: String,
    pub child_fuel: u64,
    pub target_node: Option<String>,
    pub memory_snapshot: Vec<u8>,
    pub alias: String,
    pub manifest: crate::models::manifest::AgentManifest,
}

impl ForkRequest {
    pub async fn execute(
        &self, 
        mesh: &crate::mesh::TetMesh,
        gateway: &crate::gateway::SovereignGateway,
        vfs_layer: Arc<crate::memory::VectorVfs>,
    ) -> Result<String, ForkError> {
        let child_tet_id = uuid::Uuid::new_v4().to_string();
        
        let exec_req = TetExecutionRequest {
            payload: Some(self.memory_snapshot.clone()),
            alias: Some(self.alias.clone()),
            allocated_fuel: self.child_fuel,
            max_memory_mb: self.manifest.constraints.max_memory_pages as u32 * 64 / 1024,
            env: HashMap::new(),
            injected_files: HashMap::new(),
            parent_snapshot_id: None,
            call_depth: 0,
            voucher: None,
            manifest: Some(self.manifest.clone()),
            egress_policy: None,
            target_function: None,
        };

        if let Some(target) = &self.target_node {
            // Forward fork to target node
            // Not MVP required for remote if local works, but we can do a MeshCallRequest 
            // In a real system, we'd send the full ExecReq over network.
            return Err(ForkError::Network("Remote fork not implemented yet".to_string()));
        } else {
            // Fork locally via Mesh (mesh worker will pick it up and execute it)
            // Wait, TetMesh doesn't have an enqueue_execution natively. 
            // We can wrap it as a MeshCallRequest? No, we need it to boot as a fresh sibling!
            // Actually, we can just send it to SovereignGateway or Mesh Worker if they had an internal channel.
            // Better: WasmtimeSandbox has `execute`, but we only have `mesh` here.
            // Let's pass the request over Mesh!
            return Err(ForkError::Engine("ForkRequest requires a direct engine pointer".into()));
        }
    }
}
