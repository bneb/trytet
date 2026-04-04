pub mod bridge;
pub mod registry;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Defines a cryptographic authorization to consume Wasm computational fuel.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FuelVoucher {
    pub agent_id: String,      // Ed25519 PubKey of the Agent (hex encoded)
    pub provider_id: String,   // Ed25519 PubKey of the Compute Node (hex encoded)
    pub fuel_limit: u64,       // Total authorized instructions
    pub expiry_timestamp: u64, // Unix epoch expiry
    pub nonce: String,         // Replay attack prevention UUID
    pub signature: Vec<u8>,    // Signed by the Agent or Payment Gateway
}

/// Advertising schema for nodes participating in the Tet-Hive.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MarketOffer {
    pub node_id: String,
    pub price_per_million_fuel: u64,
    pub min_reputation_score: u32,
    pub available_capacity_mb: u32,
}

/// Thread-safe manager for tracking exhausted vouchers to eliminate replay attacks locally.
pub struct VoucherManager {
    used_nonces: RwLock<HashSet<String>>,
    expected_provider_id: String,
}

impl VoucherManager {
    pub fn new(expected_provider_id: String) -> Self {
        Self {
            used_nonces: RwLock::new(HashSet::new()),
            expected_provider_id,
        }
    }

    /// Verifies the mathematical integrity of the voucher (Signature, Expiry, Provider)
    /// and ensures the Nonce hasn't been replayed.
    pub fn verify_and_claim(&self, voucher: &FuelVoucher) -> Result<(), String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if voucher.expiry_timestamp < now {
            return Err("Voucher expired".to_string());
        }

        if voucher.provider_id != self.expected_provider_id {
            return Err(format!(
                "Voucher valid for provider {} but this node is {}",
                voucher.provider_id, self.expected_provider_id
            ));
        }

        // Verify Cryptographic Signature
        let pub_key_bytes = hex::decode(&voucher.agent_id)
            .map_err(|_| "Invalid agent_id hex format".to_string())?;

        let verifying_key = VerifyingKey::try_from(pub_key_bytes.as_slice())
            .map_err(|_| "Invalid Ed25519 Public Key".to_string())?;

        // Reconstruct exact signed bytes: agent_id + provider_id + fuel(u64) + expiry(u64) + nonce
        let mut signed_data = Vec::new();
        signed_data.extend_from_slice(voucher.agent_id.as_bytes());
        signed_data.extend_from_slice(voucher.provider_id.as_bytes());
        signed_data.extend_from_slice(&voucher.fuel_limit.to_be_bytes());
        signed_data.extend_from_slice(&voucher.expiry_timestamp.to_be_bytes());
        signed_data.extend_from_slice(voucher.nonce.as_bytes());

        let sig = Signature::from_slice(&voucher.signature)
            .map_err(|_| "Invalid signature format".to_string())?;

        verifying_key
            .verify(&signed_data, &sig)
            .map_err(|_| "Cryptographic signature verification failed".to_string())?;

        // Final phase: prevent replay attacks by atomically claiming the Nonce
        let mut used = self.used_nonces.write().unwrap();
        if !used.insert(voucher.nonce.clone()) {
            return Err("Voucher nonce already used (Replay Attack)".to_string());
        }

        Ok(())
    }
}
