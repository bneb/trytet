use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum QuorumStatus {
    Proposed,
    Locked { node_id: String, expires_at: u64 }, // Locked by NodeID until timestamp
    Committed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSignature {
    pub node_id: String,
    pub sig_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasProposal {
    pub alias_hash: [u8; 32],
    pub owner_pubkey: Vec<u8>,
    pub signatures: Vec<NodeSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositProposal {
    pub tx_hash: String,
    pub amount: u64,
    pub signatures: Vec<NodeSignature>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConsensusError {
    #[error("Validation Failure: Origin Author Key is mismatched")]
    ValidationFailure,
    #[error("Timeout: Failed to reach Quorum")]
    Timeout,
    #[error("Rejected by Quorum Peers")]
    Rejected,
    #[error("Alias Locked: {0}")]
    AliasLocked(String),
}

pub struct HiveConsensus {
    pub local_node_id: String,
}

impl HiveConsensus {
    pub fn new(local_node_id: String) -> Self {
        Self { local_node_id }
    }

    /// Synchronous verification of Quorum rule (N / 2 + 1)
    pub fn verify_majority(&self, proposal: &AliasProposal, total_nodes: usize) -> bool {
        let required = (total_nodes / 2) + 1;
        proposal.signatures.len() >= required
    }

    pub fn verify_deposit_majority(&self, proposal: &DepositProposal, total_nodes: usize) -> bool {
        let required = (total_nodes / 2) + 1;
        proposal.signatures.len() >= required
    }
}
