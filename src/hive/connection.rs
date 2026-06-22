//! Connection handling for the Hive P2P protocol.
//!
//! Contains `handle_connection` and `handle_migration_packet` — the two
//! largest methods on `HiveServer`. Extracted to keep `hive.rs` under 400 lines.

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{error, info};

use crate::hive::link::MigrationPacket;
use crate::hive::{
    HiveCommand, HiveDhtCommand, HiveEconomyCommand, HiveMigrationCommand, HiveNetworkCommand,
    HivePeers, HiveRegistryCommand, HiveServer, MigrationManager, TeleportationEnvelope,
};

impl HiveServer {
    pub(crate) async fn handle_connection<
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    >(
        socket: &mut S,
        peers: HivePeers,
        mesh: crate::mesh::TetMesh,
        sandbox: Arc<crate::sandbox::WasmtimeSandbox>,
        migration_manager: Arc<MigrationManager>,
        registry_client: Option<Arc<crate::registry::oci::OciClient>>,
        vitality_manager: Arc<crate::network::vitality::VitalityManager>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::network::tunnel::Tunnel;
        let mut tunnel = Tunnel::init_responder_nn()?;
        info!("Upgrading to Noise NN Tunnel.");
        let mut len_buf = [0u8; 4];
        socket.read_exact(&mut len_buf).await?;
        let req_len = u32::from_be_bytes(len_buf) as usize;
        let mut req_payload = vec![0u8; req_len];
        socket.read_exact(&mut req_payload).await?;
        let mut rx_buf = vec![0u8; 65535];
        tunnel
            .noise_state
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Noise state missing"))?
            .read_message(&req_payload, &mut rx_buf)
            .map_err(|e| anyhow::anyhow!("HS P1: {}", e))?;
        let mut ix_buf = vec![0u8; 65535];
        let len = tunnel
            .noise_state
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Noise state missing"))?
            .write_message(&[], &mut ix_buf)?;
        socket.write_all(&(len as u32).to_be_bytes()).await?;
        socket.write_all(&ix_buf[..len]).await?;
        tunnel.to_transport()?;
        let mut lb = [0u8; 4];
        socket.read_exact(&mut lb).await?;
        let clen = u32::from_be_bytes(lb) as usize;
        if clen > 200 * 1024 * 1024 {
            return Err("Payload too large".into());
        }
        let mut payload = vec![0u8; clen];
        socket.read_exact(&mut payload).await?;
        let command = tunnel
            .decrypt_payload(&payload)
            .map_err(|e| anyhow::anyhow!("Decrypt: {}", e))?;
        Self::process_command(
            command,
            socket,
            &mut tunnel,
            &peers,
            &mesh,
            &sandbox,
            &migration_manager,
            registry_client,
            &vitality_manager,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn process_command<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
        command: HiveCommand,
        socket: &mut S,
        tunnel: &mut crate::network::tunnel::Tunnel,
        peers: &HivePeers,
        mesh: &crate::mesh::TetMesh,
        sandbox: &Arc<crate::sandbox::WasmtimeSandbox>,
        migration_manager: &Arc<MigrationManager>,
        registry_client: Option<Arc<crate::registry::oci::OciClient>>,
        vitality_manager: &Arc<crate::network::vitality::VitalityManager>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match command {
            HiveCommand::Network(HiveNetworkCommand::Join(id)) => {
                info!("Hive Node joined: {} ({})", id.node_id, id.public_addr);
                peers.add_peer(id).await;
                let enc =
                    tunnel.encrypt_command(&HiveCommand::Network(HiveNetworkCommand::Pulse))?;
                socket.write_all(&(enc.len() as u32).to_be_bytes()).await?;
                socket.write_all(&enc).await?;
            }
            HiveCommand::Network(HiveNetworkCommand::Heartbeat(hb)) => {
                vitality_manager.record_heartbeat(hb);
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Network(HiveNetworkCommand::Pulse)
            | HiveCommand::Dht(HiveDhtCommand::ResolveAliasResponse(_))
            | HiveCommand::Dht(HiveDhtCommand::QuorumVote(_)) => {
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Dht(HiveDhtCommand::ResolveAlias(alias)) => {
                let meta = mesh.resolve_local(&alias).await;
                let resp = HiveCommand::Dht(HiveDhtCommand::ResolveAliasResponse(meta));
                let bytes = bincode::serialize(&resp)?;
                socket
                    .write_all(&(bytes.len() as u32).to_be_bytes())
                    .await?;
                socket.write_all(&bytes).await?;
            }
            HiveCommand::Migration(HiveMigrationCommand::MigrateRequest(env)) => {
                handle_live_migration(*env, sandbox, socket).await?;
            }
            HiveCommand::Migration(HiveMigrationCommand::MigrationPacket(pkt)) => {
                Self::handle_migration_packet(pkt, sandbox.clone(), migration_manager.clone())
                    .await?;
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Migration(HiveMigrationCommand::MigrationNotice {
                reference,
                manifest,
                ..
            }) => {
                handle_registry_migration(reference, manifest, sandbox, registry_client, socket)
                    .await?;
            }
            HiveCommand::Migration(HiveMigrationCommand::TransitLock {
                alias,
                node_id,
                ttl_seconds,
            }) => {
                info!(
                    "TransitLock for {} by {} (ttl: {}s)",
                    alias, node_id, ttl_seconds
                );
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Migration(HiveMigrationCommand::TransitRelease(alias)) => {
                info!("TransitRelease for {}", alias);
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Dht(HiveDhtCommand::DhtUpdate {
                alias,
                node_ip,
                signature,
            }) => {
                info!("DHT Update: {} -> {} (sig: {})", alias, node_ip, signature);
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Dht(HiveDhtCommand::ProposeAlias(proposal)) => {
                info!(
                    "ProposeAlias for {}",
                    hex::encode(&proposal.alias_hash[..4])
                );
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Economy(HiveEconomyCommand::TransferCredit(tx)) => {
                info!(
                    "TransferCredit of {} for {}",
                    tx.amount,
                    hex::encode(&tx.to[..4])
                );
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Economy(HiveEconomyCommand::BillRequest {
                source_alias,
                target_alias,
                amount,
            }) => {
                info!(
                    "BillRequest {} -> {}: {}",
                    source_alias, target_alias, amount
                );
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Economy(HiveEconomyCommand::WithdrawalPending(intent)) => {
                info!(
                    "WithdrawalPending: {} fuel to {}",
                    intent.internal_fuel, intent.target_address
                );
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Economy(HiveEconomyCommand::MarketBidPacket(bid)) => {
                sandbox.market_handle.process_bid(bid);
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
            HiveCommand::Registry(HiveRegistryCommand::RegistryQuery(cid)) => {
                info!("RegistryQuery for CID {}", cid);
                let home_dir = home::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
                let block_path = home_dir.join(".trytet").join("registry_cas").join(&cid);
                let available = block_path.exists();
                let resp = HiveCommand::Registry(HiveRegistryCommand::RegistryQueryResponse {
                    cid,
                    available,
                });
                let bytes = bincode::serialize(&resp)?;
                socket
                    .write_all(&(bytes.len() as u32).to_be_bytes())
                    .await?;
                socket.write_all(&bytes).await?;
            }
            HiveCommand::Registry(HiveRegistryCommand::RegistryQueryResponse { .. })
            | HiveCommand::Registry(HiveRegistryCommand::ChunkStream { .. }) => {
                socket.write_all(&0u32.to_be_bytes()).await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn handle_migration_packet(
        packet: MigrationPacket,
        sandbox: Arc<crate::sandbox::WasmtimeSandbox>,
        migration_manager: Arc<MigrationManager>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::hive::link::MigrationPacket::*;
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
                        let full: Vec<u8> = chunks.into_iter().flat_map(|(_, b)| b).collect();
                        let payload: crate::sandbox::SnapshotPayload =
                            bincode::deserialize(&full).map_err(|e| anyhow::anyhow!(e))?;
                        use crate::engine::TetSandbox;
                        let snap_id = sandbox
                            .import_snapshot(payload)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;
                        let alias = manifest.metadata.name.clone();
                        let sbox = sandbox.clone();
                        let m = manifest.clone();
                        tokio::spawn(async move {
                            let req = crate::models::TetExecutionRequest {
                                alias: Some(alias),
                                payload: None,
                                parent_snapshot_id: Some(snap_id.clone()),
                                allocated_fuel: 50_000_000,
                                max_memory_mb: 64,
                                env: Default::default(),
                                injected_files: Default::default(),
                                call_depth: 0,
                                voucher: None,
                                manifest: Some(m),
                                egress_policy: None,
                                target_function: None,
                            };
                            let _ = sbox.fork(&snap_id, req).await;
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

// Helper free functions for command processing

async fn handle_live_migration<S: tokio::io::AsyncWrite + Unpin>(
    envelope: TeleportationEnvelope,
    sandbox: &Arc<crate::sandbox::WasmtimeSandbox>,
    socket: &mut S,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::engine::TetSandbox;
    info!("Live Migration: Tet {}", envelope.manifest.metadata.name);
    match sandbox.import_snapshot(envelope.snapshot).await {
        Ok(snap_id) => {
            let alias = envelope.manifest.metadata.name.clone();
            let sbox = sandbox.clone();
            let manifest = envelope.manifest.clone();
            tokio::spawn(async move {
                let req = crate::models::TetExecutionRequest {
                    alias: Some(alias),
                    payload: None,
                    parent_snapshot_id: Some(snap_id.clone()),
                    allocated_fuel: 50_000_000,
                    max_memory_mb: 64,
                    env: Default::default(),
                    injected_files: Default::default(),
                    call_depth: 0,
                    voucher: None,
                    manifest: Some(manifest),
                    egress_policy: None,
                    target_function: None,
                };
                let _ = sbox.fork(&snap_id, req).await;
            });
            socket.write_all(&0u32.to_be_bytes()).await?;
        }
        Err(e) => {
            error!("Failed to import teleported payload: {:?}", e);
            socket.write_all(&0u32.to_be_bytes()).await?;
        }
    }
    Ok(())
}

async fn handle_registry_migration<S: tokio::io::AsyncWrite + Unpin>(
    reference: String,
    manifest: crate::models::manifest::AgentManifest,
    sandbox: &Arc<crate::sandbox::WasmtimeSandbox>,
    registry_client: Option<Arc<crate::registry::oci::OciClient>>,
    socket: &mut S,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::engine::TetSandbox;
    info!(
        "Registry Migration Notice for agent: {}",
        manifest.metadata.name
    );
    let registry = registry_client.ok_or_else(|| anyhow::anyhow!("Registry not configured"))?;
    let payload = registry
        .pull_state(&reference)
        .await
        .map_err(|e| anyhow::anyhow!("Pull: {}", e))?;
    match sandbox.import_snapshot(payload).await {
        Ok(snap_id) => {
            let alias = manifest.metadata.name.clone();
            let sbox = sandbox.clone();
            tokio::spawn(async move {
                let req = crate::models::TetExecutionRequest {
                    alias: Some(alias),
                    payload: None,
                    parent_snapshot_id: Some(snap_id.clone()),
                    allocated_fuel: 50_000_000,
                    max_memory_mb: 64,
                    env: Default::default(),
                    injected_files: Default::default(),
                    call_depth: 0,
                    voucher: None,
                    manifest: Some(manifest),
                    egress_policy: None,
                    target_function: None,
                };
                let _ = sbox.fork(&snap_id, req).await;
            });
            info!("Agent successfully pulled and resurrected");
            socket.write_all(&0u32.to_be_bytes()).await?;
        }
        Err(e) => {
            error!("Import failed: {:?}", e);
            return Err(e.into());
        }
    }
    Ok(())
}
