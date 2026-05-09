# Trytet Client SDK for TypeScript

The `@trytet/client` SDK is the official way to interact with the Trytet Engine via TypeScript and Node.js. 

## Features
- Execute compiled `.wasm` agents.
- Snapshot and fork memory boundaries.
- Teleport active state across the Hive network.
- Strongly typed execution payloads and results.

## Usage

```typescript
import { TrytetClient } from '@trytet/client';
import fs from 'fs';

async function main() {
    const client = new TrytetClient({ baseUrl: 'http://localhost:3000' });
    const wasmBytes = fs.readFileSync('./agent.wasm');

    const result = await client.execute({
        payload: Array.from(wasmBytes),
        allocated_fuel: 50_000_000,
        max_memory_mb: 64,
        alias: 'test-agent'
    });

    console.log(result.status);
    console.log(result.telemetry.stdout_lines);
}
```
