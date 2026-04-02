use wasm_bindgen::prelude::*;
use tet_core::sandbox::{SnapshotPayload, WebNativeSandbox};
use tet_core::engine::TetSandbox;

#[wasm_bindgen]
pub struct BrowserEngine {
    sandbox: WebNativeSandbox,
}

#[wasm_bindgen]
impl BrowserEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        Self {
            sandbox: WebNativeSandbox::new(),
        }
    }

    #[wasm_bindgen]
    pub async fn import_snapshot(&self, bincode_payload: &[u8]) -> Result<String, JsValue> {
        let payload: SnapshotPayload = bincode::deserialize(bincode_payload)
            .map_err(|e| JsValue::from_str(&format!("Bincode err: {}", e)))?;

        self.sandbox
            .import_snapshot(payload)
            .await
            .map_err(|e| JsValue::from_str(&format!("Tet error: {:?}", e)))
    }
}
