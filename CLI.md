# CLI Reference

The `tet` CLI manages sandboxes, cartridges, and the MCP server.

## Global Flags

- `--json`: Machine-readable JSON output.

## Core Commands

### `tet up <file.tet>`
Boot a `.tet` agent artifact with optional fuel override. The agent executes within a fuel-metered Wasmtime sandbox.
```bash
tet up ./my-agent.tet
tet up ./my-agent.tet --fuel 100000000
```

### `tet serve`
Start the API server on port 3000. Serves the HTTP API for remote sandbox execution.
```bash
tet serve
```
Alias for the `tet-core` binary. Accepts `REGISTRY_PATH`, `DATABASE_URL`, `REGISTRY_URL`, and `CORS_ORIGIN` env vars.

### `tet mcp`
Start the MCP server over stdio. Connect from Claude Desktop, Cursor, or any MCP-compatible client.
```bash
tet mcp              # Start the server
tet mcp --list-tools # List registered tools without starting
```
Boots in ~50ms. Cartridges compile lazily on first tool call.

### `tet doctor`
Diagnose install health: binary location, cartridge paths, toolchain status, MCP config snippet.
```bash
tet doctor
```

## Lifecycle

### `tet run <file.wasm> --alias <name>`
Execute a Wasm payload and wait for completion.

### `tet snapshot <alias> <tag>`
Capture live memory and VFS state.

### `tet fork <snapshot_id>`
Fork a new agent from a snapshot.

## Cartridges

### `tet publish <path> --tag <name>`
Publish a cartridge .wasm file to the registry.

### `tet search <query>`
Search the cartridge registry.

### `tet validate <path>`
Validate a .wasm file as a cartridge (checks Wasm header, WIT conformance).

## Development

### `tet init [name]`
Scaffold a new agent project with a template `agent.ts` and `tet.toml`.

### `tet build <entry> -o <output>`
Compile a TypeScript/JavaScript entry point to a .tet artifact.

## Observability

### `tet ps`
List running agents with memory and status.

### `tet metrics`
Run the benchmark suite and print a report.

### `tet logs -f <alias>`
Tail telemetry from an agent.

## Cluster (experimental)

### `tet teleport <alias> <target_node>`
Migrate a live agent to another node.

### `tet hive-list`
List P2P peers in the current Hive.

### `tet market-list`
Show fuel bid multipliers and node thermal status.
