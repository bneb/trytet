use std::collections::HashMap;
use std::sync::Arc;
use tet_core::engine::TetSandbox;
use tet_core::hive::HivePeers;
use tet_core::mesh::TetMesh;
use tet_core::models::manifest::{AgentManifest, CapabilityPolicy, Metadata, ResourceConstraints};
use tet_core::models::{ExecutionStatus, TetExecutionRequest};
use tet_core::sandbox::WasmtimeSandbox;

/// Helper: compile a WAT module that calls trytet::model_predict.
///
/// The module writes a JSON InferenceRequest to guest memory, calls model_predict,
/// and drops the result code. Uses hex-escaped bytes to avoid WAT string quoting issues.
fn compile_model_predict_wat(inference_json: &str, buf_size: i32) -> Vec<u8> {
    let json_len = inference_json.len();
    let json_hex: String = inference_json
        .bytes()
        .map(|b| format!("\\{b:02x}"))
        .collect();

    let buf_bytes = buf_size.to_le_bytes();
    let buf_hex: String = buf_bytes.iter().map(|b| format!("\\{b:02x}")).collect();

    let wat = format!(
        r#"
(module
  (import "trytet" "model_predict" (func $model_predict (param i32 i32 i32 i32) (result i32)))
  (import "trytet" "model_load" (func $model_load (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 2)
  ;; InferenceRequest JSON at offset 0 (hex-encoded)
  (data (i32.const 0) "{json_hex}")
  ;; Model alias "mock-model" at offset 16384
  (data (i32.const 16384) "mock-model")
  ;; Model path "/dev/null" at offset 16400
  (data (i32.const 16400) "/dev/null")
  ;; out_len_ptr at 8192: buffer size (little-endian i32)
  (data (i32.const 8192) "{buf_hex}")

  (func (export "_start")
    ;; First load the model
    (call $model_load
      (i32.const 16384) ;; alias_ptr "mock-model"
      (i32.const 10)    ;; alias_len
      (i32.const 16400) ;; path_ptr "/dev/null"
      (i32.const 9)     ;; path_len
    )
    (drop)

    ;; Then call model_predict
    (call $model_predict
      (i32.const 0)     ;; request_ptr (InferenceRequest JSON)
      (i32.const {json_len}) ;; request_len
      (i32.const 4096)  ;; out_ptr (response will be written here)
      (i32.const 8192)  ;; out_len_ptr (buffer size)
    )
    (drop)
  )
)
"#
    );
    wat.into_bytes()
}

/// Construct the standard test InferenceRequest JSON for "What is 2+2?"
fn make_inference_json(
    model_alias: &str,
    prompt: &str,
    temperature: f32,
    max_tokens: u32,
) -> String {
    serde_json::json!({
        "model_alias": model_alias,
        "prompt": prompt,
        "temperature": temperature,
        "max_tokens": max_tokens,
        "stop_sequences": [],
        "deterministic_seed": 42
    })
    .to_string()
}

fn make_manifest() -> AgentManifest {
    AgentManifest {
        metadata: Metadata {
            name: "test_inference".to_string(),
            version: "1".to_string(),
            author_pubkey: None,
        },
        constraints: ResourceConstraints {
            max_memory_pages: 10,
            fuel_limit: 50_000_000,
            max_egress_bytes: 1_000_000,
        },
        permissions: CapabilityPolicy {
            can_egress: vec![],
            can_persist: false,
            can_teleport: false,
            is_genesis_factory: false,
            can_fork: false,
        },
    }
}

// ----------------------------------------------------
// TDD Case 1: Token Billing
// ----------------------------------------------------
#[tokio::test]
async fn test_token_billing() {
    let (mesh, _rx) = TetMesh::new(10, HivePeers::new());
    let voucher_mgr = Arc::new(tet_core::economy::VoucherManager::new("t1".to_string()));
    let sandbox =
        Arc::new(WasmtimeSandbox::new(mesh, voucher_mgr, false, "t1".to_string()).unwrap());

    let inference_json = make_inference_json("mock-model", "What is 2+2?", 0.7, 256);
    let wat = compile_model_predict_wat(&inference_json, 32768);
    let wasm_bytes = wat::parse_bytes(&wat).unwrap().into_owned();

    let allocated_fuel: u64 = 10_000_000;
    let req = TetExecutionRequest {
        alias: None,
        payload: Some(wasm_bytes),
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel,
        max_memory_mb: 64,
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        manifest: Some(make_manifest()),
        egress_policy: None,
        target_function: None,
    };

    let res = sandbox.execute(req).await.unwrap();
    assert_eq!(
        res.status,
        ExecutionStatus::Success,
        "Execution should succeed, got {:?}",
        res.status
    );

    // The MockInferenceProvider returns "The answer is 4." for "2+2"
    // Input tokens  = max(1, ceil("What is 2+2?".len() / 4)) = max(1, ceil(12/4)) = 3
    // Output tokens = max(1, ceil("The answer is 4.".len() / 4)) = max(1, ceil(16/4)) = 4
    //
    // Fuel formula: (InputTokens + OutputTokens) × C_TOKEN_WEIGHT + C_BASE_OVERHEAD
    //             = (3 + 4) × 30 + 5000
    //             = 210 + 5000
    //             = 5210
    //
    // Plus model_load cost: 10_000
    // Total inference fuel = 5_210 + 10_000 = 15_210
    //
    // But fuel_consumed also includes Wasm instruction execution overhead,
    // so we verify the inference portion is correctly deducted.
    let expected_inference_fuel: u64 = (3 + 4) * 30 + 5_000; // = 5210
    let model_load_fuel: u64 = 10_000;

    // fuel_consumed must be at least inference_fuel + model_load_fuel
    assert!(
        res.fuel_consumed >= expected_inference_fuel + model_load_fuel,
        "Fuel consumed ({}) must be >= inference fuel ({}) + model_load ({})",
        res.fuel_consumed,
        expected_inference_fuel,
        model_load_fuel
    );

    // Verify determinism: run again, fuel should be identical
    let inference_json = make_inference_json("mock-model", "What is 2+2?", 0.7, 256);
    let wat = compile_model_predict_wat(&inference_json, 32768);
    let wasm_bytes = wat::parse_bytes(&wat).unwrap().into_owned();

    let req2 = TetExecutionRequest {
        alias: None,
        payload: Some(wasm_bytes),
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel,
        max_memory_mb: 64,
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        manifest: Some(make_manifest()),
        egress_policy: None,
        target_function: None,
    };

    let res2 = sandbox.execute(req2).await.unwrap();
    assert_eq!(
        res.fuel_consumed, res2.fuel_consumed,
        "Fuel consumption must be perfectly deterministic across identical inference calls!"
    );
}

// ----------------------------------------------------
// TDD Case 2: Thought Replay
// ----------------------------------------------------
#[tokio::test]
async fn test_thought_replay() {
    // Node A
    let (mesh_a, _rx) = TetMesh::new(10, HivePeers::new());
    let v_mgr = Arc::new(tet_core::economy::VoucherManager::new("t1".to_string()));
    let sandbox_a =
        Arc::new(WasmtimeSandbox::new(mesh_a, v_mgr.clone(), false, "node-a".to_string()).unwrap());

    // Node B
    let (mesh_b, _rx) = TetMesh::new(10, HivePeers::new());
    let sandbox_b =
        Arc::new(WasmtimeSandbox::new(mesh_b, v_mgr, false, "node-b".to_string()).unwrap());

    let inference_json = make_inference_json("mock-model", "What is 2+2?", 0.7, 256);
    let wat = compile_model_predict_wat(&inference_json, 32768);
    let wasm_bytes = wat::parse_bytes(&wat).unwrap().into_owned();

    // Execute on Node A
    let req_a = TetExecutionRequest {
        alias: Some("thought-agent".to_string()),
        payload: Some(wasm_bytes.clone()),
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel: 10_000_000,
        max_memory_mb: 64,
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        manifest: Some(make_manifest()),
        egress_policy: None,
        target_function: None,
    };

    let res_a = sandbox_a.execute(req_a).await.unwrap();
    assert_eq!(res_a.status, ExecutionStatus::Success);

    // Snapshot and teleport to Node B
    let snapshot_id = sandbox_a.snapshot("thought-agent").await.unwrap();
    let payload = sandbox_a.export_snapshot(&snapshot_id).await.unwrap();
    let imported_id = sandbox_b.import_snapshot(payload).await.unwrap();

    // Execute on Node B with the parent snapshot (cache should be in the VFS tarball)
    let req_b = TetExecutionRequest {
        alias: Some("thought-agent-b".to_string()),
        payload: Some(wasm_bytes.clone()),
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel: 10_000_000,
        max_memory_mb: 64,
        parent_snapshot_id: Some(imported_id),
        call_depth: 0,
        voucher: None,
        manifest: Some(make_manifest()),
        egress_policy: None,
        target_function: None,
    };

    let res_b = sandbox_b.execute(req_b).await.unwrap();
    assert_eq!(res_b.status, ExecutionStatus::Success);

    // Both executions must consume identical fuel (deterministic Oracle replay)
    assert_eq!(
        res_a.fuel_consumed, res_b.fuel_consumed,
        "Fuel must be identical: Node A consumed {} but Node B consumed {}",
        res_a.fuel_consumed, res_b.fuel_consumed
    );
}

// ----------------------------------------------------
// TDD Case 3: Context Overflow Trap
// ----------------------------------------------------
#[tokio::test]
async fn test_context_overflow_trap() {
    let (mesh, _rx) = TetMesh::new(10, HivePeers::new());
    let voucher_mgr = Arc::new(tet_core::economy::VoucherManager::new("t1".to_string()));
    let sandbox =
        Arc::new(WasmtimeSandbox::new(mesh, voucher_mgr, false, "t1".to_string()).unwrap());

    // MockInferenceProvider has context_limit = 4096 tokens
    // Create a prompt that is WAY over 4096 tokens (4096 * 4 = 16384 chars minimum)
    // With the 1.15x safety factor, even a smaller prompt should trigger overflow
    let huge_prompt = "x".repeat(20_000); // ~5000 tokens * 1.15 = ~5750 >> 4096

    let inference_json = make_inference_json("mock-model", &huge_prompt, 0.7, 256);
    let wat = compile_model_predict_wat(&inference_json, 32768);
    let wasm_bytes = wat::parse_bytes(&wat).unwrap().into_owned();

    let req = TetExecutionRequest {
        alias: None,
        payload: Some(wasm_bytes),
        env: HashMap::new(),
        injected_files: HashMap::new(),
        allocated_fuel: 10_000_000,
        max_memory_mb: 64,
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        manifest: Some(make_manifest()),
        egress_policy: None,
        target_function: None,
    };

    let res = sandbox.execute(req).await.unwrap();

    // The execution should succeed (no Host crash), but the model_predict
    // host function should have returned error code 7 (ContextExceeded).
    // Since the WAT module drops the return code, execution completes normally.
    // The key assertion: no crash, no infrastructure error.
    assert_eq!(
        res.status,
        ExecutionStatus::Success,
        "Context overflow must be handled gracefully, got {:?}",
        res.status
    );

    // Additional verification: the fuel consumed should be LESS than
    // a normal inference call because model_predict returned early with code 7
    // (only model_load fuel + wasm overhead, no inference billing).
    let model_load_fuel: u64 = 10_000;
    let max_expected_fuel = model_load_fuel + 100_000; // generous Wasm overhead
    assert!(
        res.fuel_consumed < max_expected_fuel,
        "Context overflow should consume minimal fuel ({}), but consumed {}",
        max_expected_fuel,
        res.fuel_consumed
    );
}
