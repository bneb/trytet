# Benchmarks

Run the built-in benchmark suite with `tet metrics`. All measurements use `std::time::Instant` for monotonic nanosecond-precision timestamps.

## Measured Performance

These numbers are from a MacBook Pro (M-series, 2024) in release build. Debug builds are 3-10x slower due to lack of compiler optimizations.

| Metric | Measured | Notes |
|---|---|---|
| **MCP Server Boot** | 15-70ms | Without cartridge precompilation. First tool call adds ~400ms for Cranelift compilation of the cartridge (one-time cost, cached thereafter). |
| **Bincode Round-trip (16MB)** | 3-5s debug / ~100ms release | Serialize + deserialize a 16MB SnapshotPayload via bincode. |
| **Wasm Module Execution** | ~100-500µs | Execute a minimal Wasm module (no-op `_start`). Includes instantiation and WASI setup. |
| **Cartridge Compilation** | ~400ms first call | Cranelift AOT compilation of a cartridge Wasm component to native code. Cached on subsequent calls. |
| **Cartridge Instantiation (cached)** | <500µs | Instantiate a pre-compiled cartridge, call `execute`, reclaim memory. |

## What We Don't Claim

- The `<200µs` numbers in older docs were aspirational targets for cached hot-path instantiation on specific hardware. They are not representative of end-to-end latency.
- "Sub-millisecond cold start" refers to Wasm module instantiation time, not full sandbox initialization (which includes WASI preopened directory setup, VFS mounting, and host function registration).
- The Hive P2P mesh and teleportation protocol exist in the codebase but have not been benchmarked on a multi-node cluster. Single-node teleport (bincode round-trip) is measured above.

## Running Benchmarks

```bash
# Built-in benchmark suite
tet metrics

# API endpoint
curl http://localhost:3000/v1/swarm/metrics

# Criterion benchmarks (statistical)
cargo bench
```

## Comparison Context

| Runtime | Cold Start | Isolation | Notes |
|---|---|---|---|
| Docker container | ~2s | Full OS | Includes cgroups, network setup, daemon overhead |
| V8 isolate | ~5ms | None | JavaScript only, no memory limits without extra tooling |
| Trytet | ~15ms (full init) | Fuel-bounded Wasm | Deterministic traps, polyglot via Wasm components |

Trytet's advantage is not raw speed — it's **deterministic failure**. When an LLM generates an infinite loop, a Docker container hangs until a wall-clock timeout fires (2-30 seconds). Trytet traps it at the instruction level in microseconds, and the agent gets a structured error it can respond to.
