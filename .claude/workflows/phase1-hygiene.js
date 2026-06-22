export const meta = {
  name: 'phase1-repo-hygiene',
  description: 'LICENSE, Cargo.toml metadata, delete dead crates/files, fix broken links, move sprint docs',
  phases: [
    { title: 'Fix', detail: 'Apply hygiene fixes' },
    { title: 'Red Team', detail: 'Adversarial review of changes' },
  ],
}

phase('Fix')
const results = await pipeline([
  {
    key: 'license',
    prompt: `Add a proper MIT LICENSE file to the Trytet project at /Users/kevin/projects/trytet.

The README.md already states the project is MIT licensed, but no LICENSE file exists.

Create a file at /Users/kevin/projects/trytet/LICENSE with the standard MIT license text. Use the copyright holder format: "Copyright (c) 2025 Trytet Contributors"

This must be the EXACT standard MIT license text. Do not add commentary. Do not modify the template. Just the standard MIT license.`
  },
  {
    key: 'cargo-metadata',
    prompt: `Fix the Cargo.toml metadata at /Users/kevin/projects/trytet/Cargo.toml.

Read the file first. Add these missing fields to the [package] section:
- license = "MIT"
- repository = "https://github.com/bneb/trytet"
- homepage = "https://github.com/bneb/trytet"
- documentation = "https://github.com/bneb/trytet"
- keywords = ["wasm", "sandbox", "wasmtime", "ai", "mcp", "code-execution"]
- categories = ["development-tools", "web-programming"]
- rust-version = "1.80"
- readme = "README.md"

Also check all workspace member crates in crates/*/Cargo.toml — if any are missing license, add "MIT".

IMPORTANT: Only add fields that are actually missing. Read each file first. Use Edit for surgical changes. Do NOT change existing values. Do NOT add commentary.

Valid crates.io categories: https://crates.io/category_slugs — use exact slugs.`
  },
  {
    key: 'dead-crates',
    prompt: `Delete dead/stub crates from the Trytet workspace at /Users/kevin/projects/trytet.

Check these crates for stub/placeholder code:
- crates/lean-cartridge/
- crates/replay-debugger/
- crates/mcts-guest/

A crate is "dead" if its src/lib.rs contains only cargo-new template code like:
  pub fn add(left: u64, right: u64) -> u64 { left + right }

For each confirmed dead crate:
1. Delete the entire directory
2. Remove it from workspace members in Cargo.toml
3. Verify no other source files import from it (grep the workspace)

IMPORTANT: Read src/lib.rs of each crate first. Only delete confirmed stubs. Run "cargo check" after.`
  },
  {
    key: 'stale-workflow',
    prompt: `Read both /Users/kevin/projects/trytet/.github/workflows/ci.yml and /Users/kevin/projects/trytet/.github/workflows/deploy.yml.

Compare them. If deploy.yml is a stale/duplicate variant with no unique deployment logic, delete it.

IMPORTANT: Read both files first. Only delete if deploy.yml truly adds nothing beyond ci.yml. Verify the filename before deleting.`
  },
  {
    key: 'sprint-docs',
    prompt: `Move internal sprint/development documents out of the root directory so they don't confuse visitors.

Create a "notes/" directory at /Users/kevin/projects/trytet/notes/.

Move these files there (do NOT modify their contents — they're historical records):
- PROMPTS.md -> notes/PROMPTS.md
- RED_TEAM_SHIP.md -> notes/RED_TEAM_SHIP.md
- SPRINT_SHIP.md -> notes/SPRINT_SHIP.md
- SPRINT.md -> notes/SPRINT.md
- RED_TEAM.md -> notes/RED_TEAM.md

After moving, check if any other files reference these by their old root paths and update those references.

Use "git mv" if possible. Do NOT rewrite file contents.`
  },
], (item) => agent(item.prompt, { label: item.key, schema: {
  type: 'object',
  properties: {
    changes_made: { type: 'array', items: { type: 'string' } },
    files_modified: { type: 'array', items: { type: 'string' } },
    files_deleted: { type: 'array', items: { type: 'string' } },
    files_created: { type: 'array', items: { type: 'string' } },
    verification: { type: 'string' },
    issues: { type: 'array', items: { type: 'string' } },
  },
  required: ['changes_made', 'files_modified', 'files_deleted', 'files_created', 'verification'],
}}));

phase('Red Team')

const redTeam = await parallel([
  () => agent(`RED TEAM: Review ALL changes from this hygiene pass for AI slop, cringe, or errors.

Check every modified/created file in /Users/kevin/projects/trytet. Flag:
- Marketing fluff, AI-speak ("delve", "unleash", "robust", "seamless")
- Cringe phrases, emoji spam
- Factual errors (wrong license text, broken URLs, incorrect metadata)
- Typos
- Placeholder text that wasn't replaced

Be ruthless. This goes in a portfolio.`, {
    label: 'red-team:cringe',
    phase: 'Red Team',
    schema: {
      type: 'object',
      properties: {
        findings: { type: 'array', items: { type: 'object', properties: {
          file: { type: 'string' }, line: { type: 'integer' },
          issue: { type: 'string' }, fix: { type: 'string' },
          severity: { type: 'string', enum: ['CRITICAL', 'WARNING'] },
        }, required: ['file', 'issue', 'fix', 'severity']}},
        clean: { type: 'boolean' },
      },
      required: ['findings', 'clean'],
    }
  }),
  () => agent(`RED TEAM: Verify correctness of all hygiene changes.

Check:
1. LICENSE file: is it the exact standard MIT text? No modifications?
2. Cargo.toml: are all new fields valid? Does "cargo check" work? Are the categories valid slugs?
3. Were dead crates fully removed? No remaining references in workspace members or imports?
4. Was deploy.yml safely deleted? Does ci.yml still exist and is it the correct one?
5. Sprint docs: were they moved correctly? No broken references? Did git mv preserve history?

Verify by reading files and running commands.`, {
    label: 'red-team:correctness',
    phase: 'Red Team',
    schema: {
      type: 'object',
      properties: {
        findings: { type: 'array', items: { type: 'object', properties: {
          file: { type: 'string' }, issue: { type: 'string' }, fix: { type: 'string' },
          severity: { type: 'string', enum: ['CRITICAL', 'WARNING'] },
        }, required: ['file', 'issue', 'fix', 'severity']}},
        all_clean: { type: 'boolean' },
      },
      required: ['findings', 'all_clean'],
    }
  }),
]);

return { results: results.filter(Boolean), redTeam: redTeam.filter(Boolean) }
