# Neuro-Symbolic Demo

Full workflow in 45 seconds: agent delegates a constraint problem to a solver, solver runs fuel-bounded, host survives if the solver hangs.

## Script

```
1. Boot the scheduler agent
   $ tet up demos/neuro_symbolic/scheduler.wasm --fuel 50000000

2. Agent receives: "Schedule 5 meetings this week, no conflicts"

3. Agent calls invoke_component("z3-solver", constraints_json, 1000000)
   CartridgeManager instantiates the Z3 stub.
   Z3 returns a satisfiable schedule in < 0.5ms.
   Agent formats the result.

4. Logic bomb: what if the solver hangs?
   $ tet cartridge load bomb demos/neuro_symbolic/logic_bomb.wasm
   Agent calls invoke_component("bomb", ..., 100000)
   FuelExhausted fires in < 100µs.
   Agent is still alive.

5. Live migration
   $ tet teleport scheduler edge-tokyo
   State serialized, transferred, revived in < 2ms.
```

## Files

| File | Purpose |
|:---|:---|
| `z3_stub.wat` | Returns a hardcoded SAT schedule |
| `logic_bomb.wat` | Infinite loop for fuel exhaustion |

## Build

```bash
wat2wasm z3_stub.wat -o z3_stub.wasm
wat2wasm logic_bomb.wat -o logic_bomb.wasm
```

## Why

Agent frameworks run tool calls in the same process as orchestration. Tool hangs, agent hangs. Tool OOMs, agent crashes. No isolation boundary.

Trytet provides that boundary. The solver gets its own Wasm Component sandbox with a fixed fuel budget and independent memory limits. Fuel traps fire at the instruction level, not the wall-clock level. The parent agent's execution is decoupled.

Agents can safely invoke any deterministic computation (SAT solvers, SMT provers, type checkers, symbolic engines) without risking orchestration stability.
