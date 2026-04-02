//! The Sovereign Inference — Phase 10: Native Neural Execution
//!
//! This module provides the neural inference abstraction layer, allowing
//! Wasm agents to perform local LLM inference without leaving the sandbox.
//! The `NeuralEngine` trait decouples the inference backend (llama.cpp, mock, etc.)
//! from the sandbox plumbing, enabling zero-cost testing and hardware-agnostic teleportation.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Domain Models
// ---------------------------------------------------------------------------

/// A request from a Wasm agent to perform neural inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    /// The alias of the loaded model (e.g., "llama-3-8b")
    pub model_alias: String,
    /// The text prompt to feed the model
    pub prompt: String,
    /// Sampling temperature (0.0 = greedy, 1.0 = creative)
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Maximum number of tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Stop sequences — generation halts when any of these are produced
    #[serde(default)]
    pub stop_sequences: Vec<String>,
    /// Optional KV-cache session ID for multi-turn continuation
    #[serde(default)]
    pub session_id: Option<String>,
    /// Deterministic seed for exact Context Replay teleportation
    #[serde(default = "default_seed")]
    pub deterministic_seed: u64,
}

fn default_temperature() -> f32 { 0.7 }
fn default_max_tokens() -> u32 { 256 }
fn default_seed() -> u64 { 42 }

/// The result of a neural inference operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    /// The generated text
    pub text: String,
    /// Number of tokens consumed from the prompt
    pub prompt_tokens: u32,
    /// Number of tokens generated
    pub tokens_generated: u32,
    /// Total Wasm Fuel burned for this inference
    pub fuel_burned: u64,
    /// Session ID for multi-turn continuation (the "train of thought")
    pub session_id: String,
    /// Model alias used
    pub model_alias: String,
    /// Reason for early text cutoff, e.g., "OutOfFuel"
    pub trap_reason: Option<String>,
}

/// Tracks the "train of thought" for teleportation continuity.
/// When an agent migrates, we serialize the prompt history and replay
/// it on the destination node to rebuild the KV-cache deterministically.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InferenceSession {
    /// Unique session identifier
    pub session_id: String,
    /// The model alias this session is bound to
    pub model_alias: String,
    /// Ordered history of all prompts fed to this session
    pub prompt_history: Vec<String>,
    /// Ordered history of all generated responses
    pub response_history: Vec<String>,
    /// The partial generated string, used if trapped out of fuel mid-sentence
    pub current_generation: String,
    /// Total tokens processed in this session
    pub total_tokens: u32,
}

/// Information about a loaded model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub alias: String,
    pub path: String,
    pub parameters_b: f32,    // Billions of parameters (estimated)
    pub context_length: u32,   // Max context window
    pub loaded: bool,
}

// ---------------------------------------------------------------------------
// Fuel Accounting
// ---------------------------------------------------------------------------

/// The economic metering engine for neural inference.
///
/// Inference is the most expensive operation in the Trytet economy.
/// We charge fuel based on tokens, not wall-clock time, ensuring
/// deterministic billing regardless of host hardware speed.
pub struct InferenceFuelCalculator;

impl InferenceFuelCalculator {
    /// Weight for input/prompt tokens (cheap: just KV-fill)
    const W_IN: u64 = 10;
    /// Weight for output/generated tokens (expensive: autoregressive decode)
    const W_OUT: u64 = 50;
    /// Flat cost for model loading
    const MODEL_LOAD_COST: u64 = 10_000;

    /// Calculate the fuel cost for an inference operation.
    ///
    /// Formula: InferenceFuel = (PromptTokens × W_in) + (GeneratedTokens × W_out)
    pub fn calculate(prompt_tokens: u32, generated_tokens: u32) -> u64 {
        (prompt_tokens as u64 * Self::W_IN) + (generated_tokens as u64 * Self::W_OUT)
    }

    /// Flat fuel cost charged when loading a model into host RAM.
    pub fn model_load_cost() -> u64 {
        Self::MODEL_LOAD_COST
    }
}

