use serde::{Deserialize, Serialize};
use crate::models::manifest::AgentManifest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MigrationPacket {
    Handshake { manifest: AgentManifest, snapshot_id: String },
    Payload { chunk: Vec<u8>, sequence: u32 },
    Commit { signature: Vec<u8> },
}
