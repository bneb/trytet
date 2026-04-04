# The Northstar Performance Benchmarks

At Trytet, we formalize the difference between "Dumb Infrastructure" and "Living Intelligence" via the "Sovereign Delta". 

To prove this, `tet-core` has a built-in benchmark runner. Run `tet metrics`. All metrics are evaluated using nanosecond-precision monotonic clocks (`std::time::Instant`) within synchronous contexts.

## The Metrics

| Metric | Ceiling | Typical Value | What it measures |
|---|---|---|---|
| **Teleport Warp** | `< 200ms` | `< 2ms` | The time it takes for a full Wasm Agent's VM state buffer to be serialized, transferred from memory, and de-serialized on a foreign machine. |
| **Mitosis Constant** | `< 15ms` | `< 1ms` | The latency penalty to fork an existing running Agent, resolving internal locks, and deduplicating the VFS copy-on-write pointers. |
| **Oracle Fidelity** | `< 5ms` | `< 1ms` | The cryptography overhead associated with guaranteeing the Wasm payload originates from an authenticated wallet (e.g. Ed25519 hash validation). |
| **Market Evacuation** | `< 800ms` | `< 5ms` | The cluster "Thermal Panic Drill" — how quickly an entire slice of nodes can identify an arbitrage opportunity and begin shedding 1,000s of agents to neighbor nodes. |

## Why it Matters: Docker vs Trytet

A Docker container cold-start takes at best ~2 seconds to initialize underlying cgroups, boot the daemon, assign IP configurations, and start execution.

Trytet boots *and teleports* in ~`1.5 milliseconds` — over **1000x faster**. Under extreme thermal or CPU pressure, Trytet shifts its load transparently before the Linux layer ever hits a warning limit.

## Verify Locally

Launch a local cluster and poll the endpoint to witness Trytet's speed:
```bash
curl -X GET http://localhost:3000/v1/swarm/metrics
```

Or view the visually enriched gauges directly on your **Console** at `http://localhost:3000/console`.
