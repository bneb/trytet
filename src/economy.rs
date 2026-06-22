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

/// Errors returned by [`VoucherManager::verify_and_claim`].
#[derive(Debug, thiserror::Error)]
pub enum VoucherError {
    /// The voucher's expiry timestamp is in the past.
    #[error("Voucher expired")]
    Expired,

    /// The voucher was issued for a different provider.
    #[error("Voucher valid for provider {voucher_provider} but this node is {expected}")]
    WrongProvider {
        voucher_provider: String,
        expected: String,
    },

    /// The `agent_id` field is not valid hex.
    #[error("Invalid agent_id hex format")]
    InvalidHexFormat,

    /// The decoded hex bytes do not form a valid Ed25519 public key.
    #[error("Invalid Ed25519 Public Key")]
    InvalidPublicKey,

    /// The voucher's signature bytes could not be parsed.
    #[error("Invalid signature format")]
    InvalidSignature,

    /// The cryptographic signature did not validate against the agent's public key.
    #[error("Cryptographic signature verification failed")]
    SignatureVerificationFailed,

    /// The voucher's nonce has already been claimed (replay attack).
    #[error("Voucher nonce already used (Replay Attack)")]
    NonceReused,
}
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

    /// Verifies the voucher's signature, expiry, and provider, and checks the nonce hasn't been replayed.
    pub fn verify_and_claim(&self, voucher: &FuelVoucher) -> Result<(), VoucherError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before unix epoch")
            .as_secs();

        if voucher.expiry_timestamp < now {
            return Err(VoucherError::Expired);
        }

        if voucher.provider_id != self.expected_provider_id {
            return Err(VoucherError::WrongProvider {
                voucher_provider: voucher.provider_id.clone(),
                expected: self.expected_provider_id.clone(),
            });
        }

        let pub_key_bytes =
            hex::decode(&voucher.agent_id).map_err(|_| VoucherError::InvalidHexFormat)?;

        let verifying_key = VerifyingKey::try_from(pub_key_bytes.as_slice())
            .map_err(|_| VoucherError::InvalidPublicKey)?;

        let mut signed_data = Vec::new();
        signed_data.extend_from_slice(voucher.agent_id.as_bytes());
        signed_data.extend_from_slice(voucher.provider_id.as_bytes());
        signed_data.extend_from_slice(&voucher.fuel_limit.to_be_bytes());
        signed_data.extend_from_slice(&voucher.expiry_timestamp.to_be_bytes());
        signed_data.extend_from_slice(voucher.nonce.as_bytes());

        let sig = Signature::from_slice(&voucher.signature)
            .map_err(|_| VoucherError::InvalidSignature)?;

        verifying_key
            .verify(&signed_data, &sig)
            .map_err(|_| VoucherError::SignatureVerificationFailed)?;

        let mut used = self.used_nonces.write().expect("RwLock poisoned");
        if !used.insert(voucher.nonce.clone()) {
            return Err(VoucherError::NonceReused);
        }

        Ok(())
    }
}
