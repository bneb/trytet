# CLI Reference

The `tet` CLI manages agents, clusters, and the fuel economy.

### Global Flags
- `--json`: Machine-readable JSON output.

## Lifecycle

### `tet up <file.wasm> [--fuel 100000]`
Boot an agent. Assigns an alias if not provided. Registers in the Hive.

### `tet run <file.wasm> --alias <name>`
Execute a stateless payload and wait for completion. For scripts, not daemons.

### `tet snapshot <alias> <tag>`
Capture live memory and VFS. Export as OCI artifact.

## Teleportation & Clustering

### `tet teleport <alias> <target_node>`
Migrate a live agent to `target_node`. Pause, serialize, stream, resume.

### `tet swarm`
Topological map of all agents and interconnects.

### `tet hive-list`
List connected P2P nodes in the current Hive.

### `tet bridge <alias> --path <route>`
Expose an agent to HTTP. `tet bridge web-ui --path /demo` routes `GET /demo/` to agent memory.

## Economy & Market

### `tet market-list`
Real-time Fuel bid multipliers and thermal stress across the Hive.

### `tet pay <from> <to> <amount>`
Transfer Fuel between agents.

## Observability

### `tet ps`
Running agents: memory, latency, error counts, market multipliers.

### `tet metrics`
Northstar benchmark report across five critical latencies.

### `tet logs -f <alias>`
Tail telemetry. Enriched with icons: ✈️ TeleportWarp, 🧠 OracleFidelity, 💰 FuelBurn, 🌡️ Evacuation.
