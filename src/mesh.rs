//! Tet-Mesh Registry and RPC Router
//!
//! Manages zero-trust discovery between Tets using arbitrary aliases
//! and routes `MeshCallRequest`s securely without relying on OS networking.

use crate::models::{MeshCallRequest, MeshCallResponse, TetMetadata, TopologyEdge};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot, RwLock};

/// A message routed across the Tet-Mesh.
#[derive(Debug)]
pub enum MeshMessage {
    /// A remote procedure call from one Tet to another.
    Call {
        req: MeshCallRequest,
        reply: oneshot::Sender<MeshCallResponse>,
    },
    /// Natively spawn a new Tet instance from a given execution request (Biological auto-scaling).
    Fork {
        req: Box<crate::models::TetExecutionRequest>,
    },
    /// Route an economic transmission across the mesh.
    EconomyPacket(crate::hive::HiveCommand),
    /// Forcibly stops a designated child Agent Sandbox natively recovering spent fuel bounds.
    Reclaim { child_id: String },
}

/// The Tet-Mesh handles discovery (Registry) and RPC routing.
#[derive(Clone)]
pub struct TetMesh {
    /// Zero-Trust Registry mapping aliases -> active instances
    registry: Arc<RwLock<HashMap<String, Vec<TetMetadata>>>>,
    /// Router channel to send cross-Tet instructions.
    tx: mpsc::Sender<MeshMessage>,
    pub hive_peers: crate::hive::HivePeers,
    /// Swarm Telemetry map natively tracking all multi-agent hops.
    topology: Arc<RwLock<HashMap<String, TopologyEdge>>>,
}

impl TetMesh {
    /// Creates a new TetMesh and returns its Receiver for the Engine to poll.
    pub fn new(
        capacity: usize,
        hive_peers: crate::hive::HivePeers,
    ) -> (Self, mpsc::Receiver<MeshMessage>) {
        let (tx, rx) = mpsc::channel(capacity);
        (
            Self {
                registry: Arc::new(RwLock::new(HashMap::new())),
                tx,
                hive_peers,
                topology: Arc::new(RwLock::new(HashMap::new())),
            },
            rx,
        )
    }

    /// Records a new metric data point inside the Live Swarm Topology Edge.
    pub async fn record_telemetry(
        &self,
        source: String,
        target: String,
        bytes: u64,
        latency_us: u64,
        is_error: bool,
    ) {
        let key = format!("{}->{}", source, target);
        let mut edges = self.topology.write().await;
        let edge = edges.entry(key).or_insert(TopologyEdge {
            source,
            target,
            call_count: 0,
            error_count: 0,
            total_latency_us: 0,
            total_bytes: 0,
            last_seen_ns: 0,
        });

        edge.call_count += 1;
        if is_error {
            edge.error_count += 1;
        }
        edge.total_latency_us += latency_us;
        edge.total_bytes += bytes;
        edge.last_seen_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
    }

    /// Returns a flat vector of all native edges currently witnessed on the Mesh.
    pub async fn get_topology(&self) -> Vec<TopologyEdge> {
        self.topology.read().await.values().cloned().collect()
    }

    pub async fn register(&self, alias: String, metadata: crate::models::TetMetadata) {
        let mut reg = self.registry.write().await;
        let entries = reg.entry(alias).or_default();
        if let Some(existing) = entries.iter_mut().find(|e| e.tet_id == metadata.tet_id) {
            *existing = metadata;
        } else {
            entries.push(metadata);
        }
    }

    pub async fn resolve_local(&self, alias: &str) -> Option<crate::models::TetMetadata> {
        let reg = self.registry.read().await;
        if let Some(entries) = reg.get(alias) {
            if entries.is_empty() {
                return None;
            }
            // Simple round-robin distribution for auto-scaling clones
            let count = entries.len();
            let idx = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as usize;
            return Some(entries[idx % count].clone());
        }
        None
    }

    /// Resolves an alias by checking local registry, then broadcasting to Hive.
    pub async fn resolve(&self, alias: &str) -> Option<TetMetadata> {
        if let Some(meta) = self.resolve_local(alias).await {
            return Some(meta);
        }

        // Global Mesh Lookup
        let nodes = self.hive_peers.list_peers().await;
        for target_node in nodes {
            let cmd = crate::hive::HiveCommand::Dht(crate::hive::HiveDhtCommand::ResolveAlias(alias.to_string()));
            if let Ok(crate::hive::HiveCommand::Dht(crate::hive::HiveDhtCommand::ResolveAliasResponse(Some(meta)))) =
                crate::hive::HiveClient::rpc_call(&target_node.public_addr, cmd).await
            {
                // We found it remotely! Note: It contains a remote node boundary in the future
                // For now, returning TetMetadata tells the engine it exists.
                return Some(meta);
            }
        }
        None
    }

    /// Removes an alias from the registry.
    pub async fn deregister(&self, alias: &str) {
        self.registry.write().await.remove(alias);
    }

    /// Removes a specific snapshot metadata entry for an alias.
    pub async fn remove_by_snapshot(&self, alias: &str, snapshot_id: &str) {
        let mut reg = self.registry.write().await;
        if let Some(entries) = reg.get_mut(alias) {
            entries.retain(|e| e.snapshot_id.as_deref() != Some(snapshot_id));
            if entries.is_empty() {
                reg.remove(alias);
            }
        }
    }

    /// Sends a remote procedure call across the internal channel.
    pub async fn send_call(&self, req: MeshCallRequest) -> Result<MeshCallResponse, &'static str> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let msg = MeshMessage::Call {
            req,
            reply: reply_tx,
        };

        if self.tx.send(msg).await.is_err() {
            return Err("Mesh channel closed");
        }

        reply_rx.await.map_err(|_| "Mesh call dropped")
    }

    /// Sends a fork execution request to be picked up by the mesh worker (Autonomous scaling execution)
    pub async fn send_fork(
        &self,
        req: crate::models::TetExecutionRequest,
    ) -> Result<(), &'static str> {
        let msg = MeshMessage::Fork { req: Box::new(req) };
        if self.tx.send(msg).await.is_err() {
            return Err("Mesh channel closed");
        }
        Ok(())
    }

    /// Broadcasts an economy packet (like TransferCredit or BillRequest).
    pub async fn send_economy_packet(
        &self,
        pkt: crate::hive::HiveCommand,
    ) -> Result<(), &'static str> {
        let msg = MeshMessage::EconomyPacket(pkt);
        if self.tx.send(msg).await.is_err() {
            return Err("Mesh channel closed");
        }
        Ok(())
    }

    /// Signals the termination and fuel sweep of an active workspace locally dynamically matching identities.
    pub async fn send_reclaim(&self, child_id: String) -> Result<(), &'static str> {
        let msg = MeshMessage::Reclaim { child_id };
        if self.tx.send(msg).await.is_err() {
            return Err("Mesh channel closed");
        }
        Ok(())
    }
}