// ---------------------------------------------------------------------------
// Neural Engine Trait
// ---------------------------------------------------------------------------

/// The abstraction boundary for neural inference backends.
///
/// Implementations:
/// - `MockNeuralEngine`: Zero-weight test double for `cargo test`
/// - `LlamaCppEngine`: Production backend wrapping `llama-cpp-2`
#[async_trait]
pub trait NeuralEngine: Send + Sync {
    /// Load a model from a file path into host RAM.
    async fn load_model(&self, alias: &str, path: &str) -> Result<ModelInfo, String>;

    /// Perform inference on a loaded model.
    /// `fuel_limit` provides a hard boundary on computation; if generation exceeds this, it traps mid-sentence.
    async fn predict(&self, request: &InferenceRequest, fuel_limit: u64) -> Result<InferenceResponse, String>;

    /// Check if a model is currently loaded.
    async fn is_model_loaded(&self, alias: &str) -> bool;

    /// List all loaded models.
    async fn list_models(&self) -> Vec<ModelInfo>;

    /// Get or create an inference session for multi-turn conversation.
    async fn get_session(&self, session_id: &str) -> Option<InferenceSession>;

    /// Serialize all active sessions (for snapshot/teleportation).
    async fn serialize_sessions(&self) -> Vec<u8>;

    /// Restore sessions from serialized data (for fork/teleportation).
    async fn restore_sessions(&self, data: &[u8]);
}

// ---------------------------------------------------------------------------
// Model Registry
// ---------------------------------------------------------------------------

/// Thread-safe registry of loaded models in host RAM.
/// Prevents redundant weight loading when multiple Tets reference the same model.
pub struct ModelRegistry {
    models: Arc<RwLock<HashMap<String, ModelInfo>>>,
    sessions: Arc<RwLock<HashMap<String, InferenceSession>>>,
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            models: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_model(&self, info: ModelInfo) {
        self.models.write().await.insert(info.alias.clone(), info);
    }

    pub async fn get_model(&self, alias: &str) -> Option<ModelInfo> {
        self.models.read().await.get(alias).cloned()
    }

    pub async fn list_models(&self) -> Vec<ModelInfo> {
        self.models.read().await.values().cloned().collect()
    }

    pub async fn get_session(&self, session_id: &str) -> Option<InferenceSession> {
        self.sessions.read().await.get(session_id).cloned()
    }

    pub async fn get_or_create_session(&self, session_id: &str, model_alias: &str) -> InferenceSession {
        let mut sessions = self.sessions.write().await;
        sessions.entry(session_id.to_string()).or_insert_with(|| InferenceSession {
            session_id: session_id.to_string(),
            model_alias: model_alias.to_string(),
            prompt_history: Vec::new(),
            response_history: Vec::new(),
            current_generation: String::new(),
            total_tokens: 0,
        }).clone()
    }

    pub async fn update_session(&self, session: InferenceSession) {
        self.sessions.write().await.insert(session.session_id.clone(), session);
    }

    pub async fn serialize_sessions(&self) -> Vec<u8> {
        let sessions = self.sessions.read().await;
        let session_vec: Vec<InferenceSession> = sessions.values().cloned().collect();
        bincode::serialize(&session_vec).unwrap_or_default()
    }

