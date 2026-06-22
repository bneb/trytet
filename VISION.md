# Trytet: Product Direction

## The Problem

AI agents need to execute code. LLMs generate JavaScript, Python, SQL, and shell commands that run inside the agent process. When that generated code contains an infinite loop, a memory bomb, or a bad syscall, the entire agent crashes. Current approaches:

1. **Subprocess isolation** — fork a child process per tool call. Slow (hundreds of ms to spawn), no resource limits, process leaks.
2. **Docker containers** — secure but take seconds to cold-start. Running a container per tool call destroys latency and economics.
3. **V8 isolates** — fast but JavaScript-only, no memory limits, no deterministic termination.

## What Trytet Does

Trytet executes untrusted code inside WebAssembly sandboxes where every instruction is metered. When an LLM generates an infinite loop, Trytet traps it in microseconds instead of the process hanging until a wall-clock timeout fires. The agent gets a structured error and can decide what to do next — rewrite the code, reduce the fuel budget, or try a different approach.

## The Cartridge Model

A cartridge is a single-purpose WebAssembly component that does one thing well. Current cartridges:

| Cartridge | What it does |
|---|---|
| JS Evaluator | Execute JavaScript with fuel and memory limits |
| Regex Evaluator | Run regex patterns safely (ReDoS-protected) |
| JMESPath Evaluator | Query JSON with JMESPath expressions |
| JSON Schema Validator | Validate JSON data against a schema |
| SQL/Structured Data | Query JSON arrays with SQL-like operations |
| SQLite (experimental) | Full SQLite compiled to Wasm via WASI SDK |

Cartridges implement a simple WIT interface:

```wit
interface cartridge-v1 {
    execute: func(input: string) -> result<string, string>;
}
```

Anyone can write a cartridge in Rust, compile it with `cargo component build`, and publish it. Cartridges run in sub-sandboxes with independent fuel and memory budgets.

## Distribution

Trytet's primary distribution channel is MCP (Model Context Protocol). Users add `tet mcp` to their Claude Desktop or Cursor config, and their AI tool can suddenly execute sandboxed JavaScript, run regex, query JSON, and validate schemas — without crashing.

We also provide HTTP API access and SDKs for TypeScript and Python.

## What We're Not

- **Not an agent framework.** Trytet is a runtime — it executes code. It doesn't plan, reason, or orchestrate agents.
- **Not a general-purpose sandbox.** It's designed for the AI tool-use pattern: short-lived, deterministic execution in fuel-bounded WebAssembly.
- **Not a replacement for Docker.** If you need a full Linux environment, use Docker. If you need to evaluate a 10-line JavaScript snippet an LLM just wrote, use Trytet.

## Roadmap

### Now
- MCP server with 5 cartridge tools
- Pre-built binaries for macOS
- Docker image on ghcr.io
- TypeScript SDK on npm
- Python SDK on PyPI
- API key authentication
- `tet doctor` diagnostic tool

### Next
- x86_64 Linux binary builds
- Cartridge registry (publish/search/install)
- Prometheus metrics endpoint
- HTTP fetch cartridge (sandboxed outbound HTTP)

### Later
- Multi-node Hive mesh deployment
- WASI preview 2 support
- Streaming cartridge output
- Cartridge marketplace
