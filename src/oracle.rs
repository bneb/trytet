//! The Sovereign Oracle
//!
//! Bridges the Trytet Zero-Trust Sandbox to the Legacy Internet via
//! deterministic proxy gateways. Enforces Egress domain whitelists
//! and exposes Ingress listening surfaces mapped to internal aliases.

use serde::{Deserialize, Serialize};

/// Defines the security boundaries for a Tet's external internet access.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EgressPolicy {
    /// List of exact host domains the agent is allowed to access (e.g., "api.openai.com").
    pub allowed_domains: Vec<String>,

    /// The maximum number of deterministic network bytes this Tet can transmit/receive
    /// across all combined egress calls per execution lifecycle.
    pub max_daily_bytes: u64,

    /// Strict TLS enforcement. If `true`, the `fetch` host function will outright
    /// reject `http://` prefix requests to prevent plaintext leakage.
    pub require_https: bool,
}

/// A mapping projecting a public Legacy HTTP path into a specific Trytet Mesh Alias.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IngressRoute {
    /// The public facing suffix (e.g., `/v1/chat`).
    pub public_path: String,

    /// The registered internal Tet Mesh Alias (e.g., `chat-agent`).
    pub target_alias: String,

    /// Which HTTP methods are permitted to bridge.
    pub method_filter: Vec<String>,
}

use crate::crypto::AgentWallet;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct SignedTruth {
    pub request_hash: [u8; 32],
    pub response_body: Vec<u8>,
    pub status_code: u16,
    pub timestamp: u64,
    pub node_signature: Vec<u8>,
    /// Phase 15.2: Model identifier for inference-specific cache hits.
    #[serde(default)]
    pub model_id: Option<String>,
    /// Phase 15.2: Temperature parameter for inference-specific cache hits.
    #[serde(default)]
    pub temperature: Option<f32>,
}

pub struct OracleRequest {
    pub url: String,
    pub method: String,
    pub body: Vec<u8>,
}

pub struct MeshOracle {
    pub wallet: AgentWallet,
    pub http_client: reqwest::Client,
}

impl MeshOracle {
    pub fn new() -> anyhow::Result<Self> {
        let wallet = AgentWallet::load_or_create()?;
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        Ok(Self {
            wallet,
            http_client,
        })
    }

    pub async fn resolve(
        &self,
        req: OracleRequest,
        cache_dir: &PathBuf,
    ) -> anyhow::Result<(u16, Vec<u8>)> {
        let mut hasher = Sha256::new();
        hasher.update(req.method.as_bytes());
        hasher.update(b"|");
        hasher.update(req.url.as_bytes());
        hasher.update(b"|");
        hasher.update(&req.body);
        let request_hash: [u8; 32] = hasher.finalize().into();
        let hash_hex = hex::encode(request_hash);
        let cache_file = cache_dir.join(format!("{}.json", hash_hex));

        if cache_file.exists() {
            if let Ok(bytes) = std::fs::read(&cache_file) {
                if let Ok(truth) = serde_json::from_slice::<SignedTruth>(&bytes) {
                    return Ok((truth.status_code, truth.response_body));
                }
            }
        }

        let method = match req.method.as_str() {
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "DELETE" => reqwest::Method::DELETE,
            _ => reqwest::Method::GET,
        };

        let response = self
            .http_client
            .request(method, &req.url)
            .body(req.body.clone())
            .send()
            .await?;

        let status_code = response.status().as_u16();
        let response_body = response.bytes().await?.to_vec();

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        // Sign truth payload (excluding signature)
        let mut sign_payload = Vec::new();
        sign_payload.extend_from_slice(&request_hash);
        sign_payload.extend_from_slice(&response_body);
        sign_payload.extend_from_slice(&status_code.to_le_bytes());
        sign_payload.extend_from_slice(&timestamp.to_le_bytes());

        let node_signature = self.wallet.sign_bytes(&sign_payload);

        let truth = SignedTruth {
            request_hash,
            response_body: response_body.clone(),
            status_code,
            timestamp,
            node_signature,
            model_id: None,
            temperature: None,
        };

        let truth_bytes = serde_json::to_vec(&truth)?;
        if !cache_dir.exists() {
            std::fs::create_dir_all(cache_dir)?;
        }
        std::fs::write(&cache_file, truth_bytes)?;

        Ok((status_code, response_body))
    }

    /// Phase 17.1: Resolve with injected Sovereign Identity headers.
    /// Behaves identically to `resolve`, but adds `extra_headers` to the outbound request.
    pub async fn resolve_with_headers(
        &self,
        req: OracleRequest,
        cache_dir: &PathBuf,
        extra_headers: Vec<(String, String)>,
    ) -> anyhow::Result<(u16, Vec<u8>)> {
        let mut hasher = Sha256::new();
        hasher.update(req.method.as_bytes());
        hasher.update(b"|");
        hasher.update(req.url.as_bytes());
        hasher.update(b"|");
        hasher.update(&req.body);
        let request_hash: [u8; 32] = hasher.finalize().into();
        let hash_hex = hex::encode(request_hash);
        let cache_file = cache_dir.join(format!("{}.json", hash_hex));

        // Cache hit — return immediately (no headers needed)
        if cache_file.exists() {
            if let Ok(bytes) = std::fs::read(&cache_file) {
                if let Ok(truth) = serde_json::from_slice::<SignedTruth>(&bytes) {
                    return Ok((truth.status_code, truth.response_body));
                }
            }
        }

        let method = match req.method.as_str() {
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "DELETE" => reqwest::Method::DELETE,
            _ => reqwest::Method::GET,
        };

        let mut request_builder = self
            .http_client
            .request(method, &req.url)
            .body(req.body.clone());

        // Inject Sovereign Identity headers
        for (name, value) in &extra_headers {
            request_builder = request_builder.header(name, value);
        }

        let response = request_builder.send().await?;

        let status_code = response.status().as_u16();
        let response_body = response.bytes().await?.to_vec();

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let mut sign_payload = Vec::new();
        sign_payload.extend_from_slice(&request_hash);
        sign_payload.extend_from_slice(&response_body);
        sign_payload.extend_from_slice(&status_code.to_le_bytes());
        sign_payload.extend_from_slice(&timestamp.to_le_bytes());

        let node_signature = self.wallet.sign_bytes(&sign_payload);

        let truth = SignedTruth {
            request_hash,
            response_body: response_body.clone(),
            status_code,
            timestamp,
            node_signature,
            model_id: None,
            temperature: None,
        };

        let truth_bytes = serde_json::to_vec(&truth)?;
        if !cache_dir.exists() {
            std::fs::create_dir_all(cache_dir)?;
        }
        std::fs::write(&cache_file, truth_bytes)?;

        Ok((status_code, response_body))
    }

    /// Phase 15.2: Resolve an inference-specific Oracle lookup.
    /// Uses a pre-computed session hash and checks the cache directory.
    pub async fn resolve_inference(
        &self,
        session_hash: &[u8; 32],
        cache_dir: &std::path::Path,
    ) -> Option<SignedTruth> {
        let hash_hex = hex::encode(session_hash);
        let cache_file = cache_dir.join(format!("inference_{}.json", hash_hex));

        if cache_file.exists() {
            if let Ok(bytes) = std::fs::read(&cache_file) {
                if let Ok(truth) = serde_json::from_slice::<SignedTruth>(&bytes) {
                    return Some(truth);
                }
            }
        }
        None
    }
}
