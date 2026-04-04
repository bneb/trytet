# API Integration Reference

The Trytet API drives the `tet` CLI underneath the hood. All requests and responses are heavily reliant on `application/json`.
By default, the engine binds to `localhost:3000`.

## Swagger / Core Routes

### Base Operations
- `GET /health` : Node vital check.
- `GET /console`: View embedded command center html dashboard.

### Swarm & Metrics
- `GET /v1/swarm/metrics` : Fetch the Northstar Report (`NorthstarReport`).
- `GET /v1/swarm/stream` : WebSocket URL. Subscribe to JSON-encoded telemetry events originating from agents.
- `POST /v1/swarm/up` : Same behavior as `tet up`. Upload a raw Wasm artifact.
- `GET /v1/topology` : Fetch the topology metrics of agents across nodes. 

### Agent Lifecycle
- `POST /v1/tet/execute` : Execute a payload raw.
- `POST /v1/tet/snapshot/{id}` : Create a point-in-time state artifact. 
- `POST /v1/tet/fork/{id}` : Duplicate the state artifact and spawn an identical child agent.

### Gateway & RPC
- `POST /v1/tet/memory/{id}` : Read memory from a specific Vector File System isolated layer.
- `POST /v1/tet/infer/{id}` : Post an inference prompt directly via Llama RPC.

## Authentication
By default, Trytet clusters are authenticated via **Ed25519 Wallet Signatures**. Each packet specifies:
- `pubkey`: The source agent's Ed25519 public key.
- `signature`: The cryptographic hash of the JSON payload.

## Rate Limits & Meters
Requests spanning over compute limits trigger an HTTP 402 Payment Required or HTTP 503 if the node has declared itself to be at Thermal Exhaustion. Nodes meter "Fuel" dynamically based on Hive Market Multipliers.
