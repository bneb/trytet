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
}

/// The Tet-Mesh handles discovery (Registry) and RPC routing.
#[derive(Clone)]
pub struct TetMesh {
    /// Zero-Trust Registry mapping aliases -> TetMetadata
    registry: Arc<RwLock<HashMap<String, TetMetadata>>>,
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

    /// Registers a new alias pointing to a Tet.
    pub async fn register(&self, alias: String, metadata: crate::models::TetMetadata) {
        self.registry.write().await.insert(alias, metadata);
    }

    pub async fn resolve_local(&self, alias: &str) -> Option<crate::models::TetMetadata> {
        self.registry.read().await.get(alias).cloned()
    }

    /// Resolves an alias by checking local registry, then broadcasting to Hive.
    pub async fn resolve(&self, alias: &str) -> Option<TetMetadata> {
        if let Some(meta) = self.resolve_local(alias).await {
            return Some(meta);
        }

        // Global Mesh Lookup
        let nodes = self.hive_peers.list_peers().await;
        for target_node in nodes {
            let cmd = crate::hive::HiveCommand::ResolveAlias(alias.to_string());
            if let Ok(crate::hive::HiveCommand::ResolveAliasResponse(Some(meta))) =
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
}
