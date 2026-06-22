import os
import re

files_to_check = []
for root, _, files in os.walk("."):
    for file in files:
        if file.endswith(".rs") and "target" not in root:
            files_to_check.append(os.path.join(root, file))

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

for filepath in files_to_check:
    with open(filepath, 'r') as f:
        content = f.read()

    original_content = content
    
    # 1. HiveCommand::Variant(args) => HiveCommand::Cat(HiveCatCommand::Variant(args))
    # 2. HiveCommand::Variant { args } => HiveCommand::Cat(HiveCatCommand::Variant { args })
    # 3. HiveCommand::Variant => HiveCommand::Cat(HiveCatCommand::Variant)
    
    # We will iterate through each variant and do targeted replacements in the content.
    # To avoid parsing balanced parens, we can just replace the start and end of the match arm or construction if possible.
    
    # It's actually easier to just manually fix `src/hive.rs` since it's the largest, and fix the rest via simple string replace.
    # Let's do regex replacements.
    for variant, cat in rev_map.items():
        # Case 1: Struct variants like HiveCommand::DhtUpdate { ... }
        # This is a bit tricky, but we know what's inside them from the code.
        # We can use regex to find `HiveCommand::Variant { [^}]* }`
        
        # Replace `HiveCommand::Variant {` with `HiveCommand::Cat(HiveCatCommand::Variant {`
        # and then we need to add a closing `)` after the `}`. 
        # Since structs might have nested {}, we can just find the matching } for the struct variant.
        pass

    # Actually, a simpler approach:
    # `HiveCommand::Join(identity)` -> `HiveCommand::Network(HiveNetworkCommand::Join(identity))`
    
    # Let's use re.sub with a custom function to match balanced parens/braces.
    def replace_variant(match):
        variant = match.group(1)
        if variant not in rev_map:
            return match.group(0)
        
        cat = rev_map[variant]
        prefix = f"HiveCommand::{cat}(crate::hive::Hive{cat}Command::{variant}"
        # Wait, inside src/hive.rs we don't need `crate::hive::`, but we might in other files.
        # Let's just use `HiveCommand::{cat}(Hive{cat}Command::{variant}` for src/hive.rs 
        # and `crate::hive::Hive{cat}Command` elsewhere. We can just use the short version and rely on imports,
        # or fully qualify it.
        # Actually, let's just fully qualify the inner enum to be safe: `crate::hive::Hive{cat}Command::{variant}`
        return prefix

    # Let's just do targeted string replacements for the known usages.
