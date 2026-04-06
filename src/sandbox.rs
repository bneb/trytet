use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotPayload {
    pub memory_bytes: Vec<u8>,
    pub wasm_bytes: Vec<u8>,
    pub fs_tarball: Vec<u8>,
    pub vector_idx: Vec<u8>,
    #[serde(default)]
    pub inference_state: Vec<u8>,
}

#[cfg(not(target_arch = "wasm32"))]
pub mod sandbox_wasmtime;
#[cfg(not(target_arch = "wasm32"))]
pub use sandbox_wasmtime::*;

#[cfg(target_arch = "wasm32")]
mod sandbox_polyfill;
#[cfg(target_arch = "wasm32")]
pub use sandbox_polyfill::*;

pub mod security;
