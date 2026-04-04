# Trytet Architecture

Trytet is a true "Polyglot Monolith" built to orchestrate, execute, and migrate Wasm-based Sovereign Agents at sub-millisecond latencies. 

## System Layers

### 1. The Sandbox (`src/sandbox.rs`, `src/interpreter.rs`)
At the core of Trytet is a highly tuned Wasmtime execution engine. Instead of wrapping an OS layer, it safely compiles WebAssembly modules AOT (Ahead of Time) and provisions memory bounds. 
- **Fuel Determinism**: By injecting `consume_fuel()` opcodes, infinite loops and excessive resource usages are hard-halted before causing node instability.

### 2. Copy-on-Write Vector File System (CoW VFS) (`src/memory.rs`, `src/shards.rs`)
Traditional agents carry bloated state. Trytet's agents are backed by a Tiered Geometric LSM-Vector store. 
When an agent `forks` or is teleported, memory pages and vector storage are instantly deduplicated. Writes target Layer 1 (DashMap) while reads fall through to Layer 2 (Shared RwLock).

### 3. Mesh Router (`src/mesh.rs`, `src/gateway.rs`)
Agents communicate via the Tet Mesh. The gateway translates HTTP ingress to Mesh RPC calls. If an Agent exists on a different machine, the Mesh delegates it to the...

### 4. Hive P2P Substrate (`src/hive.rs`, `src/consensus.rs`)
All Trytet nodes auto-discover sequentially and form the Trytet Hive. 
Migrating an agent from Node A to Node B uses the **Teleportation Protocol** (Phase 14.4).
- **Consensus Lock**: Ensures the agent cannot "fork-bomb" the network by double-execution. 
- Locks use an $O(1)$ multi-phase commit.

### 5. Market Scheduler (`src/market.rs`, `src/economy.rs`)
The cluster is dynamically load-balanced through an **Economic Market Scheduler**:
- Nodes broadcast Market Bids detailing their thermal stress ($T^\circ$) and CPU availability.
- The Engine identifies "arbitrage" opportunities — e.g. Node B is offering a 50% discount on Fuel.
- Highly stressed nodes initiate "Evacuation Teleports" to neighbors to shed load.

## Module Map

| File | Purpose | Layer |
|---|---|---|
| `src/main.rs` | Boots the HTTP Server and Engine | Daemon |
| `src/sandbox.rs` | Wasmtime Host Configuration | Compute |
| `src/market.rs` | Market bidding metrics and arbitrage | Orchestration |
| `src/economy.rs` | Fuel voucher issuance and settlement | Orchestration |
| `src/mesh.rs` | Inter-agent process RPC routing | Network |
| `src/hive.rs` | Multi-node P2P cluster discovery | Network |
| `src/resurrection.rs` | Context-aware Agent artifact reanimation | Lifecycle |
| `src/telemetry.rs` | Nano-second metrics collection event stream | Observability |
| `src/benchmarks.rs` | Northstar Performance instrumentation suite | Diagnostics |

## Teleportation Flow

1. Node receives `tet teleport agent --node target_id`.
2. Agent's internal execution is paused.
3. Wasm WebAssembly memory buffer gets snapshotted and bincode serialized.
4. CoW VFS vectors are differential-snapshotted.
5. Node A acquires a transit lock over Hive Gossip.
6. Payload is streamed to Node B (`/v1/tet/execute`).
7. Node B deserializes and injects it back directly into Wasmtime in under $200\mu s$.
