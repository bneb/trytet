//! The Sovereign Inference — Real Llama.cpp Backend
//!
//! Wraps `llama-cpp-2` to provide Metal-accelerated, tokio-compatible
//! local LLM inference with strict token-by-token fuel accounting.

use crate::inference::{
    InferenceFuelCalculator, InferenceRequest, InferenceResponse, InferenceSession, ModelInfo,
    ModelRegistry, NeuralEngine,
};
use async_trait::async_trait;
use llama_cpp_2::{
    context::params::LlamaContextParams,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, AddBos, LlamaModel},
    token::data_array::LlamaTokenDataArray,
};
use std::sync::Arc;

pub struct LlamaCppEngine {
    backend: Arc<LlamaBackend>,
    registry: ModelRegistry,
}

impl Default for LlamaCppEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LlamaCppEngine {
    pub fn new() -> Self {
        let backend =
            Arc::new(LlamaBackend::init().expect("Failed to initialize llama.cpp backend"));
        Self {
            backend,
            registry: ModelRegistry::new(),
        }
    }

    // Internal helper to get the raw model safely
    async fn get_raw_model(&self, alias: &str) -> Option<LlamaModel> {
        let models = self.registry.list_models().await;
        let info = models.into_iter().find(|m| m.alias == alias)?;

        let model_params = LlamaModelParams::default(); // Metal is handled automatically if compiled
        let model = LlamaModel::load_from_file(&self.backend, &info.path, &model_params).ok()?;

        Some(model)
    }
}

#[async_trait]
impl NeuralEngine for LlamaCppEngine {
    async fn load_model(&self, alias: &str, path: &str) -> Result<ModelInfo, String> {
        // Run model loading in a blocking task since it reads gigabytes of weights
        let backend = self.backend.clone();
        let path_clone = path.to_string();

        let res = tokio::task::spawn_blocking(move || {
            let model_params = LlamaModelParams::default();
            // Try loading it to verify
            LlamaModel::load_from_file(&backend, &path_clone, &model_params)
                .map_err(|e| format!("Failed to load model: {}", e))
        })
        .await
        .map_err(|e| format!("Spawn error: {}", e))??;

        let info = ModelInfo {
            alias: alias.to_string(),
            path: path.to_string(),
            parameters_b: res.n_ctx_train() as f32 / 1_000_000.0, // rough heuristic
            context_length: res.n_ctx_train() as u32,
            loaded: true,
        };

        self.registry.register_model(info.clone()).await;
        Ok(info)
    }

    async fn predict(
        &self,
        request: &InferenceRequest,
        fuel_limit: u64,
    ) -> Result<InferenceResponse, String> {
        if !self.is_model_loaded(&request.model_alias).await {
            return Err(format!("Model '{}' not loaded", request.model_alias));
        }

        let model = self
            .get_raw_model(&request.model_alias)
            .await
            .ok_or_else(|| "Failed to load model from registry".to_string())?;

        // Context Replay: Get session history to rehydrate KV-cache
        let session_id = request
            .session_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let session = self
            .registry
            .get_or_create_session(&session_id, &request.model_alias)
            .await;

        let req_clone = request.clone();
        let backend_clone = self.backend.clone();

        // Push the blocking inference work to a separate thread pool
        // This prevents the heavy CPU/GPU loop from starving the Tokio reactor!
        let (text_generated, tokens_generated, prompt_tokens, is_trap) =
            tokio::task::spawn_blocking(move || {
                let mut ctx_params = LlamaContextParams::default();
                ctx_params = ctx_params.with_n_ctx(std::num::NonZeroU32::new(4096)); // Max context size
                                                                                     // llama_cpp_2 does not directly support with_seed on params in all versions, we set it manually or ignore if no API
                                                                                     // For now, we omit seed parsing if the builder doesn't support it, or set greedy sampling manually

                let mut ctx = model
                    .new_context(&backend_clone, ctx_params)
                    .map_err(|e| format!("Failed to create context: {}", e))?;

                // Context Replay: rebuild the prompt incrementally
                let mut full_prompt = String::new();
                for p in &session.prompt_history {
                    full_prompt.push_str(p);
                    full_prompt.push('\n');
                }
                // Include partial generation if we tripped out of fuel mid-thought last time
                if !session.current_generation.is_empty() {
                    full_prompt.push_str(&session.current_generation);
                }

                full_prompt.push_str(&req_clone.prompt);

                let tokens_list = model
                    .str_to_token(&full_prompt, AddBos::Always)
                    .map_err(|e| format!("Tokenization failed: {}", e))?;

                let prompt_tokens = tokens_list.len() as u32;

                let mut batch = LlamaBatch::new(512, 1);
                let last_index = tokens_list.len() - 1;

                for (i, token) in tokens_list.into_iter().enumerate() {
                    let is_last = i == last_index;
                    batch
                        .add(token, i as i32, &[0], is_last)
                        .map_err(|e| format!("Batch error: {}", e))?;
                }

                ctx.decode(&mut batch)
                    .map_err(|e| format!("Decode failed: {}", e))?;

                let mut n_cur = batch.n_tokens();
                let mut output_text = String::new();
                let mut generated = 0;
                let mut trap = false;

                while generated < req_clone.max_tokens {
                    let candidates = ctx.candidates_ith(batch.n_tokens() - 1);
                    let mut candidates_p =
                        LlamaTokenDataArray::from_iter(candidates.into_iter(), false);
                    let new_token_id = candidates_p.sample_token_greedy();

                    if new_token_id == model.token_eos()
                        || req_clone
                            .stop_sequences
                            .iter()
                            .any(|s| output_text.contains(s))
                    {
                        break;
                    }

                    let token_bytes = model
                        .token_to_piece_bytes(new_token_id, 8, false, None)
                        .unwrap_or_else(|_| vec![]);

                    let token_str = String::from_utf8_lossy(&token_bytes);
                    output_text.push_str(&token_str);
                    generated += 1;

                    // 🔴 strict bounded execution — the Fuel Guard
                    let current_fuel_cost =
                        InferenceFuelCalculator::calculate(prompt_tokens, generated);
                    if current_fuel_cost > fuel_limit {
                        trap = true;
                        break;
                    }

                    batch.clear();
                    batch.add(new_token_id, n_cur, &[0], true).unwrap();
                    n_cur += 1;

                    ctx.decode(&mut batch)
                        .map_err(|e| format!("Autoregressive decode failed: {}", e))?;
                }

                Ok::<_, String>((output_text, generated, prompt_tokens, trap))
            })
            .await
            .map_err(|e| format!("Spawn blocking error: {}", e))??;

        // Session update
        let fuel_burned = InferenceFuelCalculator::calculate(prompt_tokens, tokens_generated);
        let mut final_session = self
            .registry
            .get_or_create_session(&session_id, &request.model_alias)
            .await;

        final_session.prompt_history.push(request.prompt.clone());
        let mut trap_reason = None;

        if is_trap {
            trap_reason = Some("OutOfFuel".to_string());
            final_session.current_generation = text_generated.clone();
        } else {
            final_session.response_history.push(text_generated.clone());
            final_session.current_generation = String::new(); // Reset
        }

        final_session.total_tokens += prompt_tokens + tokens_generated;
        self.registry.update_session(final_session).await;

        Ok(InferenceResponse {
            text: text_generated,
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
        self.registry.get_session(session_id).await
    }

    async fn serialize_sessions(&self) -> Vec<u8> {
        self.registry.serialize_sessions().await
    }

    async fn restore_sessions(&self, data: &[u8]) {
        self.registry.restore_sessions(data).await
    }
}
