# /goal — Context for the Trytet ship sprint

We are shipping the first usable release of Trytet (v0.2.1).
The code is production-quality (0 unsafe, 0 test failures, 0 clippy warnings, auth wired, sandboxing sound).
The distribution is zero — no release binaries, no npm package, no Docker image, no published MCP server listing.

Our users are AI engineers who need crash-proof code execution for LLM-generated code.
They discover tools through MCP directories (Smithery, MCP.so, Glama), GitHub, npm, and word of mouth.
Their first experience must be: install in one command, run `tet mcp`, add to Claude Desktop, execute sandboxed JS.
If they have to read docs, build from source, or debug missing files, we've lost them.

## Success criteria (what "done" looks like)
- macOS user runs `curl -sL https://github.com/bneb/trytet/releases/latest/download/tet-darwin-arm64.tar.gz | tar xz` → `./tet mcp` works
- Claude Desktop config: paste 5 lines → tools appear → execute JS → fuel trap works
- npm: `npm install @trytet/client` installs and imports
- Smithery, MCP.so, Glama: Trytet is listed
- GitHub Release: binaries + cartridges + checksums
- No broken claims in docs — real numbers, real instructions

## Constraints
- macOS arm64 primary target (AI devs overwhelmingly use Macs)
- Ship 3 proven cartridges (JS, Regex, JMESPath) — mark rest experimental
- Don't gate launch on playground demo or Docker image
- Every commit must pass `cargo build`, `cargo test`, `cargo clippy`
- Be honest about latency: 400ms first call (Cranelift), <500µs cached

## Reference files
- SPRINT_SHIP.md — full 6-phase plan with tasks
- RED_TEAM_SHIP.md — blockers (B1-B3), risks (R1-R6), missing items (M1-M5)
- RED_TEAM.md — code security review (all 7 findings resolved)
- SPRINT.md — completed v0.2.0 sprint (Phases 1-6 done)
- BENCHMARKS.md — honest performance numbers

---

# /goal prompt — paste exactly this:

```
Ship Trytet v0.2.1 as the first usable release. The codebase is production-quality (0 unsafe, 0 test failures, 0 clippy warnings, auth wired). The distribution is zero — no binaries, no npm package, no Docker image, no MCP directory listing. Our users are AI engineers who need crash-proof code execution for LLM-generated code. They discover tools through MCP directories, GitHub, npm, and word of mouth. Their first experience must be: install in one command, run tet mcp, add to Claude Desktop, execute sandboxed JS — all under a minute with zero docs. If they have to read docs, build from source, or debug missing files, we have lost them.

Success criteria: macOS user runs curl install.sh → tet mcp works; Claude Desktop config works with 5-line paste; npm install @trytet/client works; Smithery/MCP.so/Glama list Trytet; GitHub Release has binaries+cartridges+checksums; no broken claims in docs — real numbers, real instructions.

Constraints: macOS arm64 primary, 3 proven cartridges (JS/Regex/JMESPath), rest experimental. Don't gate on playground or Docker. Every commit passes build+test+clippy. Honest latency claims: 400ms first call, <500µs cached.

Reference files: SPRINT_SHIP.md (6-phase plan), RED_TEAM_SHIP.md (2 blockers, 6 risks, 5 missing items).
```

---

# /loop prompt — paste exactly this:

```
Work through SPRINT_SHIP.md Phase 1 tasks in dependency order (1.1 install.sh fix, 1.2 Makefile, 1.3 cartridge builds, 1.4 tarball, 1.5 release binary, 1.6 end-to-end test).

For each task:
1. Read the relevant source files to understand current state
2. Implement the change — prefer small, focused edits
3. Verify immediately: cargo build (or cargo check for quick feedback), run the thing if it's a script
4. Commit with a conventional-commit message (feat:/fix:/chore:)
5. Mark the task [x] in SPRINT_SHIP.md
6. Report: what you did, what verification passed, what the next task is

Before starting each task, check if any RED_TEAM_SHIP.md blockers (B1-B3) apply and address them first. If you hit a blocker that needs user action (npm login, PyPI token), stop and ask — don't work around it silently.

Stop and report immediately if cargo build or cargo test fails after a change. Never proceed with a broken build.

After Phase 1 is complete (all tasks [x]), do the same for Phase 2, then 3, etc. Skip Phase 5 (landing page) if the playground build fix takes more than 3 attempts.
```

