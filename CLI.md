# Trytet CLI Operator's Reference (v27.1)

The Trytet CLI (`tet`) is the primary interface for managing your local sandbox and cluster.

### Global Flags
- `--json`: Output raw JSON blocks instead of human-readable formats. Standardized for piping into `jq`.

## Lifecycle Commands

### `tet up <file.wasm> [--fuel 100000]`
Boots a local WebAssembly agent into the Engine.
- Automatically assigns an execution alias if one is not provided.
- Registers them in the global Hive dictionary.

### `tet run <file.wasm> --alias <name>`
Executes a stateless Wasm payload and waits for completion. Best for scripts, not Daemons.

### `tet snapshot <alias> <tag>`
Captures the live memory and VFS of the `alias` and exports to an OCI artifact with name `tag`.

## Teleportation & Clustering

### `tet teleport <alias> <target_node>`
Force-migrates a live agent from the current node's active memory to the memory of `target_node`. Pauses the agent, serializes them, and ships them via RPC.

### `tet swarm`
Outputs a topological map of all agents across the cluster and their interconnects.

### `tet hive-list`
List all securely connected P2P Trytet nodes in the current Hive context.

### `tet bridge <alias> --path <route>`
Exposes a Wasm agent to the external internet. Example: `tet bridge web-ui --path /demo` means `GET localhost:3000/demo/` resolves directly to the Memory of the agent `web-ui`.

## Economy & Market

### `tet market-list`
List all real-time Fuel bid multipliers and Thermal stress factors across the Hive. 

### `tet pay <from> <to> <amount>`
Transfer Trytet Fuel from the balance of one Agent to another. Useful for triggering Genesis Factories.

## Observability

### `tet ps`
A live snapshot table of all running agents, memory consumption, latency averages across calls, error counts, and vital Market multipliers.

### `tet metrics`
Runs and prints the **Northstar Benchmarking report** across four critical sub-millisecond latencies. Pass/Fail ceilings ensure the core engine stays compliant.

### `tet logs -f <alias>`
Tail the real-time TelemetryHub event stream. Enriched with human-readable emojis for `TeleportWarp` (✈️), `OracleFidelity` (🧠), `FuelBurn` (💰), and `Evacuation` (🌡️).
