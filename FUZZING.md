# Fuzzing Trytet with cargo-fuzz

This project uses [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) and
libFuzzer for coverage-guided fuzzing of security-critical components.

## Prerequisites

- Rust **nightly** toolchain (libFuzzer requires `-Zsanitizer=address`)
- `cargo-fuzz` — installed automatically when you `cargo +nightly fuzz ...`

Install nightly (if not already installed):

```bash
rustup toolchain install nightly
```

## Available targets

| Target          | What it tests                                      |
|-----------------|----------------------------------------------------|
| `wasm_parse`    | Arbitrary bytes -> `wasmtime::Module::new`         |
| `fuel_voucher`  | Arbitrary bytes -> `FuelVoucher` parse & validate  |

## Running a fuzzer

```bash
# List available targets
cargo +nightly fuzz list

# Build (check compilation only)
cargo +nightly fuzz build

# Run a target indefinitely (Ctrl-C to stop)
cargo +nightly fuzz run wasm_parse

# Run with a timeout (60 seconds)
cargo +nightly fuzz run wasm_parse -- -max_total_time=60

# Run with a custom corpus directory (preserves interesting inputs across runs)
cargo +nightly fuzz run wasm_parse fuzz/corpus/wasm_parse -- -max_total_time=60
```

> **Note:** The first build will compile `wasmtime` from source, which takes
> several minutes. Subsequent runs reuse the build cache.

## Interpreting results

- **CRASH** — the fuzzer found input that causes a panic, assertion failure,
  or undefined behavior. The crashing input is written to
  `fuzz/artifacts/<target>/`. Minimize it with:
  ```bash
  cargo +nightly fuzz tmin wasm_parse fuzz/artifacts/wasm_parse/crash-<hash>
  ```

- **No crashes after N iterations** — the target is handling the input space
  without panicking.

## CI

The CI workflow (`.github/workflows/ci.yml`) runs each fuzzer for 60 seconds on
every push/PR to catch regressions early. This is intentionally short — a full
fuzz campaign should be run locally or as a scheduled nightly job.

## Adding a new target

```bash
cargo +nightly fuzz add <target_name>
```

Then edit `fuzz/fuzz_targets/<target_name>.rs` with your fuzz logic.
