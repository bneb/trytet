# Trytet Code Health Audit

**Date:** May 9, 2026
**Scope:** `src/`, `crates/`, `sdk/`, `playground/`

## Executive Summary

The Trytet codebase demonstrates high technical sophistication but suffers from significant architectural "gravity" in the core execution and CLI layers. As the project has scaled from Phase 1 to Phase 36, several key components have become monolithic, leading to extreme function lengths and deep nesting that threaten long-term maintainability.

---

## 1. Primary Health Violations

### 1.1 The "God Function" Antipattern
*   **Location:** `src/sandbox/sandbox_wasmtime.rs` -> `execute_inner`
*   **Details:** This function is approximately **1,400 lines long** (lines 247-1600+).
*   **Impact:** It violates the Single Responsibility Principle. It handles VFS setup, telemetry broadcasting, Wasmtime configuration, and defines dozens of complex asynchronous host functions (RPC, forking, economy) within its body.
*   **Recommendation:** Refactor host functions into a separate `HostApi` trait or module. Move VFS and telemetry setup into dedicated lifecycle hooks.

### 1.2 CLI Command Monolith
*   **Location:** `src/bin/tet.rs` -> `main`
*   **Details:** The `main` function (~500 lines) contains a massive match statement for every CLI subcommand.
*   **Impact:** Adding new commands requires modifying a single massive file, increasing merge conflict risk and cognitive load.
*   **Recommendation:** Use a command pattern where each subcommand is its own module or struct implementing a `Command` trait.

### 1.3 Extreme Indentation (Nesting Depth)
*   **Location:** `src/sandbox/sandbox_wasmtime.rs`
*   **Details:** Over **800 lines** have indentation levels exceeding 5 levels (20+ spaces). This is primarily due to nested `async` blocks and `match` statements within the host function definitions.
*   **Impact:** Makes the code extremely difficult to read and debug.
*   **Recommendation:** Use helper functions and the `?` operator to flatten logic. Convert complex closures into named functions.

### 1.4 Enum Bloat
*   **Location:** `src/hive.rs` -> `HiveCommand`
*   **Details:** This enum has over **25 variants**, handling everything from P2P joins to market bids and registry queries.
*   **Impact:** Any change to the Hive protocol requires modifying this central enum and updating massive match statements across the mesh worker.
*   **Recommendation:** Group related commands into sub-enums (e.g., `HiveMarketCommand`, `HiveRegistryCommand`) to improve modularity.

---

## 2. Stability & Type Safety Issues

### 2.1 Fragile Error Handling (`.unwrap()`)
*   **Locations:** `src/hive.rs` (8), `src/memory.rs` (6), `src/bin/tet.rs` (6).
*   **Details:** While not pervasive, these unwraps are in critical paths (P2P networking and VFS).
*   **Impact:** Unexpected network latency or filesystem permissions could cause the entire node to panic.
*   **Recommendation:** Replace with `.ok_or()` or `context()` to provide meaningful error messages.

### 2.2 TypeScript Type Leakage (`any`)
*   **Locations:** `playground/src/app/web-demo/page.tsx` (8), `playground/src/app/benchmark/page.tsx` (5).
*   **Details:** Frequent use of `any` for API responses and event handlers.
*   **Impact:** Negates the benefits of TypeScript, leading to potential runtime "Cannot read property of undefined" errors.
*   **Recommendation:** Use the `@trytet/client` SDK interfaces strictly. Define local interfaces for benchmark results.

---

## 3. Performance & Resource Health

### 3.1 Synchronous IO in Async Contexts
*   **Location:** `src/sandbox/sandbox_wasmtime.rs`
*   **Details:** Some `fs::write` calls are performed directly inside the `async` function without `tokio::fs` or `spawn_blocking`.
*   **Impact:** Can stall the Tokio executor thread, impacting throughput under high load.
*   **Recommendation:** Strictly use `tokio::fs` for all VFS operations.

---

## 4. Prioritized Refactoring Roadmap

1.  **High:** Decompose `execute_inner` in `sandbox_wasmtime.rs`. This is the single biggest technical debt item.
2.  **Medium:** Modularize the `tet` CLI into subcommand files.
3.  **Medium:** Implement strict linting rules for `any` and `unwrap()` to prevent further regression.
4.  **Low:** Consolidate `HiveCommand` variants into functional groups.
