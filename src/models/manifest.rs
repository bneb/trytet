use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("Manifest parse error: {0}")]
    ParseError(#[from] toml::de::Error),
    #[error("Missing identity: author_pubkey is required for hermetic sovereignty")]
    MissingIdentity,
    #[error("Constraint violation: max_memory_pages ({pages}) exceeds physical headroom limit ({limit})")]
    ExceedsHardwareHeadroom { pages: u32, limit: u32 },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Metadata {
    pub name: String,
    pub version: String,
    pub author_pubkey: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResourceConstraints {
    pub max_memory_pages: u32, // Wasm pages (64KiB each)
    pub fuel_limit: u64,       // Deterministic instruction count
    /// Phase 17.1: Maximum egress bytes per execution lifecycle.
    /// Defaults to 1MB (1_000_000) when not specified.
    #[serde(default = "default_max_egress_bytes")]
    pub max_egress_bytes: u64,
}

fn default_max_egress_bytes() -> u64 {
    1_000_000
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CapabilityPolicy {
    pub can_egress: Vec<String>,
    pub can_persist: bool,
    pub can_teleport: bool,
    #[serde(default)]
    pub is_genesis_factory: bool,
    #[serde(default)]
    pub can_fork: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentManifest {
    pub metadata: Metadata,
    pub constraints: ResourceConstraints,
    pub permissions: CapabilityPolicy,
}

impl AgentManifest {
    pub fn from_toml(raw: &str) -> Result<Self, ManifestError> {
        let manifest: AgentManifest = toml::from_str(raw)?;

        if manifest.metadata.author_pubkey.is_none()
            || manifest.metadata.author_pubkey.as_ref().unwrap().is_empty()
        {
            return Err(ManifestError::MissingIdentity);
        }

        // Just an arbitrary physical limit assumed for the host builder logic, e.g. 10GB = ~160000 pages
        // The real evaluation will be done in the builder or host

        Ok(manifest)
    }
}
