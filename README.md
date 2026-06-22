# Trytet

[![GitHub Release](https://img.shields.io/github/v/release/bneb/trytet)](https://github.com/bneb/trytet/releases)
[![npm](https://img.shields.io/npm/v/trytet-client)](https://www.npmjs.com/package/trytet-client)
[![PyPI](https://img.shields.io/pypi/v/trytet-client)](https://pypi.org/project/trytet-client/)

A WebAssembly sandbox engine that runs AI-generated code without crashing. Agents invoke deterministic Wasm components ("cartridges") inside fuel-bounded sandboxes — infinite loops trap in microseconds instead of hanging until a timeout, memory bombs get capped instead of OOMing.

## Installation

```bash
# macOS (Apple Silicon)
curl -sL https://github.com/bneb/trytet/releases/latest/download/tet-darwin-arm64.tar.gz | tar xz
./tet doctor
./tet mcp --list-tools

# Docker
docker pull ghcr.io/bneb/trytet:latest
docker run -p 3000:3000 ghcr.io/bneb/trytet:latest
```

Also available via SDK: `npm install trytet-client` | `pip install trytet-client`

## Quickstart

**Claude Desktop / Cursor** — add to `claude_desktop_config.json`:
```json
{
  "mcpServers": {
    "trytet": {
      "command": "tet",
      "args": ["mcp"]
    }
  }
}
```

Then tools appear automatically. Run `tet mcp --list-tools` to verify.

**SDK** — call Trytet from your own agent code:
```bash
npm install trytet-client        # TypeScript
pip install trytet-client        # Python
```

**API server:**
```bash
tet up                            # Starts on port 3000
curl http://localhost:3000/health # Health check
```

## Performance

| Operation | Latency |
|---|---|
| Cached cartridge call | <500µs |
| First cartridge call (Cranelift compilation) | ~400ms (one-time, cached) |
| MCP server boot | ~50ms |
| Infinite loop trap | <100µs (instruction-level fuel exhaustion) |

## MCP Tools

Trytet exposes 5 tools via the Model Context Protocol (3 ship with compiled .wasm, 2 experimental):

| Tool | Status |
|---|---|
| `trytet_js_evaluator` | shipped — Execute JavaScript with fuel and memory limits |
| `trytet_regex_evaluator` | shipped — Run regex patterns safely (ReDoS-protected) |
| `trytet_jmespath_evaluator` | shipped — Query JSON with JMESPath expressions |
| `trytet_scraper` | experimental — Parse HTML with CSS selectors |
| `trytet_structured_data` | experimental — SQLite-powered queries over JSON arrays |

## Architecture

Three layers:

1. **Sandbox** — Wasmtime engine with instruction-level fuel metering. Each Wasm instruction deducts from a fuel budget. Exhaustion produces a deterministic trap — no wall-clock timeouts, no OS process overhead.

2. **Cartridge Substrate** — Wasm Components are loaded, compiled, and executed in sub-sandboxes with independent fuel and memory limits. The host controls all resources; cartridges own nothing.

3. **Hive Mesh** (experimental) — Agent snapshot, fork, and teleport between nodes for state migration.

## Measured Performance

- **MCP Server boot**: ~50ms
- **Cached cartridge call**: <500µs
- **First cartridge call**: ~400ms (Cranelift compilation, one-time)
- **Test suite**: 0 failures across 64 test files

Detailed benchmarks: [BENCHMARKS.md](BENCHMARKS.md)

## CLI

```bash
tet up          # Start the API server
tet mcp         # Start the MCP server (for Claude Desktop, Cursor)
tet ps          # List running agents
tet metrics     # Run benchmark suite
tet init [name] # Scaffold a new agent project
```

Full reference: [CLI.md](CLI.md)

## SDKs

```bash
npm install trytet-client     # TypeScript
pip install trytet-client     # Python
```

## Project Status

The core sandbox engine, cartridge substrate, and MCP server are functional. The project is under active development. Production deployments should pin to a specific release.

## License

MIT
