use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::path::PathBuf;
use std::sync::Arc;
use crate::gateway::GlobalRegistry;
use crate::crypto::AgentWallet;
use sha2::{Sha256, Digest};
use std::fs;
use std::time::SystemTime;
use thiserror::Error;
use crate::hive::HivePeers;

#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("Signature Verification Failed")]
    SignatureVerificationFailed,
    #[error("Block not found in CAS")]
    BlockNotFound,
    #[error("Artifact hash mismatch")]
    HashMismatch,
    #[error("Network resolution failed")]
    ResolutionFailed,
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    SerdeError(#[from] serde_json::Error),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ArtifactManifest {
    pub alias: String,
    pub wasm_cid: String,
    pub gene_layers: Vec<Uuid>,
    pub timestamp: u64,
    pub author_pubkey: String,
    pub signature: Vec<u8>,
}

impl ArtifactManifest {
    pub fn verify(&self) -> bool {
        let payload = format!("{}:{}:{}", self.alias, self.wasm_cid, self.timestamp);
        let sig_hex = hex::encode(&self.signature);
        AgentWallet::verify_signature(&self.author_pubkey, payload.as_bytes(), &sig_hex)
    }
}

pub struct SovereignRegistry {
    pub local_cache: PathBuf,
    pub dht_handle: Arc<dyn GlobalRegistry>,
}

impl SovereignRegistry {
    pub fn new(dht_handle: Arc<dyn GlobalRegistry>) -> Self {
        let home_dir = home::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let local_cache = home_dir.join(".trytet").join("registry_cas");
        if !local_cache.exists() {
            fs::create_dir_all(&local_cache).unwrap();
        }
        
        Self {
            local_cache,
            dht_handle,
        }
    }

    pub async fn push_artifact(&self, manifest: ArtifactManifest, wasm_bytes: &[u8]) -> Result<String, RegistryError> {
        if !manifest.verify() {
            return Err(RegistryError::SignatureVerificationFailed);
        }

        let computed_hash = hex::encode(Sha256::digest(wasm_bytes));
        if computed_hash != manifest.wasm_cid {
            return Err(RegistryError::HashMismatch);
        }

        // Write CAS block
        let block_path = self.local_cache.join(&manifest.wasm_cid);
        fs::write(&block_path, wasm_bytes)?;

        // Broadcast to GlobalRegistry (DHT mapping)
        let manifest_bytes = serde_json::to_string(&manifest)?;
        // We encode manifest struct into the IP for prototype mapping or we use the GlobalRegistry route mapping.
        // The tests use `update_route`.
        self.dht_handle.update_route(&manifest.alias, &manifest_bytes, &hex::encode(&manifest.signature)).await.map_err(|_| RegistryError::ResolutionFailed)?;

        self.enforce_lru_cache_limits()?;

        Ok(manifest.wasm_cid.clone())
    }

    pub async fn pull_artifact(&self, alias: &str, _peers: Option<HivePeers>) -> Result<(ArtifactManifest, Vec<u8>), RegistryError> {
        let route_opt = self.dht_handle.resolve_alias(alias).await.map_err(|_| RegistryError::ResolutionFailed)?;
        if let Some(route_data) = route_opt {
            let manifest: ArtifactManifest = serde_json::from_str(&route_data).map_err(|_| RegistryError::ResolutionFailed)?;
            
            if !manifest.verify() {
                return Err(RegistryError::SignatureVerificationFailed);
            }
            
            // Check local cache
            let block_path = self.local_cache.join(&manifest.wasm_cid);
            if block_path.exists() {
                let bytes = fs::read(&block_path)?;
                return Ok((manifest, bytes));
            }
            
            self.pull_artifact_from_mesh(alias, &manifest).await
        } else {
            Err(RegistryError::ResolutionFailed)
        }
    }
    
    // Extracted out to easily bypass DHT lookup directly via Manifest during tests
    pub async fn pull_artifact_from_mesh(&self, _alias: &str, manifest: &ArtifactManifest) -> Result<(ArtifactManifest, Vec<u8>), RegistryError> {
        if !manifest.verify() {
            return Err(RegistryError::SignatureVerificationFailed);
        }

        let block_path = self.local_cache.join(&manifest.wasm_cid);
        if block_path.exists() {
            let bytes = fs::read(&block_path)?;
            return Ok((manifest.clone(), bytes));
        }

        // P2P Locality Check (Mocked for now since Mesh relies on HiveCommand internals)
        // In reality, this queries `HiveClient` via TCP loops. For tests, we simulate raw chunk download.
        // Implementation will occur inside `hive.rs` network stack.
        let downloaded_bytes = vec![]; // FIXME: P2P Integration Here

        Ok((manifest.clone(), downloaded_bytes))
    }

    fn enforce_lru_cache_limits(&self) -> Result<(), RegistryError> {
        let mut entries: Vec<std::fs::DirEntry> = fs::read_dir(&self.local_cache)?
            .filter_map(Result::ok)
            .collect();
            
        let max_size_bytes = 2_000_000_000u64; // 2GB limit
        let mut total_size: u64 = entries.iter().map(|e| e.metadata().map(|m| m.len()).unwrap_or(0)).sum();
        
        if total_size <= max_size_bytes {
            return Ok(());
        }

        entries.sort_by_key(|a| {
            a.metadata()
                .and_then(|m| m.accessed())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        });

        for entry in entries {
            if total_size <= max_size_bytes {
                break;
            }
            let size = entry.metadata()?.len();
            fs::remove_file(entry.path())?;
            total_size -= size;
        }

        Ok(())
    }
}
