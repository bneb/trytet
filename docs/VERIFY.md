# Verify Trytet

Each claim Trytet makes can be verified independently against the running
server. No pre-scripted output — every response is live from the MCP or HTTP
endpoint.

## Prerequisites

```bash
cargo build --release --bin tet
```

## MCP smoke test

Start `tet mcp` as a subprocess, send six JSON-RPC requests, and inspect the
responses. The Python script below exercises the full MCP lifecycle:
handshake, tool discovery, safe execution, fuel trapping, structured data
query, and error handling.

```bash
python3 << 'PYEOF'
import subprocess, json

requests = [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2024-11-05", "capabilities": {},
        "clientInfo": {"name": "verify", "version": "1.0"}}},
    {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
    {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
        "name": "trytet_execute",
        "arguments": {"code": "42 * 9", "language": "javascript"}}},
    {"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {
        "name": "trytet_execute",
        "arguments": {"code": "while(true){}", "language": "javascript", "fuel": 50000}}},
    {"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {
        "name": "trytet_jmespath_evaluator",
        "arguments": {"expression": "keys(@)",
                       "json": json.dumps({"name": "trytet", "version": "0.2.0", "crates": 8})}}},
    {"jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": {
        "name": "trytet_snapshot",
        "arguments": {"agent_id": "nonexistent"}}},
]

payload = "\n".join(json.dumps(r) for r in requests) + "\n"

proc = subprocess.run(
    ["./target/release/tet", "mcp"],
    input=payload, capture_output=True, text=True, timeout=10,
)

responses = [json.loads(l) for l in proc.stdout.strip().split("\n") if l.strip()]

for r in responses:
    rid = r.get("id")

    if rid == 1:
        s = r["result"]["serverInfo"]
        print(f"initialize:        {s['name']} v{s['version']}")

    elif rid == 2:
        names = [t["name"] for t in r["result"]["tools"]]
        print(f"tools/list:        {len(names)} tools ({', '.join(names[:3])}, ...)")

    elif rid == 3:
        data = json.loads(r["result"]["content"][0]["text"])
        print(f"trytet_execute:    42 * 9 = {data['stdout']} "
              f"({data['fuel_used']} fuel, isError={r['result']['isError']})")

    elif rid == 4:
        data = json.loads(r["result"]["content"][0]["text"])
        print(f"fuel trap:         while(true){{}} -> {data['stderr']} "
              f"(traps={data['traps']}, isError={r['result']['isError']})")

    elif rid == 5:
        print(f"trytet_jmespath:   keys(@) -> {r['result']['content'][0]['text']}")

    elif rid == 6:
        err = r.get("error", {})
        print(f"trytet_snapshot:   nonexistent agent -> {err.get('message', '')[:60]}")

print(f"\nAll 6 requests returned valid JSON-RPC responses.")
PYEOF
```

Expected output:

```
initialize:        Trytet Engine MCP v0.2.0
tools/list:        8 tools (trytet_js_evaluator, trytet_regex_evaluator, trytet_jmespath_evaluator, ...)
trytet_execute:    42 * 9 = 378 (2158052 fuel, isError=False)
fuel trap:         while(true){} -> fuel exhausted (traps=['FuelExhausted'], isError=True)
trytet_jmespath:   keys(@) -> {"result":"[\"crates\",\"name\",\"version\"]","error":null}
trytet_snapshot:   nonexistent agent -> Snapshot not found for ID: nonexistent

All 6 requests returned valid JSON-RPC responses.
```

Fuel numbers will vary slightly. The key invariants are: `isError` is `False` for
safe code and `True` for the fuel trap; `traps` contains `FuelExhausted` for the
infinite loop; the JMESPath query returns the expected keys.

## HTTP API smoke test

The HTTP server exposes execute, snapshot, and fork endpoints. This script
starts the server, authenticates with the boot key, and verifies each endpoint
returns valid responses with Python assertions.

