# Northstar Benchmarks

Built-in benchmark runner. Run `tet metrics`. All metrics use nanosecond-precision monotonic clocks (`std::time::Instant`).

## Metrics

| Metric | Ceiling | Typical | Description |
|---|---|---|---|
| **Teleport Warp** | `< 200ms` | `< 2ms` | Full VM state serialization, transfer, and deserialization on a remote node. |
| **Mitosis Constant** | `< 15ms` | `< 1ms` | Fork latency. Lock resolution + CoW VFS pointer deduplication. |
| **Oracle Fidelity** | `< 5ms` | `< 1ms` | Ed25519 payload authentication overhead. |
| **Market Evacuation** | `< 800ms` | `< 5ms` | Time for a node slice to identify thermal arbitrage and shed agents. |
| **Cartridge Spin-up** | `< 500µs` | `< 100µs` | Instantiate a cached Wasm Component, call `cartridge-v1`, reclaim memory. Excludes Cranelift compilation. |

## Context

Docker cold-start: ~2 seconds (cgroups, daemon, IP config, execution).
Trytet boots and teleports in ~1.5ms. 1000x.

## Verify

```bash
curl -X GET http://localhost:3000/v1/swarm/metrics
```

Or view the gauges at `http://localhost:3000/console`.
