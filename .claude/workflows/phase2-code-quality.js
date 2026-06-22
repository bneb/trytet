export const meta = {
  name: 'phase2-code-quality',
  description: 'Enum-ify error types, fix blocking I/O, deduplicate oracle, decompose large functions',
  phases: [
    { title: 'Fix', detail: 'Apply code quality fixes' },
    { title: 'Verify', detail: 'Build and test verification' },
    { title: 'Red Team', detail: 'Adversarial review of changes' },
  ],
}

phase('Fix')
const results = await pipeline([
  {
    key: 'enum-error-types',
    prompt: `Fix stringly-typed error handling in the Trytet project at /Users/kevin/projects/trytet.

PROBLEM 1: CrashReport.error_type is a String. Find CrashReport in src/models.rs (or wherever it's defined).

Replace String with a proper enum. Read the codebase to find ALL string values used for error_type, then create:
- An enum with variants for each distinct error type found
- Use #[serde(rename_all = "snake_case")] to preserve JSON compatibility
- Update CrashReport to use the enum
- Find and update ALL match statements that pattern-match on error_type strings

PROBLEM 2: VoucherManager::verify_and_claim returns Result<(), String> — find it and replace with a proper thiserror enum.

IMPORTANT: Read all relevant files first. Search for ALL callers. Preserve JSON wire format. Run "cargo check" after.`
  },
  {
    key: 'fix-blocking-io',
    prompt: `Fix blocking I/O calls in async functions in /Users/kevin/projects/trytet.

Files with blocking std::fs calls in async context:
1. src/oracle.rs — std::fs::read, write, create_dir_all in async fn resolve()
2. src/model_proxy.rs — synchronous file I/O in async functions
3. src/shards.rs — std::thread::spawn for disk work (use tokio::task::spawn_blocking)

For each: Read the file. Replace blocking calls with tokio::fs equivalents, or wrap in spawn_blocking for large I/O. Don't change public function signatures unless needed.

IMPORTANT: spawn_blocking closures must be 'static + Send. Run "cargo check".`
  },
  {
    key: 'deduplicate-oracle',
    prompt: `Read src/oracle.rs at /Users/kevin/projects/trytet.

The explore found that resolve() and resolve_with_headers() are near-identical — the only difference is header injection on the HTTP request.

Refactor to eliminate duplication. Options:
1. Make resolve_with_headers() delegate to resolve() with an optional headers parameter
2. Extract a shared private method

Check all callers first. Preserve the public API. Run "cargo check" after.`
  },
], (item) => agent(item.prompt, { label: item.key, schema: {
  type: 'object',
  properties: {
    changes_made: { type: 'array', items: { type: 'string' } },
    files_modified: { type: 'array', items: { type: 'string' } },
    files_deleted: { type: 'array', items: { type: 'string' } },
    verification: { type: 'string' },
    risks: { type: 'array', items: { type: 'string' } },
  },
  required: ['changes_made', 'files_modified', 'verification'],
}}));

phase('Verify')

const verify = await agent(`Verify ALL code quality changes in /Users/kevin/projects/trytet compile and pass tests.

Run these commands:
1. cargo check 2>&1 | tail -20
2. cargo test --release 2>&1 | tail -30
3. cargo clippy --all-targets -- -D warnings 2>&1 | tail -30

Report any compilation errors, test failures, or clippy warnings. If any fail, note the specific error so we can fix it.

This is verification only — do NOT make changes. Just report what passes and what fails.`, {
  label: 'verify-build',
  phase: 'Verify',
});

phase('Red Team')

const redTeam = await parallel([
  () => agent(`RED TEAM: Review all code quality changes for AI slop, cringe, or over-engineering.

Check every modified file in /Users/kevin/projects/trytet/src/ for:
- Over-engineered solutions (simple problem, complex answer)
- Unnecessary abstraction (traits with one impl, wrappers that add nothing)
- Comments that state the obvious ("// increment the counter" above "i += 1")
- Vague variable names introduced by the changes
- Any new code that feels "off" or non-idiomatic Rust
- AI-speak in comments or error messages

Be ruthless. Flag anything that would embarrass a senior Rust engineer reviewing this code.`, {
    label: 'red-team:code-quality',
    phase: 'Red Team',
    schema: {
      type: 'object',
      properties: {
        findings: { type: 'array', items: { type: 'object', properties: {
          file: { type: 'string' }, line: { type: 'integer' },
          issue: { type: 'string' }, fix: { type: 'string' },
          severity: { type: 'string', enum: ['CRITICAL', 'WARNING'] },
        }, required: ['file', 'issue', 'fix', 'severity']}},
        overall_quality: { type: 'string' },
      },
      required: ['findings', 'overall_quality'],
    }
  }),
  () => agent(`RED TEAM: Verify correctness of code quality changes.

Check:
1. Error type enum changes — are ALL match arms updated? Any remaining string comparisons for error_type? Does JSON serialize/deserialize correctly?
2. Blocking I/O fixes — any double-wrapping (spawn_blocking inside spawn_blocking)? Any missing Send bounds? Any deadlocks possible?
3. Oracle deduplication — does the refactored code produce identical HTTP requests? Are headers handled correctly?

Read the actual modified files and trace the logic. Verify with "cargo check".

Be precise. A portfolio piece with subtle bugs from refactoring is worse than leaving the code as-is.`, {
    label: 'red-team:correctness',
    phase: 'Red Team',
    schema: {
      type: 'object',
      properties: {
        findings: { type: 'array', items: { type: 'object', properties: {
          file: { type: 'string' }, issue: { type: 'string' }, fix: { type: 'string' },
          severity: { type: 'string', enum: ['CRITICAL', 'WARNING'] },
        }, required: ['file', 'issue', 'fix', 'severity']}},
        all_sound: { type: 'boolean' },
      },
      required: ['findings', 'all_sound'],
    }
  }),
]);

return { results: results.filter(Boolean), verify, redTeam: redTeam.filter(Boolean) }
