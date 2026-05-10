# Trytet Engine

> **The optimally solved agentic sandbox.**

Agents call solvers. Solvers hang. Trytet makes sure the agent doesn't. Deterministic Wasm execution with fuel-bounded traps, live state migration, and $O(1)$ teardown. Rust. Component Model. Sub-200µs cold start.

![Metrics Console](/Users/kevin/.gemini/antigravity/brain/c52e29ca-54f3-42ee-afee-e01033da222e/console_metrics_final_1775313386849.png)

## Why

| Technology | Cold Start | Solver Safety | Live Migration | Per-Agent Overhead |
|---|---|---|---|---|
| **Docker/K8s** | 2-5s | None | Extremely Difficult | Whole OS Stack |
| **Agent Frameworks** (LangChain, CrewAI) | Variable | None (process-level) | No | Unbounded |
| **V8 Isolates** | 5ms | None | No | High Memory |
| **Trytet** | **< 200µs** | **Fuel-bounded traps** | **Native** | **< 5MB** |

## Architecture

Multi-layer stack. Technical details in the [Architecture Guide](ARCHITECTURE.md).

```mermaid
graph TD
    A[Mesh Gateway] -->|Ingress| B(Trytet API)
    B --> C{Mesh Router}
    C -->|Local Execution| D[Wasmtime Sandbox]
    C -->|P2P RPC| E[Hive Peers]
    D -->|Persistent VFS| F[(LSM-Vector Store)]
    D -->|Pricing| G[Market Scheduler]
    E -->|Arbitrage| G
    
    style A fill:#00d4ff,stroke:#1e1e2e,color:#0a0a0f
    style D fill:#00ff88,stroke:#1e1e2e,color:#0a0a0f
    style F fill:#ff4466,stroke:#1e1e2e,color:#0a0a0f
```

## Features (v33.1)

- **Neuro-Symbolic Cartridges**: Load deterministic Wasm Components (solvers, verifiers, constraint engines) into an agent's execution graph. Each cartridge runs in a fuel-bounded sub-sandbox with $O(1)$ teardown. The agent reasons; the cartridge computes.
- **Teleportation**: Serialize agent state into a `.tet` artifact, transfer over P2P, revive on a remote node.
- **Copy-on-Write VFS**: Isolated Vector File System with sub-1µs reads and native deduplication on fork.
- **Market Scheduling**: Elastic resource market. Nodes bid for agent workloads using Fuel Vouchers scaled by thermal pressure and CPU availability.
- **Fuel Determinism**: Strict instruction-level fuel limits. Infinite loops are trapped, not timed out.
- **Consensus Locks**: $O(1)$ multi-phase commit prevents double-execution during migration.
- **Northstar Benchmarks**: Built-in latency instrumentation across five critical paths.
- **Path Jailing**: Host filesystem isolation, OOB bounds checking, and preemptive watchdogs for inference loads.

## Frictionless Quickstart

Experience Trytet's uncrashable determinism in under 60 seconds. 

### Option 1: The "Wow" Demo (2 Commands)
Launch the Time-Travel Debugger or the Sub-Millisecond Wasm benchmark instantly.

```bash
# 1. Install the CLI
curl -sL https://trytet.io/install.sh | bash

# 2. Run the interactive Uncrashable Demo
tet mcp
```

### Option 2: Build Your Own Agent (3 Commands)
Compile standard TypeScript into a secure, deterministic Trytet agent and execute it.

```bash
# 1. Write an agent script
echo "console.log('Hello from a deterministic sandbox');" > agent.ts

# 2. Compile to a Trytet Component (.tet)
tet build agent.ts -o agent.tet

# 3. Boot it in the Trytet Engine
tet up agent.tet --fuel 1000000
```

Dashboard at `http://localhost:3000/console`.

## Documentation

- **[Product Vision (The Manifesto)](VISION.md)**
- [Architecture](ARCHITECTURE.md)
- [CLI Reference](CLI.md)
- [Benchmarks](BENCHMARKS.md)
- [API Reference](API.md)
- [Deployment](DEPLOYMENT.md)
