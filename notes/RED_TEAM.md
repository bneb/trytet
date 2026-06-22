# Red Team Security Review — v0.2.0

**Date:** 2026-06-20
**Scope:** Full source tree (`src/`, `crates/`, API surface)

---

## Findings Summary

| # | Severity | Title | Status |
|---|---|---|---|
| SEC-01 | High | Unauthenticated API endpoints — auth module exists but not wired in | ✅ Resolved |
| SEC-02 | Medium | Node.js `vm` sandbox escape risk in `/v1/benchmark/node` | ✅ Resolved |
| SEC-03 | Medium | `PathJailer` fallback to CWD on root-path parent lookup | ✅ Resolved |
| SEC-04 | Low | `CorsLayer::permissive()` on all routes | ✅ Resolved |
| SEC-05 | Low | Panic via `unwrap()` on serde in MCP hot path | ✅ Resolved |
| SEC-06 | Low | No dependency vulnerability scanning configured | ✅ Resolved |
| SEC-07 | Info | `local_secret` slice panic if < 32 bytes in Tunnel init | ✅ Resolved |

---

## SEC-01 — Unauthenticated API Endpoints (High)

**File:** `src/api.rs`, `src/auth.rs`

The `KeyStore` auth module in `src/auth.rs` implements API key generation, SHA-256 validation, usage tracking, and revocation — but it is **never wired into any API route**. All 15 endpoints in `src/api.rs` are unauthenticated.

The `AppState` struct in `src/api.rs` has no `KeyStore` field. No middleware validates API keys. An attacker can:
- Invoke cartridges with arbitrary fuel/memory budgets
- Register ingress proxy routes
- Push/pull from the registry
- Replay/recover agent state

**Fix:** Add `KeyStore` to `AppState`, create an `ApiKeyLayer` middleware, apply to all `/v1/*` routes. The MCP server (stdio) can remain unauthenticated since it runs locally.

---

## SEC-02 — Node.js `vm` Sandbox Escape Risk (Medium)

**File:** `src/api/handlers/all.rs:89-127`

The `/v1/benchmark/node` endpoint executes user-provided JavaScript via `node -e` with `vm.runInNewContext`. While input sanitization is sound (typed `u64` timeout, JSON-escaped snippet), the Node.js `vm` module has a history of sandbox escape CVEs:
- CVE-2023-30589, CVE-2023-32314, CVE-2024-21892

If an attacker chains this with the missing auth (SEC-01), they could attempt known or zero-day VM escapes to execute arbitrary code on the host.

**Fix:** Either (a) remove this endpoint (node benchmarks can run via Wasm js-evaluator), (b) gate it behind auth + a feature flag, or (c) switch to `isolated-vm` npm package which provides true V8 isolate isolation.

---

## SEC-03 — `PathJailer` CWD Fallback (Medium)

**File:** `src/sandbox/security.rs:38`

```rust
let parent = full_path.parent().unwrap_or(Path::new(""));
```

When `full_path` is a root-level path (e.g., `//etc/passwd` after joining), `.parent()` returns `None` on Unix. The fallback `Path::new("")` resolves to the current working directory, which then passes the `starts_with(canon_root)` check. An attacker controlling the guest path could potentially bypass the jail.

The same pattern appears on lines 35, 40, 47 with `canonicalize().unwrap_or()` — if canonicalization fails (symlink loop, permission error), the unchecked path is used.

**Fix:** Return `Err(SecurityError::PathTraversalAttempt)` when `.parent()` returns `None`, and log a warning when `canonicalize()` fails before using the fallback path.

---

## SEC-04 — Permissive CORS (Low, Accepted)

**File:** `src/api.rs:73`

`CorsLayer::permissive()` allows all origins, methods, and headers. This is acceptable for a local development tool (MCP server runs on localhost) but should be configurable for any production deployment.

**Fix:** Read `CORS_ORIGIN` from environment, default to permissive for local dev. Document in DEPLOYMENT.md.

---

## SEC-05 — `unwrap()` Panics in MCP Hot Path (Low)

**File:** `src/mcp/server.rs:85,97,189,198,228,252,254`

Seven `.unwrap()` calls on `RwLock` and `serde_json::to_value()`. While these should not fail under normal operation:
- `RwLock::write().unwrap()` panics if the lock is poisoned (a previous panic while holding the lock)
- `serde_json::to_value(make_response(...)).unwrap()` should never fail, but a future refactor could change that

**Fix:** Replace with `.expect("message")` to document why each is infallible, or propagate errors where possible.

---

## SEC-06 — No Dependency Scanning (Low)

**File:** CI/CD configuration

`cargo-audit` is not installed or configured. The project has large dependencies (wasmtime, boa_engine, reqwest, rustls) that have had security advisories in the past.

**Fix:** Add `cargo audit` to CI pipeline. Run `cargo install cargo-audit && cargo audit` in CI.

---

## SEC-07 — Tunnel Key Slice Panic (Info)

**File:** `src/network/tunnel.rs:39,52`

```rust
key.copy_from_slice(&local_secret[..32]);
```

If `local_secret.len() < 32`, this panics. The caller is the Hive mesh which passes internally-generated keys, so the risk is low, but a corrupted key store could cause a crash.

**Fix:** Return `TunnelError` instead of panicking:
```rust
let bytes = local_secret.get(..32).ok_or(TunnelError::InvalidKey)?;
key.copy_from_slice(bytes);
```

---

## Items Verified as Safe

- **Wasm cartridge sandbox**: All 5 cartridges run inside fuel-metered Wasmtime with memory limits. Code generation (LLM output) can't escape the sandbox.
- **MCP over stdio**: Local-only by design, newline-delimited JSON with 10MB frame limit.
- **No hardcoded secrets**: No passwords, API keys, or tokens in source.
- **No `unsafe` blocks**: Zero unsafe Rust in the codebase.
- **Scraper cartridge**: Takes pre-fetched HTML + CSS selector, does NOT make HTTP requests (despite tool description saying "Fetch a URL").
- **`req.snippet` sanitization**: Properly JSON-escaped before embedding in node script.
- **`req.timeout_ms`**: Typed `u64`, can't inject arbitrary characters.
- **Path traversal gate**: `PathJailer` catches `..` and null bytes before canonicalization.

---

## Recommended Sprint Tasks

Add to SPRINT.md, Phase 6 (Security Hardening):

- [ ] **6.1** Wire `KeyStore` auth into API routes — add to `AppState`, create middleware, apply to `/v1/*`
- [ ] **6.2** Replace `Node::vm` benchmark endpoint with Wasm-based js-evaluator execution (or gate behind feature flag)
- [ ] **6.3** Fix `PathJailer` parent-fallback bug — return error instead of CWD fallback
- [ ] **6.4** Replace `.unwrap()` calls in MCP server with `.expect()` or proper error handling
- [ ] **6.5** Add `cargo audit` to CI pipeline
- [ ] **6.6** Make CORS configurable via `CORS_ORIGIN` env var
- [ ] **6.7** Fix `Tunnel::init_initiator` key slice panic — return error on short key
