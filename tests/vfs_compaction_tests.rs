use std::time::Instant;
use tet_core::memory::{VectorRecord, VectorVfs};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_vfs_concurrent_compaction() {
    let vfs = VectorVfs::new();
    // Background worker starts automatically in new()

    let vfs = std::sync::Arc::new(vfs);
    let mut handles = Vec::new();

    // Simulate 10 concurrent agents all triggering the 2048 boundary simultaneously.
    for t in 0..10 {
        let vfs_clone = vfs.clone();
        handles.push(tokio::spawn(async move {
            let start = Instant::now();
            let col = format!("collection_bench"); // Use the same collection or distinct?
                                                   // Using same collection forces contention!
            for i in 0..250 {
                // 250 * 10 = 2500 breaches 2048
                let vec = vec![0.1; 64];
                let record = VectorRecord {
                    id: format!("rec_{}_{}", t, i),
                    vector: vec,
                    metadata: std::collections::HashMap::new(),
                };
                vfs_clone.remember(&col, record);
            }
            start.elapsed()
        }));
    }

    let mut max_dur = std::time::Duration::from_secs(0);
    for h in handles {
        let dur = h.await.unwrap();
        if dur > max_dur {
            max_dur = dur;
        }
    }

    println!("Max duration for 2500 inserts: {:?}", max_dur);
    assert!(
        max_dur < std::time::Duration::from_millis(500),
        "remember() stalled caller!"
    );
    drop(vfs);
}