    pub async fn restore_sessions(&self, data: &[u8]) {
        if let Ok(sessions) = bincode::deserialize::<Vec<InferenceSession>>(data) {
            let mut store = self.sessions.write().await;
            for session in sessions {
                store.insert(session.session_id.clone(), session);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Mock Neural Engine (for testing)
// ---------------------------------------------------------------------------

/// A zero-weight test double that simulates inference without loading
/// any actual model weights. Used by `cargo test` to achieve sub-second
/// test execution while verifying the full host function plumbing.
pub struct MockNeuralEngine {
    registry: ModelRegistry,
}

impl Default for MockNeuralEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MockNeuralEngine {
    pub fn new() -> Self {
        Self {
            registry: ModelRegistry::new(),
        }
    }

    /// Simulate tokenization: ~4 characters per token (rough English average).
    fn estimate_tokens(text: &str) -> u32 {
        std::cmp::max(1, (text.len() as u32).div_ceil(4))
    }

    /// Deterministic mock response based on the prompt.
    fn mock_generate(prompt: &str, max_tokens: u32) -> (String, u32) {
        // Deterministic responses for known test prompts
        let response = if prompt.contains("2+2") || prompt.contains("2 + 2") {
            "The answer is 4.".to_string()
        } else if prompt.contains("capital") && prompt.contains("France") {
            "The capital of France is Paris.".to_string()
        } else if prompt.contains("hello") || prompt.contains("Hello") {
            "Hello! How can I help you today?".to_string()
        } else {
            // Generic response for unknown prompts
            format!("I received your prompt of {} characters. Processing complete.", prompt.len())
        };

        let tokens = Self::estimate_tokens(&response).min(max_tokens);
        (response, tokens)
    }
}

#[async_trait]
impl NeuralEngine for MockNeuralEngine {
    async fn load_model(&self, alias: &str, path: &str) -> Result<ModelInfo, String> {
        let info = ModelInfo {
            alias: alias.to_string(),
            path: path.to_string(),
            parameters_b: 0.001, // Mock: tiny
            context_length: 4096,
            loaded: true,
        };
        self.registry.register_model(info.clone()).await;
        Ok(info)
    }

    async fn predict(&self, request: &InferenceRequest, fuel_limit: u64) -> Result<InferenceResponse, String> {
        // Verify model is loaded
        if !self.is_model_loaded(&request.model_alias).await {
            return Err(format!("Model '{}' not loaded", request.model_alias));
        }

        let prompt_tokens = Self::estimate_tokens(&request.prompt);
        let mut tokens_generated = 0;
        let mut text_generated = String::new();
        let mut trap_reason = None;
        let mut text = String::new();
        
        let (full_text, _desired_tokens) = Self::mock_generate(&request.prompt, request.max_tokens);
        
        // Simulate token by token generation to respect fuel limits
        let words: Vec<&str> = full_text.split_whitespace().collect();
        for word in words {
            let next_chunk = format!("{word} ");
            let chunk_tokens = Self::estimate_tokens(&next_chunk);
            let next_fuel = InferenceFuelCalculator::calculate(prompt_tokens, tokens_generated + chunk_tokens);
            
            if next_fuel > fuel_limit {
                trap_reason = Some("OutOfFuel".to_string());
                break;
            }
            
            text_generated.push_str(&next_chunk);
            text.push_str(&next_chunk);
            tokens_generated += chunk_tokens;
        }

        let fuel_burned = InferenceFuelCalculator::calculate(prompt_tokens, tokens_generated);

        // Manage session
        let session_id = request.session_id.clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let mut session = self.registry.get_or_create_session(&session_id, &request.model_alias).await;
        session.prompt_history.push(request.prompt.clone());
        
        if trap_reason.is_none() {
            session.response_history.push(text.clone());
            session.current_generation = String::new();
        } else {
            session.current_generation = text.clone();
        }
        
        session.total_tokens += prompt_tokens + tokens_generated;
        self.registry.update_session(session).await;

        Ok(InferenceResponse {
            text,
            prompt_tokens,
            tokens_generated,
            fuel_burned,
            session_id,
            model_alias: request.model_alias.clone(),
            trap_reason,
        })
    }

    async fn is_model_loaded(&self, alias: &str) -> bool {
        self.registry.get_model(alias).await.is_some()
    }

    async fn list_models(&self) -> Vec<ModelInfo> {
        self.registry.list_models().await
    }

    async fn get_session(&self, session_id: &str) -> Option<InferenceSession> {
        self.registry.sessions.read().await.get(session_id).cloned()
    }

    async fn serialize_sessions(&self) -> Vec<u8> {
        self.registry.serialize_sessions().await
    }

    async fn restore_sessions(&self, data: &[u8]) {
        self.registry.restore_sessions(data).await;
    }
}
