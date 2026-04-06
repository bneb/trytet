use serde::{Deserialize, Serialize};
use dashmap::DashMap;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum NodeStatus {
    Active,
    Suspect,
    Dead,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    pub node_id: String,
    pub timestamp_us: u64,
    pub signature: Vec<u8>,
}

pub struct VitalityManager {
    pub nodes: DashMap<String, (u64, NodeStatus)>,
    pub timeout_limit_us: u64,
}

impl VitalityManager {
    pub fn new(timeout_limit_us: u64) -> Self {
        Self {
            nodes: DashMap::new(),
            timeout_limit_us,
        }
    }

    pub fn record_heartbeat(&self, heartbeat: Heartbeat) {
        let now = Self::current_time_us();
        self.nodes.insert(
            heartbeat.node_id.clone(),
            (now, NodeStatus::Active),
        );
    }

    pub fn current_time_us() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
    }

    /// Triggers expiration logic and identifies newly transitioned DEAD nodes.
    pub fn calculate_unresponsive(&self) -> Vec<String> {
        let now = Self::current_time_us();
        let mut dead_nodes = Vec::new();

        for mut kv in self.nodes.iter_mut() {
            let last_seen_us = kv.value().0;
            let elapsed_us = now.saturating_sub(last_seen_us);
            
            let status = &mut kv.value_mut().1;
            
            // Suspect: >= 5s
            if elapsed_us >= 5_000_000 && *status == NodeStatus::Active {
                *status = NodeStatus::Suspect;
            }
            
            // Dead: >= 15s
            if elapsed_us >= 15_000_000 && *status == NodeStatus::Suspect {
                *status = NodeStatus::Dead;
                dead_nodes.push(kv.key().clone());
            }
        }
        
        dead_nodes
    }
}
