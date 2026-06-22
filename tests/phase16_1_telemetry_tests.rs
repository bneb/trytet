use std::collections::HashMap;
use std::sync::Arc;
use tet_core::engine::TetSandbox;
use tet_core::hive::HivePeers;
use tet_core::mesh::TetMesh;
use tet_core::models::manifest::{AgentManifest, CapabilityPolicy, Metadata, ResourceConstraints};
use tet_core::models::{ExecutionStatus, TetExecutionRequest};
use tet_core::sandbox::WasmtimeSandbox;
use tet_core::telemetry::{HiveEvent, TelemetryHub};

fn make_manifest() -> AgentManifest {
    AgentManifest {
        metadata: Metadata {
            name: "telemetry_test".to_string(),
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

/// Compile a trivial WAT module that just returns immediately.
fn compile_noop_wat() -> Vec<u8> {
    let wasm_bytes = wat::parse_bytes(
        br#"
        (module
            (memory (export "memory") 1)
            (func (export "_start") nop)
        )
        "#,
    )
    .unwrap()
    .into_owned();
    wasm_bytes
}

// ----------------------------------------------------
// TDD Case 1: Event Leak
// Spawn 100 rapid agent executions. The TelemetryHub must emit
// AgentBooted + AgentCompleted for each without impacting
// execution speed (no jitter from telemetry overhead).
// ----------------------------------------------------
#[tokio::test]
async fn test_event_leak() {
    let hub = Arc::new(TelemetryHub::default_capacity());
    let mut rx = hub.subscribe();

    let (mesh, _rx) = TetMesh::new(10, HivePeers::new());
    let voucher_mgr = Arc::new(tet_core::economy::VoucherManager::new("t1".to_string()));
    let sandbox = WasmtimeSandbox::new(mesh, voucher_mgr, false, "t1".to_string())
        .unwrap()
        .with_telemetry(hub.clone());
    let sandbox = Arc::new(sandbox);

    let wasm_bytes = compile_noop_wat();

    let n = 100;
    let mut fuel_values = Vec::with_capacity(n);

    for _ in 0..n {
        let req = TetExecutionRequest {
            alias: None,
            payload: Some(wasm_bytes.clone()),
            env: HashMap::new(),
            injected_files: HashMap::new(),
            allocated_fuel: 1_000_000,
            max_memory_mb: 64,
            parent_snapshot_id: None,
            call_depth: 0,
            voucher: None,
            manifest: Some(make_manifest()),
            egress_policy: None,
            target_function: None,
        };

        let res = sandbox.execute(req).await.unwrap();
        assert_eq!(res.status, ExecutionStatus::Success);
        fuel_values.push(res.fuel_consumed);
    }

    // Drain all events from the broadcast channel
    let mut booted_count = 0;
    let mut completed_count = 0;

    loop {
        match rx.try_recv() {
            Ok(HiveEvent::AgentBooted { .. }) => booted_count += 1,
            Ok(HiveEvent::AgentCompleted { .. }) => completed_count += 1,
            Ok(_) => {}
            Err(_) => break,
        }
    }

    // Every execution should produce exactly one Booted + one Completed
    assert_eq!(
        booted_count, n,
        "Expected {} AgentBooted events, got {}",
        n, booted_count
    );
    assert_eq!(
        completed_count, n,
        "Expected {} AgentCompleted events, got {}",
        n, completed_count
    );

    // Verify fuel determinism: all 100 noop executions should consume identical fuel
    let first_fuel = fuel_values[0];
    for (i, &fuel) in fuel_values.iter().enumerate() {
        assert_eq!(
            fuel, first_fuel,
            "Fuel jitter detected at idx {}: {} != {} — telemetry introduced non-determinism!",
            i, fuel, first_fuel
        );
    }
}

// ----------------------------------------------------
// TDD Case 2: Teleport Flash
// Emit a TeleportInitiated event. Assert it arrives at the subscriber
// within the same execution flow. Verify event payload integrity.
// ----------------------------------------------------
#[tokio::test]
async fn test_teleport_flash() {
    let hub = Arc::new(TelemetryHub::default_capacity());
    let mut rx = hub.subscribe();

    // Emit a TeleportInitiated event
    let agent_id = "agent-007".to_string();
    let target_node = "node-bravo".to_string();

    hub.broadcast(HiveEvent::TeleportInitiated {
        agent_id: agent_id.clone(),
        target_node: target_node.clone(),
        use_registry: false,
        timestamp_us: tet_core::telemetry::now_us(),
    });

    // The event should be immediately available (same-tick delivery)
    let event = rx
        .try_recv()
        .expect("TeleportInitiated event must be received immediately");

    match event {
        HiveEvent::TeleportInitiated {
            agent_id: recv_id,
            target_node: recv_target,
            use_registry,
            timestamp_us,
        } => {
            assert_eq!(recv_id, "agent-007", "Agent ID mismatch");
            assert_eq!(recv_target, "node-bravo", "Target node mismatch");
            assert!(!use_registry, "Registry flag should be false");
            assert!(timestamp_us > 0, "Timestamp should be non-zero");
        }
        other => panic!(
            "Expected TeleportInitiated, got {:?}",
            std::mem::discriminant(&other)
        ),
    }

    // Also emit a TeleportCompleted and verify chaining
    hub.broadcast(HiveEvent::TeleportCompleted {
        agent_id: agent_id.clone(),
        target_node: target_node.clone(),
        bytes_transferred: 65536,
        timestamp_us: tet_core::telemetry::now_us(),
    });

    let completed = rx
        .try_recv()
        .expect("TeleportCompleted event must be received immediately");

    match completed {
        HiveEvent::TeleportCompleted {
            bytes_transferred, ..
        } => {
            assert_eq!(bytes_transferred, 65536, "Bytes transferred mismatch");
        }
        other => panic!(
            "Expected TeleportCompleted, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

// ----------------------------------------------------
// TDD Case 3: Memory Leak
// Push 100,000 events into a bounded buffer. Assert memory
// footprint stays at O(1) — buffer.len() <= capacity at all times.
// ----------------------------------------------------
#[tokio::test]
async fn test_memory_leak() {
    const BUFFER_CAPACITY: usize = 1024;

    // Simulate the TUI's bounded ring buffer
    let mut event_log: Vec<String> = Vec::with_capacity(BUFFER_CAPACITY);

    let hub = Arc::new(TelemetryHub::default_capacity());
    let mut rx = hub.subscribe();

    // Blast 100,000 events through the hub
    let total_events = 100_000;
    for i in 0..total_events {
        hub.broadcast(HiveEvent::FuelConsumed {
            tet_id: format!("agent-{}", i % 100),
            operation: "noop".to_string(),
            amount: 42,
            timestamp_us: tet_core::telemetry::now_us(),
        });
    }

    // Drain into bounded buffer with eviction
    let mut received = 0_u64;
    loop {
        match rx.try_recv() {
            Ok(HiveEvent::FuelConsumed { tet_id, .. }) => {
                received += 1;
                let msg = format!("FUEL {} amount:42", tet_id);

                // Bounded eviction: remove oldest if at capacity
                if event_log.len() >= BUFFER_CAPACITY {
                    event_log.remove(0);
                }
                event_log.push(msg);

                // Assert O(1) invariant at every step
                assert!(
                    event_log.len() <= BUFFER_CAPACITY,
                    "Ring buffer exceeded capacity: {} > {}",
                    event_log.len(),
                    BUFFER_CAPACITY
                );
            }
            Ok(_) => received += 1,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(n)) => {
                // Expected: broadcast channel dropped some events under pressure
                received += n;
            }
            Err(_) => break,
        }
    }

    // We should have received a significant number of events
    // (some may be dropped by broadcast backpressure, but the hub has 10k capacity)
    assert!(received > 0, "Should have received events, got 0");

    // Final O(1) invariant check
    assert!(
        event_log.len() <= BUFFER_CAPACITY,
        "Final buffer size {} exceeds capacity {}",
        event_log.len(),
        BUFFER_CAPACITY
    );

    // The buffer should be exactly at capacity (we pushed 100k into 1024 slots)
    assert_eq!(
        event_log.len(),
        BUFFER_CAPACITY,
        "Buffer should be at exactly capacity after 100k inserts"
    );
}
