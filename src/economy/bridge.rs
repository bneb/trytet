use crate::consensus::{DepositProposal, HiveConsensus};
use crate::economy::registry::VoucherRegistry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("Transaction Not Found")]
    TxNotFound,
    #[error("Quorum Failed to Validated Deposit")]
    QuorumFailed,
    #[error("Discrepancy In Multi-Bridge Query")]
    Discrepancy,
    #[error("Internal Framework Error")]
    InternalError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeIntent {
    pub internal_fuel: u64,
    pub external_asset: String,
    pub target_address: String,
    pub agent_signature: Vec<u8>,
}

#[async_trait::async_trait]
pub trait ChainListener: Send + Sync {
    /// Verifies external transaction and returns (author_pubkey_hex, external_amount)
    async fn verify_transaction(&self, tx_hash: &str) -> Result<(String, u64), BridgeError>;
}

pub struct SettlementBridge {
    pub chain_connectors: HashMap<String, Box<dyn ChainListener>>,
    pub quorum_handle: Arc<HiveConsensus>,
    pub registry: Arc<VoucherRegistry>,
}

impl SettlementBridge {
    pub fn new(quorum_handle: Arc<HiveConsensus>, registry: Arc<VoucherRegistry>) -> Self {
        Self {
            chain_connectors: HashMap::new(),
            quorum_handle,
            registry,
        }
    }

    pub fn register_listener(&mut self, chain: &str, listener: Box<dyn ChainListener>) {
        self.chain_connectors.insert(chain.to_string(), listener);
    }

    pub async fn process_deposit(
        &self,
        chain: &str,
        tx_hash: &str,
        total_nodes: usize,
    ) -> Result<u64, BridgeError> {
        let listener = self
            .chain_connectors
            .get(chain)
            .ok_or(BridgeError::InternalError)?;

        // 1. Verify Tx on external chain
        let (author_pubkey, external_amount) = listener.verify_transaction(tx_hash).await?;

        // Constant Exchange Rate Determinism for MVP: 1 external unit = 1000 fuel max scaling
        let internal_fuel = external_amount * 1000;

        // 2. Request Quorum validation generically using `DepositProposal`
        let proposal = DepositProposal {
            tx_hash: tx_hash.to_string(),
            amount: internal_fuel,
            signatures: vec![], // Populated asynchronously or mockingly in testing Phase 23.1
        };

        if !self
            .quorum_handle
            .verify_deposit_majority(&proposal, total_nodes)
        {
            return Err(BridgeError::QuorumFailed);
        }

        // 3. Quorum succeeded! Ensure fuel minting via Registry natively!
        // author_pubkey is expected to be hex representation. Convert to Vec<u8>.
        let pubkey_bytes = hex::decode(&author_pubkey).map_err(|_| BridgeError::InternalError)?;

        self.registry.mint(pubkey_bytes, internal_fuel);

        Ok(internal_fuel) // Return Fuel Amount credited
    }
}
