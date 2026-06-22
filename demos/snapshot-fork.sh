#!/usr/bin/env bash
# Trytet snapshot/fork demo — "Git for RAM": checkpoint state and
# branch execution from saved snapshots.
#
# Usage: ./demos/snapshot-fork.sh
# Prerequisite: cargo build --release --bin tet
#               cargo run --release --example compile_wat -- demos/counter.wat demos/counter.wasm
set -euo pipefail

TET="${TET:-./target/release/tet}"
WASM="${WASM:-demos/counter.wasm}"
WASM_BYTES=$(python3 -c "import json; print(json.dumps(list(open('$WASM','rb').read())))")

say() { printf "\n%s─── %s%s\n" "$(tput bold)" "$*" "$(tput sgr0)"; }
note() { printf "    %s\n" "$*"; }

cleanup() { lsof -ti :3000 2>/dev/null | xargs kill 2>/dev/null || true; }
trap cleanup EXIT
lsof -ti :3000 2>/dev/null | xargs kill 2>/dev/null || true

echo ""
echo "══════════════════════════════════════════════════════════════"
echo "  Trytet — Snapshot & Fork: Git for RAM"
echo "══════════════════════════════════════════════════════════════"

cat << 'EOF'

  counter.wasm: a WebAssembly module that stores a counter at
  linear memory address 0 and increments it on each invocation.

  This demo shows three primitives that no other sandbox provides:

    1. Execute → the counter advances
    2. Snapshot → capture the counter state
    3. Fork → branch from the snapshot; each branch evolves independently

EOF

# ── Start server ─────────────────────────────────────────────────────

SERVER_LOG=$(mktemp)
"$TET" serve > /dev/null 2> "$SERVER_LOG" &
SERVER_PID=$!
sleep 2

# Extract boot key from server logs
BOOT_KEY=$(grep -o 'tet_[a-f0-9-]\{36\}' "$SERVER_LOG" | head -1 || echo "")
AUTH_HEADER="Authorization: Bearer ${BOOT_KEY}"

say "Step 1: Execute counter.wasm (no parent snapshot)"
RESP1=$(curl -s -X POST http://localhost:3000/v1/tet/execute \
  -H "Content-Type: application/json" \
  -H "$AUTH_HEADER" \
  -d "{\"payload\":$WASM_BYTES,\"allocated_fuel\":1000000,\"max_memory_mb\":10,\"alias\":\"counter\"}")
TET_ID1=$(echo "$RESP1" | python3 -c "import sys,json; print(json.load(sys.stdin).get('tet_id',''))" 2>/dev/null || echo "")
note "Counter initialized: tet_id = ${TET_ID1:0:8}..."
note "The counter starts at 0. _start increments it to 1."

say "Step 2: Execute again on the same agent"
RESP2=$(curl -s -X POST http://localhost:3000/v1/tet/execute \
  -H "Content-Type: application/json" \
  -H "$AUTH_HEADER" \
  -d "{\"allocated_fuel\":1000000,\"max_memory_mb\":10,\"alias\":\"counter\"}")
note "Counter: 1 → 2 (accumulates across invocations)"

say "Step 3: Snapshot the agent's state"
SNAP=$(curl -s -X POST "http://localhost:3000/v1/tet/snapshot/$TET_ID1" \
  -H "Content-Type: application/json" \
  -H "$AUTH_HEADER")
SNAP_ID=$(echo "$SNAP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('snapshot_id',''))" 2>/dev/null || echo "")
note "Snapshot captured: ${SNAP_ID:0:8}..."
note "The snapshot contains the counter at value 2."

say "Step 4: Fork from the snapshot"
FORK=$(curl -s -X POST "http://localhost:3000/v1/tet/fork/$SNAP_ID" \
  -H "Content-Type: application/json" \
  -H "$AUTH_HEADER" \
  -d "{\"allocated_fuel\":1000000,\"max_memory_mb\":10,\"alias\":\"branch\"}")
FORK_ID=$(echo "$FORK" | python3 -c "import sys,json; print(json.load(sys.stdin).get('tet_id',''))" 2>/dev/null || echo "")
note "Forked agent: ${FORK_ID:0:8}..."
note "The fork starts with the snapshot's state (counter = 2)."

say "Step 5: Execute on the fork — diverges independently"
RESP3=$(curl -s -X POST http://localhost:3000/v1/tet/execute \
  -H "Content-Type: application/json" \
  -H "$AUTH_HEADER" \
  -d "{\"allocated_fuel\":1000000,\"max_memory_mb\":10,\"alias\":\"branch\"}")
note "Fork counter: 2 → 3"

RESP4=$(curl -s -X POST http://localhost:3000/v1/tet/execute \
  -H "Content-Type: application/json" \
  -H "$AUTH_HEADER" \
  -d "{\"allocated_fuel\":1000000,\"max_memory_mb\":10,\"alias\":\"counter\"}")
note "Original counter: 2 → 3 (evolves separately from fork)"

kill "$SERVER_PID" 2>/dev/null || true
wait "$SERVER_PID" 2>/dev/null || true

echo ""
cat << 'EOF'
  ┌────────────┐
  │  Execute   │  counter: 0 → 1
  └─────┬──────┘
        │
  ┌─────▼──────┐
  │  Execute   │  counter: 1 → 2
  └─────┬──────┘
        │
  ┌─────▼──────┐
  │  Snapshot  │  captures counter = 2
  └─────┬──────┘
        │
   ┌────┴────┐
   │         │
   ▼         ▼
 Original   Fork
   │         │
   ▼         ▼
 counter   counter
 2 → 3     2 → 3     (same starting state, independent futures)

  Why this matters:

  An agent can snapshot before exploring a risky branch.
  If the branch traps, the agent hasn't lost its state.
  If the branch succeeds, it can merge or continue independently.
  This is Git for RAM — checkpoint, branch, explore.

══════════════════════════════════════════════════════════════
EOF
