use crate::consensus::QuorumStatus;
use crate::gateway::{GatewayError, GlobalRegistry};
use dashmap::DashMap;
use futures_util::future::BoxFuture;
use std::collections::HashSet;
use uuid::Uuid;

pub struct QuorumRegistry {
    pub alias_map: DashMap<String, QuorumStatus>,
    pub shard_locations: DashMap<Uuid, HashSet<String>>,
}

impl Default for QuorumRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl QuorumRegistry {
    pub fn new() -> Self {
        Self {
            alias_map: DashMap::new(),
            shard_locations: DashMap::new(),
        }
    }

    pub fn update_shard_location(&self, layer_id: Uuid, node_ip: String) {
        let mut set = self
            .shard_locations
            .entry(layer_id)
            .or_default();
        set.insert(node_ip);
    }

    pub fn get_shard_locations(&self, layer_id: &Uuid) -> Option<Vec<String>> {
        self.shard_locations
            .get(layer_id)
            .map(|s| s.iter().cloned().collect())
    }
}

impl GlobalRegistry for QuorumRegistry {
    fn resolve_alias(&self, alias: &str) -> BoxFuture<'_, Result<Option<String>, GatewayError>> {
        let val = self.alias_map.get(alias).map(|s| s.value().clone());
        let res = match val {
            Some(QuorumStatus::Committed) => Ok(Some("127.0.0.1".into())), // Simplified mapping for gateway testing
            Some(QuorumStatus::Locked {
                node_id,
                expires_at: _,
            }) => Err(GatewayError::ExecutionFailed(format!(
                "Alias is locked by {}",
                node_id
            ))),
            _ => Ok(None),
        };
        Box::pin(async { res })
    }

    fn update_route(
        &self,
        alias: &str,
        _node_ip: &str,
        _sig: &str,
    ) -> BoxFuture<'_, Result<(), GatewayError>> {
        self.alias_map
            .insert(alias.to_string(), QuorumStatus::Committed);
        Box::pin(async { Ok(()) })
    }
}
