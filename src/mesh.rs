//! Tet-Mesh Registry and RPC Router
//! 
//! Manages zero-trust discovery between Tets using arbitrary aliases
//! and routes `MeshCallRequest`s securely without relying on OS networking.

use crate::models::{MeshCallRequest, MeshCallResponse, TetMetadata};
use std::collections::HashMap;
use std::sync::Arc;
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
}

impl TetMesh {
    /// Creates a new TetMesh and returns its Receiver for the Engine to poll.
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<MeshMessage>) {
        let (tx, rx) = mpsc::channel(capacity);
        (
            Self {
                registry: Arc::new(RwLock::new(HashMap::new())),
                tx,
            },
            rx,
        )
    }

    /// Registers a new alias pointing to a Tet.
    pub async fn register(&self, alias: String, metadata: TetMetadata) {
        self.registry.write().await.insert(alias, metadata);
    }

    /// Resolves an alias to its Metadata.
    pub async fn resolve(&self, alias: &str) -> Option<TetMetadata> {
        self.registry.read().await.get(alias).cloned()
    }

    /// Removes an alias from the registry.
    pub async fn deregister(&self, alias: &str) {
        self.registry.write().await.remove(alias);
    }

    /// Sends a remote procedure call across the internal channel.
    pub async fn send_call(&self, req: MeshCallRequest) -> Result<MeshCallResponse, &'static str> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let msg = MeshMessage::Call { req, reply: reply_tx };
        
        if self.tx.send(msg).await.is_err() {
            return Err("Mesh channel closed");
        }
        
        reply_rx.await.map_err(|_| "Mesh call dropped")
    }
}
