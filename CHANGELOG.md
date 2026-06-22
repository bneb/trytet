# Changelog

## v0.2.0 (2026-06-22)

### MCP tools
- Added `trytet_execute`: general-purpose sandbox execution (JS, Python) via MCP
- Added `trytet_snapshot`: capture agent memory/filesystem state via MCP
- Added `trytet_fork`: branch a new agent from a saved snapshot via MCP
- Cartridge tools unchanged: JS evaluator, regex, JMESPath, scraper, structured data

### CLI
- `tet up` (no arguments) now starts the API server on port 3000
- `tet up <file.tet>` boots a .tet agent artifact (existing behavior)
- `tet serve` added as an explicit alias for the API server
- Removed 10 non-functional commands (teleport, hive-list, market-list, swarm,
  bridge, memory, infer, pay, pull, login) — these were HTTP clients calling
  endpoints that don't exist. They can return when the backend does.
- `tet doctor` and `tet ps` verified working

### Code quality
- `CrashReport.error_type` changed from `String` to `CrashType` enum
- `VoucherManager::verify_and_claim` returns typed `VoucherError` instead of `String`
- Blocking `std::fs` calls replaced with `tokio::fs` in async contexts (oracle, model_proxy, shards)
- Duplicated `resolve`/`resolve_with_headers` methods merged in oracle module
- Server startup extracted from `main.rs` into `src/server/start.rs` — reusable by
  both `tet-core` binary and `tet serve` CLI command
- `require_payment` set to `false` until voucher acquisition is implemented

### Configuration
- Centralized config: `Config::from_env()` consolidates all env vars with
  validation, defaults, and secret redaction in `tet doctor` output

### Testing
- 42 tests added across 7 previously-untested cartridge crates
- 3 previously-ignored tests un-ignored with fixes
- All remaining `#[ignore]` tests have explicit reason strings
- Fuzz harness added: WASM module parser and fuel voucher validation targets
- Fuzz CI job runs 60 seconds per target on each push

### CI/CD
- Added `cargo fmt --check` job
- Added `cargo-audit` security scanning job
- Added Docker image build/push to `ghcr.io/bneb/trytet` on release tags
- Release workflow generates SHA256SUMS for all tarballs
- install.sh: SHA-256 verification, fixed HTTP error handling, fixed fallback
  race condition, corrected ARM architecture suffix

### Documentation
- CONTRIBUTING.md, SECURITY.md, CODE_OF_CONDUCT.md added
- Mermaid architecture diagrams added to ARCHITECTURE.md
- SDK READMEs rewritten (TypeScript: 32→387 lines, Python: 56→341 lines)
- Demos: `mcp-walkthrough.sh` (60s hook) and `snapshot-fork.sh` (90s deep dive)
- `docs/VERIFY.md`: assertion-based verification of all demo claims

### Removed
- Playground (Next.js static site) — was a duplicate of the root README
- `crates/tet-web` (wasm-bindgen browser bridge) — no in-tree consumers
- `crates/lean-cartridge`, `crates/replay-debugger` — cargo-new template stubs
- 10 non-functional CLI commands
- Stale `deploy.yml` workflow (duplicate of `ci.yml`)
- Sprint docs moved to `notes/`

---

## v0.1.0 (2026-05-01)

Initial release with:
- Wasm sandbox engine (Wasmtime + fuel metering)
- 5 cartridge tools: JS, Regex, JMESPath, Python, SQL
- MCP server over stdio and HTTP
- TypeScript SDK
- Interactive playground
- Docker image
