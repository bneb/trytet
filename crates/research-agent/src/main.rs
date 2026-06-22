use serde::{Deserialize, Serialize};
use std::env;

#[derive(Serialize, Deserialize)]
pub struct InferenceRequest {
    pub model_alias: String,
    pub prompt: String,
    pub temperature: f32,
    pub max_tokens: u32,
    #[serde(default)]
    pub stop_sequences: Vec<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub deterministic_seed: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InferenceResponse {
    pub text: String,
    pub prompt_tokens: u32,
    pub tokens_generated: u32,
    pub fuel_burned: u64,
    pub session_id: String,
    pub model_alias: String,
    pub trap_reason: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct VectorRecord {
    pub id: String,
    pub vector: Vec<f32>,
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Serialize, Deserialize)]
pub struct SearchQuery {
    pub collection: String,
    pub query_vector: Vec<f32>,
    pub limit: u32,
    pub min_score: f32,
}

#[link(wasm_import_module = "trytet")]
unsafe extern "C" {
    fn model_load(alias_ptr: *const u8, alias_len: usize, path_ptr: *const u8, path_len: usize) -> i32;
    fn model_predict(req_ptr: *const u8, req_len: usize, out_ptr: *mut u8, out_len_ptr: *mut i32) -> i32;
    fn remember(col_ptr: *const u8, col_len: usize, rec_ptr: *const u8, rec_len: usize) -> i32;
    fn recall(query_ptr: *const u8, query_len: usize, out_ptr: *mut u8, out_len_ptr: *mut i32) -> i32;
    fn request_migration(target_ptr: *const u8, target_len: usize) -> i32;
}

fn do_predict(req: &InferenceRequest) -> Result<InferenceResponse, String> {
    let req_json = serde_json::to_vec(req).unwrap();
    let mut out_len: i32 = 4096;
    let mut out_buf = vec![0u8; out_len as usize];

    let mut res = unsafe {
        model_predict(
            req_json.as_ptr(),
            req_json.len(),
            out_buf.as_mut_ptr(),
            &mut out_len,
        )
    };

    if res == 2 { // Buffer too small
        out_buf.resize(out_len as usize, 0);
        res = unsafe {
            model_predict(
                req_json.as_ptr(),
                req_json.len(),
                out_buf.as_mut_ptr(),
                &mut out_len,
            )
        };
    }

    if res == 0 || res == 3 {
        // Success or crashed/OOF
        let response_bytes = &out_buf[..(out_len as usize)];
        if let Ok(resp) = serde_json::from_slice::<InferenceResponse>(response_bytes) {
            return Ok(resp);
        }
    }
    
    Err(format!("Prediction failed with code {}", res))
}

fn do_remember(collection: &str, record: &VectorRecord) -> i32 {
    let rec_json = serde_json::to_vec(record).unwrap();
    unsafe {
        remember(
            collection.as_ptr(),
            collection.len(),
            rec_json.as_ptr(),
            rec_json.len(),
        )
    }
}

fn main() {
    println!("Sovereign Research Agent starting...");
    let topic = env::var("RESEARCH_TOPIC").unwrap_or_else(|_| "Quantization of neural networks".to_string());
    
    // Attempt to parse out our assignment
    let req = InferenceRequest {
        model_alias: env::var("MODEL_ALIAS").unwrap_or_else(|_| "mock_llm".to_string()),
        prompt: format!("I am a researcher studying {{ {} }}. Produce a profound insight about this.", topic),
        temperature: 0.7,
        max_tokens: 128,
        stop_sequences: Vec::new(),
        session_id: Some("agent-session-1".to_string()),
        deterministic_seed: 42,
    };

    println!("Consulting Neural Engine on topic: {}", topic);
    match do_predict(&req) {
        Ok(reply) => {
            println!("Neural insight received within {} tokens.", reply.tokens_generated);
            println!("Insight: {}", reply.text);
            
            // Storing to memory
            let record = VectorRecord {
                id: format!("insight-{}", uuid::Uuid::new_v4()),
                vector: vec![0.1, 0.4, 0.5, -0.2], // In a real system, we'd call an embedding model here
                metadata: std::collections::HashMap::from([
                    ("topic".to_string(), topic.clone()),
                    ("content".to_string(), reply.text.clone()),
                ]),
            };
            
            let save_res = do_remember("insights", &record);
            if save_res == 0 {
                println!("Insight successfully embedded into Tiered Vector VFS.");
            } else {
                println!("Failed to embed insight into Vector VFS: {}", save_res);
            }
        }
        Err(e) => {
            println!("Neural inference failed: {}", e);
        }
    }

    println!("Agent cycle complete. Yielding.");
}
