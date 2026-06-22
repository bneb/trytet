# Trytet Python SDK — `trytet-client`

Official Python client for the [Trytet Engine](https://trytet.com) — a WebAssembly sandbox runtime for AI agent code execution.

## Installation

```bash
pip install trytet-client
```

Requires Python 3.10+. Runtime dependencies: `httpx>=0.25`, `websockets>=12.0`, `pydantic>=2.0`.

## Quick Start

```python
import asyncio
from trytet_client import TrytetClient, TetExecutionRequest

async def main():
    async with TrytetClient(base_url="http://localhost:3000") as client:
        # Read and execute a compiled Wasm agent
        with open("agent.wasm", "rb") as f:
            payload = list(f.read())

        result = await client.execute(
            TetExecutionRequest(
                payload=payload,
                allocated_fuel=50_000_000,
                max_memory_mb=64,
                alias="my-agent",
            )
        )

        print(f"Status: {result.status}")
        print(f"Stdout: {result.telemetry.stdout_lines}")
        print(f"Fuel consumed: {result.fuel_consumed}")

        # Check for execution failure
        if result.status == "OutOfFuel":
            print("Agent ran out of fuel — increase allocated_fuel")
        elif result.status == "MemoryExceeded":
            print("Agent exceeded memory limit — increase max_memory_mb")

asyncio.run(main())
```

## API Reference

### `TrytetClient`

#### Constructor

```python
TrytetClient(
    base_url: str = "http://localhost:3000",
    max_retries: int = 3,
    initial_delay_ms: int = 100,
)
```

| Argument        | Type    | Default                    | Description                        |
| --------------- | ------- | -------------------------- | ---------------------------------- |
| base_url        | `str`   | `"http://localhost:3000"` | Trytet Engine HTTP endpoint        |
| max_retries     | `int`   | `3`                       | Max retries on 502/503/504         |
| initial_delay_ms| `int`   | `100`                     | Initial backoff delay in ms        |

The client implements `async with` context manager for automatic cleanup. Call `client.close()` explicitly if not using the context manager.

#### Agent Execution

```python
async execute(req: TetExecutionRequest) -> TetExecutionResult
```

Submit a Wasm payload for execution. Accepts a `TetExecutionRequest` Pydantic model.

```python
async snapshot(tet_id: str) -> SnapshotResponse
```

Create a memory snapshot. Returns `SnapshotResponse(snapshot_id=..., size_bytes=...)`.

```python
async fork(snapshot_id: str, req: TetExecutionRequest) -> TetExecutionResult
```

Fork a new Tet from a prior snapshot. The request can override execution parameters.

```python
async teleport(alias: str, target_node: str) -> None
```

Teleport a running Tet to another Hive node. Raises `RuntimeError` on failure.

#### Network Topology

```python
async get_topology() -> list[TopologyEdge]
async get_swarm_metrics() -> NorthstarReport
async get_health() -> dict
```

#### Cartridge Management

```python
async invoke_cartridge(invocation: CartridgeInvocation) -> CartridgeResult
```

Invoke a named cartridge component.

#### MCP (Model Context Protocol)

```python
async list_tools() -> list[McpTool]
async call_tool(name: str, arguments: dict) -> Any
async list_resources() -> list[McpResource]
async list_prompts() -> list[McpPrompt]
```

#### Telemetry Stream

```python
create_telemetry_stream() -> TelemetryStream
```

Create a WebSocket telemetry stream. See [TelemetryStream](#telemetrystream-1).

### `TelemetryStream`

```python
async connect() -> None          # Connect and begin receiving events (blocks until closed)
on_event(callback) -> Callable   # Register a callback; returns an unregister function
close() -> None                  # Close the stream and stop reconnecting
```

The stream reconnects automatically with exponential backoff (1s initial, 30s max). `TelemetryEvent` shape:

```python
class TelemetryEvent(BaseModel):
    event_type: str
    tet_id: str
    timestamp_us: int
    data: dict
```

Usage:

```python
from trytet_client import TelemetryEvent

stream = client.create_telemetry_stream()

def on_event(event: TelemetryEvent):
    print(f"[{event.event_type}] tet={event.tet_id}")

unregister = stream.on_event(on_event)

# Run connect in a background task:
asyncio.create_task(stream.connect())
```

## Types Reference

All types are Pydantic v2 `BaseModel` classes exported from `trytet_client`.

### `TetExecutionRequest`

```python
class TetExecutionRequest(BaseModel):
    payload: list[int] | None = None            # Raw Wasm bytes
    alias: str | None = None                    # Human-readable name
    env: dict[str, str] | None = None           # Environment variables
    injected_files: dict[str, str] | None = None # Virtual files (path -> content)
    allocated_fuel: int | None = None           # Fuel budget
    max_memory_mb: int | None = None            # Max linear memory in MB
    parent_snapshot_id: str | None = None       # Fork from snapshot
    target_function: str | None = None          # WASM export to call
    call_depth: int = 0                         # Call stack depth limit
    voucher: FuelVoucher | None = None          # Pre-signed fuel voucher
    manifest: AgentManifest | None = None       # Agent declaration
    egress_policy: EgressPolicy | None = None   # Network egress rules
```

### `TetExecutionResult`

```python
class TetExecutionResult(BaseModel):
    tet_id: str
    status: Any                                # "Success" | "OutOfFuel" | ... | {"Crash": {...}}
    telemetry: StructuredTelemetry             # stdout_lines, stderr_lines, memory_used_kb
    execution_duration_us: int
    fuel_consumed: int
    mutated_files: dict[str, str]              # Files written by the agent
    migrated_to: str | None = None             # Target node if migrated
```

### `StructuredTelemetry`

```python
class StructuredTelemetry(BaseModel):
    stdout_lines: list[str] = []
    stderr_lines: list[str] = []
    memory_used_kb: int = 0
```

### `CrashReport`

```python
class CrashReport(BaseModel):
    error_type: str
    message: str
    instruction_offset: int | None = None
```

### `AgentManifest`

```python
class AgentManifest(BaseModel):
    metadata: AgentManifestMetadata             # name, version, author_pubkey
    constraints: AgentManifestConstraints       # max_memory_pages, fuel_limit, max_egress_bytes
    permissions: AgentManifestPermissions       # can_egress, can_persist, can_teleport, ...
```

### `EgressPolicy`

```python
class EgressPolicy(BaseModel):
    allowed_domains: list[str]
    max_daily_bytes: int
    require_https: bool = True
```

### `FuelVoucher`

```python
class FuelVoucher(BaseModel):
    tet_id: str
    fuel_limit: int
    nonce: int
    signature: list[int]
```

### `CartridgeInvocation` / `CartridgeResult`

```python
class CartridgeInvocation(BaseModel):
    component_id: str                           # e.g., "js-evaluator"
    payload: str                                # Input to the cartridge
    fuel: int                                   # Fuel budget
    max_memory_mb: int = 512                    # Memory limit

class CartridgeResult(BaseModel):
    output: str
    fuel_consumed: int
    duration_us: int
```

### `SnapshotResponse`

```python
class SnapshotResponse(BaseModel):
    snapshot_id: str
    size_bytes: int
```

### `TopologyEdge`

```python
class TopologyEdge(BaseModel):
    source: str
    target: str
    latency_us: int
    bytes_transferred: int
```

### `NorthstarReport`

```python
class NorthstarReport(BaseModel):
    teleport_warp_us: int
    mitosis_constant_us: int
    oracle_fidelity_us: int
    market_evacuation_us: int
    cartridge_spinup_us: int
    timestamp: str
```

### MCP Types

```python
class McpTool(BaseModel):
    name: str
    description: str
    inputSchema: dict = {}

class McpResource(BaseModel):
    uri: str
    name: str
    description: str | None = None
    mimeType: str | None = None

class McpPrompt(BaseModel):
    name: str
    description: str | None = None
    arguments: list[McpPromptArgument] | None = None

class McpPromptArgument(BaseModel):
    name: str
    description: str | None = None
    required: bool | None = None
```

## Error Handling

All methods raise `RuntimeError` on non-2xx responses with the HTTP status code and response body (first 500 chars) in the message. Methods retry on 502, 503, and 504 with exponential backoff (doubling delay, 5s cap).

```python
from trytet_client import TrytetClient

client = TrytetClient()

try:
    result = await client.execute(TetExecutionRequest(payload=payload))
except RuntimeError as e:
    print(f"Request failed: {e}")
    # Network errors, server errors (after retries exhausted), 4xx responses
```

WebSocket errors in `TelemetryStream` are handled internally via reconnection. Individual parse errors on messages are silently skipped. Use `close()` to terminate the stream.

## Development

```bash
cd sdk/python
pip install -e ".[dev]"
ruff check .
pytest
```
