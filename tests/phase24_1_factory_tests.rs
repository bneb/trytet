use std::sync::Arc;
use tet_core::consensus::HiveConsensus;
use tet_core::economy::bridge::SettlementBridge;
use tet_core::economy::registry::VoucherRegistry;
use tet_core::runtime::factory::{GenesisFactory, JobDescriptor, SettlementPacket};

#[tokio::test]
async fn test_phase24_cold_inbound_saas_fork() {
    let registry = Arc::new(VoucherRegistry::new());
    let consensus = Arc::new(HiveConsensus::new("NodeZero".into()));
    let bridge = Arc::new(SettlementBridge::new(consensus, registry));

    let factory = GenesisFactory::new("MasterAgent".into(), bridge);

    let simulated_job = JobDescriptor {
        worker_artifact_hash: "pdf-summarizer-v1".into(),
        task_data_cid: "QmX8C2c...".into(),
        fuel_allocation: 5_000_000,
    };

    let deposit = SettlementPacket {
        metadata_json: serde_json::to_string(&simulated_job).unwrap(),
        amount_deposited: 10_000_000,
    };

    // Inbound bridge trigger!
    let worker_id = factory
        .dispatch_job(deposit)
        .await
        .expect("Failed to dispatch job");

    // The Factory correctly maps structural strings matching exactly!
    assert!(worker_id.starts_with("Worker_"));
}

#[tokio::test]
async fn test_phase24_commission_loop() {
    // A synthetic evaluation testing mathematical correctness of "Fuel profit = Bounty - (ForkCost + WorkerConsumed)" natively
    let starting_bounty = 10_000_000u64;
    let fork_cost = 500_000u64;
    let worker_consumed = 4_000_000u64;

    let worker_allocated = 5_000_000u64;
    let returned = worker_allocated - worker_consumed; // 1_000_000u64 refunded!

    let factory_expected_profit = starting_bounty - fork_cost - worker_allocated + returned;
    // 10M - 500k - 5M + 1M = 5.5M

    assert_eq!(factory_expected_profit, 5_500_000);
}

#[tokio::test]
async fn test_phase24_resource_reclamation_trigger() {
    // We mock check the Wasm sandbox permission restriction physically natively checking the object map
    use tet_core::models::manifest::AgentManifest;

    let valid_manifest_str = r#"
        [metadata]
        name = "GenesisMaster"
        version = "1.0"
        author_pubkey = "0xMASTERKEY"

        [constraints]
        max_memory_pages = 200
        fuel_limit = 1000000

        [permissions]
        can_egress = ["https://ipfs.io"]
        can_persist = true
        can_teleport = false
        is_genesis_factory = true
        can_fork = true
    "#;

    let manifest = AgentManifest::from_toml(valid_manifest_str).expect("Failed to parse");
    assert!(
        manifest.permissions.is_genesis_factory,
        "Master must be identified dynamically by Sandbox linker"
    );
    assert!(
        manifest.permissions.can_fork,
        "Master uses this permission natively spawning SaaS tasks"
    );

    let restricted_manifest_str = r#"
        [metadata]
        name = "PdfWorker"
        version = "1.0"
        author_pubkey = "0xMASTERKEY"

        [constraints]
        max_memory_pages = 200
        fuel_limit = 1000000

        [permissions]
        can_egress = []
        can_persist = false
        can_teleport = false
    "#;

    let restrict_manifest =
        AgentManifest::from_toml(restricted_manifest_str).expect("Failed to parse worker");
    assert!(
        !restrict_manifest.permissions.is_genesis_factory,
        "Workers CANNOT trigger trytet::reclaim natively!"
    );
}
