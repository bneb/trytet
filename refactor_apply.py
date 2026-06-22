import os
import re

mapping = {
    "Network": ["Join", "Heartbeat", "Pulse"],
    "Dht": ["ResolveAlias", "ResolveAliasResponse", "DhtUpdate", "ProposeAlias", "QuorumVote"],
    "Migration": ["MigrateRequest", "MigrationPacket", "MigrationNotice", "TransitLock", "TransitRelease"],
    "Economy": ["TransferCredit", "BillRequest", "WithdrawalPending", "MarketBidPacket"],
    "Registry": ["RegistryQuery", "RegistryQueryResponse", "ChunkStream"]
}

rev_map = {}
for cat, variants in mapping.items():
    for v in variants:
        rev_map[v] = cat

def process_content(content):
    # Iterate over all variants
    for variant, cat in rev_map.items():
        # Look for `HiveCommand::Variant`
        
        # 1. HiveCommand::Variant(args)
        # Matches `HiveCommand::Variant(` -> `HiveCommand::Cat(HiveCatCommand::Variant(`
        # Wait, for matching in `match`, `HiveCommand::Join(identity) =>` needs to become `HiveCommand::Network(HiveNetworkCommand::Join(identity)) =>`
        # This means we MUST balance parentheses/braces.
        pass

# It is actually much simpler to just write the specific string replacements for the known occurrences.

