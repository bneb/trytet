# Deploying Trytet

## Pre-built Binaries

Download from GitHub Releases:

```bash
# macOS (Apple Silicon)
curl -sL https://github.com/bneb/trytet/releases/latest/download/tet-darwin-arm64.tar.gz | tar xz
./tet doctor
```

Linux and x86_64 builds coming soon. For now, use the Docker image on non-macOS platforms.

## Docker

```bash
docker pull ghcr.io/bneb/trytet:latest
docker run -p 3000:3000 ghcr.io/bneb/trytet:latest
```

## Compiling from Source

```bash
cargo build --release --bin tet
```

Requires:
- Rust 1.80+
- `cmake`, `clang`, `protobuf-compiler` (for Wasmtime C runtime)
- `cargo-component` (for cartridge builds)

## Environment Variables

| Variable | Purpose | Default |
|---|---|---|
| `RUST_LOG` | Log level | `info` |
| `TRYTET_CARTRIDGE_DIR` | Cartridge search path (colon-separated) | `~/.trytet/cartridges` |
| `CORS_ORIGIN` | Allowed CORS origin | Permissive (all origins) |
| `REGISTRY_PATH` | Wasm artifact storage | `~/.trytet/registry` |
| `BASE_TET_PATH` | Agent state storage | `~/.trytet/base_tets` |
| `REGISTRY_URL` | Remote OCI registry | None (local-only) |
| `REGISTRY_TOKEN` | Registry auth token | None |
