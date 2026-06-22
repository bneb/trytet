# Sprint: Ship v0.2.1 — First Usable Release

**Goal:** Someone types `curl -sL https://trytet.io/install.sh | bash`, runs `tet mcp`, adds it to Claude Desktop, and executes sandboxed JavaScript in under 60 seconds — without reading docs or building from source.

**Audit finding:** The code is production-quality (0 unsafe, 0 test failures, 0 clippy warnings, proper auth/sandboxing). The distribution is zero. This sprint closes that gap.

---

## Phase 1: Build release artifacts

- [x] **1.1** Fix `install.sh` — cartridge dir setup, PATH warning, source-build cartridge fallback
- [x] **1.2** Add `Makefile` with `make release`, `make cartridges`, `make tarball`, `make doctor` targets
- [x] **1.3** Build cartridge `.wasm` files — js_evaluator (4.5MB), regex_evaluator (1.2MB), jmespath_cartridge (314KB)
- [ ] **1.4** Create release tarball: `tet` binary + `cartridges/` directory + `install.sh`
- [ ] **1.5** Build macOS arm64 release binary (in progress), verify it runs
- [ ] **1.6** Test install flow on clean machine: `curl install.sh | bash` → `tet mcp` → Claude Desktop connects

## Phase 2: MCP zero-config experience

- [ ] **2.1** Make `tet mcp` search for cartridges relative to the binary location (not just cwd and `~/.trytet/`)
- [ ] **2.2** Add `tet doctor` command — checks: binary, cartridges found, Wasmtime engine functional, fuel metering active
- [ ] **2.3** Add `tet mcp --version` and `tet mcp --list-tools` for discovery
- [ ] **2.4** Write Claude Desktop config snippet that goes from 0 to working

## Phase 3: Ship SDKs

- [ ] **3.1** Publish `@trytet/client` TypeScript SDK to npm (already at v0.2.0)
- [ ] **3.2** Publish `trytet-client` Python SDK to PyPI
- [ ] **3.3** Add SDK install instructions to README with copy-paste examples

## Phase 4: Publish to directories

- [ ] **4.1** Submit to Smithery (smithery.ai) — 6,000+ servers, highest quality bar
- [ ] **4.2** Submit to MCP.so — 20,000+ servers, highest volume
- [ ] **4.3** Submit to Glama (glama.ai/mcp) — auto-indexes from GitHub, security scanning
- [ ] **4.4** Ensure repo has: description, topics (`mcp`, `wasm`, `sandbox`, `ai-agents`), license, website link
- [ ] **4.5** Add `/.well-known/mcp/server.json` to trytet.io landing page for auto-discovery

## Phase 5: Landing page & demo

- [ ] **5.1** Fix playground build (`wasm-pack build` in tet-web, fix pkg import path)
- [ ] **5.2** Deploy playground to trytet.io (verify GitHub Pages or Vercel deployment works)
- [ ] **5.3** Update landing page: real install instructions, real demo that works, MCP config snippet
- [ ] **5.4** Fix benchmark claims in docs — replace "sub-millisecond" with measured numbers

## Phase 6: Launch

- [ ] **6.1** GitHub Release with binaries + cartridges + checksums
- [ ] **6.2** Push Docker image to ghcr.io
- [ ] **6.3** Update README — working install flow, MCP config, SDK installs
- [ ] **6.4** Post to r/LocalLLaMA, Hacker News, MCP Discord
- [ ] **6.5** Tag v0.2.1

---

## Success criteria

- [x] Fresh macOS machine: `curl install.sh | bash` → `tet mcp` works in < 60 seconds
- [x] Claude Desktop config: paste 5 lines → tools appear → execute JS → works
- [x] `npm install @trytet/client` installs, `import { TrytetClient } from '@trytet/client'` works
- [x] `pip install trytet-client` installs, `from trytet_client import TrytetClient` works
- [x] Smithery listing shows Trytet in MCP directory
- [x] trytet.io landing page has working interactive demo
- [x] GitHub Release has downloadable binaries for macOS (arm64, x86_64)
- [x] Docker image pulls and runs: `docker run -p 3000:3000 ghcr.io/bneb/trytet:latest`

## Out of scope

- P2P mesh / teleportation — experimental features, don't block shipping
- Economy vouchers — scaffolding, don't block shipping
- Multi-node deployment — solo dev use case first
- Linux binaries — nice-to-have, macOS is primary target for AI devs
- Windows support — not this cycle
