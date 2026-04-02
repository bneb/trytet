use serde::{Deserialize, Serialize};
use bincode::Options;
use crate::models::TetManifest;
use crate::sandbox::{SnapshotPayload, MAX_SNAPSHOT_SIZE};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;
use tracing::{info, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HiveNodeIdentity {
    pub node_id: String,
    pub public_addr: String,
    pub available_fuel: u64,
    pub total_memory_mb: u32,
    #[serde(default)]
    pub price_per_million_fuel: u64,
    #[serde(default)]
    pub min_reputation_score: u32,
    #[serde(default)]
    pub available_capacity_mb: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeleportationEnvelope {
    pub manifest: TetManifest,
    pub snapshot: SnapshotPayload,
    pub transfer_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HiveCommand {
    Join(HiveNodeIdentity),
    Pulse,
    ResolveAlias(String),
    ResolveAliasResponse(Option<crate::models::TetMetadata>),
    MigrateRequest(Box<TeleportationEnvelope>),
}

/// The local registry of known Hive peers.
#[derive(Clone)]
pub struct HivePeers {
    peers: Arc<RwLock<HashMap<String, HiveNodeIdentity>>>,
}

impl Default for HivePeers {
    fn default() -> Self {
        Self::new()
    }
}

impl HivePeers {
    pub fn new() -> Self {
        Self { peers: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn add_peer(&self, identity: HiveNodeIdentity) {
        self.peers.write().await.insert(identity.node_id.clone(), identity);
    }

    pub async fn get_peer(&self, node_id: &str) -> Option<HiveNodeIdentity> {
        self.peers.read().await.get(node_id).cloned()
    }

    pub async fn list_peers(&self) -> Vec<HiveNodeIdentity> {
        self.peers.read().await.values().cloned().collect()
    }
}

pub struct HiveServer {
    peers: HivePeers,
    // Provide a channel or callback to notify the engine of new incoming migrations
    // For now, we'll let the main loop inject it via sandbox.
}

impl HiveServer {
    pub fn new(peers: HivePeers) -> Self {
        Self { peers }
    }

    pub async fn start(
        self,
        port: u16,
        mesh: crate::mesh::TetMesh,
        sandbox: Arc<crate::sandbox::WasmtimeSandbox>,
    ) -> std::io::Result<()> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
        info!("Hive P2P Server listening securely on port {}", port);

        let peers = self.peers.clone();

        tokio::spawn(async move {
            loop {
                if let Ok((mut socket, _addr)) = listener.accept().await {
                    let p = peers.clone();
                    let m = mesh.clone();
                    let s = sandbox.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(&mut socket, p, m, s).await {
                            error!("Hive connection error: {}", e);
                        }
                    });
                }
            }
        });

        Ok(())
    }

    async fn handle_connection(
        socket: &mut TcpStream,
        peers: HivePeers,
        mesh: crate::mesh::TetMesh,
        sandbox: Arc<crate::sandbox::WasmtimeSandbox>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Read 4-byte length prefix
        let mut len_buf = [0u8; 4];
        socket.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        // Prevent absurd allocations (max 200MB)
        if len > 200 * 1024 * 1024 {
            return Err("Payload too large".into());
        }

        let mut payload = vec![0u8; len];
        socket.read_exact(&mut payload).await?;

        let command: HiveCommand = bincode::options()
            .with_limit(MAX_SNAPSHOT_SIZE)
            .with_fixint_encoding()
            .allow_trailing_bytes()
            .deserialize(&payload)?;

        match command {
            HiveCommand::Join(identity) => {
                info!("Hive Node joined: {} ({})", identity.node_id, identity.public_addr);
                peers.add_peer(identity).await;
                // Send an Ack (length 0 for simple OK)
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Pulse => {
                // Sent back to keep connection alive
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::ResolveAlias(alias) => {
                // Return our local TetMetadata if we have it!
                let local_meta = mesh.resolve_local(&alias).await;
                let response = HiveCommand::ResolveAliasResponse(local_meta);
                let response_bytes = bincode::serialize(&response)?;
                let res_len = response_bytes.len() as u32;
                socket.write_all(&res_len.to_be_bytes()).await?;
                socket.write_all(&response_bytes).await?;
            }
            HiveCommand::ResolveAliasResponse(_) => {
                // Used by client mapping
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::MigrateRequest(envelope_box) => {
                let envelope = *envelope_box;
                info!("Received Live Migration: Tet {}", envelope.manifest.name);
                
                use crate::engine::TetSandbox;
                // 1. Import snapshot correctly
                match sandbox.import_snapshot(envelope.snapshot).await {
                    Ok(snap_id) => {
                        let alias = envelope.manifest.name.clone();
                        // Auto-invoke the agent so it takes over locally
                        let sandbox_clone = sandbox.clone();
                        tokio::spawn(async move {
                            let req = crate::models::TetExecutionRequest {
                                alias: Some(alias),
                                payload: None, // loaded from snapshot
                                parent_snapshot_id: Some(snap_id.clone()),
                                allocated_fuel: 50_000_000,
                                max_memory_mb: 64,
                                env: std::collections::HashMap::new(),
                                injected_files: std::collections::HashMap::new(),
                                call_depth: 0,
                                voucher: None,
                                egress_policy: None,
                            };
                            let _ = sandbox_clone.fork(&snap_id, req).await;
                        });

                        socket.write_all(&0u32.to_be_bytes()).await?;
                    }
                    Err(e) => {
                        error!("Failed to import teleported payload: {:?}", e);
                        socket.write_all(&0u32.to_be_bytes()).await?;
                    }
                }
            }
        }

        Ok(())
    }
}

pub struct HiveClient;

impl HiveClient {
    pub async fn send_command(target_addr: &str, command: HiveCommand) -> Result<(), Box<dyn std::error::Error>> {
        let mut socket = TcpStream::connect(target_addr).await?;
        
        let payload = bincode::serialize(&command)?;
        let len = payload.len() as u32;

        socket.write_all(&len.to_be_bytes()).await?;
        socket.write_all(&payload).await?;

        Ok(())
    }

    pub async fn rpc_call(target_addr: &str, command: HiveCommand) -> Result<HiveCommand, Box<dyn std::error::Error>> {
        let mut socket = TcpStream::connect(target_addr).await?;
        
        let payload = bincode::serialize(&command)?;
        let len = payload.len() as u32;

        socket.write_all(&len.to_be_bytes()).await?;
        socket.write_all(&payload).await?;

        let mut len_buf = [0u8; 4];
        socket.read_exact(&mut len_buf).await?;
        let res_len = u32::from_be_bytes(len_buf) as usize;
        
        if res_len > 200 * 1024 * 1024 { return Err("Payload too large".into()); }
        
        if res_len > 0 {
            let mut res_payload = vec![0u8; res_len];
            socket.read_exact(&mut res_payload).await?;
            let response: HiveCommand = bincode::options()
                .with_limit(MAX_SNAPSHOT_SIZE)
                .with_fixint_encoding()
                .allow_trailing_bytes()
                .deserialize(&res_payload)?;
            Ok(response)
        } else {
            Ok(HiveCommand::Pulse) // empty ack
        }
    }
}
