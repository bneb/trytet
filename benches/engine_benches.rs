use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;

use tet_core::economy::VoucherManager;
use tet_core::engine::TetSandbox;
use tet_core::hive::HivePeers;
use tet_core::memory::{SearchQuery, VectorRecord, VectorVfs};
use tet_core::mesh::TetMesh;
use tet_core::models::TetExecutionRequest;
use tet_core::sandbox::{SnapshotPayload, WasmtimeSandbox};

fn build_sandbox() -> Arc<WasmtimeSandbox> {
    let hive_peers = HivePeers::new();
    let (mesh, call_rx) = TetMesh::new(10, hive_peers);
    let sandbox = Arc::new(
        WasmtimeSandbox::new(
            mesh.clone(),
            Arc::new(VoucherManager::new("test-node".to_string())),
            false,
            "test-node".to_string(),
        )
        .expect("Failed to init sandbox"),
    );
    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), call_rx);
    sandbox
}

async fn run_concurrent_invokes(scale: usize, wasm_bytes: Vec<u8>, sandbox: Arc<WasmtimeSandbox>) {
    let mut handles = vec![];
    for i in 0..scale {
        let req = TetExecutionRequest {
            payload: Some(wasm_bytes.clone()),
            alias: Some(format!("bench-agent-{}", i)),
            env: HashMap::new(),
            injected_files: HashMap::new(),
            allocated_fuel: 1_000_000,
            max_memory_mb: 10,
            parent_snapshot_id: None,
            call_depth: 0,
            voucher: None,
            egress_policy: None,
        };
        let sbox = sandbox.clone();
        handles.push(tokio::spawn(async move {
            let _ = TetSandbox::execute(&*sbox, req).await;
        }));
    }

    for h in handles {
        let _ = h.await;
    }
}

fn bench_wasm_invoke(c: &mut Criterion) {
    let mut group = c.benchmark_group("wasm_invoke_concurrency");
    group.sample_size(10);

    let wat = r#"
        (module
            (memory (export "memory") 1)
            (func (export "run") (result i32)
                (i32.const 42)
            )
        )
    "#;
    let wasm = wat::parse_str(wat).unwrap();

    let rt = Runtime::new().unwrap();
    let _guard = rt.enter();
    let sbox = build_sandbox();

    for size in [20, 80, 320].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &scale| {
            b.to_async(&rt)
                .iter(|| run_concurrent_invokes(scale, wasm.clone(), sbox.clone()));
        });
    }
    group.finish();
}

async fn run_vector_vfs(scale: usize, vfs: Arc<VectorVfs>) {
    let mut handles = vec![];

    for i in 0..scale {
        let vfs_clone = vfs.clone();
        handles.push(tokio::spawn(async move {
            let vec_data = vec![0.5 + (i as f32) * 0.001; 64];
            vfs_clone.remember(
                "default",
                VectorRecord {
                    id: format!("fact_{}", i),
                    vector: vec_data.clone(),
                    metadata: std::collections::HashMap::new(),
                },
            );

            let _ = vfs_clone.recall(&SearchQuery {
                collection: "default".to_string(),
                query_vector: vec_data,
                limit: 1,
                min_score: 0.0,
            });
        }));
    }

    for h in handles {
        let _ = h.await;
    }
}

fn bench_vector_vfs(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_vfs_concurrency");
    group.sample_size(10);
    let rt = Runtime::new().unwrap();

    let db = Arc::new(VectorVfs::new());

    for size in [20, 80, 320].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &scale| {
            b.to_async(&rt).iter(|| run_vector_vfs(scale, db.clone()));
        });
    }
    group.finish();
}

fn bench_teleport_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("teleport_encode_bincode");
    group.sample_size(10);

    let sim_payload = SnapshotPayload {
        wasm_bytes: vec![0u8; 1024 * 1024 * 2],   // 2MB Wasm
        memory_bytes: vec![0u8; 1024 * 1024 * 5], // 5MB Heap
        fs_tarball: vec![0u8; 1024 * 1024],       // 1MB disk
        vector_idx: vec![0u8; 1024 * 500],        // 500KB semantic graph
        inference_state: vec![0u8; 1024 * 100],   // 100KB KV state
    };

    let total_bytes = sim_payload.wasm_bytes.len()
        + sim_payload.memory_bytes.len()
        + sim_payload.fs_tarball.len()
        + sim_payload.vector_idx.len()
        + sim_payload.inference_state.len();

    group.throughput(Throughput::Bytes(total_bytes as u64));

    group.bench_function("bincode_serialize_large", |b| {
        b.iter(|| bincode::serialize(&sim_payload).unwrap())
    });
    group.finish();
}

use tet_core::api::context::models::{BlockType, InputContentBlock, SwarmSession};
use tet_core::api::context::router::{ContextRouter, EvictionStrategy};

fn bench_context_router(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_router_eviction");
    group.sample_size(10);

    // Create a 10,000 block session
    let mut blocks = vec![InputContentBlock::new(
        BlockType::System,
        "SYSTEM".to_string(),
    )];
    for i in 0..10_000 {
        blocks.push(InputContentBlock::new(
            if i % 2 == 0 { BlockType::User } else { BlockType::Assistant },
            "This is a standard length sentence expected in conversational AI histories. It provides enough characters to simulate real context padding.".to_string()
        ));
    }

    group.bench_function("prune_10k_blocks", |b| {
        b.iter(|| {
            let mut session = SwarmSession {
                session_id: "bench_session".to_string(),
                blocks: blocks.clone(),
            };
            let router = ContextRouter {
                max_tokens: 16000,
                strategy: EvictionStrategy::Hybrid,
            };
            let _ = router.optimize(&mut session);
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_wasm_invoke,
    bench_vector_vfs,
    bench_teleport_encode,
    bench_context_router
);
criterion_main!(benches);
