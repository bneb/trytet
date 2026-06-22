export const meta = {
  name: 'phase4-cicd',
  description: 'rustfmt check, cargo-audit, Docker publish, install.sh checksums, consolidate config',
  phases: [
    { title: 'Fix', detail: 'CI/CD hardening and config consolidation' },
    { title: 'Red Team', detail: 'Verify CI configs and config system' },
  ],
}

phase('Fix')
const results = await pipeline([
  {
    key: 'ci-fmt-and-audit',
    prompt: `Harden the CI workflow at /Users/kevin/projects/trytet/.github/workflows/ci.yml.

Read the current ci.yml. Add these jobs (matching existing style and naming):

1. "fmt" job: runs "cargo fmt --check --all" on ubuntu-22.04
2. "security-audit" job: installs cargo-audit via "cargo install cargo-audit", runs "cargo audit", fails on critical vulnerabilities

Keep existing jobs unchanged. Match the existing workflow's indentation, runner, and Rust toolchain approach.`
  },
  {
    key: 'docker-publish',
    prompt: `Add Docker image publishing to /Users/kevin/projects/trytet/.github/workflows/release.yml.

The README references "docker pull ghcr.io/bneb/trytet:latest" but no CI publishes Docker images.

Read release.yml and the Dockerfile. Add a job that:
1. Builds and pushes the Docker image to ghcr.io/bneb/trytet
2. Tags with :latest and the git tag version
3. Only runs on v* tag pushes
4. Uses docker/build-push-action or docker metadata actions

Add packages: write permission to the workflow. Match existing style.`
  },
  {
    key: 'install-sh-security',
    prompt: `Fix security in /Users/kevin/projects/trytet/install.sh.

Read the file. Critical issues:
1. No checksum verification — add SHA-256 verification against a SHA256SUMS file from the release
2. No -f flag on curl (doesn't fail on HTTP errors) — add it
3. Race condition in source-compile fallback — the temp dir check after git clone

Also update release.yml to generate SHA256SUMS files for all release artifacts.

IMPORTANT: Read install.sh fully first. Maintain "set -euo pipefail". Keep changes minimal and correct. Validate bash syntax.`
  },
  {
    key: 'consolidate-config',
    prompt: `Create a centralized configuration system for /Users/kevin/projects/trytet.

Configuration is scattered across modules as individual env vars. Create src/config.rs:

1. A Config struct consolidating all env vars: REGISTRY_PATH, BASE_TET_PATH, DATABASE_URL, REGISTRY_URL, REGISTRY_TOKEN, CORS_ORIGIN, FLY_REGION, TRYTET_CARTRIDGE_DIR
2. Config::from_env() with validation that fails fast with clear messages
3. Sensible defaults where possible
4. Update main.rs to use Config
5. Add a way to print config (redacting secrets) — used by "tet doctor"

IMPORTANT:
- Read the current env var usage first (grep std::env::var in src/).
- Don't change env var names — they're the public interface.
- Use a simple approach (manual FromEnv, not a heavy config crate).
- Redact REGISTRY_TOKEN and DATABASE_URL passwords.
- Run "cargo check" after.`
  },
], (item) => agent(item.prompt, { label: item.key, schema: {
  type: 'object',
  properties: {
    changes_made: { type: 'array', items: { type: 'string' } },
    files_modified: { type: 'array', items: { type: 'string' } },
    files_created: { type: 'array', items: { type: 'string' } },
    verification: { type: 'string' },
  },
  required: ['changes_made', 'files_modified', 'verification'],
}}));

phase('Red Team')

const redTeam = await parallel([
  () => agent(`RED TEAM: Verify all CI/CD changes are correct and won't break.

Check:
1. ci.yml: valid YAML? Job names unique? Correct event triggers? fmt and security-audit jobs use correct commands?
2. release.yml: valid YAML? Docker job references correct Dockerfile path? Container registry permissions correct?
3. install.sh: valid bash? Checksum verification actually works? No shellcheck issues?
4. src/config.rs: all env vars covered? Validation catches errors? Secrets redacted?

Read the files. Run validation where possible. Flag any issues.`, {
    label: 'red-team:ci-correctness',
    phase: 'Red Team',
    schema: {
      type: 'object',
      properties: {
        findings: { type: 'array', items: { type: 'object', properties: {
          file: { type: 'string' }, issue: { type: 'string' }, fix: { type: 'string' },
          severity: { type: 'string', enum: ['CRITICAL', 'WARNING'] },
        }, required: ['file', 'issue', 'fix', 'severity']}},
        all_valid: { type: 'boolean' },
      },
      required: ['findings', 'all_valid'],
    }
  }),
  () => agent(`RED TEAM: Review config system for AI slop and design issues.

Read src/config.rs. Flag:
- Over-engineered: did we use a heavy config framework when manual parsing would do?
- Under-engineered: are there missing validations that could cause runtime failures?
- Cringe: any "configuration management system" language? It should just be a struct with from_env().
- Missing env vars: grep for std::env::var in src/ — are there any still reading env vars directly?
- Bad defaults: any defaults that would be surprising or unsafe?

Be precise. The config system should be boring and correct.`, {
    label: 'red-team:config-design',
    phase: 'Red Team',
    schema: {
      type: 'object',
      properties: {
        findings: { type: 'array', items: { type: 'object', properties: {
          issue: { type: 'string' }, fix: { type: 'string' },
          severity: { type: 'string', enum: ['CRITICAL', 'WARNING', 'NIT'] },
        }, required: ['issue', 'fix', 'severity']}},
        is_boring_and_correct: { type: 'boolean' },
      },
      required: ['findings', 'is_boring_and_correct'],
    }
  }),
]);

return { results: results.filter(Boolean), redTeam: redTeam.filter(Boolean) }
