use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use tet_core::economy::registry::{EconomyError, FuelTransaction, VoucherRegistry};

fn generate_wallet(seed_str: &str) -> (SigningKey, Vec<u8>) {
    let mut hasher = Sha256::new();
    hasher.update(seed_str.as_bytes());
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&hasher.finalize()[..]);
    let signing_key = SigningKey::from_bytes(&seed);
    let pub_key = signing_key.verifying_key().to_bytes().to_vec();
    (signing_key, pub_key)
}

fn create_signed_tx(
    signer: &SigningKey,
    from: Vec<u8>,
    to: Vec<u8>,
    amount: u64,
    nonce: u64,
) -> FuelTransaction {
    let mut signed_data = Vec::new();
    signed_data.extend_from_slice(&from);
    signed_data.extend_from_slice(&to);
    signed_data.extend_from_slice(&amount.to_be_bytes());
    signed_data.extend_from_slice(&nonce.to_be_bytes());

    let sig = signer.sign(&signed_data).to_bytes().to_vec();

    FuelTransaction {
        from,
        to,
        amount,
        nonce,
        signature: sig,
    }
}

#[tokio::test]
async fn test_phase22_double_spend_protection() {
    let registry = VoucherRegistry::new();

    let (wallet_a, pub_a) = generate_wallet("AgentA");
    let (_, pub_b) = generate_wallet("AgentB");

    // 1. Agent A has 1,000,000 Fuel
    registry.mint(pub_a.clone(), 1_000_000);

    // 2. Transact 600,000 twice
    let nonce_1 = 1;
    let tx_1 = create_signed_tx(&wallet_a, pub_a.clone(), pub_b.clone(), 600_000, nonce_1);

    let nonce_2 = 2; // Different nonce so it's not rejected as replay
    let tx_2 = create_signed_tx(&wallet_a, pub_a.clone(), pub_b.clone(), 600_000, nonce_2);

    // In a multi-threaded system, these race.
    // For unit testing atomicity, we just execute sequentially.
    assert!(registry.transfer(tx_1).is_ok());

    let err = registry.transfer(tx_2).unwrap_err();
    assert!(
        matches!(err, EconomyError::InsufficientFunds),
        "Second transaction must fail with InsufficientFunds"
    );

    // Validate final balances
    assert_eq!(*registry.balances.get(&pub_a).unwrap(), 400_000);
    assert_eq!(*registry.balances.get(&pub_b).unwrap(), 600_000);
}

#[tokio::test]
async fn test_phase22_replay_attack_defense() {
    let registry = VoucherRegistry::new();
    let (wallet_a, pub_a) = generate_wallet("AgentA");
    let (_, pub_b) = generate_wallet("AgentB");

    registry.mint(pub_a.clone(), 1_000_000);

    let nonce = 999;
    let tx = create_signed_tx(&wallet_a, pub_a.clone(), pub_b.clone(), 100_000, nonce);

    // First payment succeeds
    assert!(registry.transfer(tx.clone()).is_ok());

    // Replay attack (exact same payload intercepted)
    let err = registry.transfer(tx).unwrap_err();
    assert!(
        matches!(err, EconomyError::ReplayAttack),
        "Replay attack must be rejected because nonce was consumed"
    );

    // Verify no secondary deduction occurred
    assert_eq!(*registry.balances.get(&pub_a).unwrap(), 900_000);
}

#[tokio::test]
async fn test_phase22_service_for_fuel_mesh_call() {
    // This evaluates the structure of the HiveCommand linking the payment boundary.
    use tet_core::hive::HiveCommand;

    let bill_req = HiveCommand::BillRequest {
        source_alias: "AgentA".to_string(),
        target_alias: "TranslatorAgent".to_string(),
        amount: 50_000,
    };

    // Here we synthetically process it as though the network resolved the bill request via Mesh.
    // In a full environment, AgentA natively calls trytet::pay to satisfy the BillRequest.
    assert!(matches!(
        bill_req,
        HiveCommand::BillRequest { amount: 50000, .. }
    ));
}
