//! The Model Proxy — Phase 15.2: Deterministic Inference Shim
//!
//! Bridges the Wasm sandbox to the Host's LLM capabilities (local or remote)
//! through the MeshOracle. Every "thought" is cached, signed, and metered
//! at a fixed token-based fuel cost to ensure 100% execution replayability.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;

use crate::oracle::{MeshOracle, SignedTruth};

// ---------------------------------------------------------------------------
// Fuel Constants
// ---------------------------------------------------------------------------

/// Unified token weight for the deterministic billing formula.
/// `Fuel = (InputTokens + OutputTokens) × C_TOKEN_WEIGHT + C_BASE_OVERHEAD`
pub const C_TOKEN_WEIGHT: u64 = 30;

/// Base overhead charged for every inference syscall, regardless of token count.
pub const C_BASE_OVERHEAD: u64 = 5_000;

// ---------------------------------------------------------------------------
// InferenceProvider Trait
// ---------------------------------------------------------------------------

/// Abstraction boundary for neural inference backends.
///
/// Implementations:
/// - `MockInferenceProvider`: Zero-weight test double for `cargo test`
/// - Future: `GeminiProvider`, `OpenAIProvider`, `LlamaCppProvider`
#[async_trait]
pub trait InferenceProvider: Send + Sync {
    /// Perform inference and return the raw text response plus exact token counts.
    async fn predict(
        &self,
        prompt: &str,
        model_id: &str,
        temperature: f32,
        max_tokens: u32,
    ) -> Result<ProviderResponse, String>;

    /// Return the hard context window limit for a given model.
    fn context_limit(&self, model_id: &str) -> usize;
}

/// Raw response from an inference provider before Oracle signing.
#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub text: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

// ---------------------------------------------------------------------------
// Mock Inference Provider
// ---------------------------------------------------------------------------

/// Zero-weight test double that returns deterministic responses
/// with exact token counts for TDD verification.
pub struct MockInferenceProvider {
    pub context_limit: usize,
}

impl MockInferenceProvider {
    pub fn new() -> Self {
        Self {
            context_limit: 4096,
        }
    }

    pub fn with_context_limit(limit: usize) -> Self {
        Self {
            context_limit: limit,
        }
    }

    /// Deterministic tokenizer: ~4 characters per token.
    fn estimate_tokens(text: &str) -> u32 {
        std::cmp::max(1, (text.len() as u32).div_ceil(4))
    }
}

impl Default for MockInferenceProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InferenceProvider for MockInferenceProvider {
    async fn predict(
        &self,
        prompt: &str,
        _model_id: &str,
        _temperature: f32,
        max_tokens: u32,
    ) -> Result<ProviderResponse, String> {
        let input_tokens = Self::estimate_tokens(prompt);

        // Deterministic mock responses
        let response_text = if prompt.contains("2+2") || prompt.contains("2 + 2") {
            "The answer is 4.".to_string()
        } else if prompt.contains("capital") && prompt.contains("France") {
            "The capital of France is Paris.".to_string()
        } else if prompt.contains("hello") || prompt.contains("Hello") {
            "Hello! How can I help you today?".to_string()
        } else {
            format!(
                "I received your prompt of {} characters. Processing complete.",
                prompt.len()
            )
        };

        let output_tokens = Self::estimate_tokens(&response_text).min(max_tokens);

        Ok(ProviderResponse {
            text: response_text,
            input_tokens,
            output_tokens,
        })
    }

    fn context_limit(&self, _model_id: &str) -> usize {
        self.context_limit
    }
}

// ---------------------------------------------------------------------------
// Proxy Request / Response
// ---------------------------------------------------------------------------

/// A request to the ModelProxy for Oracle-mediated inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceProxyRequest {
    pub prompt: String,
    pub model_id: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

/// The signed, deterministic response from the ModelProxy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceProxyResponse {
    pub text: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub fuel_consumed: u64,
    pub session_hash: String,
    pub node_signature: String,
    /// True if returned from Oracle cache (no LLM call made).
    pub cached: bool,
}

// ---------------------------------------------------------------------------
// Model Proxy
// ---------------------------------------------------------------------------

/// The Sovereign Inference Proxy.
///
/// Mediates all inference requests through the MeshOracle to ensure
/// deterministic, replayable, and cryptographically signed "thoughts."
pub struct ModelProxy {
    pub provider: Arc<dyn InferenceProvider>,
    pub oracle: Arc<MeshOracle>,
}

impl ModelProxy {
    pub fn new(provider: Arc<dyn InferenceProvider>, oracle: Arc<MeshOracle>) -> Self {
        Self { provider, oracle }
    }

