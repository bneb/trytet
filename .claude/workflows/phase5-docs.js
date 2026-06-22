export const meta = {
  name: 'phase5-documentation',
  description: 'CONTRIBUTING.md, SECURITY.md, architectural diagrams, SDK READMEs, fix broken links',
  phases: [
    { title: 'Fix', detail: 'Create and improve documentation' },
    { title: 'Red Team', detail: 'Review docs for AI slop and accuracy' },
  ],
}

phase('Fix')
const results = await pipeline([
  {
    key: 'contributing-security',
    prompt: `Create essential community docs for /Users/kevin/projects/trytet.

1. CONTRIBUTING.md:
- Dev environment setup (Rust 1.80+, cmake, clang, protobuf)
- How to run tests, clippy, fmt
- Commit conventions (conventional commits)
- PR process: create branch, run tests+clippy+fmt, open PR
- Link to ARCHITECTURE.md
- Reference CODE_OF_CONDUCT.md (if it exists)

2. SECURITY.md:
- How to report vulnerabilities (private disclosure)
- Supported versions (0.2.x)
- Security model summary (fuel-bounded, sandbox isolation)
- Disclosure policy (90-day)

3. CODE_OF_CONDUCT.md:
- Standard Contributor Covenant 2.1 (the industry standard, copy verbatim)
- Contact email placeholder

Keep files concise and professional. No marketing fluff. No emoji spam. No AI-speak. Write like a senior engineer documenting their project — direct, clear, minimal.`
  },
  {
    key: 'architecture-diagrams',
    prompt: 'Add Mermaid diagrams to /Users/kevin/projects/trytet/ARCHITECTURE.md.\n\nRead the current file first. Add these diagrams using ```mermaid code blocks (GitHub renders these natively):\n\n1. System architecture: Three layers (Sandbox, Cartridge Substrate, Hive Mesh), how they connect, external interfaces (MCP, HTTP API, CLI)\n2. Agent lifecycle sequence: boot -> execute -> snapshot -> fork -> teleport -> suspend/resume\n\nKeep diagrams focused (max 15 nodes). Reflect ACTUAL code structure, not aspirations. Verify Mermaid syntax — no unescaped quotes, valid arrows, valid node names.\n\nInsert diagrams in appropriate sections. Do not replace existing content.'
  },
  {
    key: 'sdk-readmes',
    prompt: `Improve SDK READMEs at /Users/kevin/projects/trytet.

1. Read sdk/typescript/README.md. Improve it with:
- Clear installation instructions
- Complete usage example (import, create client, execute, handle errors)
- API reference for each public method
- Error handling documentation
- TypeScript types reference if available

2. Read sdk/python/README.md. Ensure equivalent coverage.

IMPORTANT: Read the actual SDK source code to verify documentation accuracy. Do NOT document APIs that don't exist. Keep examples runnable. Technical and precise — no tutorial fluff.`
  },
  {
    key: 'fix-docs-links',
    prompt: `Check and fix all links in /Users/kevin/projects/trytet/docs/index.html.

Read the file. Check every href:
- Does CARTRIDGE.md exist? If not, link to ARCHITECTURE.md or the GitHub repo instead
- Are GitHub URLs correct (https://github.com/bneb/trytet)?
- Are any links 404?

Also check README.md, ARCHITECTURE.md, CLI.md, BENCHMARKS.md, DEPLOYMENT.md for broken links.

Fix any broken links found. Do NOT change working links.`
  },
], (item) => agent(item.prompt, { label: item.key, schema: {
  type: 'object',
  properties: {
    changes_made: { type: 'array', items: { type: 'string' } },
    files_created: { type: 'array', items: { type: 'string' } },
    files_modified: { type: 'array', items: { type: 'string' } },
    verification: { type: 'string' },
  },
  required: ['changes_made', 'verification'],
}}));

phase('Red Team')

const redTeam = await parallel([
  () => agent(`RED TEAM: Review ALL new documentation for AI slop and cringe. This is the MOST IMPORTANT review — docs are the first thing people read.

Read every new and modified doc file:
- CONTRIBUTING.md, SECURITY.md, CODE_OF_CONDUCT.md
- ARCHITECTURE.md (check new diagrams)
- sdk/typescript/README.md, sdk/python/README.md
- docs/index.html

Flag with EXTREME PREJUDICE:
- AI-speak: "delve", "unleash", "robust", "seamless", "cutting-edge", "state-of-the-art", "game-changing", "revolutionary"
- Marketing bullshit: "designed with developer experience in mind", "built for scale", "next-generation"
- Cringe: emoji, "welcome!", "happy coding!", "let's build together", exclamation marks
- Vague platitudes: sentences that say nothing ("Trytet provides a comprehensive solution for...")
- Wordiness: 20 words that could be 5
- Any sentence that sounds like a corporate blog post

For each finding, provide the exact replacement text. This is going in a portfolio — every word will be scrutinized.`, {
    label: 'red-team:docs-cringe',
    phase: 'Red Team',
    schema: {
      type: 'object',
      properties: {
        findings: { type: 'array', items: { type: 'object', properties: {
          file: { type: 'string' }, line: { type: 'integer' },
          offending: { type: 'string' }, replacement: { type: 'string' },
          severity: { type: 'string', enum: ['CRITICAL', 'WARNING'] },
        }, required: ['file', 'offending', 'replacement', 'severity']}},
        portfolio_ready: { type: 'boolean' },
      },
      required: ['findings', 'portfolio_ready'],
    }
  }),
  () => agent(`RED TEAM: Verify factual accuracy of all documentation.

Check:
1. Do installation instructions actually work? (check against Cargo.toml deps)
2. Do Mermaid diagrams render? (check for valid syntax: no unescaped quotes, valid node names)
3. Are API examples correct? (check against actual source code)
4. Are CLI command examples accurate? (check against actual CLI implementation)
5. Do SDK examples compile/run? (check against actual SDK source)
6. Are all links valid? (verify each href points to something that exists)
7. Is the CODE_OF_CONDUCT.md the standard Contributor Covenant text (not a hallucinated variant)?
8. Is MIT license text exact (in LICENSE file)?

Flag anything factually wrong. Speculation or aspirational claims about features that don't exist are UNACCEPTABLE.`, {
    label: 'red-team:docs-accuracy',
    phase: 'Red Team',
    schema: {
      type: 'object',
      properties: {
        factual_errors: { type: 'array', items: { type: 'object', properties: {
          file: { type: 'string' }, claim: { type: 'string' }, reality: { type: 'string' },
          severity: { type: 'string', enum: ['CRITICAL', 'WARNING'] },
        }, required: ['file', 'claim', 'reality', 'severity']}},
        all_accurate: { type: 'boolean' },
      },
      required: ['factual_errors', 'all_accurate'],
    }
  }),
]);

return { results: results.filter(Boolean), redTeam: redTeam.filter(Boolean) }
