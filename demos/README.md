# Trytet Demos

Two demos — one for the hook, one for the deep dive.

## Prerequisites

```bash
cargo build --release --bin tet
# For snapshot-fork demo only:
cargo run --release --example compile_wat -- demos/counter.wat demos/counter.wasm
```

## Demo 1: `mcp-walkthrough.sh` — The Hook

**Time:** 60 seconds. **Audience:** Anyone who runs AI-generated code.

What it demonstrates:

| Scenario | What happens | Why it matters |
|----------|-------------|----------------|
| LLM generates a buggy `nextPrime()` with missing `candidate++` | Trytet traps it with `FuelExhausted` in microseconds | LLMs produce bugs that look correct. A hung process is the worst failure mode — the caller doesn't know if it's computing or dead. Trytet returns a deterministic error. |
| LLM generates correct array sum | Returns `55`, reports 2.5M fuel instructions consumed | Even correct code is metered. You know exactly what each invocation cost. |
| JMESPath query against nested JSON | Extracts `orders[*].items[*].name` through a sandboxed cartridge | Cartridges run in isolated sub-sandboxes with their own fuel budgets. No shell injection risk from user-provided query strings. |

**Key takeaway:** Fuel metering turns hangs into errors. Every instruction is accounted for.

## Demo 2: `snapshot-fork.sh` — The Deep Dive

**Time:** 90 seconds. **Audience:** Developers evaluating sandbox architectures.

What it demonstrates:

| Step | What happens | Why it matters |
|------|-------------|----------------|
| Execute `counter.wasm` (no parent) | Counter initializes: 0 → 1 | A fresh agent starts with zeroed memory |
| Execute again on same agent | Counter: 1 → 2 | State accumulates across invocations |
| Snapshot the agent | Captures counter = 2 into a snapshot ID | Checkpoint: freeze the agent's linear memory and VFS at a known-good point |
| Fork from the snapshot | Creates a new agent with counter = 2 | Branch: the fork inherits the snapshot's full memory state |
| Execute on fork vs original | Both advance to 3, independently | Each branch evolves in isolation — no shared mutable state |

**Execution graph:**

```
Execute → counter: 0→1
    │
Execute → counter: 1→2
    │
Snapshot → captures counter=2
    │
    ├── Original: counter 2→3
    │
    └── Fork:     counter 2→3  (same start, independent future)
```

**Key takeaway:** Snapshot/fork enables checkpoint-and-branch execution. An agent can snapshot before exploring a risky path, then fork N times to test N hypotheses without paying setup cost N times. This is Git for RAM.

## Check Trytet's work

Full verification guide with assertion-based tests: **[docs/VERIFY.md](../docs/VERIFY.md)**

Below are the key verification commands — copy, paste, and inspect the raw
responses.

The demos prettify the output, but every claim can be independently verified
against raw API responses. Copy and paste.

### Verify the fuel trap (MCP)

```bash
# Claim: while(true){} traps with FuelExhausted in microseconds
printf '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"trytet_execute","arguments":{"code":"while(true){}","language":"javascript","fuel":50000}}}\n' \
  | ./target/release/tet mcp 2>/dev/null \
  | python3 -c "
import sys, json
d = json.load(sys.stdin)
data = json.loads(d['result']['content'][0]['text'])
print(f'isError: {d[\"result\"][\"isError\"]}')         # expected: True
print(f'stderr:  {data[\"stderr\"]}')                    # expected: fuel exhausted
print(f'traps:   {data[\"traps\"]}')                     # expected: ['FuelExhausted']
print(f'stdout:  {repr(data[\"stdout\"])}')              # expected: ''
"
```

### Verify execute → snapshot → fork (HTTP API)

```bash
# Clean up any previous server and start fresh
lsof -ti :3000 2>/dev/null | xargs kill 2>/dev/null || true
SERVER_LOG=$(mktemp)
./target/release/tet serve > /dev/null 2> "$SERVER_LOG" &
sleep 2
BOOT_KEY=$(grep -o 'tet_[a-f0-9-]\{36\}' "$SERVER_LOG" | head -1)
AUTH="Authorization: Bearer $BOOT_KEY"

# Compile the counter module if not already built
cargo run --release --example compile_wat -- demos/counter.wat demos/counter.wasm 2>/dev/null
WASM_BYTES=$(python3 -c "import json; print(json.dumps(list(open('demos/counter.wasm','rb').read())))")

# Claim 1: execute returns a real tet_id, fuel_consumed is non-zero
EXEC=$(curl -s -X POST http://localhost:3000/v1/tet/execute \
  -H "Content-Type: application/json" -H "$AUTH" \
  -d "{\"payload\":$WASM_BYTES,\"allocated_fuel\":1000000,\"max_memory_mb\":10}")
echo "$EXEC" | python3 -m json.tool
echo "$EXEC" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert d['tet_id'], 'tet_id should not be empty'
assert d['fuel_consumed'] > 0, 'fuel_consumed should be > 0'
print(f'tet_id: {d[\"tet_id\"]}  fuel: {d[\"fuel_consumed\"]} instr  duration: {d[\"execution_duration_us\"]}µs  OK')
"

# Claim 2: snapshot returns a real snapshot_id
TET_ID=$(echo "$EXEC" | python3 -c "import sys,json; print(json.load(sys.stdin)['tet_id'])")
SNAP=$(curl -s -X POST "http://localhost:3000/v1/tet/snapshot/$TET_ID" \
  -H "Content-Type: application/json" -H "$AUTH")
echo "$SNAP" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert d['snapshot_id'], 'snapshot_id should not be empty'
print(f'snapshot_id: {d[\"snapshot_id\"]}  OK')
"

# Claim 3: fork creates a new tet_id distinct from the original
SNAP_ID=$(echo "$SNAP" | python3 -c "import sys,json; print(json.load(sys.stdin)['snapshot_id'])")
FORK=$(curl -s -X POST "http://localhost:3000/v1/tet/fork/$SNAP_ID" \
  -H "Content-Type: application/json" -H "$AUTH" \
  -d "{\"allocated_fuel\":1000000,\"max_memory_mb\":10}")
echo "$FORK" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert d['tet_id'], 'fork tet_id should not be empty'
assert d['tet_id'] != '$TET_ID', 'fork should have a different tet_id from original'
print(f'fork tet_id: {d[\"tet_id\"]}  distinct from original  OK')
"

# Cleanup
kill %1 2>/dev/null; wait 2>/dev/null
```