replacements = [
    # src/hive.rs
    ("HiveCommand::Join(identity)", "HiveCommand::Network(HiveNetworkCommand::Join(identity))"),
    ("HiveCommand::Heartbeat(hb)", "HiveCommand::Network(HiveNetworkCommand::Heartbeat(hb))"),
    ("HiveCommand::Pulse =>", "HiveCommand::Network(HiveNetworkCommand::Pulse) =>"),
    ("HiveCommand::ResolveAlias(alias)", "HiveCommand::Dht(HiveDhtCommand::ResolveAlias(alias))"),
    ("HiveCommand::ResolveAliasResponse(_)", "HiveCommand::Dht(HiveDhtCommand::ResolveAliasResponse(_))"),
    ("HiveCommand::MigrateRequest(envelope_box)", "HiveCommand::Migration(HiveMigrationCommand::MigrateRequest(envelope_box))"),
    ("HiveCommand::MigrationPacket(packet)", "HiveCommand::Migration(HiveMigrationCommand::MigrationPacket(packet))"),
    ("HiveCommand::MigrationNotice {", "HiveCommand::Migration(HiveMigrationCommand::MigrationNotice {"),
    ("snapshot_id: _,\\n            } => {", "snapshot_id: _,\\n            }) => {"),
    ("HiveCommand::DhtUpdate {", "HiveCommand::Dht(HiveDhtCommand::DhtUpdate {"),
    ("signature,\\n            } => {", "signature,\\n            }) => {"),
    ("HiveCommand::ProposeAlias(proposal)", "HiveCommand::Dht(HiveDhtCommand::ProposeAlias(proposal))"),
    ("HiveCommand::QuorumVote(_)", "HiveCommand::Dht(HiveDhtCommand::QuorumVote(_))"),
    ("HiveCommand::TransitLock {", "HiveCommand::Migration(HiveMigrationCommand::TransitLock {"),
    ("ttl_seconds,\\n            } => {", "ttl_seconds,\\n            }) => {"),
    ("HiveCommand::TransitRelease(alias)", "HiveCommand::Migration(HiveMigrationCommand::TransitRelease(alias))"),
    ("HiveCommand::TransferCredit(tx)", "HiveCommand::Economy(HiveEconomyCommand::TransferCredit(tx))"),
    ("HiveCommand::BillRequest {", "HiveCommand::Economy(HiveEconomyCommand::BillRequest {"),
    ("amount,\\n            } => {", "amount,\\n            }) => {"),
    ("HiveCommand::WithdrawalPending(intent)", "HiveCommand::Economy(HiveEconomyCommand::WithdrawalPending(intent))"),
    ("HiveCommand::MarketBidPacket(bid)", "HiveCommand::Economy(HiveEconomyCommand::MarketBidPacket(bid))"),
    ("HiveCommand::RegistryQuery(cid)", "HiveCommand::Registry(HiveRegistryCommand::RegistryQuery(cid))"),
    ("HiveCommand::RegistryQueryResponse { .. } =>", "HiveCommand::Registry(HiveRegistryCommand::RegistryQueryResponse { .. }) =>"),
    ("HiveCommand::ChunkStream { .. } =>", "HiveCommand::Registry(HiveRegistryCommand::ChunkStream { .. }) =>"),
    
    # other constructions in src/hive.rs
    ("encrypt_command(&HiveCommand::Pulse)", "encrypt_command(&HiveCommand::Network(HiveNetworkCommand::Pulse))"),
    ("HiveCommand::ResolveAliasResponse(local_meta)", "HiveCommand::Dht(HiveDhtCommand::ResolveAliasResponse(local_meta))"),
    ("HiveCommand::RegistryQueryResponse { cid, available }", "HiveCommand::Registry(HiveRegistryCommand::RegistryQueryResponse { cid, available })"),
    ("Ok(HiveCommand::Pulse)", "Ok(HiveCommand::Network(HiveNetworkCommand::Pulse))"),
    
    # src/hive/dht.rs
    ("let cmd = HiveCommand::ResolveAlias(alias.clone());", "let cmd = HiveCommand::Dht(crate::hive::HiveDhtCommand::ResolveAlias(alias.clone()));"),
    ("Ok(HiveCommand::ResolveAliasResponse(Some(_meta)))", "Ok(HiveCommand::Dht(crate::hive::HiveDhtCommand::ResolveAliasResponse(Some(_meta))))"),
    ("let cmd = HiveCommand::DhtUpdate {", "let cmd = HiveCommand::Dht(crate::hive::HiveDhtCommand::DhtUpdate {"),
    ("signature: signature.clone(),\\n            };", "signature: signature.clone(),\\n            });"),
    
    # src/mesh.rs
    ("crate::hive::HiveCommand::ResolveAlias(alias.to_string())", "crate::hive::HiveCommand::Dht(crate::hive::HiveDhtCommand::ResolveAlias(alias.to_string()))"),
    ("Ok(crate::hive::HiveCommand::ResolveAliasResponse(Some(meta)))", "Ok(crate::hive::HiveCommand::Dht(crate::hive::HiveDhtCommand::ResolveAliasResponse(Some(meta))))"),
    
    # src/teleport.rs
    ("HiveCommand::MigrationNotice {", "HiveCommand::Migration(crate::hive::HiveMigrationCommand::MigrationNotice {"),
    ("snapshot_id: snapshot_id.clone(),\\n                }", "snapshot_id: snapshot_id.clone(),\\n                })"),
    ("HiveCommand::MigrationPacket(MigrationPacket::Handshake {", "HiveCommand::Migration(crate::hive::HiveMigrationCommand::MigrationPacket(MigrationPacket::Handshake {"),
    ("snapshot_id: snapshot_id.clone(),\\n                })", "snapshot_id: snapshot_id.clone(),\\n                }))"),
    ("HiveCommand::MigrationPacket(MigrationPacket::Payload {", "HiveCommand::Migration(crate::hive::HiveMigrationCommand::MigrationPacket(MigrationPacket::Payload {"),
    ("sequence,\\n                    })", "sequence,\\n                    }))"),
    ("HiveCommand::MigrationPacket(MigrationPacket::Commit { signature: vec![] }),", "HiveCommand::Migration(crate::hive::HiveMigrationCommand::MigrationPacket(MigrationPacket::Commit { signature: vec![] })),"),
    
    # tests/cli_tests.rs
    ("tet_core::hive::HiveCommand::Join(tet_core::hive::HiveNodeIdentity {", "tet_core::hive::HiveCommand::Network(tet_core::hive::HiveNetworkCommand::Join(tet_core::hive::HiveNodeIdentity {"),
    ("available_capacity_mb: 1024,\\n    });", "available_capacity_mb: 1024,\\n    }));"),
    
    # tests/phase14_5_mtls_tests.rs
    ("let cmd = HiveCommand::Pulse;", "let cmd = HiveCommand::Network(crate::hive::HiveNetworkCommand::Pulse);"),
    
    # tests/phase22_1_economy_tests.rs
    ("let bill_req = HiveCommand::BillRequest {", "let bill_req = HiveCommand::Economy(crate::hive::HiveEconomyCommand::BillRequest {"),
    ("amount: 50000,\\n    };", "amount: 50000,\\n    });"),
    ("HiveCommand::BillRequest { amount: 50000, .. }", "HiveCommand::Economy(crate::hive::HiveEconomyCommand::BillRequest { amount: 50000, .. })"),
    
    # tests/phase23_1_bridge_tests.rs
    ("let cmd = HiveCommand::WithdrawalPending(intent);", "let cmd = HiveCommand::Economy(crate::hive::HiveEconomyCommand::WithdrawalPending(intent));"),
    ("HiveCommand::WithdrawalPending(ref i)", "HiveCommand::Economy(crate::hive::HiveEconomyCommand::WithdrawalPending(ref i))"),
    
    # tests/phase29_1_mesh_vpn_tests.rs
    ("let cmd = HiveCommand::Pulse;", "let cmd = HiveCommand::Network(crate::hive::HiveNetworkCommand::Pulse);"),
    ("let cmd = HiveCommand::ChunkStream {", "let cmd = HiveCommand::Registry(crate::hive::HiveRegistryCommand::ChunkStream {"),
    ("chunk,\\n    };", "chunk,\\n    });"),
]

for root, _, files in os.walk("."):
    for file in files:
        if file.endswith(".rs") and "target" not in root:
            filepath = os.path.join(root, file)
            with open(filepath, 'r') as f:
                content = f.read()
            
            new_content = content
            for old, new in replacements:
                if old in new_content:
                    new_content = new_content.replace(old, new)
            
            if new_content != content:
                with open(filepath, 'w') as f:
                    f.write(new_content)
                print(f"Updated {filepath}")
