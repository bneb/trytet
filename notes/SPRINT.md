# Sprint Plan — June 2026

**Sprint goal:** Land the pending refactoring (module splits, workspace config, docs, SDK, JSON Schema cartridge), fix the native build, and get the repo to a clean, shippable state.

---

## Current State Assessment

- **Branch:** `main`
- **Last tag:** `v0.1.0`
- **Working tree:** 76 files modified (+1,917 / −4,744), 3 files deleted (split into submodules), ~15 new files untracked
- **Build:** Broken — `js-evaluator` (Wasm cdylib) fails to link natively on arm64 macOS. The root crate (`tet`) builds fine; only the Wasm cartridge crates fail when targeted natively.
- **Tests:** All passing on committed code; uncommitted tests untested
- **Code health:** 0 warnings in core, 0 `.unwrap()` calls, mutation testing configured

## What's in the Diff

The pending changes represent a cohesive refactoring sprint:

1. **Module splits** — 3 monolithic files (~2,400 lines total) split into submodules:
   - `src/bin/tet-tui.rs` → `src/bin/tet-tui/`
   - `src/bin/tet_cli/status.rs` → `src/bin/tet_cli/status/`
   - `src/sandbox/host_api.rs` → `src/sandbox/host_api/` (17 modules)

2. **Workspace configuration** — Cartridge crates added to workspace, build profiles moved to root, `reqwest` gained `blocking` feature.

3. **Documentation refresh** — All 8 markdown docs updated to reflect current architecture, benchmark numbers, CLI surface, and deployment steps.

4. **SDK expansion** — TypeScript SDK grew ~400 lines (new API surface), Python SDK added.

5. **New cartridge** — `json-schema-cartridge` (untracked, needs staging).

6. **New tests** — Phase 39–40 test files (MCP, cartridge, sandbox, mutation gate, comprehensive coverage).

7. **Playground UI** — How-to page rewritten, onboarding modal updated.

8. **Landing page** — HTML significantly streamlined.

---

## Sprint Tasks

### Phase 1: Stabilize (commit the pending work)
- [ ] **1.1** Add `.gitignore` entries for `*.profraw`, `mutants.out*`, `dist/`
- [ ] **1.2** Stage and commit the module splits + workspace config
- [ ] **1.3** Stage and commit the documentation refresh
- [ ] **1.4** Stage and commit SDK changes + playground updates
- [ ] **1.5** Stage and commit the new `json-schema-cartridge` crate
- [ ] **1.6** Stage and commit new tests (phase 39–40)
- [ ] **1.7** Stage and commit landing page updates

### Phase 2: Fix the build
- [x] **2.1** Add `[target.wasm32-wasip2]` conditional crate-type in cartridge Cargo.tomls
- [x] **2.2** Or: add a `Makefile` / `justfile` target for `cargo build --target wasm32-wasip2`
- [x] **2.3** Verify `cargo build` (native) succeeds for the root crate
- [x] **2.4** Verify `cargo test` passes for all committed code

### Phase 3: Polish (roadmap "Now" items)
- [x] **3.1** Run full test suite, fix any regressions (all suites pass, 0 failures)
- [x] **3.2** Verify MCP server boots and serves all 5 cartridge tools (verified via phase37_1 + phase39_1 tests)
- [x] **3.3** Verify playground builds and deploys (`npm run build` in `playground/`) — pre-existing `tet_web.js` import path issue noted
- [x] **3.4** Run `cargo clippy` on the workspace, fix warnings — 8 auto-fixes applied
- [ ] **3.5** Run mutation testing on `js-evaluator` crate, verify 0 missed mutants

### Phase 4: Ship (roadmap "Next" items)
- [x] **4.1** Tag `v0.2.0` with release notes — tagged, CHANGELOG.md written
- [ ] **4.2** Build x86_64 binaries for macOS and Linux — needs CI cross-compilation (arm64 binary compiles)
- [ ] **4.3** Publish TypeScript SDK to npm — ready, needs `npm login` + `npm publish`
- [ ] **4.4** Publish Python SDK to PyPI — ready, needs `twine` + PyPI token
- [x] **4.5** Docker image ready — Dockerfile committed

### Phase 5: Technical debt (from CODE_HEALTH.md)
- [x] **5.1** Rename `Sovereign*` types throughout codebase — 39 references, 20 files, file rename
- [x] **5.2** Strip phase-number doc comments — 36 references across 15 files
- [x] **5.3** Make cartridge paths configurable — `TRYTET_CARTRIDGE_DIR` env var, `register_tool()` API

### Phase 6: Security Hardening (from RED_TEAM.md)

- [x] **6.1** Wire `KeyStore` auth into API routes — middleware on all `/v1/*`, boot key on first run
- [x] **6.2** Replace Node.js `vm` benchmark with Wasm sandbox — `/v1/benchmark/sandbox`
- [x] **6.3** Fix `PathJailer` parent-fallback bug — returns error instead of CWD fallback
- [x] **6.4** Replace `.unwrap()` calls in MCP server — all now `.expect()` with messages
- [x] **6.5** Add CI pipeline with `cargo-audit` dependency scanning (`.github/workflows/ci.yml`)
- [x] **6.6** Make CORS configurable via `CORS_ORIGIN` env var — defaults permissive for dev
- [x] **6.7** Fix `Tunnel` key slice panic — returns `TunnelError::InvalidKey` on short key

---

## Success Criteria

- `cargo build` succeeds on macOS (arm64) and Linux (x86_64)
- `cargo test` passes with 0 failures
- `cargo clippy` passes with 0 warnings
- MCP server boots and exposes 5+ cartridge tools
- Playground builds and serves without errors
- All documentation is internally consistent (no stale paths, no conflicting claims)
- Working tree is clean
- Git tag `v0.2.0` pushed

## Loop Prompt

For incremental progress, use this loop prompt:

```
Work through the SPRINT.md tasks in order. Before each task, read the relevant files and verify the current state matches expectations. After each task, run the relevant verification step (build, test, lint, or manual check). Commit each completed task group separately with a conventional-commit message. Stop and report when you hit a blocker.
```

To run it:

```
/loop "Work through the SPRINT.md tasks in order. Before each task, read the relevant files and verify the current state matches expectations. After each task, run the relevant verification step (build, test, lint, or manual check). Commit each completed task group separately with a conventional-commit message. Stop and report when you hit a blocker."
```
