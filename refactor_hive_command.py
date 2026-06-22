import os
import re

mapping = {
    "Network": ["Join", "Heartbeat", "Pulse"],
    "Dht": ["ResolveAlias", "ResolveAliasResponse", "DhtUpdate", "ProposeAlias", "QuorumVote"],
    "Migration": ["MigrateRequest", "MigrationPacket", "MigrationNotice", "TransitLock", "TransitRelease"],
    "Economy": ["TransferCredit", "BillRequest", "WithdrawalPending", "MarketBidPacket"],
    "Registry": ["RegistryQuery", "RegistryQueryResponse", "ChunkStream"]
}

# Reverse mapping for quick lookup
variant_to_category = {}
for category, variants in mapping.items():
    for variant in variants:
        variant_to_category[variant] = category

def process_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()

    new_content = content
    # Handle pattern matching and construction:
    # 1. HiveCommand::Variant
    # 2. HiveCommand::Variant(args)
    # 3. HiveCommand::Variant { args }
    
    # We can match HiveCommand::<Variant> and replace it with HiveCommand::<Category>(Hive<Category>Command::<Variant>
    # Note: for struct-like variants, this might be tricky if we just replace the prefix.
    # Actually, Rust enum construction/matching syntax:
    # HiveCommand::Pulse => HiveCommand::Network(HiveNetworkCommand::Pulse)
    # HiveCommand::Join(id) => HiveCommand::Network(HiveNetworkCommand::Join(id))
    # HiveCommand::DhtUpdate { ... } => HiveCommand::Dht(HiveDhtCommand::DhtUpdate { ... })
    
    # If we just replace `HiveCommand::Variant` with `HiveCommand::Category(HiveCategoryCommand::Variant)`
    # it won't work perfectly for matching/destructuring without closing parentheses.
    # e.g., if we have `let cmd = HiveCommand::Join(id);`, it becomes `let cmd = HiveCommand::Network(HiveNetworkCommand::Join)(id);` which is WRONG!
    # Same for matching: `HiveCommand::Join(id) => ...` becomes `HiveCommand::Network(HiveNetworkCommand::Join)(id) => ...`
    pass

