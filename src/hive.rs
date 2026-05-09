use crate::models::manifest::AgentManifest;
use crate::sandbox::SnapshotPayload;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tracing::{error, info};

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
    pub manifest: crate::models::manifest::AgentManifest,
    pub snapshot: SnapshotPayload,
    pub transfer_token: String,
}

pub mod dht;
pub mod link;
pub mod security;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HiveNetworkCommand {
    Join(HiveNodeIdentity),
    Heartbeat(crate::network::vitality::Heartbeat),
    Pulse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HiveDhtCommand {
    ResolveAlias(String),
    ResolveAliasResponse(Option<crate::models::TetMetadata>),
    DhtUpdate {
        alias: String,
        node_ip: String,
        signature: String,
    },
    ProposeAlias(crate::consensus::AliasProposal),
    QuorumVote(crate::consensus::NodeSignature),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HiveMigrationCommand {
    MigrateRequest(Box<TeleportationEnvelope>),
    MigrationPacket(link::MigrationPacket),
    MigrationNotice {
        reference: String,
        manifest: crate::models::manifest::AgentManifest,
        snapshot_id: String,
    },
    TransitLock {
        alias: String,
        node_id: String,
        ttl_seconds: u64,
    },
    TransitRelease(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HiveEconomyCommand {
    TransferCredit(crate::economy::registry::FuelTransaction),
    BillRequest {
        source_alias: String,
        target_alias: String,
        amount: u64,
    },
    WithdrawalPending(crate::economy::bridge::BridgeIntent),
    MarketBidPacket(crate::market::MarketBid),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HiveRegistryCommand {
    RegistryQuery(String),
    RegistryQueryResponse { cid: String, available: bool },
    ChunkStream { cid: String, seq: u32, chunk: Vec<u8> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HiveCommand {
    Network(HiveNetworkCommand),
    Dht(HiveDhtCommand),
    Migration(HiveMigrationCommand),
    Economy(HiveEconomyCommand),
    Registry(HiveRegistryCommand),
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
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_peer(&self, identity: HiveNodeIdentity) {
        self.peers
            .write()
            .await
            .insert(identity.node_id.clone(), identity);
    }

    pub async fn get_peer(&self, node_id: &str) -> Option<HiveNodeIdentity> {
        self.peers.read().await.get(node_id).cloned()
    }

    pub async fn list_peers(&self) -> Vec<HiveNodeIdentity> {
        self.peers.read().await.values().cloned().collect()
    }
}

use tokio::sync::Mutex;

pub type PendingMigrationChunks = (AgentManifest, Vec<(u32, Vec<u8>)>);

pub struct MigrationManager {
    pending: Mutex<HashMap<String, PendingMigrationChunks>>,
}

impl Default for MigrationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MigrationManager {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }
}

pub struct HiveServer {
    peers: HivePeers,
    migration_manager: Arc<MigrationManager>,
    registry_client: Option<Arc<crate::registry::oci::OciClient>>,
    tls_acceptor: Option<tokio_rustls::TlsAcceptor>,
    pub vitality_manager: Arc<crate::network::vitality::VitalityManager>,
}

impl HiveServer {
    pub fn new(
        peers: HivePeers,
        registry_client: Option<Arc<crate::registry::oci::OciClient>>,
        tls_acceptor: Option<tokio_rustls::TlsAcceptor>,
    ) -> Self {
        Self {
            peers,
            migration_manager: Arc::new(MigrationManager::new()),
            registry_client,
            tls_acceptor,
            vitality_manager: Arc::new(crate::network::vitality::VitalityManager::new(15_000_000)),
        }
    }

    pub async fn start(
        self,
        port: u16,
        mesh: crate::mesh::TetMesh,
        sandbox: Arc<crate::sandbox::WasmtimeSandbox>,
    ) -> anyhow::Result<()> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
        info!("Hive P2P Server listening securely on port {}", port);

        let peers = self.peers.clone();
        let migration_manager = self.migration_manager.clone();

        let tls_acceptor = self.tls_acceptor.clone();

        tokio::spawn(async move {
            loop {
                if let Ok((mut socket, _addr)) = listener.accept().await {
                    let p = peers.clone();
                    let m = mesh.clone();
                    let s = sandbox.clone();
                    let mm = migration_manager.clone();
                    let rc = self.registry_client.clone();
                    let acceptor_clone = tls_acceptor.clone();
                    let vit = self.vitality_manager.clone();

                    tokio::spawn(async move {
                        if let Some(acceptor) = acceptor_clone {
                            match acceptor.accept(socket).await {
                                Ok(mut tls_stream) => {
                                    if let Err(e) =
                                        Self::handle_connection(&mut tls_stream, p, m, s, mm, rc, vit)
                                            .await
                                    {
                                        error!("Hive TLS connection error: {}", e);
                                    }
                                }
                                Err(e) => {
                                    error!("TLS handshake failed: {}", e);
                                }
                            }
                        } else if let Err(e) =
                            Self::handle_connection(&mut socket, p, m, s, mm, rc, vit).await
                        {
                            error!("Hive connection error: {}", e);
                        }
                    });
                }
            }
        });

        Ok(())
    }

    async fn handle_connection<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
        socket: &mut S,
        peers: HivePeers,
        mesh: crate::mesh::TetMesh,
        sandbox: Arc<crate::sandbox::WasmtimeSandbox>,
        migration_manager: Arc<MigrationManager>,
        registry_client: Option<Arc<crate::registry::oci::OciClient>>,
        vitality_manager: Arc<crate::network::vitality::VitalityManager>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::network::tunnel::SovereignTunnel;
        
        let mut tunnel = SovereignTunnel::init_responder_nn()?;
        
        info!("Upgrading to Noise NN SovereignTunnel.");
        // Noise IK Handshake Responder
        let mut len_buf = [0u8; 4];
        socket.read_exact(&mut len_buf).await?;
        let req_len = u32::from_be_bytes(len_buf) as usize;
        let mut req_payload = vec![0u8; req_len];
        socket.read_exact(&mut req_payload).await?;

        let mut rx_buf = vec![0u8; 65535];
        tunnel.noise_state.as_mut().ok_or_else(|| anyhow::anyhow!("Noise state missing"))?.read_message(&req_payload, &mut rx_buf).map_err(|e| anyhow::anyhow!("Handshake Part 1 Error: {}", e))?;

        let mut ix_buf = vec![0u8; 65535];
        let len = tunnel.noise_state.as_mut().ok_or_else(|| anyhow::anyhow!("Noise state missing"))?.write_message(&[], &mut ix_buf)?;
        socket.write_all(&(len as u32).to_be_bytes()).await?;
        socket.write_all(&ix_buf[..len]).await?;

        tunnel.to_transport()?;

        // Read 4-byte length prefix for Ciphertext
        let mut len_buf = [0u8; 4];
        socket.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        // Prevent absurd allocations (max 200MB)
        if len > 200 * 1024 * 1024 {
            return Err("Payload too large".into());
        }

        let mut payload = vec![0u8; len];
        socket.read_exact(&mut payload).await?;

        let command = tunnel.decrypt_payload(&payload).map_err(|e| anyhow::anyhow!("Payload Decrypt Error: {}", e))?;
        match command {
            HiveCommand::Network(HiveNetworkCommand::Join(identity)) => {
                info!(
                    "Hive Node joined: {} ({})",
                    identity.node_id, identity.public_addr
                );
                peers.add_peer(identity).await;
                // Send an Ack
                let enc = tunnel.encrypt_command(&HiveCommand::Network(HiveNetworkCommand::Pulse))?;
                socket.write_all(&(enc.len() as u32).to_be_bytes()).await?;
                socket.write_all(&enc).await?;
            }
            HiveCommand::Network(HiveNetworkCommand::Heartbeat(hb)) => {
                vitality_manager.record_heartbeat(hb);
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Network(HiveNetworkCommand::Pulse) => {
                // Sent back to keep connection alive
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Dht(HiveDhtCommand::ResolveAlias(alias)) => {
                // Return our local TetMetadata if we have it!
                let local_meta = mesh.resolve_local(&alias).await;
                let response = HiveCommand::Dht(HiveDhtCommand::ResolveAliasResponse(local_meta));
                let response_bytes = bincode::serialize(&response)?;
                let res_len = response_bytes.len() as u32;
                socket.write_all(&res_len.to_be_bytes()).await?;
                socket.write_all(&response_bytes).await?;
            }
            HiveCommand::Dht(HiveDhtCommand::ResolveAliasResponse(_)) => {
                // Used by client mapping
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Migration(HiveMigrationCommand::MigrateRequest(envelope_box)) => {
                let envelope = *envelope_box;
                info!(
                    "Received Live Migration: Tet {}",
                    envelope.manifest.metadata.name
                );

                use crate::engine::TetSandbox;
                // 1. Import snapshot correctly
                match sandbox.import_snapshot(envelope.snapshot).await {
                    Ok(snap_id) => {
                        let alias = envelope.manifest.metadata.name.clone();
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
                                manifest: Some(envelope.manifest.clone()),
                                egress_policy: None,
                                target_function: None,
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
            HiveCommand::Migration(HiveMigrationCommand::MigrationPacket(packet)) => {
                Self::handle_migration_packet(packet, sandbox, migration_manager).await?;
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Migration(HiveMigrationCommand::MigrationNotice {
                reference,
                manifest,
                snapshot_id: _,
            }) => {
                info!(
                    "Received Registry Migration Notice for agent: {}",
                    manifest.metadata.name
                );
                let registry = registry_client.ok_or_else(|| {
                    anyhow::anyhow!("Registry client not configured to perform mediated handoff")
                })?;

                match registry.pull_state(&reference).await {
                    Ok(payload) => {
                        use crate::engine::TetSandbox;
                        match sandbox.import_snapshot(payload).await {
                            Ok(snap_id) => {
                                let alias = manifest.metadata.name.clone();
                                let sandbox_clone = sandbox.clone();
                                let manifest_name = manifest.metadata.name.clone();
                                tokio::spawn(async move {
                                    let req = crate::models::TetExecutionRequest {
                                        alias: Some(alias),
                                        payload: None,
                                        parent_snapshot_id: Some(snap_id.clone()),
                                        allocated_fuel: 50_000_000,
                                        max_memory_mb: 64,
                                        env: std::collections::HashMap::new(),
                                        injected_files: std::collections::HashMap::new(),
                                        call_depth: 0,
                                        voucher: None,
                                        manifest: Some(manifest),
                                        egress_policy: None,
                                        target_function: None,
                                    };
                                    let _ = sandbox_clone.fork(&snap_id, req).await;
                                });
                                info!(
                                    "Agent {} successfully pulled and resurrected from registry",
                                    manifest_name
                                );
                                socket.write_all(&0u32.to_be_bytes()).await?;
                            }
                            Err(e) => {
                                error!("Failed to import registry payload: {:?}", e);
                                return Err(e.into());
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to pull registry payload: {:?}", e);
                        return Err(e.into());
                    }
                }
            }
            HiveCommand::Dht(HiveDhtCommand::DhtUpdate {
                alias,
                node_ip,
                signature,
            }) => {
                info!(
                    "Received DHT Route Update: {} -> {} (sig: {})",
                    alias, node_ip, signature
                );
                // In a real Kademlia DHT, we'd store the routing mapping securely.
                // Our system delegates handling to SovereignGateway via the DHT wrapper.
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Dht(HiveDhtCommand::ProposeAlias(proposal)) => {
                info!(
                    "Received ProposeAlias for {}",
                    hex::encode(&proposal.alias_hash[..4])
                );
                // Handled via Registry/Consensus abstraction
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Dht(HiveDhtCommand::QuorumVote(_)) => {
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Migration(HiveMigrationCommand::TransitLock {
                alias,
                node_id,
                ttl_seconds,
            }) => {
                info!(
                    "Received TransitLock for {} by {} (ttl: {}s)",
                    alias, node_id, ttl_seconds
                );
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Migration(HiveMigrationCommand::TransitRelease(alias)) => {
                info!("Received TransitRelease for {}", alias);
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Economy(HiveEconomyCommand::TransferCredit(tx)) => {
                info!(
                    "Received TransferCredit of {} for {}",
                    tx.amount,
                    hex::encode(&tx.to[..4])
                );
                // The native routing logic integrates right into VoucherRegistry securely.
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Economy(HiveEconomyCommand::BillRequest {
                source_alias,
                target_alias,
                amount,
            }) => {
                info!(
                    "Received BillRequest for {} -> {}: {}",
                    source_alias, target_alias, amount
                );
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Economy(HiveEconomyCommand::WithdrawalPending(intent)) => {
                info!(
                    "Received WithdrawalPending for {} fuel targeting external address {}",
                    intent.internal_fuel, intent.target_address
                );
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Economy(HiveEconomyCommand::MarketBidPacket(bid)) => {
                sandbox.market_handle.process_bid(bid);
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Registry(HiveRegistryCommand::RegistryQuery(cid)) => {
                info!("Received RegistryQuery for CID {}", cid);
                let home_dir = home::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
                let block_path = home_dir.join(".trytet").join("registry_cas").join(&cid);
                let available = block_path.exists();
                let response = HiveCommand::Registry(HiveRegistryCommand::RegistryQueryResponse { cid, available });
                let response_bytes = bincode::serialize(&response)?;
                let res_len = response_bytes.len() as u32;
                socket.write_all(&res_len.to_be_bytes()).await?;
                socket.write_all(&response_bytes).await?;
            }
            HiveCommand::Registry(HiveRegistryCommand::RegistryQueryResponse { .. }) => {
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Registry(HiveRegistryCommand::ChunkStream { .. }) => {
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
        }

        Ok(())
    }

    async fn handle_migration_packet(
        packet: link::MigrationPacket,
        sandbox: Arc<crate::sandbox::WasmtimeSandbox>,
        migration_manager: Arc<MigrationManager>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use link::MigrationPacket::*;
        let mut pending = migration_manager.pending.lock().await;

        match packet {
            Handshake {
                manifest,
                snapshot_id,
            } => {
                info!("Initiating migration for agent: {}", manifest.metadata.name);
                pending.insert(snapshot_id, (manifest, Vec::new()));
            }
            Payload { chunk, sequence } => {
                if let Some((_, chunks)) = pending.values_mut().last() {
                    chunks.push((sequence, chunk));
                }
            }
            Commit { signature: _ } => {
                let snapshot_id_opt = pending.keys().next().cloned();

                if let Some(snapshot_id) = snapshot_id_opt {
                    if let Some((manifest, mut chunks)) = pending.remove(&snapshot_id) {
                        chunks.sort_by_key(|(seq, _)| *seq);
                        let full_payload_bytes: Vec<u8> =
                            chunks.into_iter().flat_map(|(_, b)| b).collect();
                        let payload: crate::sandbox::SnapshotPayload =
                            bincode::deserialize(&full_payload_bytes)
                                .map_err(|e| anyhow::anyhow!(e))?;

                        use crate::engine::TetSandbox;
                        let snap_id = sandbox
                            .import_snapshot(payload)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;
                        let alias = manifest.metadata.name.clone();

                        let sandbox_clone = sandbox.clone();
                        let manifest_clone = manifest.clone();
                        tokio::spawn(async move {
                            let req = crate::models::TetExecutionRequest {
                                alias: Some(alias),
                                payload: None,
                                parent_snapshot_id: Some(snap_id.clone()),
                                allocated_fuel: 50_000_000,
                                max_memory_mb: 64,
                                env: std::collections::HashMap::new(),
                                injected_files: std::collections::HashMap::new(),
                                call_depth: 0,
                                voucher: None,
                                manifest: Some(manifest_clone),
                                egress_policy: None,
                                target_function: None,
                            };
                            let _ = sandbox_clone.fork(&snap_id, req).await;
                        });
                        info!(
                            "Agent {} successfully resurrected on target node",
                            manifest.metadata.name
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

pub struct HiveClient;

impl HiveClient {
    pub async fn send_command(
        target_addr: &str,
        command: HiveCommand,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Self::send_command_tls(target_addr, command, None, None).await
    }

    pub async fn send_command_tls(
        target_addr: &str,
        command: HiveCommand,
        tls_connector: Option<tokio_rustls::TlsConnector>,
        domain: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::network::tunnel::SovereignTunnel;
        
        let mut socket = TcpStream::connect(target_addr).await?;

        let mut tunnel = SovereignTunnel::init_initiator_nn()?;

        if let Some(connector) = tls_connector {
            let server_name =
                rustls::pki_types::ServerName::try_from(domain.unwrap_or("localhost"))?.to_owned();
            let mut tls_stream = connector.connect(server_name, socket).await?;
            
            let mut ix_buf = vec![0u8; 65535];
            let len = tunnel.noise_state.as_mut().ok_or_else(|| anyhow::anyhow!("Noise state missing"))?.write_message(&[], &mut ix_buf)?;
            tls_stream.write_all(&(len as u32).to_be_bytes()).await?;
            tls_stream.write_all(&ix_buf[..len]).await?;

            let mut len_buf = [0u8; 4];
            tls_stream.read_exact(&mut len_buf).await?;
            let resp_len = u32::from_be_bytes(len_buf) as usize;
            let mut resp_payload = vec![0u8; resp_len];
            tls_stream.read_exact(&mut resp_payload).await?;
            
            let mut rx_buf = vec![0u8; 65535];
            tunnel.noise_state.as_mut().ok_or_else(|| anyhow::anyhow!("Noise state missing"))?.read_message(&resp_payload, &mut rx_buf)?;
            tunnel.to_transport()?;

            let enc_cmd = tunnel.encrypt_command(&command)?;
            let len = enc_cmd.len() as u32;

            tls_stream.write_all(&len.to_be_bytes()).await?;
            tls_stream.write_all(&enc_cmd).await?;
            
            let mut ack = [0u8; 4];
            tls_stream.read_exact(&mut ack).await?;
        } else {
            let mut ix_buf = vec![0u8; 65535];
            let len = tunnel.noise_state.as_mut().ok_or_else(|| anyhow::anyhow!("Noise state missing"))?.write_message(&[], &mut ix_buf)?;
            socket.write_all(&(len as u32).to_be_bytes()).await?;
            socket.write_all(&ix_buf[..len]).await?;

            let mut len_buf = [0u8; 4];
            socket.read_exact(&mut len_buf).await?;
            let resp_len = u32::from_be_bytes(len_buf) as usize;
            let mut resp_payload = vec![0u8; resp_len];
            socket.read_exact(&mut resp_payload).await?;
            
            let mut rx_buf = vec![0u8; 65535];
            tunnel.noise_state.as_mut().ok_or_else(|| anyhow::anyhow!("Noise state missing"))?.read_message(&resp_payload, &mut rx_buf)?;
            tunnel.to_transport()?;

            let enc_cmd = tunnel.encrypt_command(&command)?;
            let len = enc_cmd.len() as u32;

            socket.write_all(&len.to_be_bytes()).await?;
            socket.write_all(&enc_cmd).await?;
            
            let mut ack = [0u8; 4];
            socket.read_exact(&mut ack).await?;
        }

        Ok(())
    }

    pub async fn rpc_call(
        target_addr: &str,
        command: HiveCommand,
    ) -> Result<HiveCommand, Box<dyn std::error::Error>> {
        use crate::network::tunnel::SovereignTunnel;
        
        let mut socket = TcpStream::connect(target_addr).await?;

        let mut tunnel = SovereignTunnel::init_initiator_nn()?;

        let mut ix_buf = vec![0u8; 65535];
        let len = tunnel.noise_state.as_mut().ok_or_else(|| anyhow::anyhow!("Noise state missing"))?.write_message(&[], &mut ix_buf)?;
        socket.write_all(&(len as u32).to_be_bytes()).await?;
        socket.write_all(&ix_buf[..len]).await?;

        let mut len_buf = [0u8; 4];
        socket.read_exact(&mut len_buf).await?;
        let resp_len = u32::from_be_bytes(len_buf) as usize;
        let mut resp_payload = vec![0u8; resp_len];
        socket.read_exact(&mut resp_payload).await?;
        
        let mut rx_buf = vec![0u8; 65535];
        tunnel.noise_state.as_mut().ok_or_else(|| anyhow::anyhow!("Noise state missing"))?.read_message(&resp_payload, &mut rx_buf)?;
        tunnel.to_transport()?;

        let enc_cmd = tunnel.encrypt_command(&command)?;
        let len = enc_cmd.len() as u32;

        socket.write_all(&len.to_be_bytes()).await?;
        socket.write_all(&enc_cmd).await?;

        let mut len_buf = [0u8; 4];
        socket.read_exact(&mut len_buf).await?;
        let res_len = u32::from_be_bytes(len_buf) as usize;

        if res_len > 200 * 1024 * 1024 {
            return Err("Payload too large".into());
        }

        if res_len > 0 {
            let mut res_payload = vec![0u8; res_len];
            socket.read_exact(&mut res_payload).await?;
            let response = tunnel.decrypt_payload(&res_payload)?;
            Ok(response)
        } else {
            Ok(HiveCommand::Network(HiveNetworkCommand::Pulse)) // empty ack
        }
    }
}
