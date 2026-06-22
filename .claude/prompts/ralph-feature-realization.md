# Trytet Feature Realization

You are working on the Trytet codebase at `/Users/kevin/projects/trytet` — a Rust project that runs untrusted code in fuel-metered WebAssembly sandboxes via Wasmtime.

## What already works

- `tet mcp` — MCP server over stdio (5 cartridge tools: JS eval, regex, JMESPath, scraper, SQL)
- `tet serve` — HTTP API server on port 3000
- `tet doctor` — install diagnostics
- Cargo test/clippy/fmt all pass clean

## What you will build

### 1. General-purpose sandbox as an MCP tool

Add a `trytet_execute` MCP tool that accepts:
- `code` (string, required) — JavaScript, Python, or raw WAT
- `language` (string, default "javascript") — "javascript" | "python" | "wat"
- `fuel` (int, default 5_000_000) — fuel budget
- `memory_mb` (int, default 64) — memory cap

It executes the code in a fresh Wasmtime sandbox and returns `{stdout, stderr, fuel_used, memory_kb, traps: []}`.

Wire this in `src/mcp/server.rs` by adding a tool entry alongside the existing 5. The execution path already exists in `src/sandbox/sandbox_wasmtime.rs` — `execute_inner`. Reuse it, avoid duplicating.

### 2. `tet up` must start the API server by default

`tet up` currently requires a `.tet` file argument. Make it so:
- `tet up` (no arguments) = start the API server on port 3000
- `tet up <file.tet>` = boot a .tet artifact (existing behavior)

The server startup code is already extracted in `src/server/start.rs`. Call `start(Config::from_env())`.

### 3. Fix or cull broken CLI commands

These commands are declared in `src/bin/tet.rs` but are stubs or broken:

**Fix:**
- `tet ps` — list active agents. Read from `WasmtimeSandbox`'s active state tracking. If no agents are active, print "No active agents."
- `tet logs -f <alias>` — tail telemetry events from an agent to stdout. Use `TelemetryHub::subscribe()` then poll/print.
- `tet build <entry> -o <output>` — compile a `.ts`/`.js` entry to a `.tet` artifact. Check `tet_cli/run.rs` for existing build logic.
- `tet init [name]` — verify the inline template in the CLI match block creates a valid project. Fix if broken.

**Mark as unimplemented** (these require external infrastructure):
- `teleport`, `hive-list`, `market-list`, `swarm`, `bridge`, `memory`, `infer`, `pay`, `pull`, `login`

Replace each stub with a printed message: "`tet <command>` is not yet implemented." Return exit code 0 so scripting isn't broken.

### 4. Snapshot and fork via MCP

Add two MCP tools:

- `trytet_snapshot` — `{agent_id: string}` → `{snapshot_id, memory_kb, vfs_files}`
- `trytet_fork` — `{snapshot_id: string}` → `{new_agent_id}`

These call `TetSandbox::snapshot()` and `TetSandbox::fork()`. The sandbox stores snapshots in `WasmtimeSandbox.snapshots: Arc<RwLock<HashMap<String, SnapshotPayload>>>`.

## Constraints

- **No new dependencies** in Cargo.toml
- **No new Rust modules** unless strictly necessary — extend existing ones
- **Do not change** the `TetSandbox` trait signature
- **Do not touch** cartridge WASM crates (`crates/*-evaluator`, `crates/*-cartridge`)
- **Do not modify** any test file except to fix assertions broken by your changes
- **Do not add** AI-speak comments, marketing language, emoji, or "// TODO" comments
- **Either implement or don't mention** — no stubs masquerading as features
- **Write like a senior engineer** — minimal, precise, no fluff

## Quality gates

Every gate must pass zero-exit before work is complete:

```
cargo build --release --bin tet          # compiles
cargo test --release                     # 0 failures
cargo clippy --all-targets -- -D warnings # clean
cargo fmt --check --all                   # clean
./target/release/tet mcp --list-tools     # shows 8 tools
./target/release/tet up                   # starts API server
./target/release/tet ps                   # prints agent list or "No active agents"
./target/release/tet doctor               # prints diagnostics
```

## Files you will modify

| File | What |
|------|------|
| `src/mcp/server.rs` | Add `trytet_execute`, `trytet_snapshot`, `trytet_fork` tools |
| `src/bin/tet.rs` | Fix `tet up` default, fix `ps`/`logs`/`build`, mark stubs |
| `src/bin/tet_cli/` | Handlers for `ps`, `logs`, `build` |
| `src/sandbox/sandbox_wasmtime.rs` | Expose active agent list, wire snapshot/fork for MCP |

## Files you may read but must not modify

| File | Why |
|------|-----|
| `src/engine.rs` | TetSandbox trait — the contract you're building against |
| `src/models.rs` | Request/response types you'll use |
| `src/telemetry.rs` | TelemetryHub — subscribe for `tet logs` |
| `src/cartridge.rs` | CartridgeManager — execution path reference |
| `wit/cartridge.wit` | WIT interface definition |
| `src/server/start.rs` | Server startup — call for `tet up` |
| `src/config.rs` | Config struct — use for defaults |
