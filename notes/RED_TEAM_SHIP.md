# Red Team Review: SPRINT_SHIP.md

## Blockers (can't proceed without resolving)

### B1. Cartridge crate-type vs cargo-component
**Finding:** Our Phase 2 fix changed cartridge `crate-type` from `["cdylib"]` to `["rlib"]` with `[target.wasm32-wasip2.lib] crate-type = ["cdylib"]`. `cargo component build` targets `wasm32-wasip1` (not wasip2), so the cdylib override is never applied. The cartridge compiles but doesn't produce a component `.wasm`.

**Fix:** Change to `crate-type = ["cdylib", "rlib"]` (both). Native tests get rlib, cargo-component gets cdylib. Tested pattern — `tet-web` and `trytet-guest` already use it.

### B2. npm not logged in
**Finding:** `npm whoami` fails. Can't publish `@trytet/client` to npm without credentials.
**Fix:** Run `npm login` (needs npmjs.com account + 2FA). Add npm token to GitHub Secrets for CI publishing.

### B3. PyPI not configured
**Finding:** `twine` not installed. Can't publish `trytet-client` to PyPI.
**Fix:** `pip install twine`, configure `~/.pypirc` with PyPI token. Add to CI.

## Risks (could derail a phase)

### R1. Install flow assumes GitHub Releases with binaries
**Finding:** The install.sh downloads from GitHub Releases. If the release tarball structure is wrong (missing cartridges, wrong paths), the install silently fails. No `tet doctor` command exists yet to diagnose.
**Mitigation:** Add `tet doctor` BEFORE building the release. Test the full install flow on a clean environment before publishing.

### R2. Playground build is broken (pre-existing)
**Finding:** `npm run build` in playground/ fails with missing `../../pkg/tet_web.js`. The import path is wrong (should be `../../../pkg/tet_web.js`). This is NOT a regression from our sprint — it's pre-existing.
**Mitigation:** Fix is a one-line path change. But `wasm-pack build` in `crates/tet-web` must succeed first.

### R3. "60 second install" claim is aggressive
**Finding:** First `tet mcp` invocation compiles cartridges with Cranelift (~400ms per cartridge, one-time). Install script downloads ~50MB+ of binaries. Network speed varies.
**Mitigation:** Change claim to "installs in one command, works in under a minute" — more honest.

### R4. Docker image depends on Linux binary
**Finding:** The Dockerfile uses `cargo build --release` inside a Linux container. This works but takes 20-40 minutes in CI. A faster approach is to build the binary natively (GitHub Actions macOS runner) and copy it into the Docker image.
**Mitigation:** Build binary in CI, use multi-stage Docker COPY from GitHub Release artifact.

### R5. MCP directories have submission queues
**Finding:** Smithery manually reviews submissions. Glama auto-indexes from GitHub. MCP.so is high volume. Submitting to all 3 in parallel is fine, but listing may take days.
**Mitigation:** Submit early (Phase 4 runs in parallel with Phase 1-3). Don't gate launch on directory listings.

### R6. SDK packages need scope/organization
**Finding:** `@trytet/client` requires the `@trytet` npm org to exist. `trytet-client` on PyPI needs the name to be available.
**Mitigation:** Verify npm org and PyPI name availability before Phase 3. Fall back to `trytet-client` (no scope) on npm.

## Missing from the plan

### M1. No telemetry or usage tracking
**Finding:** If we ship and nobody tells us, we won't know. The auth module tracks API key usage but doesn't report it anywhere.
**Add:** Opt-in telemetry ping on first `tet mcp` startup ("trytet would like to send anonymous usage stats"). Or at minimum, a `/v1/swarm/metrics` endpoint that reports cartridge invocations.

### M2. No error telemetry or crash reporting
**Finding:** If the MCP server crashes or a cartridge fails to load, there's no feedback loop to the developers.
**Add:** Write MCP errors to `~/.trytet/logs/` by default. Add `tet doctor --verbose` that reads and reports them.

### M3. No Windows or Linux ARM testing
**Finding:** Primary AI dev audience uses macOS. But many production deployments are Linux. No mention of `musl` static linking for Linux portability.
**Add:** Phase 1 should include a `x86_64-unknown-linux-musl` build target for portable Linux binaries.

### M4. No upgrade path
**Finding:** Once someone installs v0.2.1, how do they get v0.3.0? No `tet update` command, no version check.
**Add:** `tet --version` already works. Add `tet update` that checks GitHub Releases for newer versions and downloads them.

### M5. Cartridge build is fragile
**Finding:** `boa_engine` 0.19.1 compiled successfully today but is a heavy dependency (~150 transitive crates). The Python evaluator cartridge (`rustpython-vm`) has known issues. The SAT cartridge is untested.
**Decision needed:** Ship 3 proven cartridges (JS, Regex, JMESPath) as defaults. Mark Python, SQL, Scraper, SAT as experimental. Don't let unproven cartridges block the release.

## Things that are fine

- `gh` CLI is authenticated — GitHub Releases creation will work
- `cargo-component` 0.21.1 installed and functional
- `wasm-pack` 0.15.0 installed — playground WASM build possible
- Docker daemon running — image build possible
- Code quality is excellent (0 unsafe, 0 test failures, 0 clippy warnings)
- Auth is wired — API endpoints are protected
- MCP server protocol is correct — compatible with Claude Desktop

## Revised risk-adjusted plan

Cut from initial plan:
- Linux binaries → nice-to-have, not launch blocker
- All 7 cartridges → ship 3 proven ones, mark rest experimental
- Playground interactive demo → fix build, but don't gate launch on it
- "60 second" claim → "one command, works immediately"

Add to plan:
- `tet doctor` command (diagnoses install health)
- `tet update` command (self-update from GitHub Releases)
- `cargo component build --all` in Makefile (build all cartridges at once)
- Opt-in telemetry ping (know if anyone uses it)
- `~/.trytet/logs/` error logging (debug user issues)
