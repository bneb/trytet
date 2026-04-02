# Trytet Deployment Guide (Phase 14)

This repository serves a true "Polyglot Monolith." It contains the high-performance native Rust Wasm engine (`tet-core`), the browser-native Wasm polyfill bridge (`crates/tet-web`), and the frontend Next.js interactive environment (`playground`).

## CI/CD Pipeline (GitHub Actions)
Deployments are fully automated via GitHub Actions (`.github/workflows/deploy.yml`). Pushing to `main` triggers a dual-phase rollout:
1. **Fly.io Backend**: Deploys the monolithic `tet-core` Engine via standard Docker builders.
2. **Cloudflare Pages**: Installs Rust/Wasm toolchains, builds the optimized WebAssembly bridge (`.wasm`), statically exports the Next.js target (`/out`), and routes via Wrangler to the edge.

### Required Repository Secrets:
You must define the following Github Secrets in your repository:
- `FLY_API_TOKEN`: Used to authenticate builder deployments to Fly.io. Grab via `fly auth token`.
- `CLOUDFLARE_API_TOKEN`: Your Cloudflare worker/pages deployment token.
- `CLOUDFLARE_ACCOUNT_ID`: Your Cloudflare Account ID string.

## Manual Deployment Checklists

### 1. Fly.io Monolith (Backend)
If CI fails or you need manual provisioning:
```bash
# Authorize
fly auth login
# Optionally provision volume if region-shifting
fly volumes create trytet_data --region sjc --size 1
# Deploy using standard App Config
fly deploy
```

### 2. Cloudflare Edge (Frontend WebWorker UI)
If needing manual frontend proxy deployments:
```bash
# 1. Compile Optimized Wasm Bundle First
cd crates/tet-web
wasm-pack build --target web --release --out-dir ../../playground/pkg

# 2. Build the Static Next.js Target
cd ../../playground
npm install
npm run build

# 3. Ship to Pages
npx wrangler pages deploy out --project-name trytet-web
```

Important: The Next.js static asset compiler implicitly expects `trytet/playground/pkg/` to contain valid `.wasm` target blobs. Never run the `npm build` command without re-generating `wasm-pack` first if `tet-core` code has mutated.
