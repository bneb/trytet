export const meta = {
  name: 'phase6-final-red-team',
  description: 'Final adversarial review of entire project for portfolio readiness',
  phases: [
    { title: 'Review', detail: 'Four independent adversarial reviewers' },
    { title: 'Fix', detail: 'Fix all CRITICAL findings from reviews' },
    { title: 'Verify', detail: 'Verify fixes and final build' },
  ],
}

phase('Review')

// Four independent adversarial reviews, each with a different lens
const reviews = await parallel([
  // Lens 1: Portfolio first impression
  () => agent(`ADVERSARIAL REVIEW: Portfolio First Impression

You are a hiring manager at an AI infrastructure company. You have 2 minutes to review the Trytet repo at /Users/kevin/projects/trytet and decide if the author gets an interview.

Check in order:
1. README.md — does it tell you what this is, why it exists, and how to use it in 30 seconds?
2. ARCHITECTURE.md — does it show depth without being a novel?
3. LICENSE, CONTRIBUTING.md, SECURITY.md — are they present and professional?
4. Cargo.toml — is metadata complete? Does it look like someone cares about craft?
5. Quick file scan — any obviously dead code, TODO comments, placeholder files?
6. tests/ — is there evidence of serious testing?

Score each area: STRONG / ADEQUATE / WEAK / MISSING

What would make you PASS vs FAIL this candidate? Be brutally honest — this is a competitive role.`, {
    label: 'review:first-impression',
    phase: 'Review',
    schema: {
      type: 'object',
      properties: {
        scores: { type: 'object', properties: {
          readme: { type: 'string' }, architecture: { type: 'string' },
          community_files: { type: 'string' }, cargo_metadata: { type: 'string' },
          code_scan: { type: 'string' }, testing: { type: 'string' },
        }, required: ['readme', 'architecture', 'community_files', 'cargo_metadata', 'code_scan', 'testing']},
        verdict: { type: 'string', enum: ['PASS', 'PASS_WITH_RESERVATIONS', 'FAIL'] },
        top_3_issues: { type: 'array', items: { type: 'string' } },
        what_impressed: { type: 'string' },
        what_embarrassed: { type: 'string' },
      },
      required: ['scores', 'verdict', 'top_3_issues', 'what_impressed', 'what_embarrassed'],
    }
  }),

  // Lens 2: Senior Rust engineer code review
  () => agent(`ADVERSARIAL REVIEW: Senior Rust Engineer Deep Read

You are a staff Rust engineer. Spend time reading the core source files in /Users/kevin/projects/trytet/src/ and crates/.

Evaluate:
1. Error handling — is there a coherent strategy? Or thiserror mixed with anyhow/String?
2. Async hygiene — any blocking in async? spawn_blocking used correctly? No deadlock patterns?
3. API design — are traits well-designed? Are public APIs ergonomic? Are there obvious missing impls?
4. Module structure — logical separation? Any god objects? Circular deps?
5. Unsafe — any unnecessary unsafe? (check: grep for unsafe)
6. Dependencies — any bloated deps? Stale versions? Unnecessary features enabled?
7. Idioms — is the code idiomatic Rust 2021? Or does it feel like a C++/Python transplant?

Rate each area and provide specific examples. Flag anything that would make a senior engineer say "this person doesn't really know Rust."`, {
    label: 'review:rust-engineer',
    phase: 'Review',
    schema: {
      type: 'object',
      properties: {
        ratings: { type: 'object' },
        rust_idiom_score: { type: 'string', enum: ['NATIVE', 'COMPETENT', 'LEARNING', 'TRANSPLANT'] },
        top_3_code_smells: { type: 'array', items: { type: 'string' } },
        top_3_strengths: { type: 'array', items: { type: 'string' } },
        would_merge: { type: 'boolean' },
      },
      required: ['ratings', 'rust_idiom_score', 'top_3_code_smells', 'top_3_strengths', 'would_merge'],
    }
  }),

  // Lens 3: AI/bullshit detector
  () => agent(`ADVERSARIAL REVIEW: AI Slop & Bullshit Sweep

You are an expert at detecting AI-generated content, marketing fluff, and technical dishonesty. Read ALL files in /Users/kevin/projects/trytet — every doc, every comment, every error message.

FLAG FOR IMMEDIATE REMOVAL:
- Marketing jargon: "unleash", "revolutionary", "game-changing", "next-gen", "cutting-edge", "state-of-the-art", "best-in-class", "industry-leading"
- AI-speak verbs: "delve", "unlock", "empower", "harness", "supercharge", "reimagine", "transform"
- Floral bullshit: "it's not just X, it's Y", "seamlessly integrates", "designed from the ground up", "purpose-built"
- Cringe: emoji in docs, "welcome!", "happy coding!", "let's build the future", excessive exclamation marks
- Corporate speak: "robust", "scalable", "flexible", "comprehensive", "enterprise-grade" — unless backed by specific evidence
- Word bloat: sentences that say in 20 words what could be said in 5
- Technical dishonesty: claims about features that don't exist, performance numbers without benchmarks, aspirational language presented as fact

For each finding:
- Exact file, line number, and offending text
- Suggested replacement (if applicable) or "DELETE"
- Severity

This is the final sweep. Nothing that smells like AI gets through.`, {
    label: 'review:bullshit-detector',
    phase: 'Review',
    schema: {
      type: 'object',
      properties: {
        findings: { type: 'array', items: { type: 'object', properties: {
          file: { type: 'string' }, line: { type: 'integer' },
          offending: { type: 'string' }, action: { type: 'string' },
          severity: { type: 'string', enum: ['CRITICAL', 'WARNING', 'NIT'] },
        }, required: ['file', 'offending', 'action', 'severity']}},
        bullshit_score: { type: 'integer', description: '0-100 where 0 = pristine, 100 = startup pitch deck' },
        clean: { type: 'boolean' },
      },
      required: ['findings', 'bullshit_score', 'clean'],
    }
  }),

  // Lens 4: Architecture coherence
  () => agent(`ADVERSARIAL REVIEW: Architecture & Completeness

You are reviewing a portfolio project that claims to be "v0.2.0 — pre-release but functional."

Does the code tell a coherent story? Check:

1. Module map — does every module in src/lib.rs have a clear purpose? Any orphans?
2. Cartridge crates — consistent structure? Do they all implement the same interface? Are there naming inconsistencies (evaluator vs cartridge)?
3. Feature completeness — does the MCP server actually serve all 5 tools? Does the HTTP API actually handle all documented endpoints? Does the CLI actually implement all documented commands?
4. Error story — can you trace how errors flow from a Wasm trap back to an HTTP response? Is it coherent or ad-hoc?
5. Config story — after Phase 4, is there a single config entry point or still scattered env vars?
6. Telemetry story — does the TelemetryHub actually get wired into all the places it should?
7. Testing story — do the tests form a coherent pyramid (unit -> integration -> e2e) or a scattered mess?

For each: coherent / ad-hoc / missing. Provide specific evidence from the code.

Also: identify the SINGLE most impressive technical aspect of this codebase AND the SINGLE most embarrassing thing still present.`, {
    label: 'review:architecture',
    phase: 'Review',
    schema: {
      type: 'object',
      properties: {
        module_coherence: { type: 'string' },
        cartridge_consistency: { type: 'string' },
        feature_completeness: { type: 'string' },
        error_story: { type: 'string' },
        config_story: { type: 'string' },
        telemetry_story: { type: 'string' },
        testing_story: { type: 'string' },
        most_impressive: { type: 'string' },
        most_embarrassing: { type: 'string' },
        portfolio_ready: { type: 'boolean' },
      },
      required: ['module_coherence', 'feature_completeness', 'most_impressive', 'most_embarrassing', 'portfolio_ready'],
    }
  }),
]);

