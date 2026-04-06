use tet_core::crypto::AgentWallet;
use tet_core::registry::sovereign::{ArtifactManifest, SovereignRegistry, RegistryError};
use std::sync::Arc;
use uuid::Uuid;
use ed25519_dalek::{SigningKey, Signer};
use sha2::{Sha256, Digest};
use tet_core::hive::HivePeers;
use futures_util::future::BoxFuture;

pub struct MockGlobalRegistry {
    pub routes: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl MockGlobalRegistry {
    pub fn new() -> Self {
        Self {
            routes: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl tet_core::gateway::GlobalRegistry for MockGlobalRegistry {
    fn resolve_alias(&self, alias: &str) -> BoxFuture<'_, Result<Option<String>, tet_core::gateway::GatewayError>> {
        let routes = self.routes.lock().unwrap();
        let res = routes.get(alias).cloned();
        Box::pin(async move { Ok(res) })
    }
    
    fn update_route(
        &self,
        alias: &str,
        node_ip: &str,
        _signature: &str,
    ) -> BoxFuture<'_, Result<(), tet_core::gateway::GatewayError>> {
        let mut routes = self.routes.lock().unwrap();
        routes.insert(alias.to_string(), node_ip.to_string());
        Box::pin(async move { Ok(()) })
    }
}

#[tokio::test]
async fn test_signature_wall_rejection() {
    // 1. Node A creates a valid original manifest for Alias: "Alpha"
    let wallet_a = AgentWallet::load_or_create().unwrap();
    let dht = Arc::new(MockGlobalRegistry::new());
    
    // We mock the local Sovereign Registry to track valid signatures.
    let registry_a = SovereignRegistry::new(Arc::clone(&dht) as Arc<dyn tet_core::gateway::GlobalRegistry>);
    
    let wasm_bytes = b"original valid wasm binary";
    let wasm_hash = hex::encode(Sha256::digest(wasm_bytes));
    
    let mut manifest = ArtifactManifest {
        alias: "Alpha".to_string(),
        wasm_cid: wasm_hash.clone(),
        gene_layers: vec![],
        timestamp: 100,
        author_pubkey: wallet_a.public_key_hex(),
        signature: vec![],
    };
    
    let signature_payload = format!("{}:{}:{}", manifest.alias, manifest.wasm_cid, manifest.timestamp);
    manifest.signature = wallet_a.sign_bytes(signature_payload.as_bytes());
    
    // Push the original artifact (this simulates broadcasting to consensus)
    // and storing in global registry.
    let _cid_a = registry_a.push_artifact(manifest.clone(), wasm_bytes).await.expect("Original push should succeed");
    
    // 2. Node B (Malicious) tries to hijack the alias with a different Wasm binary.
    // Node B has a different private key.
    let key_bytes: [u8; 32] = rand::random();
    let signing_key_b = SigningKey::from_bytes(&key_bytes);
    
    // Malicious binary
    let malicious_bytes = b"malicious binary replacing alpha!";
    let malicious_hash = hex::encode(Sha256::digest(malicious_bytes));
    
    let mut bad_manifest = ArtifactManifest {
        alias: "Alpha".to_string(),
        wasm_cid: malicious_hash.clone(),
        gene_layers: vec![],
        timestamp: 105, // Malicious actor tries to update the timeline
        author_pubkey: wallet_a.public_key_hex(), // Impersonates author A pubkey inside struct
        signature: vec![],
    };
    
    // But signs it with Node B's private key because they don't have Node A's.
    let bad_sig_payload = format!("{}:{}:{}", bad_manifest.alias, bad_manifest.wasm_cid, bad_manifest.timestamp);
    bad_manifest.signature = signing_key_b.sign(bad_sig_payload.as_bytes()).to_bytes().to_vec();
    
    let result = registry_a.push_artifact(bad_manifest, malicious_bytes).await;
    
    match result {
        Err(RegistryError::SignatureVerificationFailed) => {
            // Test Passes: The system caught the malicious signature!
        },
        _ => panic!("Expected SignatureVerificationFailed error! Got: {:?}", result),
    }
}

#[tokio::test]
async fn test_p2p_locality_pull() {
    let dht = Arc::new(MockGlobalRegistry::new());
    let wallet_a = Arc::new(AgentWallet::load_or_create().unwrap());
    
    let _peers_a = HivePeers::new();
    let peers_b = HivePeers::new();
    
    // Add Node_A to Node_B's routing table manually
    peers_b.add_peer(tet_core::hive::HiveNodeIdentity {
        node_id: "node_A".to_string(),
        public_addr: "127.0.0.1:4001".to_string(),
        available_fuel: 1000,
        total_memory_mb: 1000,
        price_per_million_fuel: 1,
        min_reputation_score: 0,
        available_capacity_mb: 1000,
    }).await;
    
    let registry_a = SovereignRegistry::new(Arc::clone(&dht) as Arc<dyn tet_core::gateway::GlobalRegistry>);
    // Node A holds the artifact for Beta. We mock the cache insertion.
    let wasm_bytes = b"hello beta world";
    let wasm_hash = hex::encode(Sha256::digest(wasm_bytes));
    
    let mut manifest = ArtifactManifest {
        alias: "Beta".to_string(),
        wasm_cid: wasm_hash.clone(),
        gene_layers: vec![],
        timestamp: 100,
        author_pubkey: wallet_a.public_key_hex(),
        signature: vec![],
    };
    
    let signature_payload = format!("{}:{}:{}", manifest.alias, manifest.wasm_cid, manifest.timestamp);
    manifest.signature = wallet_a.sign_bytes(signature_payload.as_bytes());
    
    registry_a.push_artifact(manifest.clone(), wasm_bytes).await.unwrap();
    
    // Node B pulls
    let registry_b = SovereignRegistry::new(Arc::clone(&dht) as Arc<dyn tet_core::gateway::GlobalRegistry>);
    // We attach DHT routing table to Registry B explicitly for tests.
    // In actual the mesh lookup happens via HiveServer, but for tests we will verify Registry B successfully resolves alias to Node A and reads the chunk!
    
    // Note: Due to test boundaries, we will implement inner registry pulls inside the application logic. 
    // This is asserting the top level workflow.
    let (pulled_manifest, pulled_bytes) = registry_b.pull_artifact("Beta", Some(peers_b)).await.expect("Node B should pull Beta successfully");
    assert_eq!(pulled_manifest.alias, "Beta");
    assert_eq!(pulled_bytes, wasm_bytes);
}

#[tokio::test]
async fn test_gene_reconstruction() {
    let dht = Arc::new(MockGlobalRegistry::new());
    let wallet_a = AgentWallet::load_or_create().unwrap();
    let registry_a = SovereignRegistry::new(Arc::clone(&dht) as Arc<dyn tet_core::gateway::GlobalRegistry>);
    
    let wasm_bytes = b"reconstructing genes";
    let wasm_hash = hex::encode(Sha256::digest(wasm_bytes));
    
    let layer1 = Uuid::new_v4();
    let layer2 = Uuid::new_v4();
    let layer3 = Uuid::new_v4();
    
    let mut manifest = ArtifactManifest {
        alias: "Omega".to_string(),
        wasm_cid: wasm_hash.clone(),
        gene_layers: vec![layer1, layer2, layer3],
        timestamp: 100,
        author_pubkey: wallet_a.public_key_hex(),
        signature: vec![],
    };
    
    let signature_payload = format!("{}:{}:{}", manifest.alias, manifest.wasm_cid, manifest.timestamp);
    manifest.signature = wallet_a.sign_bytes(signature_payload.as_bytes());
    
    // We explicitly mock the gene layers in registry Dht.
    registry_a.push_artifact(manifest.clone(), wasm_bytes).await.unwrap();
    
    let registry_b = SovereignRegistry::new(Arc::clone(&dht) as Arc<dyn tet_core::gateway::GlobalRegistry>);
    let (pulled_manifest, _) = registry_b.pull_artifact_from_mesh("Omega", &manifest).await.expect("Should reconstruct");
    
    assert_eq!(pulled_manifest.gene_layers.len(), 3);
    assert_eq!(pulled_manifest.gene_layers[0], layer1);
    assert_eq!(pulled_manifest.gene_layers[2], layer3);
}
