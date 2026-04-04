# Deploying Trytet (Phase 27.1)

This repository holds a "Polyglot Monolith" featuring the high-performance native Rust Wasm engine (`tet-core`), the browser-native Wasm polyfill bridge (`crates/tet-web`), and testing submodules.

## Quickstart Binary Compilation
To compile the raw binary for production operations, simply run:
```bash
cargo build --release
```
The compiled Gateway and Engine (`tet`) is found at `./target/release/tet`. It is a fully statically sized, zero-dependency executable. 

## Node Topology & Local Environment 

### Environment Variables
For multi-node cluster settings:
- `TRYTET_API_URL`: Override the default `http://localhost:3000`.
- `TRYTET_HIVE_BIND`: Override default port `2026` for secure P2P Gossip RPC.
- `REGISTRY_PATH`: Wasm storage directory (defaults to `~/.trytet/registry`). 

You can boot secondary testing nodes trivially:
```bash
TRYTET_API_URL=http://localhost:3001 TRYTET_HIVE_BIND=0.0.0.0:2027 cargo run --bin tet-core
```

## Docker and Fly.io
Deployments to centralized cloud platforms are defined by standard App Configs. Pushing to `main` seamlessly rolls out an update to attached Github Actions. 

```bash
# Authorize Fly
fly auth login
# Optionally provision persistent storage
fly volumes create trytet_data --region sjc --size 1
# Deploy using the existing fly.toml
fly deploy
```

> [!TIP]
> Make sure `fly.toml` forwards external ports `80`/`443` internally to `3000` (API & Console) and reserves `2026` for peer-to-peer data ingestion if you run a multi-region deployment.
