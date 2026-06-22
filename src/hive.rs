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

pub mod connection;
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
    RegistryQueryResponse {
        cid: String,
        available: bool,
    },
    ChunkStream {
        cid: String,
        seq: u32,
        chunk: Vec<u8>,
    },
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
                                    if let Err(e) = Self::handle_connection(
                                        &mut tls_stream,
                                        p,
                                        m,
                                        s,
                                        mm,
                                        rc,
                                        vit,
                                    )
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
        use crate::network::tunnel::Tunnel;

        let mut socket = TcpStream::connect(target_addr).await?;

        let mut tunnel = Tunnel::init_initiator_nn()?;

        if let Some(connector) = tls_connector {
            let server_name =
                rustls::pki_types::ServerName::try_from(domain.unwrap_or("localhost"))?.to_owned();
            let mut tls_stream = connector.connect(server_name, socket).await?;

            let mut ix_buf = vec![0u8; 65535];
            let len = tunnel
                .noise_state
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("Noise state missing"))?
                .write_message(&[], &mut ix_buf)?;
            tls_stream.write_all(&(len as u32).to_be_bytes()).await?;
            tls_stream.write_all(&ix_buf[..len]).await?;

            let mut len_buf = [0u8; 4];
            tls_stream.read_exact(&mut len_buf).await?;
            let resp_len = u32::from_be_bytes(len_buf) as usize;
            let mut resp_payload = vec![0u8; resp_len];
            tls_stream.read_exact(&mut resp_payload).await?;

            let mut rx_buf = vec![0u8; 65535];
            tunnel
                .noise_state
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("Noise state missing"))?
                .read_message(&resp_payload, &mut rx_buf)?;
            tunnel.to_transport()?;

            let enc_cmd = tunnel.encrypt_command(&command)?;
            let len = enc_cmd.len() as u32;

            tls_stream.write_all(&len.to_be_bytes()).await?;
            tls_stream.write_all(&enc_cmd).await?;

            let mut ack = [0u8; 4];
            tls_stream.read_exact(&mut ack).await?;
        } else {
            let mut ix_buf = vec![0u8; 65535];
            let len = tunnel
                .noise_state
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("Noise state missing"))?
                .write_message(&[], &mut ix_buf)?;
            socket.write_all(&(len as u32).to_be_bytes()).await?;
            socket.write_all(&ix_buf[..len]).await?;

            let mut len_buf = [0u8; 4];
            socket.read_exact(&mut len_buf).await?;
            let resp_len = u32::from_be_bytes(len_buf) as usize;
            let mut resp_payload = vec![0u8; resp_len];
            socket.read_exact(&mut resp_payload).await?;

            let mut rx_buf = vec![0u8; 65535];
            tunnel
                .noise_state
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("Noise state missing"))?
                .read_message(&resp_payload, &mut rx_buf)?;
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
        use crate::network::tunnel::Tunnel;

        let mut socket = TcpStream::connect(target_addr).await?;

        let mut tunnel = Tunnel::init_initiator_nn()?;

        let mut ix_buf = vec![0u8; 65535];
        let len = tunnel
            .noise_state
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Noise state missing"))?
            .write_message(&[], &mut ix_buf)?;
        socket.write_all(&(len as u32).to_be_bytes()).await?;
        socket.write_all(&ix_buf[..len]).await?;

        let mut len_buf = [0u8; 4];
        socket.read_exact(&mut len_buf).await?;
        let resp_len = u32::from_be_bytes(len_buf) as usize;
        let mut resp_payload = vec![0u8; resp_len];
        socket.read_exact(&mut resp_payload).await?;

        let mut rx_buf = vec![0u8; 65535];
        tunnel
            .noise_state
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Noise state missing"))?
            .read_message(&resp_payload, &mut rx_buf)?;
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