    /// Hash the inference request parameters for Oracle cache keying.
    /// Includes prompt + model_id + temperature to ensure cache hits
    /// only when all parameters are identical.
    pub fn hash_request(req: &InferenceProxyRequest) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"inference|");
        hasher.update(req.prompt.as_bytes());
        hasher.update(b"|");
        hasher.update(req.model_id.as_bytes());
        hasher.update(b"|");
        hasher.update(req.temperature.to_le_bytes());
        hasher.update(b"|");
        hasher.update(req.max_tokens.to_le_bytes());
        hasher.finalize().into()
    }

    /// Calculate the deterministic fuel cost for an inference call.
    ///
    /// Formula: `Fuel = (InputTokens + OutputTokens) × C_TOKEN_WEIGHT + C_BASE_OVERHEAD`
    pub fn calculate_fuel(input_tokens: u32, output_tokens: u32) -> u64 {
        (input_tokens as u64 + output_tokens as u64) * C_TOKEN_WEIGHT + C_BASE_OVERHEAD
    }

    /// Resolve an inference request through the Oracle.
    ///
    /// 1. Hash the request (prompt + model_id + temperature + max_tokens)
    /// 2. Check Oracle cache for a matching SignedTruth
    /// 3. On hit: return cached response immediately (zero LLM call)
    /// 4. On miss: call provider, sign, persist, return
    pub async fn predict(
        &self,
        req: InferenceProxyRequest,
        cache_dir: &PathBuf,
    ) -> Result<InferenceProxyResponse, String> {
        let session_hash = Self::hash_request(&req);
        let hash_hex = hex::encode(session_hash);
        let cache_file = cache_dir.join(format!("inference_{}.json", hash_hex));

        // --- Oracle Cache Check (The Memory) ---
        if cache_file.exists() {
            if let Ok(bytes) = std::fs::read(&cache_file) {
                if let Ok(truth) = serde_json::from_slice::<SignedTruth>(&bytes) {
                    // Deserialize the cached response
                    if let Ok(cached_resp) =
                        serde_json::from_slice::<CachedInferencePayload>(&truth.response_body)
                    {
                        let fuel = Self::calculate_fuel(
                            cached_resp.input_tokens,
                            cached_resp.output_tokens,
                        );
                        return Ok(InferenceProxyResponse {
                            text: cached_resp.text,
                            input_tokens: cached_resp.input_tokens,
                            output_tokens: cached_resp.output_tokens,
                            fuel_consumed: fuel,
                            session_hash: hash_hex,
                            node_signature: hex::encode(&truth.node_signature),
                            cached: true,
                        });
                    }
                }
            }
        }

        // --- Provider Call (The Discovery) ---
        let provider_resp = self
            .provider
            .predict(&req.prompt, &req.model_id, req.temperature, req.max_tokens)
            .await?;

        let fuel = Self::calculate_fuel(provider_resp.input_tokens, provider_resp.output_tokens);

        // --- The Signing ---
        let cached_payload = CachedInferencePayload {
            text: provider_resp.text.clone(),
            input_tokens: provider_resp.input_tokens,
            output_tokens: provider_resp.output_tokens,
            model_id: req.model_id.clone(),
            temperature: req.temperature,
        };
        let response_body =
            serde_json::to_vec(&cached_payload).map_err(|e| format!("Serialize error: {e}"))?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("Time error: {e}"))?
            .as_secs();

        // Construct signing payload
        let mut sign_payload = Vec::new();
        sign_payload.extend_from_slice(&session_hash);
        sign_payload.extend_from_slice(&response_body);
        sign_payload.extend_from_slice(&200u16.to_le_bytes());
        sign_payload.extend_from_slice(&timestamp.to_le_bytes());

        let node_signature = self.oracle.wallet.sign_bytes(&sign_payload);

        let truth = SignedTruth {
            request_hash: session_hash,
            response_body: response_body.clone(),
            status_code: 200,
            timestamp,
            node_signature: node_signature.clone(),
            model_id: Some(req.model_id),
            temperature: Some(req.temperature),
        };

        // --- Persistence ---
        let truth_bytes =
            serde_json::to_vec(&truth).map_err(|e| format!("Truth serialize error: {e}"))?;
        if !cache_dir.exists() {
            std::fs::create_dir_all(cache_dir)
                .map_err(|e| format!("Cache dir creation error: {e}"))?;
        }
        std::fs::write(&cache_file, truth_bytes).map_err(|e| format!("Cache write error: {e}"))?;

        Ok(InferenceProxyResponse {
            text: provider_resp.text,
            input_tokens: provider_resp.input_tokens,
            output_tokens: provider_resp.output_tokens,
            fuel_consumed: fuel,
            session_hash: hash_hex,
            node_signature: hex::encode(&node_signature),
            cached: false,
        })
    }
}

// ---------------------------------------------------------------------------
// Internal Cache Payload
// ---------------------------------------------------------------------------

/// The payload stored inside a SignedTruth for inference results.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedInferencePayload {
    pub text: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub model_id: String,
    pub temperature: f32,
}