// Aggregate findings
const allFindings = reviews.filter(Boolean);
const criticalFindings = [];
for (const review of allFindings) {
  const findings = review.findings || [];
  for (const f of findings) {
    if (f.severity === 'CRITICAL') criticalFindings.push(f);
  }
}

log(`Red team complete: ${criticalFindings.length} CRITICAL findings across ${allFindings.length} reviewers`)

if (criticalFindings.length > 0) {
  log('CRITICAL findings:')
  for (const f of criticalFindings) {
    log(`  ${f.file || f.issue}: ${f.offending || f.claim || 'see detail'}`)
  }
}

// Phase: Fix criticals
if (criticalFindings.length > 0) {
  phase('Fix')

  const fixResults = await pipeline(criticalFindings, (finding) => {
    const desc = finding.offending || finding.claim || finding.issue || JSON.stringify(finding).slice(0, 100);
    const fix = finding.fix || finding.action || finding.recommendation || 'Fix the issue described';
    return agent(`Fix this CRITICAL issue found by red team review in /Users/kevin/projects/trytet:

File: ${finding.file || 'unknown'}
Issue: ${desc}
Fix: ${fix}

Read the file, apply the fix, verify it's correct. Be surgical — minimal change, maximum impact.`, {
      label: 'fix-critical',
      phase: 'Fix',
    });
  });

  log(`Fixed ${fixResults.filter(Boolean).length} critical issues`)
}

// Phase: Final verify
phase('Verify')

const finalVerify = await agent(`FINAL VERIFICATION of /Users/kevin/projects/trytet:

Run and report results:
1. cargo check 2>&1 | tail -10
2. cargo test --release 2>&1 | tail -20
3. cargo clippy --all-targets -- -D warnings 2>&1 | tail -20
4. cargo fmt --check --all 2>&1

Report exit codes. If any fail, specify exact errors.

This is the final gate. Everything must pass.`, {
  label: 'final-verify',
  phase: 'Verify',
});

return {
  reviews: allFindings,
  criticalCount: criticalFindings.length,
  fixedCount: criticalFindings.length > 0 ? fixResults.filter(Boolean).length : 0,
  finalVerify,
}
