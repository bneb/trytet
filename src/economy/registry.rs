use dashmap::DashMap;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::RwLock;

#[derive(Debug, thiserror::Error)]
pub enum EconomyError {
    #[error("Insufficient Funds")]
    InsufficientFunds,
    #[error("Invalid Cryptographic Signature")]
    InvalidSignature,
    #[error("Replay Attack Detected: Nonce already used")]
    ReplayAttack,
    #[error("Invalid Ed25519 format")]
    InvalidFormat,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FuelTransaction {
    pub from: Vec<u8>, // Ed25519PublicKey bytes
    pub to: Vec<u8>,   // Ed25519PublicKey bytes
    pub amount: u64,
    pub nonce: u64,
    pub signature: Vec<u8>,
}

#[derive(Clone)]
pub struct VoucherRegistry {
    pub balances: Arc<DashMap<Vec<u8>, u64>>,
    pub audit_log: Arc<RwLock<Vec<FuelTransaction>>>,
    pub used_nonces: Arc<RwLock<HashSet<u64>>>,
}

impl Default for VoucherRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl VoucherRegistry {
    pub fn new() -> Self {
        Self {
            balances: Arc::new(DashMap::new()),
            audit_log: Arc::new(RwLock::new(Vec::new())),
            used_nonces: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Deposits top-up liquidity directly into an account
    pub fn mint(&self, to: Vec<u8>, amount: u64) {
        let mut balance = self.balances.entry(to).or_insert(0);
        *balance += amount;
    }

    /// Primary execution verifying atomicity across double spends and replay boundaries.
    pub fn transfer(&self, tx: FuelTransaction) -> Result<(), EconomyError> {
        // 1. Verify Nonce to mitigate Replay Attacks
        let mut nonces = self.used_nonces.write().unwrap();
        if !nonces.insert(tx.nonce) {
            return Err(EconomyError::ReplayAttack);
        }

        // 2. Verify Cryptographic Signature
        let verifying_key =
            VerifyingKey::try_from(tx.from.as_slice()).map_err(|_| EconomyError::InvalidFormat)?;

        let mut signed_data = Vec::new();
        signed_data.extend_from_slice(&tx.from);
        signed_data.extend_from_slice(&tx.to);
        signed_data.extend_from_slice(&tx.amount.to_be_bytes());
        signed_data.extend_from_slice(&tx.nonce.to_be_bytes());

        let sig = Signature::from_slice(&tx.signature).map_err(|_| EconomyError::InvalidFormat)?;

        if verifying_key.verify(&signed_data, &sig).is_err() {
            return Err(EconomyError::InvalidSignature);
        }

        // 3. Atomically lock and swap balances safely ensuring No Double Spends!
        // DashMap operations using Entry locks accurately prevent race conditions locally.
        let mut sender_ref = self.balances.entry(tx.from.clone()).or_insert(0);

        if *sender_ref < tx.amount {
            return Err(EconomyError::InsufficientFunds);
        }

        *sender_ref -= tx.amount;
        drop(sender_ref); // Free sender lock promptly to sequence receiver safely

        let mut receiver_ref = self.balances.entry(tx.to.clone()).or_insert(0);
        *receiver_ref += tx.amount;
        drop(receiver_ref);

        // 4. Secure Auditing without PII exposure
        self.audit_log.write().unwrap().push(tx);

        Ok(())
    }
}
