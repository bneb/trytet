# Trytet TypeScript SDK — `trytet-client`

Official TypeScript client for the [Trytet Engine](https://trytet.com) — a WebAssembly sandbox runtime for AI agent code execution.

## Installation

```bash
npm install trytet-client
```

Requires Node.js 18+ (for global `fetch` and `WebSocket`). No runtime dependencies; `fetch` and `WebSocket` are expected from the platform.

## Quick Start

```typescript
import { TrytetClient } from 'trytet-client';
import * as fs from 'fs';

async function main() {
  const client = new TrytetClient({ baseUrl: 'http://localhost:3000' });

  // Read and execute a compiled Wasm agent
  const wasmBytes = fs.readFileSync('./agent.wasm');
  const result = await client.execute({
    payload: Array.from(wasmBytes),
    allocated_fuel: 50_000_000,
    max_memory_mb: 64,
    alias: 'my-agent',
  });

  console.log('Status:', result.status);
  console.log('Stdout:', result.telemetry.stdout_lines);
  console.log('Fuel consumed:', result.fuel_consumed);

  // Handle different execution outcomes
  switch (result.status) {
    case 'Success':
      break;
    case 'OutOfFuel':
      console.warn('Agent ran out of fuel — increase allocated_fuel');
      break;
    case 'MemoryExceeded':
      console.warn('Agent exceeded memory limit — increase max_memory_mb');
      break;
    default:
      if (typeof result.status === 'object' && 'Crash' in result.status) {
        console.error('Agent crashed:', result.status.Crash.message);
      }
  }
}

main().catch(console.error);
```

## API Reference

### `TrytetClient`

#### Constructor

```typescript
new TrytetClient(options?: { baseUrl?: string; retry?: Partial<RetryConfig> })
```

| Option   | Type                          | Default               | Description                              |
| -------- | ----------------------------- | --------------------- | ---------------------------------------- |
| baseUrl  | `string`                      | `'http://localhost:3000'` | Trytet Engine HTTP endpoint          |
| retry    | `Partial<RetryConfig>`        | (see below)           | Override exponential-backoff settings    |

Default retry configuration:

```typescript
{ maxRetries: 3, initialDelayMs: 100, maxDelayMs: 5000, backoffMultiplier: 2 }
```

#### Agent Execution

```typescript
execute(request: TetExecutionRequest): Promise<TetExecutionResult>
```

Submit a Wasm payload for execution. See [TetExecutionRequest](#tetexecutionrequest) for the full set of fields.

```typescript
snapshot(tetId: string): Promise<SnapshotResponse>
```

Create a memory snapshot of a running or completed Tet. Returns `{ snapshot_id, size_bytes }`. Use the snapshot ID to fork later.

```typescript
fork(snapshotId: string, request: TetExecutionRequest): Promise<TetExecutionResult>
```

Fork a new Tet from a prior snapshot. The `request` can override `payload`, `env`, `allocated_fuel`, `max_memory_mb`, and other fields.

```typescript
teleport(alias: string, targetNode: string): Promise<void>
```

Teleport a running Tet to another Hive node by alias. The `targetNode` is the node ID or address.

#### Network Topology

```typescript
getTopology(): Promise<TopologyEdge[]>
```

Retrieve the current Hive network topology — nodes and their interconnecting edges with measured latency.

```typescript
getSwarmMetrics(): Promise<NorthstarReport>
```

Retrieve Northstar benchmark metrics for the swarm (teleport latency, fork constant, oracle fidelity, etc.).

```typescript
getHealth(): Promise<{ status: string }>
```

Health-check the engine endpoint.

#### Cartridge Management

```typescript
invokeCartridge(invocation: CartridgeInvocation): Promise<CartridgeResult>
```

Invoke a named cartridge component (e.g., a WASI-compiled utility). See [CartridgeInvocation](#cartridgeinvocation).

#### MCP (Model Context Protocol)

```typescript
mcpCall(method: string, params?: Record<string, unknown>): Promise<unknown>
```

Low-level JSON-RPC call to the MCP endpoint. Prefer the typed helpers below.

```typescript
listTools(): Promise<McpTool[]>
callTool(name: string, args: Record<string, unknown>): Promise<unknown>
listResources(): Promise<McpResource[]>
listPrompts(): Promise<McpPrompt[]>
```

#### Telemetry Stream

```typescript
createTelemetryStream(): TelemetryStream
```

Create a WebSocket stream for real-time swarm telemetry. See [TelemetryStream](#telemetrystream).

### `TelemetryStream`

```typescript
connect(): void              // Open the WebSocket connection
onEvent(callback: TelemetryEventCallback): () => void  // Register listener; returns unregister function
close(): void                // Close stream and clear listeners
```

The stream reconnects automatically with exponential backoff (1s initial, 30s max). `TelemetryEvent` shape:

```typescript
interface TelemetryEvent {
  event_type: string;
  tet_id: string;
  timestamp_us: number;
  data: Record<string, unknown>;
}
```

## Types Reference

### `TetExecutionRequest`

```typescript
interface TetExecutionRequest {
  payload?: number[];                // Raw Wasm bytes as an array of integers
  alias?: string;                    // Human-readable name for the Tet
  env?: Record<string, string>;      // Environment variables injected at start
  injected_files?: Record<string, string>;  // Virtual files (path -> content)
  allocated_fuel?: number;           // Fuel budget (default ~10M)
  max_memory_mb?: number;            // Max linear memory in MB (default 64)
  parent_snapshot_id?: string;       // Fork from an existing snapshot
  target_function?: string;          // Specific WASM export to call
  call_depth?: number;               // Call stack depth limit
  voucher?: FuelVoucher;             // Pre-signed fuel voucher
  manifest?: AgentManifest;          // Agent declaration (permissions, constraints)
  egress_policy?: EgressPolicy;      // Network egress rules
}
```

### `TetExecutionResult`

```typescript
interface TetExecutionResult {
  tet_id: string;
  status: ExecutionStatus;           // 'Success' | 'OutOfFuel' | 'MemoryExceeded' | 'Migrated' | 'Suspended' | { Crash: CrashReport }
  telemetry: StructuredTelemetry;    // stdout_lines, stderr_lines, memory_used_kb
  execution_duration_us: number;
  fuel_consumed: number;
  mutated_files: Record<string, string>;  // Files written by the agent
  migrated_to?: string;              // Target node if status is 'Migrated'
}
```

### `ExecutionStatus`

```typescript
type ExecutionStatus =
  | 'Success'
  | 'OutOfFuel'
  | 'MemoryExceeded'
  | 'Migrated'
  | 'Suspended'
  | { Crash: CrashReport };
```

### `CrashReport`

```typescript
interface CrashReport {
  error_type: string;            // e.g., "Trap", "RuntimeError"
  message: string;
  instruction_offset?: number;   // Wasm offset of the faulting instruction
}
```

### `AgentManifest`

```typescript
interface AgentManifest {
  metadata: {
    name: string;
    version: string;
    author_pubkey?: string;
  };
  constraints: {
    max_memory_pages: number;
    fuel_limit: number;
    max_egress_bytes: number;
  };
  permissions: {
    can_egress: string[];
    can_persist: boolean;
    can_teleport: boolean;
    is_genesis_factory: boolean;
    can_fork: boolean;
  };
}
```

### `EgressPolicy`

```typescript
interface EgressPolicy {
  allowed_domains: string[];
  max_daily_bytes: number;
  require_https: boolean;
}
```

### `FuelVoucher`

```typescript
interface FuelVoucher {
  tet_id: string;
  fuel_limit: number;
  nonce: number;
  signature: number[];
}
```

### `CartridgeInvocation` / `CartridgeResult`

```typescript
interface CartridgeInvocation {
  component_id: string;       // e.g., "js-evaluator"
  payload: string;            // Input to the cartridge
  fuel: number;               // Fuel budget for this invocation
  max_memory_mb?: number;     // Memory limit (default 512)
}

interface CartridgeResult {
  output: string;             // Cartridge output
  fuel_consumed: number;
  duration_us: number;
}
```

### MCP Types

```typescript
interface McpTool {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

interface McpResource {
  uri: string;
  name: string;
  description?: string;
  mimeType?: string;
}

interface McpPrompt {
  name: string;
  description?: string;
  arguments?: McpPromptArgument[];
}

interface McpPromptArgument {
  name: string;
  description?: string;
  required?: boolean;
}
```

### `TopologyEdge`

```typescript
interface TopologyEdge {
  source: string;
  target: string;
  latency_us: number;          // Measured round-trip latency
  bytes_transferred: number;
}
```

### `NorthstarReport`

```typescript
interface NorthstarReport {
  teleport_warp_us: number;
  mitosis_constant_us: number;
  oracle_fidelity_us: number;
  market_evacuation_us: number;
  cartridge_spinup_us: number;
  timestamp: string;
}
```

### `RetryConfig`

```typescript
interface RetryConfig {
  maxRetries: number;           // Default 3
  initialDelayMs: number;       // Default 100
  maxDelayMs: number;           // Default 5000
  backoffMultiplier: number;    // Default 2
}
```

## Error Handling

All methods throw `Error` on non-2xx responses. The error message includes the HTTP status code and response body (truncated). Methods retry on 502, 503, and 504 with exponential backoff before failing.

```typescript
import { TrytetClient } from 'trytet-client';

const client = new TrytetClient();

async function safeExecute() {
  try {
    const result = await client.execute({ payload, alias: 'demo' });
    return result;
  } catch (err) {
    if (err instanceof Error) {
      // Network errors, server errors (after retries exhausted), 4xx responses
      console.error('Execution failed:', err.message);
    }
    throw err;
  }
}
```

WebSocket errors in `TelemetryStream` are handled internally via reconnection. Register an `onEvent` callback to process events; parse errors on individual messages are silently skipped to avoid disrupting the stream.

## Building from Source

```bash
cd sdk/typescript
npm install
npm run build   # Compiles TypeScript to dist/
```
