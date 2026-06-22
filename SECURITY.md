# Security Policy

## Reporting a Vulnerability

**Do not file a public GitHub issue for security vulnerabilities.**

Send an email to (TODO) with details of the issue. If the project does not yet have a dedicated security contact, open a GitHub Issue with the title prefix `[SECURITY]` and include only enough information to route the report — do not include exploit details in the public issue.

You can expect an acknowledgment within 48 hours and a preliminary assessment within 5 business days. We will work with you to understand the severity and scope before disclosing publicly.

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.2.x   | Active development |
| < 0.2   | Not supported      |

## Security Model

Trytet executes untrusted WebAssembly in a sandbox with two primary isolation mechanisms:

- **Fuel metering**: Every Wasm instruction consumes fuel from a per-invocation budget. Infinite loops and runaway computation trap deterministically in microseconds rather than hanging until a wall-clock timeout fires.
- **Memory limits**: Per-sandbox `StoreLimits` enforce guest memory caps via Wasmtime. Out-of-memory conditions produce a structured, deterministic trap.
- **WASI isolation**: Each invocation gets an isolated temp directory. There is no host filesystem access beyond explicitly preopened paths.
- **Deterministic traps**: All resource exhaustion (fuel, memory, stack) produces a deterministic trap result, never undefined behavior or process termination.

The sandbox engine (Wasmtime) is itself a CVE-monitored dependency. We track upstream advisories and update promptly.

## Disclosure Policy

We follow a 90-day coordinated disclosure timeline:

1. **Report received** — acknowledgment sent within 48 hours.
2. **Investigation** — assessment and fix development (up to 90 days).
3. **Patch release** — a fix is published as a patch release.
4. **Public disclosure** — the vulnerability is disclosed 90 days after initial report, or immediately after a fix is shipped, whichever comes first.

If a fix cannot be developed within 90 days, we will communicate with the reporter to agree on an extension.
