use crate::models::manifest::{AgentManifest, ManifestError};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("Manifest error: {0}")]
    Manifest(#[from] ManifestError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Zstd error: {0}")]
    Zstd(std::io::Error),
    #[error("Signature mismatch: The artifact has been tampered with or corrupted")]
    SignatureMismatch,
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArtifactReceipt {
    pub hash: String,
    pub size_bytes: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TetArtifact {
    pub manifest: AgentManifest,
    pub blueprint_wasm: Vec<u8>,
    pub vfs_zstd: Vec<u8>,
    pub signature: Vec<u8>, // Ed25519 signature
}

pub struct TetBuilder {
    pub source_wasm: PathBuf,
    pub manifest_path: PathBuf,
    pub vfs_path: Option<PathBuf>,
    pub output_path: PathBuf,
    pub signing_key: Option<SigningKey>,
}

impl TetBuilder {
    pub async fn assemble(&self) -> Result<ArtifactReceipt, BuildError> {
        let manifest_str = fs::read_to_string(&self.manifest_path)?;
        let mut manifest = AgentManifest::from_toml(&manifest_str)?;

        // Prove OCI mathematical hardware bounds limitation
        let required_bytes = manifest.constraints.max_memory_pages as u64 * 65536;
        let hardware_limit = 10_000_000_000; // Assume host hardware headroom limit is roughly 10GB for this scope

        if required_bytes > hardware_limit {
            return Err(BuildError::Manifest(
                ManifestError::ExceedsHardwareHeadroom {
                    pages: manifest.constraints.max_memory_pages,
                    limit: (hardware_limit / 65536) as u32,
                },
            ));
        }

        let wasm_bytes = fs::read(&self.source_wasm)?;

        let mut vfs_tarball = Vec::new();
        if let Some(vfs_path) = &self.vfs_path {
            if vfs_path.is_file() {
                vfs_tarball = fs::read(vfs_path)?;
            }
        }

        // Layer 1 (VFS Genesis): A Zstd-compressed tarball
        let vfs_zstd = zstd::stream::encode_all(&vfs_tarball[..], 3).map_err(BuildError::Zstd)?;

        let signing_key = self
            .signing_key
            .clone()
            .unwrap_or_else(|| SigningKey::generate(&mut rand_core::OsRng));

        let vk = signing_key.verifying_key();
        manifest.metadata.author_pubkey = Some(hex::encode(vk.as_bytes()));

        // Sign combined payload
        let mut signature_payload = Vec::new();
        signature_payload.extend_from_slice(&wasm_bytes);
        signature_payload.extend_from_slice(&vfs_zstd);

        let mut hasher = Sha256::new();
        hasher.update(&signature_payload);
        let hash_bytes = hasher.finalize().to_vec();

        let signature = signing_key.sign(&hash_bytes).to_bytes().to_vec();

        let artifact = TetArtifact {
            manifest,
            blueprint_wasm: wasm_bytes,
            vfs_zstd,
            signature,
        };

        let bin_blob =
            bincode::serialize(&artifact).map_err(|e| BuildError::Serialization(e.to_string()))?;
        fs::write(&self.output_path, &bin_blob)?;

        Ok(ArtifactReceipt {
            hash: hex::encode(hash_bytes),
            size_bytes: bin_blob.len(),
        })
    }

    /// Verifies and unpacks a signed .tet artifact
    pub fn verify_and_load(artifact_bytes: &[u8]) -> Result<TetArtifact, BuildError> {
        let artifact: TetArtifact = bincode::deserialize(artifact_bytes)
            .map_err(|e| BuildError::Serialization(e.to_string()))?;

        let hex_key = artifact
            .manifest
            .metadata
            .author_pubkey
            .as_ref()
            .ok_or(BuildError::Manifest(ManifestError::MissingIdentity))?;

        let mut pk_bytes = [0u8; 32];
        hex::decode_to_slice(hex_key, &mut pk_bytes as &mut [u8])
            .map_err(|_| BuildError::SignatureMismatch)?;

        let vk = VerifyingKey::from_bytes(&pk_bytes).map_err(|_| BuildError::SignatureMismatch)?;

        let mut signature_payload = Vec::new();
        signature_payload.extend_from_slice(&artifact.blueprint_wasm);
        signature_payload.extend_from_slice(&artifact.vfs_zstd);

        let mut hasher = Sha256::new();
        hasher.update(&signature_payload);
        let hash_bytes = hasher.finalize().to_vec();

        // Reconstruct signature
        let sig: &[u8; 64] = artifact
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| BuildError::SignatureMismatch)?;

        let signature = Signature::from_bytes(sig);

        // Native ed25519 signature test
        vk.verify(&hash_bytes, &signature)
            .map_err(|_| BuildError::SignatureMismatch)?;

        Ok(artifact)
    }
}
