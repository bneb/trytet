use crate::crypto::AgentWallet;
use crate::gateway::{GatewayError, GlobalRegistry};
use crate::hive::{HiveClient, HiveCommand, HivePeers};
use futures_util::future::BoxFuture;
use std::sync::Arc;

pub struct HiveDht {
    pub peers: HivePeers,
    pub wallet: Arc<AgentWallet>,
}

impl HiveDht {
    pub fn new(peers: HivePeers, wallet: Arc<AgentWallet>) -> Self {
        Self { peers, wallet }
    }
}

impl GlobalRegistry for HiveDht {
    fn resolve_alias(&self, alias: &str) -> BoxFuture<'_, Result<Option<String>, GatewayError>> {
        let alias = alias.to_string();
        let peers = self.peers.clone();

        Box::pin(async move {
            let mut all_peers = peers.list_peers().await;
            // Pseudo-sharding: map aliases onto the peer ring
            all_peers.sort_by_key(|p| p.node_id.clone());

            // O(log N) simulated broadcast: resolve alias over the mesh
            for target_node in all_peers {
                let cmd = HiveCommand::Dht(crate::hive::HiveDhtCommand::ResolveAlias(alias.clone()));
                if let Ok(HiveCommand::Dht(crate::hive::HiveDhtCommand::ResolveAliasResponse(Some(_meta)))) =
                    HiveClient::rpc_call(&target_node.public_addr, cmd).await
                {
                    // Target node holds the agent. Return its IP/addr.
                    let base_ip = target_node
                        .public_addr
                        .split(':')
                        .next()
                        .unwrap_or(&target_node.public_addr)
                        .to_string();
                    let public_port = target_node.public_addr.split(':').nth(1).unwrap_or("80");
                    // Note: In our current test setup, nodes host their API on random ports or default 3000
                    return Ok(Some(format!("{}:{}", base_ip, public_port)));
                }
            }
            Ok(None)
        })
    }

    fn update_route(
        &self,
        alias: &str,
        node_ip: &str,
        signature: &str,
    ) -> BoxFuture<'_, Result<(), GatewayError>> {
        let alias = alias.to_string();
        let ip = node_ip.to_string();
        let sig = signature.to_string();
        let peers = self.peers.clone();

        Box::pin(async move {
            let cmd = HiveCommand::Dht(crate::hive::HiveDhtCommand::DhtUpdate {
                alias,
                node_ip: ip,
                signature: sig,
            });

            let all_peers = peers.list_peers().await;
            // Push update to the shard.
            for target_node in all_peers {
                let _ = HiveClient::send_command(&target_node.public_addr, cmd.clone()).await;
            }
            Ok(())
        })
    }
}
