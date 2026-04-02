use wasm_bindgen::prelude::*;
use tet_core::sandbox::SnapshotPayload;
use js_sys::Uint8Array;

/// BrowserEngine — the Wasm-native bridge for Trytet's snapshot primitives.
///
/// This compiles to .wasm via wasm-pack and exposes real bincode
/// serialization of SnapshotPayload — the exact same binary format
/// used by the server-side WasmtimeSandbox. A blob produced here
/// can be imported on any Trytet node via POST /v1/tet/import.
#[wasm_bindgen]
pub struct BrowserEngine {
    _private: (),
}

#[wasm_bindgen]
impl BrowserEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        Self { _private: () }
    }

    /// Serialize arbitrary state bytes into a portable SnapshotPayload via bincode.
    ///
    /// The state bytes are packed into `SnapshotPayload.memory_bytes` — the same
    /// field that the Wasmtime sandbox uses to capture Wasm linear memory.
    /// The returned Uint8Array is a real bincode blob, byte-compatible with
    /// the server-side engine.
    #[wasm_bindgen]
    pub fn snapshot_state(&self, state_bytes: &[u8]) -> Result<Uint8Array, JsValue> {
        let payload = SnapshotPayload {
            memory_bytes: state_bytes.to_vec(),
            wasm_bytes: vec![],
            fs_tarball: vec![],
            vector_idx: vec![],
            inference_state: vec![],
        };

        let encoded = bincode::serialize(&payload)
            .map_err(|e| JsValue::from_str(&format!("bincode serialize: {}", e)))?;

        let arr = Uint8Array::new_with_length(encoded.len() as u32);
        arr.copy_from(&encoded);
        Ok(arr)
    }

    /// Deserialize a bincode SnapshotPayload and extract the contained state bytes.
    ///
    /// This proves round-trip fidelity: the exact bytes you snapshotted come back
    /// unchanged. The same blob could be POSTed to /v1/tet/import on any Trytet node.
    #[wasm_bindgen]
    pub fn restore_state(&self, bincode_bytes: &[u8]) -> Result<Uint8Array, JsValue> {
        let payload: SnapshotPayload = bincode::deserialize(bincode_bytes)
            .map_err(|e| JsValue::from_str(&format!("bincode deserialize: {}", e)))?;

        let arr = Uint8Array::new_with_length(payload.memory_bytes.len() as u32);
        arr.copy_from(&payload.memory_bytes);
        Ok(arr)
    }

    /// Validate that a binary blob is a structurally valid SnapshotPayload.
    /// Returns the memory_bytes length on success, or throws on invalid format.
    #[wasm_bindgen]
    pub fn validate_snapshot(&self, bincode_bytes: &[u8]) -> Result<u32, JsValue> {
        let payload: SnapshotPayload = bincode::deserialize(bincode_bytes)
            .map_err(|e| JsValue::from_str(&format!("invalid snapshot: {}", e)))?;
        Ok(payload.memory_bytes.len() as u32)
    }
}
