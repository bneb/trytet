use std::sync::Arc;
use tet_core::engine::TetSandbox;
use tet_core::mesh::TetMesh;
use tet_core::models::TetExecutionRequest;
use tet_core::sandbox::{SnapshotPayload, WasmtimeSandbox};
use tokio::time::{sleep, Duration, Instant};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tar_extraction_starvation() {
    let peers = tet_core::hive::HivePeers::new();
    let (mesh, _rx) = TetMesh::new(100, peers);
    let vm = Arc::new(tet_core::economy::VoucherManager::new("test".to_string()));
    let sandbox = Arc::new(
        WasmtimeSandbox::new(mesh.clone(), vm, false, "test_node_id".to_string()).unwrap(),
    );

    // Create a dummy large tarball (e.g., 20MB of zeroes)
    let mut tar_bytes = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        let mut header = tar::Header::new_gnu();
        header.set_path("huge_file.bin").unwrap();
        header.set_size(20 * 1024 * 1024);
        header.set_cksum();
        let zeros = vec![0u8; 20 * 1024 * 1024];
        builder.append(&header, &zeros[..]).unwrap();
        builder.finish().unwrap();
    }

    let _snap_id = "test_snapshot_id".to_string();
    let payload = SnapshotPayload {
        wasm_bytes: vec![0, 97, 115, 109, 1, 0, 0, 0], // minimal dummy Wasm header
        memory_bytes: vec![],
        fs_tarball: tar_bytes,
        inference_state: vec![],
        vector_idx: vec![],
    };

    // Assuming we can forcefully import snapshot (sandbox.snapshots is public or has a method)
    // Actually sandbox.import_snapshot is pub
    let snap_id = sandbox.import_snapshot(payload).await.unwrap();

    let req = TetExecutionRequest {
        alias: Some("test_alias".to_string()),
        payload: None,
        parent_snapshot_id: Some(snap_id.clone()),
        allocated_fuel: 10000,
        max_memory_mb: 10,
        env: Default::default(),
        injected_files: Default::default(),
        call_depth: 0,
        voucher: None,
        egress_policy: None,
        target_function: None,
        manifest: None,
    };

    let sandbox_clone = sandbox.clone();

    // Spawn a rigid interval timer to detect starvation
    let task_timer = tokio::spawn(async move {
        let mut max_jitter = Duration::from_secs(0);
        for _ in 0..10 {
            let before = Instant::now();
            sleep(Duration::from_millis(15)).await;
            let elapsed = before.elapsed();
            if elapsed > Duration::from_millis(15) {
                let jitter = elapsed - Duration::from_millis(15);
                if jitter > max_jitter {
                    max_jitter = jitter;
                }
            }
        }
        max_jitter
    });

    // Run the extraction
    let _ = sandbox_clone.fork(&snap_id, req).await;

    let max_jitter = task_timer.await.unwrap();

    // Make sure our background async tasks weren't starved by sync tar unpack
    println!("Max jitter during extraction: {:?}", max_jitter);
    assert!(
        max_jitter < Duration::from_millis(500),
        "Timer was completely starved by synchronous VFS extraction!"
    );
}
