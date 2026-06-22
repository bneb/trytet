#!/usr/bin/env bash
# Trytet walkthrough — demonstrates why fuel-metered sandboxing matters.
# Usage: ./demos/mcp-walkthrough.sh
# Prerequisite: cargo build --release --bin tet
set -euo pipefail

TET="${TET:-./target/release/tet}"
say() { printf "\n%s─── %s%s\n" "$(tput bold)" "$*" "$(tput sgr0)"; }

cleanup() { lsof -ti :3000 2>/dev/null | xargs kill 2>/dev/null || true; }
trap cleanup EXIT
lsof -ti :3000 2>/dev/null | xargs kill 2>/dev/null || true

echo ""
echo "══════════════════════════════════════════════════════════════"
echo "  Trytet — Fuel-Metered Sandbox Demo"
echo "══════════════════════════════════════════════════════════════"

# ── Scenario 1: LLM generates plausible bug ──────────────────────────

say "Scenario 1: LLM-generated code with an infinite-loop bug"

cat << 'EOF'

  An LLM was asked: "Write a function to find the next prime
  number after a given integer."

  It produced this. Looks reasonable, right?

EOF

cat << 'ENDCODE'
    function nextPrime(n) {
        let candidate = n + 1;
        while (true) {
            let isPrime = true;
            for (let i = 2; i < candidate; i++) {
                if (candidate % i === 0) { isPrime = false; break; }
            }
            if (isPrime) return candidate;
            // BUG: candidate++ is missing — loops forever on composites
        }
    }
    nextPrime(100);
ENDCODE

echo ""
echo "  Without Trytet: hangs your process. With Trytet: deterministic trap."

say "Running through Trytet (50ms fuel budget)"

printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"demo","version":"1.0"}}}\n' > /tmp/tet_req
printf '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"trytet_execute","arguments":{"code":"function nextPrime(n) { let candidate = n + 1; while (true) { let isPrime = true; for (let i = 2; i < candidate; i++) { if (candidate %% i === 0) { isPrime = false; break; } } if (isPrime) return candidate; } } nextPrime(100);","language":"javascript","fuel":500000}}}' >> /tmp/tet_req

"$TET" mcp < /tmp/tet_req 2>/dev/null | while IFS= read -r line; do
    [ -z "$line" ] && continue
    id=$(echo "$line" | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))" 2>/dev/null || echo "")
    case "$id" in
        1) echo "  connected: Trytet Engine MCP v0.2.0" ;;
        2)
            echo "$line" | python3 -c "
import sys, json
d = json.load(sys.stdin)
data = json.loads(d['result']['content'][0]['text'])
print(f'  result:  {data[\"stderr\"]} (fuel exhausted)')
print(f'  traps:   {data[\"traps\"]}')
print(f'  isError: {d[\"result\"][\"isError\"]}')
print()
print('  In Node.js or a V8 isolate, this would hang forever.')
print('  Trytet traps it deterministically in microseconds.')
print('  The caller gets a structured error, not a hung process.')
" 2>/dev/null || echo "  $line"
            ;;
    esac
done

# ── Scenario 2: Safe execution, full accounting ─────────────────────

echo ""
say "Scenario 2: LLM generates correct code — still metered"

cat << 'EOF'

  Same LLM, fixed prompt: "Write a function that sums an array
  of numbers and returns the total."

  Correct code, but Trytet still meters every instruction.

EOF

printf '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"trytet_execute","arguments":{"code":"[1,2,3,4,5,6,7,8,9,10].reduce((a,b) => a + b, 0)","language":"javascript"}}}' > /tmp/tet_req2
"$TET" mcp < /tmp/tet_req2 2>/dev/null | while IFS= read -r line; do
    [ -z "$line" ] && continue
    id=$(echo "$line" | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))" 2>/dev/null || echo "")
    case "$id" in
        3)
            echo "$line" | python3 -c "
import sys, json
d = json.load(sys.stdin)
data = json.loads(d['result']['content'][0]['text'])
print(f'  output: {data[\"stdout\"]}')
print(f'  fuel:   {data[\"fuel_used\"]} instructions consumed')
print(f'  error:  {d[\"result\"][\"isError\"]}')
print()
print('  Every Wasm instruction burns fuel from a budget.')
print('  Memory is capped per sandbox. No process-level side effects.')
" 2>/dev/null || echo "  $line"
            ;;
    esac
done

# ── Scenario 3: JMESPath against real JSON ──────────────────────────

echo ""
say "Scenario 3: Query structured data safely"

cat << 'EOF'

  An agent needs to extract specific fields from a nested API
  response. Trytet's JMESPath cartridge runs the query in a
  sandbox — no shell injection risk from user-provided paths.

EOF

python3 -c "
import json
req = {
    'jsonrpc': '2.0',
    'id': 4,
    'method': 'tools/call',
    'params': {
        'name': 'trytet_jmespath_evaluator',
        'arguments': {
            'expression': 'orders[*].items[*].name',
            'json': json.dumps({'orders': [
                {'id': 1, 'items': [{'name': 'widget', 'price': 9.99}, {'name': 'gadget', 'price': 14.50}]},
                {'id': 2, 'items': [{'name': 'doohickey', 'price': 3.25}]}
            ]})
        }
    }
}
print(json.dumps(req))
" > /tmp/tet_req3

"$TET" mcp < /tmp/tet_req3 2>/dev/null | while IFS= read -r line; do
    [ -z "$line" ] && continue
    id=$(echo "$line" | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))" 2>/dev/null || echo "")
    case "$id" in
        4)
            echo "$line" | python3 -c "
import sys, json
d = json.load(sys.stdin)
result = d['result']['content'][0]['text']
print(f'  query:  orders[*].items[*].name')
print(f'  result: {result}')
" 2>/dev/null || echo "  $line"
            ;;
    esac
done

# ── What you get ────────────────────────────────────────────────────

echo ""
echo "══════════════════════════════════════════════════════════════"
echo "  Why this matters for AI agent code execution:"
echo ""
echo "  1. LLMs generate bugs. Fuel metering means bugs trap"
echo "     instead of hanging your process."
echo "  2. Every instruction is accounted for. You know exactly"
echo "     what each agent invocation cost."
echo "  3. Cartridges (JMESPath, regex, JS, etc.) run in"
echo "     isolated sub-sandboxes with their own budgets."
echo "  4. Snapshot/fork lets agents checkpoint state and"
echo "     explore branches — Git for RAM."
echo ""
echo "  Connect to Claude Desktop:"
echo ""
echo "  {"
echo "    \"mcpServers\": {"
echo "      \"trytet\": {"
echo "        \"command\": \"$TET\","
echo "        \"args\": [\"mcp\"]"
echo "      }"
echo "    }"
echo "  }"
echo "══════════════════════════════════════════════════════════════"
