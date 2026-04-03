use crate::hive::{HiveClient, HiveCommand, link::MigrationPacket};
use anyhow::{Result, anyhow};
use std::sync::Arc;
use tracing::info;

pub struct TeleportRequest {
    pub agent_id: String, // alias or id
    pub target_address: String,
    pub use_registry: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TeleportReceipt {
    pub agent_id: String,
    pub target_address: String,
    pub bytes_transferred: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum TeleportError {
    #[error("Permission denied: can_teleport is false in manifest")]
    PermissionDenied,
    #[error("Target node error: {0}")]
    TargetError(String),
    #[error("Network error: {0}")]
    Network(#[from] anyhow::Error),
}

use crate::engine::TetSandbox;

impl TeleportRequest {
    pub async fn execute(
        &self, 
        sandbox: Arc<dyn TetSandbox>,
        registry_client: Option<Arc<crate::registry::oci::OciClient>>
    ) -> Result<TeleportReceipt, TeleportError> {
        // 1. Resolve agent and check capabilities
        let _ = sandbox.resolve_local(&self.agent_id).await
            .ok_or_else(|| TeleportError::Network(anyhow!("Agent not found locally")))?;
            
        // We need the manifest to check permissions. 
        // In this architecture, it should be in the artifact or metadata.
        // Assuming we can retrieve it from the sandbox's active memories/snapshots.
        let snapshot_id = sandbox.snapshot(&self.agent_id).await
            .map_err(|e| TeleportError::Network(anyhow!("Failed to snapshot agent: {:?}", e)))?;
            
        let payload = sandbox.export_snapshot(&snapshot_id).await
            .map_err(|e| TeleportError::Network(anyhow!("Failed to export snapshot: {:?}", e)))?;
            
        let manifest = sandbox.export_manifest(&self.agent_id).await
            .map_err(|e| TeleportError::Network(anyhow!("Failed to export manifest: {:?}", e)))?;

        if !manifest.permissions.can_teleport {
            return Err(TeleportError::PermissionDenied);
        }

        info!("Teleportation sequence initiated for agent: {}", self.agent_id);

        let mut bytes_transferred = 0;

        if self.use_registry {
            let _registry = registry_client.ok_or_else(|| TeleportError::Network(anyhow::anyhow!("Registry client not configured")))?;
            // Option B: Registry logic
            return Err(TeleportError::Network(anyhow!("Registry-based teleport not yet fully implemented")));
        } else {
            // Option A: Direct P2P Streaming
            HiveClient::send_command(&self.target_address, HiveCommand::MigrationPacket(MigrationPacket::Handshake {
                manifest: manifest.clone(),
                snapshot_id: snapshot_id.clone(),
            })).await.map_err(|e| TeleportError::TargetError(e.to_string()))?;

            let total_payload = bincode::serialize(&payload)
                .map_err(|e| TeleportError::Network(anyhow!("Failed to serialize payload: {}", e)))?;
            
            let chunk_size = 64 * 1024; // 64KB chunks
            let mut sequence = 0;
            for chunk in total_payload.chunks(chunk_size) {
                HiveClient::send_command(&self.target_address, HiveCommand::MigrationPacket(MigrationPacket::Payload {
                    chunk: chunk.to_vec(),
                    sequence,
                })).await.map_err(|e| TeleportError::TargetError(e.to_string()))?;
                bytes_transferred += chunk.len() as u64;
                sequence += 1;
            }

            HiveClient::send_command(&self.target_address, HiveCommand::MigrationPacket(MigrationPacket::Commit {
                signature: vec![],
            })).await.map_err(|e| TeleportError::TargetError(e.to_string()))?;
        }

        // 4. Purge local if successful
        sandbox.deregister(&self.agent_id).await;
        
        Ok(TeleportReceipt {
            agent_id: self.agent_id.clone(),
            target_address: self.target_address.clone(),
            bytes_transferred,
        })
    }
}
