export const meta = {
  name: 'phase3-testing',
  description: 'Add tests to untested crates, un-ignore justified tests, add fuzz harness',
  phases: [
    { title: 'Fix', detail: 'Add tests and fuzz harness' },
    { title: 'Red Team', detail: 'Verify tests are real and fuzz harness works' },
  ],
}

phase('Fix')
const results = await pipeline([
  {
    key: 'add-crate-tests',
    prompt: `Add real tests to untested cartridge crates in /Users/kevin/projects/trytet.

Crates with zero tests (check each — some may have been deleted in Phase 1):
- crates/regex-evaluator/
- crates/jmespath-cartridge/
- crates/json-schema-cartridge/
- crates/sat-cartridge/
- crates/scraper-cartridge/
- crates/sql-cartridge/
- crates/python-evaluator/

For each crate that still exists:
1. Read src/lib.rs to understand the logic
2. Add #[cfg(test)] mod tests with:
   - A basic success test (valid input -> expected output)
   - An error path test (invalid input -> error)
   - An edge case (empty input, boundary values)
3. Use the existing test patterns from crates/js-evaluator/ as a model
4. Run "cargo test -p <crate>" to verify

CRITICAL: Tests must exercise REAL code paths. No "assert!(true)" or "assert_eq!(1, 1)". Every test must call actual cartridge/evaluator functions with real inputs and verify outputs. If a crate's architecture makes testing difficult without wasmtime, document why and add what unit tests you can on the pure functions.`
  },
  {
    key: 'fix-ignored-tests',
    prompt: `Review all #[ignore] tests in /Users/kevin/projects/trytet/tests/.

Find every #[ignore] test. For each:
1. Read the test to understand why it's ignored
2. If it can safely run (simple endpoint test, doesn't need external resources): remove #[ignore], verify it passes
3. If it needs external resources (model download, multi-node setup): keep #[ignore] but add a CLEAR reason string like #[ignore = "Downloads 350MB Qwen model — run manually with cargo test --release -- --ignored"]

Tests to check:
- test_health_check in api_tests.rs — should be simple, try to un-ignore
- test_phase_15_legacy_bridge, test_phase_16_semantic_persistence — investigate
- test_inter_tet_communication in mesh_tests.rs — may need fixtures
- test_phase18_local_ingress_registration_and_execution
- test_phase_8_git_for_ram_registry, test_phase_10_live_migration in cli_tests.rs
- test_phase_17_real_smoke — 350MB download, keep ignored with clear reason

Run each un-ignored test to verify it passes.`
  },
  {
    key: 'fuzz-harness',
    prompt: `Set up cargo-fuzz for the Trytet project at /Users/kevin/projects/trytet.

A WASM sandbox that executes untrusted code MUST have fuzzing. Do this:

1. Create fuzz/Cargo.toml with dependencies on the workspace crates
2. Create fuzz/fuzz_targets/wasm_parse.rs:
   - Feed arbitrary bytes to WASM module parsing
   - Goal: ensure no panics, no OOM, no undefined behavior on malformed input
3. Create fuzz/fuzz_targets/fuel_voucher.rs:
   - Feed arbitrary bytes to voucher validation
   - Goal: ensure validation never panics, always returns a clean error
4. Add a fuzz job to .github/workflows/ci.yml that runs each fuzzer for 60 seconds
5. Create FUZZING.md with instructions

Use libfuzzer-sys (cargo-fuzz). Keep targets simple — byte-array fuzzing is sufficient for now.

IMPORTANT: Run "cargo fuzz list" to verify setup. Write actual fuzz targets that compile. The CI job should be short (60s per target) — fuzzing is for regression catching, not exhaustive exploration.`
  },
], (item) => agent(item.prompt, { label: item.key, schema: {
  type: 'object',
  properties: {
    changes_made: { type: 'array', items: { type: 'string' } },
    files_created: { type: 'array', items: { type: 'string' } },
    files_modified: { type: 'array', items: { type: 'string' } },
    tests_added: { type: 'integer' },
    tests_unignored: { type: 'integer' },
    verification: { type: 'string' },
  },
  required: ['changes_made', 'verification'],
}}));

phase('Red Team')

const redTeam = await parallel([
  () => agent(`RED TEAM: Review ALL new tests for quality. Flag tests that are fake, useless, or poorly written.

Read every new test added to cartridge crates. Flag:
- Tests that don't actually test anything (assert true, assert 1==1)
- Tests with no assertions (just calling a function without checking output)
- Tests that are copy-pasted duplicates with different names
- Tests that test the test framework, not the code
- Overly trivial tests that add noise without signal
- Tests with "it_works" names that don't describe what they test

Also check: do the tests actually pass? Run them.`, {
    label: 'red-team:test-quality',
    phase: 'Red Team',
    schema: {
      type: 'object',
      properties: {
        fake_tests: { type: 'array', items: { type: 'string' } },
        failing_tests: { type: 'array', items: { type: 'string' } },
        weak_tests: { type: 'array', items: { type: 'string' } },
        overall: { type: 'string' },
      },
      required: ['fake_tests', 'failing_tests', 'weak_tests', 'overall'],
    }
  }),
  () => agent(`RED TEAM: Verify the fuzz harness actually works.

1. Check fuzz/Cargo.toml is valid
2. Check fuzz targets compile: cargo fuzz build 2>&1
3. Check fuzz targets actually exercise code: run each for 5 seconds with a dummy corpus
4. Verify the CI fuzz job YAML is valid
5. Check FUZZING.md is accurate and runnable

Report any issues.`, {
    label: 'red-team:fuzz-correctness',
    phase: 'Red Team',
    schema: {
      type: 'object',
      properties: {
        compiles: { type: 'boolean' },
        runs: { type: 'boolean' },
        issues: { type: 'array', items: { type: 'string' } },
      },
      required: ['compiles', 'runs', 'issues'],
    }
  }),
]);

return { results: results.filter(Boolean), redTeam: redTeam.filter(Boolean) }
