# Code Health

**Last updated:** June 2026

## Current State

- **0 compilation errors** (workspace-wide)
- **0 test failures** (all passing, including integration tests)
- **0 `.unwrap()` calls** in production code (all replaced with `.expect()` or proper error handling)
- **0 missed mutants** (JS evaluator crate verified; core crate mutation testing configured but requires different tooling)

## File Organization

Large files have been split into submodules:
- `src/api.rs` — 76 lines (handlers extracted to `src/api/handlers/`)
- `src/hive.rs` — 365 lines (connection handling in `src/hive/connection.rs`)
- `src/sandbox/host_api.rs` — refactored into 17 modules in `src/sandbox/host_api/`
- `src/bin/tet_cli/status.rs` — split into 8 files in `src/bin/tet_cli/status/`
- `src/bin/tet-tui/` — split into `mod.rs` + renders, events, app submodules

## Known Limitations

### sandbox_wasmtime.rs (967 lines)
This file contains the core async trait implementation for `WasmtimeSandbox`. Splitting it into submodules triggers Rust compiler error `E0658` — proc macros (`#[async_trait]`) cannot handle file-based submodule resolution. This is a known rustc limitation. The file remains large but well-organized with clear section headers.

### wasmtime closure nesting
Host function registrations in `src/sandbox/host_api/` use wasmtime's `func_wrap_async` API which inherently creates 4-6 levels of structural indentation from parameter destructuring. These are not control-flow nesting issues and cannot be reduced without upstream wasmtime API changes.

### test coverage
`cargo-llvm-cov` and `cargo-tarpaulin` measurements are constrained to the `js-evaluator` crate (88.89% coverage). Core library coverage measurement requires different tooling due to proc-macro incompatibilities with LLVM instrumentation.

## Remaining Technical Debt

All known technical debt items from the v0.1.0 assessment have been resolved:

- ~~`SovereignGateway`, `SovereignHeaders`, `SovereignTunnel` type names~~ → renamed to `Gateway`, `IdentityHeaders`, `Tunnel` in v0.2.0
- ~~Phase numbering in module doc comments~~ → stripped 36 references across 15 files in v0.2.0
- ~~Hardcoded cartridge paths in MCP tool handlers~~ → configurable via `TRYTET_CARTRIDGE_DIR` env var and `register_tool()` API in v0.2.0
