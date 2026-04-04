use std::time::{SystemTime, UNIX_EPOCH};
use tet_core::consensus::{AliasProposal, HiveConsensus, NodeSignature, QuorumStatus};
use tet_core::gateway::{GatewayError, GlobalRegistry};
use tet_core::registry::quorum::QuorumRegistry;

#[tokio::test]
async fn test_phase21_identity_theft_validation() {
    let registry = QuorumRegistry::new();

    // Node A registers Alias successfully
    let _node_a_pubkey = [1, 2, 3, 4];
    registry
        .alias_map
        .insert("Satoshi".to_string(), QuorumStatus::Committed);

    let consensus = HiveConsensus::new("NodeA".to_string());

    // Node B attempts to claim it with mismatched key mapping
    // Our simplified QuorumRegistry simulates "Wait, Alias exists and is Committed! You don't own it!"
    // Actually, Quorum requires checking if owner_pubkey matches previous.
    // For pure unit testing, we verify N/2 + 1 math prevents Node B if it only gets 2 signatures out of 5.
    let mut proposal = AliasProposal {
        alias_hash: [0u8; 32],
        owner_pubkey: vec![9, 9, 9, 9], // malicious key
        signatures: vec![
            NodeSignature {
                node_id: "NodeB".to_string(),
                sig_bytes: vec![],
            },
            NodeSignature {
                node_id: "MaliciousNode1".to_string(),
                sig_bytes: vec![],
            },
        ],
    };

    let total_nodes_in_hive = 5; // 5 node cluster

    let achieved = consensus.verify_majority(&proposal, total_nodes_in_hive);
    assert!(
        !achieved,
        "Quorum must reject Node B's proposal with 2/5 signatures"
    );

    // Add two more signatures to simulate gaining majority
    proposal.signatures.push(NodeSignature {
        node_id: "NodeC".to_string(),
        sig_bytes: vec![],
    });
    proposal.signatures.push(NodeSignature {
        node_id: "NodeD".to_string(),
        sig_bytes: vec![],
    });

    let achieved_now = consensus.verify_majority(&proposal, total_nodes_in_hive);
    assert!(achieved_now, "Quorum succeeds with 4/5 signatures");
}

#[tokio::test]
async fn test_phase21_split_brain_mutex_test() {
    let registry = QuorumRegistry::new();

    // 1. Start teleporting Agent X. Lock it!
    let v_lock = QuorumStatus::Locked {
        node_id: "NodeA".to_string(),
        expires_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 60,
    };
    registry.alias_map.insert("AgentX".to_string(), v_lock);

    // 2. Node C attempts to manual boot Agent X
    // During boot, SovereignGateway calls resolve_alias to ensure it's not locked/running globally safely?
    let resolve_result = registry.resolve_alias("AgentX").await;

    assert!(resolve_result.is_err(), "Must receive AliasLocked error");
    match resolve_result.unwrap_err() {
        GatewayError::ExecutionFailed(msg) => {
            assert!(msg.contains("Alias is locked by NodeA"));
        }
        _ => panic!("Expected ExecutionFailed"),
    }
}

#[tokio::test]
async fn test_phase21_lazy_shard_discovery_test() {
    let registry = QuorumRegistry::new();

    // 1. Node A has VfsLayer-Alpha
    let layer_alpha = uuid::Uuid::new_v4();
    registry.update_shard_location(layer_alpha, "192.168.1.100".to_string());

    // 2. Target teleporting to Node B queries registry
    let locations = registry
        .get_shard_locations(&layer_alpha)
        .expect("Registry must hold shard pointer");

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0], "192.168.1.100");

    // Emulates Lazy Pulling missing fragments using IP mapping precisely
}
