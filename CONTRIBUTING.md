# Contributing

## Development Environment

### Prerequisites

- **Rust 1.80+** — check with `rustc --version`. Install via [rustup](https://rustup.rs) if needed.
- **cmake** — required by Wasmtime's Cranelift backend. Install via `brew install cmake` (macOS) or your system package manager.
- **clang** — used to compile native extensions. macOS: `xcode-select --install`. Linux: `apt install clang` (or equivalent).
- **protobuf** — required for the P2P mesh protocol. Install via `brew install protobuf` (macOS) or your system package manager.
- **cargo-component** — used to build cartridge `.wasm` files. Install via `cargo install cargo-component`.
- **wasm-tools** — used to inspect and validate Wasm components. Install via `cargo install wasm-tools`.

### Setup

```bash
git clone https://github.com/bneb/trytet.git
cd trytet
cargo build --bin tet
```

To build the Wasm cartridge examples:

```bash
make cartridges
```

## Running Tests

```bash
# Run the full test suite
make test

# Or directly
cargo test -- --test-threads=4
```

## Linting

```bash
# Clippy
make clippy
# Or
cargo clippy --all-targets

# Formatting
cargo fmt --check
```

## Commit Conventions

This project uses [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <short summary>
  │       │             │
  │       │             └─ Summary in present tense, lowercase, no period
  │       └─────────────── Optional scope (sandbox, mcp, cli, mesh, cartridge, etc.)
  └─────────────────────── Type: fix, feat, chore, docs, refactor, test, ci
```

Examples:

```
fix(sandbox): reset fuel counter on store reuse
feat(mcp): add --list-registries subcommand
docs: add SECURITY.md and CONTRIBUTING.md
refactor(cartridge): split loader from compiler
```

The commit body (blank line after summary) explains the *why*, not the *what*.

## Pull Request Process

1. Create a feature branch from `main`:
   ```bash
   git checkout -b feat/my-change
   ```
2. Make your changes, keeping commits small and well-scoped.
3. Run the full test suite and lint checks:
   ```bash
   cargo test -- --test-threads=4
   cargo clippy --all-targets
   cargo fmt --check
   ```
4. Push your branch and open a pull request against `main`.
5. The title should follow conventional commit format (e.g., "fix(sandbox): ...").
6. A reviewer will respond within a few business days. Address feedback with additional commits — the branch will be squashed on merge.

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for the system design and key design decisions.

## Code of Conduct

This project is governed by the [Contributor Covenant](CODE_OF_CONDUCT.md). All participants are expected to uphold its standards.
