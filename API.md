# API Reference

JSON over HTTP. Default bind: `localhost:3000`.

## Routes

### Base
- `GET /health` : Health check.
- `GET /console` : Embedded dashboard.

### Swarm & Metrics
- `GET /v1/swarm/metrics` : Northstar benchmark report.
- `GET /v1/swarm/stream` : WebSocket telemetry stream.
- `POST /v1/swarm/up` : Upload a Wasm artifact (equivalent to `tet up`).
- `GET /v1/topology` : Agent topology across nodes.

### Agent Lifecycle
- `POST /v1/tet/execute` : Execute a Wasm payload.
- `POST /v1/tet/snapshot/{id}` : Point-in-time state snapshot.
- `POST /v1/tet/fork/{id}` : Fork from snapshot.

### Gateway & RPC
- `POST /v1/tet/memory/{id}` : Read from an agent's VFS layer.
- `POST /v1/tet/infer/{id}` : Inference prompt via Llama RPC.

### Cartridge Host Functions (Wasm ABI)

Available to guest modules via the `trytet` import namespace. Not HTTP endpoints.

- `trytet::invoke_component(component_id_ptr, component_id_len, payload_ptr, payload_len, fuel, out_ptr, out_len_ptr) -> i32`

  Invoke a pre-compiled Wasm Component by content ID. Runs in an isolated sub-sandbox.

  | Return Code | Meaning |
  |---|---|
  | `0` | Success, result written to `out_ptr` |
  | `1` | Fuel exhausted |
  | `2` | Buffer too small, required size at `out_len_ptr` |
  | `3` | Compilation failed |
  | `4` | Interface mismatch (no `cartridge-v1` export) |
  | `5` | Execution error (cartridge returned `Err`) |
  | `6` | Registry error (component ID not found) |

## Authentication

Ed25519 wallet signatures. Each packet includes:
- `pubkey`: Source agent's public key.
- `signature`: Signed hash of the JSON payload.

## Rate Limits

Requests exceeding compute limits receive HTTP 402 or 503 (thermal exhaustion). Fuel is metered per Hive Market multipliers.
