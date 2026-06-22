# API Reference

JSON over HTTP. Default bind: `localhost:3000`.

All `/v1/*` routes require an API key. Pass via `Authorization: Bearer <key>` or `X-API-Key: <key>` header. Public endpoints: `/health`, `/console`, `/v1/mcp`.

## Routes

### Health
- `GET /health` — Health check.

### Auth
- `GET /v1/auth/keys` — List active API keys.
- `POST /v1/auth/keys` — Create a new API key. Body: `{"label": "my-key"}`.
- `DELETE /v1/auth/keys/{prefix}` — Revoke an API key by prefix.

### Agent Lifecycle
- `POST /v1/tet/execute` — Execute a Wasm payload.
- `POST /v1/tet/snapshot/{id}` — Snapshot a completed execution.
- `POST /v1/tet/fork/{snapshot_id}` — Fork from a snapshot.
- `POST /v1/tet/topup` — Supply a fuel voucher to revive a suspended agent.

### Cartridges
- `POST /v1/cartridge/invoke` — Direct cartridge invocation.
- `POST /v1/benchmark/sandbox` — Execute JS in the Wasm sandbox, return benchmark metrics.

### Registry
- `POST /v1/registry/push/{tag}` — Push an artifact to the local registry.
- `GET /v1/registry/pull/{tag}` — Pull an artifact.

### MCP
- `POST /v1/mcp` — JSON-RPC 2.0 endpoint for Model Context Protocol.

### Observability
- `GET /v1/swarm/metrics` — Benchmark report.
- `GET /v1/topology` — Agent topology across nodes.

### Hive (Experimental)
- `GET /v1/hive/peers` — List connected P2P peers.
- `POST /v1/tet/teleport/{alias}` — Migrate an agent to another node.

## SDK Access

```typescript
import { TrytetClient } from 'trytet-client';
const client = new TrytetClient({ baseUrl: 'http://localhost:3000', apiKey: 'tet_...' });
const tools = await client.listTools();
await client.invokeCartridge({ component_id: 'js-evaluator', payload: '2 + 2', fuel: 1_000_000 });
```

```python
from trytet_client import TrytetClient

async with TrytetClient("http://localhost:3000", api_key="tet_...") as client:
    tools = await client.list_tools()
```