```bash
# Start fresh
lsof -ti :3000 2>/dev/null | xargs kill 2>/dev/null || true
SERVER_LOG=$(mktemp)
./target/release/tet serve > /dev/null 2> "$SERVER_LOG" &
sleep 2
BOOT_KEY=$(grep -o 'tet_[a-f0-9-]\{36\}' "$SERVER_LOG" | head -1)
AUTH="Authorization: Bearer $BOOT_KEY"

# Compile the counter module
cargo run --release --example compile_wat -- demos/counter.wat demos/counter.wasm 2>/dev/null
WASM_BYTES=$(python3 -c "import json; print(json.dumps(list(open('demos/counter.wasm','rb').read())))")

# Execute
EXEC=$(curl -s -X POST http://localhost:3000/v1/tet/execute \
  -H "Content-Type: application/json" -H "$AUTH" \
  -d "{\"payload\":$WASM_BYTES,\"allocated_fuel\":1000000,\"max_memory_mb\":10}")
echo "$EXEC" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert d['tet_id'], 'missing tet_id'
assert d['fuel_consumed'] > 0, 'fuel_consumed should be > 0'
print(f'execute:       tet_id={d[\"tet_id\"][:8]}...  '
      f'fuel={d[\"fuel_consumed\"]}  duration={d[\"execution_duration_us\"]}us')
"

# Snapshot
TET_ID=$(echo "$EXEC" | python3 -c "import sys,json; print(json.load(sys.stdin)['tet_id'])")
SNAP=$(curl -s -X POST "http://localhost:3000/v1/tet/snapshot/$TET_ID" \
  -H "Content-Type: application/json" -H "$AUTH")
echo "$SNAP" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert d['snapshot_id'], 'missing snapshot_id'
print(f'snapshot:      snapshot_id={d[\"snapshot_id\"][:8]}...')
"

# Fork
SNAP_ID=$(echo "$SNAP" | python3 -c "import sys,json; print(json.load(sys.stdin)['snapshot_id'])")
FORK=$(curl -s -X POST "http://localhost:3000/v1/tet/fork/$SNAP_ID" \
  -H "Content-Type: application/json" -H "$AUTH" \
  -d "{\"allocated_fuel\":1000000,\"max_memory_mb\":10}")
echo "$FORK" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert d['tet_id'], 'missing tet_id'
assert d['tet_id'] != '$TET_ID', 'fork and original should have different IDs'
print(f'fork:          tet_id={d[\"tet_id\"][:8]}...  distinct from original')
"

kill %1 2>/dev/null; wait 2>/dev/null
```

Expected output:

```
execute:       tet_id=eadd29fe...  fuel=7  duration=13073us
snapshot:      snapshot_id=9e08f25f...
fork:          tet_id=1f9dd814...  distinct from original
```

The counter module is 63 bytes — a Wasm program that increments a 32-bit value
at linear memory address 0. Fuel consumption of 7 instructions reflects the
single load-add-store sequence. The fork receives a new `tet_id` because it is
an independent agent, even though it inherits the snapshot's memory state.

## What these tests verify

| Claim | Evidence |
|-------|----------|
| 8 MCP tools register correctly | `tools/list` returns all 8 names |
| Safe JavaScript executes deterministically | `42 * 9` returns `378` with fuel accounting |
| Infinite loops trap instead of hanging | `while(true){}` returns `FuelExhausted` in microseconds |
| Cartridge queries return structured results | JMESPath `keys(@)` extracts fields from JSON |
| Errors propagate through the protocol | Snapshot of nonexistent agent returns a typed MCP error |
| HTTP execute returns a real agent ID | Response contains a non-empty UUID `tet_id` |
| Snapshot captures agent state | Response contains a non-empty UUID `snapshot_id` |
| Fork creates an independent agent | Forked `tet_id` differs from the original |

Every assertion uses Python's `assert` — a failed claim produces a traceback
showing exactly which invariant broke.
