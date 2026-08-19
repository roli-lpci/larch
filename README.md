# Larch

Larch is a Claude Code workflow automation framework that orchestrates multi-agent design, code review, and implementation through collaborative AI-driven processes.

> **New to larch?** First [prepare your repository](docs/preparing-your-repo.md) for agent-assisted development, then follow the flow below.

## Primary Flow

1. Create an issue describing the task/problem with `/issue` or `/file-bug`, or manually
2. Verify an existing report with `/triage` when its diagnosis needs evidence before planning
3. For contested or open-ended work, use `/debate` to produce a three-vendor proposal
4. Design it with `/design` (the detailed reviewed design is stored in the issue)
5. Implement it with `/implement`

## Support Skills

- Manage issues and their dependencies: `/issue`, `/umbrella`, `/complete-umbrella`, `/audit-umbrella`, `/file-bug`, `/triage`, `/combine-issues`, `/block-issue`, `/deps`
- Various analysis tools: `/report-tokens`, `/fluff-analysis`, `/difficulty-calibration`, `/rejected-analysis`, `/analyze-issues`, `/audit-runs`
- `larch` management: `/status`, `/upgrade-larch`, `/larch-size`

## Table of Contents

- **Setup**
  - [Installation and Setup](docs/installation-and-setup.md) — prerequisites, auth (API keys or web login) for Claude / Codex / Cursor, plugin install and permissions, `/status` validation, `/upgrade-larch`
  - [Preparing Your Repository](docs/preparing-your-repo.md). Ready your repo for larch and agent-assisted development: instruction files (`CLAUDE.md`/`AGENTS.md`), guardrails (guidelines, hooks, linters), and the `checks run-relevant` contract
  - [Contributing](docs/contributing.md) — local dev plugin install, plugin cache vs. working-tree version, Mermaid CLI setup
  - [Clean-Main Entry Contract](docs/clean-main-contract.md) — the `/implement` and `/design` clean `main` entry preconditions, plus `/implement` feature-branch continuation
  - [Fork CI Dry-Runs](docs/forked.md) — remote setup for `/implement --forked`
  - [macOS Keychain Interactions](docs/macos-keychain-interactions.md) — Cursor `CURSOR_API_KEY` and keychain auth troubleshooting
  - [Optional Helpers](docs/optional-helpers.md) — installing optional tools like ast-grep
- **Reference**
  - [Features](#features)
  - [Skills](#skills)
  - [Aliases](#aliases)
  - [Review Agents](docs/review-agents.md) — the unified `code-reviewer` archetype
  - [Run Logs](docs/run-logs.md) — remote run archives, local cache copies, manifests, and tracking-issue comments
  - [Run-log Storage Contracts](docs/run-log-archive.md) — configuration resolution, provider operations, archive and cache layouts, sync, errors, and Rust handoff
  - [Analyzer State](docs/analysis-state.md) — mutable analyzer markers, ledgers, measurements, locking, and legacy import
  - [Security References](docs/security/README.md): security document taxonomy, ownership, and runtime packaging contract
  - [Topology Projection](docs/topology.md) — stable anchors for cross-doc topology counts
  - [Linting](docs/linting.md) — linters, Makefile targets, halt-rate regression harness
  - [Issue-Anchored Plan](docs/issue-anchored-plan.md) — **live** wire format for the /design ↔ /implement plan handoff and clarification round-trip
- **Architecture and workflow**
  - [Rust Architecture](ARCHITECTURE.md) — crate ownership, dependency direction, external boundaries, and release constraints
  - [Rust Async Runtime](docs/rust-async-runtime.md) — cancellation, timeouts, bounded tasks, signals, and child shutdown
  - [Rust Parity Harness](docs/rust-parity-harness.md) — isolated Python-to-Rust command comparison and reviewed goldens
  - [Rust Testing](docs/rust-testing.md) — shared fixtures, test boundaries, coverage, and CI partitioning
  - [GitHub Service Migration Inventory](docs/github-service-inventory.md) — implementation, consumer cutover, and Python-removal status
  - [Workflow Lifecycle](docs/workflow-lifecycle.md) — how skills compose end-to-end
  - [Agent System](docs/agents.md) — parallel subagent orchestration
  - [Design Flow](docs/collaborative-sketches.md) — direct plan drafting and plan review
  - [Voting Process](docs/voting-process.md) — the voting panel protocol
  - [Point Competition](docs/point-competition.md) — reviewer scoring system

## Features

- **[Direct design planning and plan reviews](docs/collaborative-sketches.md)** — Step 2b drafts the plan directly, then the [validation panel](docs/topology.md#design.plan_review.cursor_archetypes) reviews it.
- **[Voting-based review resolution](docs/voting-process.md)** — The YES/NO panel protocol adjudicates plan and code review findings.
- **[Persistent proposal debates](docs/skills.md#debate)**: Cursor, Codex, and Claude negotiate from read-only repository evidence, then publish a cross-linked prose proposal before design.
- **[Reviewer competition scoring](docs/point-competition.md)** — Reviewers earn points based on finding quality; a scoreboard tracks accepted, neutral, and rejected findings.
- **[Tracked runs](docs/run-logs.md)** — Every skill invocation keeps local lifecycle bookkeeping. With run-log storage enabled, it publishes one terminal archive below the derived tool and client-repository root, such as `s3://zhupanov/larch/larch/run-logs/<skill>/<run-id>.tar.gz`. Without storage configuration, it warns and completes without a remote archive, synchronized cache entry, or pending publication. Nested and alias invocations keep distinct parent-linked run identities.
- **[Progress statusline](docs/progress-reporting.md)** — clone-local breadcrumbs show live larch progress without adding report text to model context.
- **Tiered architectural knowledge** — Optional `ARCHITECTURAL_INVARIANTS.md` and `ARCHITECTURAL_GUIDELINES.md` files are supplied to authoring agents plus the dedicated code-review compliance specialist and `/design` Architecture/Standards reviewer as untrusted, scope-bound evidence.

## Skills

larch ships **public skills** with the plugin (`skills/`); **private** skills live under `.claude/skills/` and are dev-only (not exported). Both are listed below; shortcut **aliases** are in the [Aliases](#aliases) section. See [docs/skills.md](docs/skills.md) for full per-skill detail.

### Public skills

<table>
  <thead>
    <tr><th>Name</th><th>Arguments</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><a href="docs/skills.md#alias"><code>/alias</code></a></td>
      <td><code>[--merge] [--private] &lt;alias-name&gt; &lt;target-skill&gt; [preset-flags...]</code></td>
    </tr>
    <tr><td colspan="2">Create an alias for a larch skill with preset flags. Auto-routes to <code>skills/&lt;n&gt;/</code> in plugin source repos and <code>.claude/skills/&lt;n&gt;/</code> elsewhere; <code>--private</code> forces the latter.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#block-issue"><code>/block-issue</code></a></td>
      <td><code>&lt;ISSUE_A&gt; &lt;ISSUE_B&gt; [--repo owner/name] --operator-invoked [--triage-controlled --expected-updated-at TIMESTAMP]</code></td>
    </tr>
    <tr><td colspan="2">Express and verify a native GitHub blocked-by relationship between two issues using the <code>addBlockedBy</code> GraphQL mutation. Live mutation requires explicit operator invocation. Triage-controlled calls add exact target freshness, protected-state, security, relation read-back, and fresh-timestamp checks.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#file-bug"><code>/file-bug</code></a></td>
      <td><code>[--urgent] &lt;bug description&gt;</code></td>
    </tr>
    <tr><td colspan="2">Investigate a user-described bug read-only, compose a detailed issue body, then file it via <code>/issue</code> with dedup enabled. <code>--urgent</code> changes the title prefix. Aborts to <code>SECURITY.md</code> disclosure if the report looks security-sensitive; never edits the repo.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#triage"><code>/triage</code></a></td>
      <td><code>&lt;issue-number&gt; [--repo OWNER/REPO] [--report-only]</code></td>
    </tr>
    <tr><td colspan="2">Verify and root-cause an eligible non-security issue against an immutable main snapshot before <code>/design</code>. A verified verdict may update or close the issue; <code>--report-only</code> and inconclusive results never mutate GitHub.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#cleanup"><code>/cleanup</code></a></td>
      <td><code>[--run-id &lt;ID&gt;]</code></td>
    </tr>
    <tr><td colspan="2">Remove stale larch session temp directories from <code>~/.cache/larch/sessions/</code>, <code>/tmp</code>, and the OS temp root <code>$TMPDIR</code> resolves to (a per-user path distinct from <code>/tmp</code> on macOS) by a bounded five-level nested-activity scan (<code>LARCH_CLEANUP_RETENTION_DAYS</code>, default 7): a directory is deleted only when the scan finds no file newer than the cutoff, so a directory with fresh deep activity is retained even when its top-level mtime is stale. Live session directories named by current environment or session pointers are retained regardless of age. Reaps dangling <code>current-design-env-*.sh</code> symlinks. Always runnable regardless of concurrent Claude sessions.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#combine-issues"><code>/combine-issues</code></a></td>
      <td><code>[--oos]</code></td>
    </tr>
    <tr><td colspan="2">Combine related open issues into fewer broader ones (closing the sources) to reduce token spend. <code>--oos</code> operates only on <code>[OOS]</code>-prefixed issues, discards stale items, and proposes an aggressive combination scheme.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#complete-umbrella"><code>/complete-umbrella</code></a></td>
      <td><code>&lt;umbrella-issue-N&gt;</code></td>
    </tr>
    <tr><td colspan="2">Implement every direct leaf of one managed umbrella serially. One durable Rust-driven bgjob owns the full leaf loop: each iteration fetches one live graph, uses it to verify the prior child and select the smallest-numbered unblocked open leaf, synchronizes clean <code>main</code>, and launches a thin child on the same Claude model with larch skills disabled. A session-keyed pointer makes the same command reattach to a live loop or recover a dead child without replacing its leaf handoff root. Before implementation, the child preserves or creates a valid issue plan and runs the canonical size gate. An oversized leaf stops early with a clear <code>/design</code> handoff before adding an active title or ship state; the driver clears a stale <code>[IMPLEMENTING]</code> prefix left by an older run. The normal path runs fresh implement, adversarial-review, and ship contexts; a standalone driver handles PR, five-minute CI, merge-queue submission or direct admin merge, issue lifecycle, and branch cleanup. An exact orphaned bgjob result may continue only after a fresh remote proof that the same leaf is already closed <code>[DONE]</code>. Other failed children write a bounded step, leaf, and reason envelope and hard-stop the run. After all leaves close, the invoking agent audits the combined result, files and attaches exact new leaves for concrete gaps, and repeats until it can mark the parent <code>[DONE]</code> and close it.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#audit-umbrella"><code>/audit-umbrella</code></a></td>
      <td><code>&lt;umbrella-issue-N&gt;</code></td>
    </tr>
    <tr><td colspan="2">Audit one open top-level managed umbrella at a fresh detached default-branch snapshot. It builds a complete evidence ledger inline, persists the full corrective batch before mutation, files only exact new leaves, reconciles native sub-issue and blocked-by relations, and reads the final graph back. It never implements leaves, closes, or retitles the umbrella.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#deps"><code>/deps</code></a></td>
      <td><code>[--repo owner/name] [--pair-cap N]</code></td>
    </tr>
    <tr><td colspan="2">Audit open issues by in-flight title prefix, conservatively refresh mutable REGULAR bodies, propose stale REGULAR closes, and infer dependencies with an explicit-ref scan plus latent semantic pass. Mutates only after <code>AskUserQuestion</code> approval. Dependency writes use <code>/block-issue</code>. <code>--pair-cap</code> is explicit partial-audit mode.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#debate"><code>/debate</code></a></td>
      <td><code>[-s|--vote-stalemates] &lt;issue-number | free-form description&gt;</code></td>
    </tr>
    <tr><td colspan="2">Run a read-only, persistent Cursor/Codex/Claude negotiation and publish a cross-linked <code>[PROPOSAL]</code> issue. Default mode asks the operator to decide stalemates; <code>-s</code> uses the anonymized voter panel without operator input.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#design"><code>/design</code></a></td>
      <td><code>[-p|--partition] [--brainstorm] [--per-round-approval] [--skip-approve|-s] [--no-dedup] [--run-id &lt;ID&gt;] [--difficulty &lt;TRIVIAL|MODERATE|HARD&gt;] &lt;issue-N | feature description&gt;</code></td>
    </tr>
    <tr><td colspan="2">Author or refresh an issue-anchored implementation plan in GitHub (plan markers in the issue body). <code>-p</code>/<code>--partition</code> routes Step 2b.5 directly to the decomposition panel when no plan-size threshold trips; size triggers show the <code>Override</code>/<code>Cancel</code> prompt. An approved partition hands its exact batch and dependencies to <code>/umbrella</code>, which keeps dedup enabled, converts the original issue in place, and leaves it open above native leaf sub-issues. Optional <code>--brainstorm</code> runs Step 1d.5 ideation before the Step 1d.7 outline-approval gate (Gate A re-entry only post-plan) (see <a href="docs/skills.md#design">docs/skills.md</a>). Gate B auto-applies accepted findings by default; <code>--per-round-approval</code> restores the explicit per-round apply prompt. <code>--skip-approve</code>/<code>-s</code> auto-approves the Step 1d.7 outline and Gate C final plan without an <code>AskUserQuestion</code> (no other prompts are skipped). The old <code>--approve</code> and <code>--hard</code> flags are rejected; use <code>--per-round-approval</code> for explicit Gate B prompts. Finalize runs upstream <code>/larch:issue</code> batch filing for accepted non-security OOS (Step <strong>5b</strong>) before writing and publishing the <code>larch:plan</code> block (<strong>5c</strong>); tmpdir cleanup is Step <strong>6</strong>.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#difficulty-calibration"><code>/difficulty-calibration</code></a></td>
      <td><code>[--log-root DIR] [--out FILE]</code></td>
    </tr>
    <tr><td colspan="2">Compare predicted and realized difficulty tiers from the synchronized run-log cache. Diagnostic only; changes no thresholds, panels, tokens, routing, or reviewer points.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#fluff-analysis"><code>/fluff-analysis</code></a></td>
      <td><code>[--include-in-progress] [--cutoff ISO8601] [--since-version X.Y.Z] [--min-group N] [--log-root DIR] [--out FILE]</code></td>
    </tr>
    <tr><td colspan="2">Characterize review <strong>fluff</strong> from the synchronized run-log cache through its Rust-owned <code>scripts/larch.sh</code> command. Analyze which <code>/design</code> and <code>/implement</code> suggestions get rejected, deferred to OOS, or accepted but low-value, then print data-driven recommendations.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#implement"><code>/implement</code></a></td>
      <td><code>[--merge] [--forked] [--draft] [--no-admin-fallback] [--no-logs-commit] [--coder &lt;claude|codex|cursor&gt;] [--run-id &lt;ID&gt;] [--force|-f] [--self-review] [--self-implement] [--difficulty &lt;TRIVIAL|MODERATE|HARD&gt;] &lt;issue-N&gt;</code></td>
    </tr>
    <tr><td colspan="2">End-to-end implementation from the <strong>positional</strong> GitHub <code>&lt;issue-N&gt;</code> after <code>/design</code> has written <code>larch:plan</code> into that issue's body. Any decision to replace the target with two or more implementation issues must use <code>/umbrella</code>; the single scope-disposition follow-up and OOS disposition issues are not target partitions. Step 5 always runs <code>review-and-fix CLI</code> with the internal review panel (no public <code>--panel</code> argv): hard ceiling of <strong>2</strong> for every tier, TRIVIAL singles, MODERATE pairs, HARD pairs with the Codex review role, pruning on round-1 productivity for round 2, <strong>three static specialists per vendor</strong> plus at most one dynamic archetype pair. Reviewer dispatch uses <code>--no-fallback</code>, so missing vendors drop rows instead of cross-vendor or Claude reviewer backfill. <code>--merge</code> enables CI, then uses the default-branch merge queue when enabled or the existing direct merge otherwise; <code>--forked</code> is mutually exclusive with <code>--merge</code>. Use <code>--force</code> (or <code>-f</code>) to skip the item 4 plan-adequacy audit and downgrade only the documented force Preflight gates to warn-and-proceed (default off; does not affect coder selection). <code>--self-review</code> skips the external panel for a thorough inline self-review at Step 5. <code>--self-implement</code> forces <code>coder=claude</code>, independent of <code>--force</code> (default off). Preflight audit refusal exits <strong>3</strong> (distinct from flag/plan hard errors exit <strong>2</strong>).</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#issue"><code>/issue</code></a></td>
      <td><code>[--input-file FILE] [--intra-batch-deps-file FILE] [--blocked-by-issue N] [--title-prefix PREFIX] [--label LABEL]... [--body-file FILE] [--dry-run] [--no-dedup] [--no-dep-llm] [--sentinel-file PATH] [&lt;issue description or title&gt;]</code></td>
    </tr>
    <tr><td colspan="2">Create one or more GitHub issues with LLM-based semantic duplicate detection and always-on inter-issue blocker-dependency analysis. <code>--no-dedup</code> skips both passes; <code>--no-dep-llm</code> keeps dedup but skips the LLM dependency pass. Batch/wiring flags (<code>--input-file</code>, <code>--body-file</code>, <code>--blocked-by-issue</code>, <code>--intra-batch-deps-file</code>, <code>--sentinel-file</code>) are used mainly by calling skills.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#umbrella"><code>/umbrella</code></a></td>
      <td><code>[--skip-approve|-s] [--no-dedup] &lt;issue-N | description&gt;</code></td>
    </tr>
    <tr><td colspan="2">Create or resume a flat <code>[UMBRELLA]</code> issue whose direct leaves are durable native sub-issues that block it. One approval precedes all mutation; <code>--skip-approve</code>/<code>-s</code> follows the same proposal, sentinel, and graph-verification path. A record-less umbrella is adopted only after typed reads prove it has no direct sub-issues and no open blockers; closed blocker bodies are not read. A nested <code>/design</code> or <code>/implement</code> split may supply its already-approved exact batch and dependency graph; <code>/umbrella</code> persists that proposal, runs deduplicating <code>/issue</code> filing, converts the managed original atomically, and writes the parent completion sentinel only after verification. A resume reconciles only an exact title/body match before creating anything else, and never nests umbrellas.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#learn-from-bugs"><code>/learn-from-bugs</code></a></td>
      <td><code>[-n COUNT] [--state closed|open|all] [--repo OWNER/REPO --root PATH] [--search QUERY] [--zones a,b] [--full] [--file|-s] [verbal description]</code></td>
    </tr>
    <tr><td colspan="2">Mine a repository's closed bug reports for recurring root-cause patterns, then propose preventions ranked by mechanical enforceability: lint rules, architectural invariants (including hook-contract best-home), guideline entries, regression tests, and issues to file for still-broken code. Report-only by default and approval-gated for apply follow-ups: it compresses each body to a compact root-cause digest with best-effort origin attribution, maps every recurring principle to the repo's existing coverage (guidelines, invariants, hooks, and lints) before proposing the residual gap, and runs the synthesis inline with no sub-agent fan-out. A durable scan marker makes the default search incremental; <code>--full</code> re-mines its prior window. The report opens Section 2 with a generated origin distribution (counts, percentages, referenced <code>#origin -&gt; #current</code> chains, regression ratio) and marks guideline-only residuals with a prose-only prevention warning. <code>--zones "a,b"</code> scopes mining to an OR-group topical query; <code>--file</code> / <code>-s</code> groups all six residual proposal categories, computes caller-supplied dependency edges from declared proposal dependencies and shared implementation files, and forwards non-empty edges through both <code>/issue</code> passes. Filing keeps <code>/issue</code>'s semantic dependency analysis enabled, requires no separate approval prompt, and does not apply proposed changes directly. Unrecognized tokens remain verbal search text; <code>-f</code> is not a filing alias.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#pause"><code>/pause</code></a></td>
      <td></td>
    </tr>
    <tr><td colspan="2">Pause a running <code>/design</code>; saves state to GitHub for cross-session resume when run-log storage is configured. Source: <a href="skills/pause/SKILL.md"><code>skills/pause/SKILL.md</code></a>.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#rejected-analysis"><code>/rejected-analysis</code></a></td>
      <td><code>--n DAYS</code></td>
    </tr>
    <tr><td colspan="2">Recover verified real rejected code-review findings from the synchronized run-log cache and file issues by default. Security-sensitive findings are not public-filed, OOS-deferred findings are excluded, and the stable <code>finding_hash</code> uses file plus concern only, excluding run metadata and filesystem state.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#report-tokens"><code>/report-tokens</code></a></td>
      <td><code>--skill &lt;design|implement&gt; [--no-issue] [--no-plot] [--run-id &lt;ID&gt;]</code></td>
    </tr>
    <tr><td colspan="2">Analyze structured token reports from the synchronized run-log cache with Rust-owned `scripts/larch.sh report-tokens analyze`, price Claude/Codex/Cursor runs through `larch_core::report::RATE_TABLE`, plot skill-aware trends, and print cost-reduction suggestions.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#research"><code>/research</code></a></td>
      <td><code>[--no-issue] &lt;research question or topic&gt;</code></td>
    </tr>
    <tr><td colspan="2">Collaborative best-effort read-only research with the fixed-shape topology documented in the research skill — planner pre-pass, <a href="docs/topology.md#research.lanes">Codex-first research lanes</a> by angle, and the <a href="docs/topology.md#research.validation_panel">validation panel</a>. Every run includes unconditional citation validation (HEAD-fetches cited URLs under SSRF guards, validates DOIs, spot-checks file:line refs) emitted as a fail-soft PASS / FAIL / UNKNOWN ledger spliced into the final report.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#review"><code>/review</code></a></td>
      <td><code>[--diff] [--subagent] [--dynamic-archetypes &lt;N&gt;] [--session-env &lt;path&gt;] [--step-prefix &lt;prefix&gt;] [--difficulty &lt;TRIVIAL|MODERATE|HARD&gt;] [&lt;description&gt;]</code></td>
    </tr>
    <tr><td colspan="2">Code review with the specialist panel described in <code>docs/review-agents.md</code>. <code>--diff</code>: review branch changes and implement fixes. <code>&lt;description&gt;</code>: review existing code; description mode records voting outcomes and OOS artifacts locally — file follow-up issues with <code>/issue</code> when you want GitHub tracking. <code>--subagent</code>, <code>--dynamic-archetypes</code>, <code>--difficulty</code>, <code>--session-env</code>, and <code>--step-prefix</code> are used mainly by <code>/implement</code> Step 5.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#review-and-fix"><code>/review-and-fix</code></a></td>
      <td><code>--findings-file &lt;path&gt; [--session-env &lt;path&gt;] [--review-tmpdir &lt;path&gt;]</code></td>
    </tr>
    <tr><td colspan="2">Apply accepted review findings as code fixes via Codex/Cursor/Claude-subagent dispatch. Internal sub-skill invoked by <code>/review</code> in diff mode and by <code>/implement</code> Step 5; not a standalone user entry point.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#set-up-forked-open-source-repo"><code>/set-up-forked-open-source-repo</code></a></td>
      <td><code>--upstream &lt;owner/repo&gt; --fork &lt;owner/repo&gt; [--mirror-confirmed] [--init-submodules]</code></td>
    </tr>
    <tr><td colspan="2">Configure the current checkout for upstream/fork OSS contribution: verify the fork, optionally sync it from upstream, rewire remotes, disable upstream pushes, and set <code>main</code> tracking.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#status"><code>/status</code></a></td>
      <td><em>(none)</em></td>
    </tr>
    <tr><td colspan="2">Print the current larch version and health status of external vendor tools (Codex and Cursor), using the same probe machinery as <code>/implement</code>.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#upgrade-larch"><code>/upgrade-larch</code></a></td>
      <td><code>[--run-id &lt;ID&gt;]</code></td>
    </tr>
    <tr><td colspan="2">Preflight the latest immutable release, refresh the runtime-only plugin install, bootstrap its matching executable, and verify the new cache root without deleting Claude-managed versions.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#voter-calibration"><code>/voter-calibration</code></a></td>
      <td><code>[--log-root DIR] [--min-votes N] [--outlier-threshold R] [--high-severity-threshold R] [--out FILE]</code></td>
    </tr>
    <tr><td colspan="2">Measure voter agreement and chronic outlier voters from the synchronized run-log cache through its Rust-owned <code>scripts/larch.sh</code> command. Diagnostic only; does not affect spawning, thresholds, tokens, or reviewer points.</td></tr>
  </tbody>
</table>

### Private skills

Dev-only: not shipped with the plugin; runnable only inside the larch source tree.

<table>
  <thead>
    <tr><th>Name</th><th>Arguments</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><a href="docs/skills.md#agnix-fix"><code>/agnix-fix</code></a></td>
      <td><code>&lt;upstream-issue-number&gt; [extra-flags...]</code></td>
    </tr>
    <tr><td colspan="2">Fix an open <code>agent-sh/agnix</code> issue end-to-end via fork-CI dry-run: fetch the upstream issue body, provision the <code>skip-changelog</code> label on the fork, then forward to <code>/implement --forked</code> with the positional upstream issue number.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#analyze-bugs"><code>/analyze-bugs</code></a></td>
      <td><code>[-n COUNT] [--deep-max M] [--deep-model sonnet|opus|fable] [--refresh] [--sample K] [--repo owner/name]</code></td>
    </tr>
    <tr><td colspan="2">Dev-only verification of recent filed <code>[BUG]</code> issues. Defaults to <code>-n 200</code>, keeps compact durable state under the local XDG state directory, prints a report by default, and offers one combined follow-up issue only after approval.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#validate-merged"><code>/validate-merged</code></a></td>
      <td><code>[--max-merges N] [--repo owner/name]</code></td>
    </tr>
    <tr><td colspan="2">Dev-only validation of recent merged changes for possible unfiled bugs. Defaults to the previous 48 hours and at most 20 merges; durable state stays under the local XDG state directory.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#analyze-issues"><code>/analyze-issues</code></a></td>
      <td><code>[--limit N] [--span-days N] [--top-K N] [--categories=auto|default] [--log-root PATH] [--repo OWNER/REPO] [--lenient] [--ground-truth-verdict] [--since-date DATE] [--min-runs N] [--min-larch-version VERSION]</code></td>
    </tr>
    <tr><td colspan="2">Generate a backlog-and-process insight report from a repo's GitHub issues. By default it synchronizes the repository-scoped run-log cache once; <code>--log-root</code> selects an offline or operator-provided corpus. Verdict mode skips the full backlog report, emits a filtered corpus block with explicit gate PASS/FAIL, and gates token allocation on a post-<code>52.1.0</code>, post-<code>2026-06-26</code>, incentivized-era realized-outcome corpus with strict <code>started_at</code> eligibility and a mechanical #5544 shipped check: closed with <code>closedByPullRequestsReferences</code>, not <code>NOT_PLANNED</code>.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#audit-runs"><code>/audit-runs</code></a></td>
      <td><code>--skill &lt;design|implement&gt; [&lt;verbal-description&gt;] [--repo owner/name] [--allow-concurrent]</code></td>
    </tr>
    <tr><td colspan="2">Audit recently-merged larch run logs for anomalies, file the chain-of-history audit-report issue, and propose bug-issue follow-ups that require explicit user direction before any filing.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#larch-size"><code>/larch-size</code></a></td>
      <td><em>(none)</em></td>
    </tr>
    <tr><td colspan="2">Report larch repository line counts for Bash, Python, Rust, and Markdown. Python and Rust counts split production from test lines. Also report <code>larch-logs</code> size breakdowns. Takes no flags.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#rebalance-tests"><code>/rebalance-tests</code></a></td>
      <td><code>[--kind {harness,python,all}] [--repo owner/name] [--n-runs N] [--branch-prefix PREFIX] [--n-verify-runs N] [--n-python-shards N] [--balance-threshold SECONDS] [--max-shard-wall-clock SECONDS] [--experimental-wall-clock-override NOTE] [--compile-affinity TARGET=GROUP:SECONDS] [--workflow FILE] [--baseline-branch BRANCH] [--dry-run]</code></td>
    </tr>
    <tr><td colspan="2">Rebalance CI test harness shards, Python unit-test shards, or both through the checked Rust workflow; create one PR and verify exact CI runs. Harness verification fails closed on incomplete evidence, wall-clock or runner-cost regression; Python timing verification also fails closed.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#release"><code>/release</code></a></td>
      <td><code>[--dry-run] [--skip-approve|-s] [--bump major|minor|patch] [--repo OWNER/REPO]</code></td>
    </tr>
    <tr><td colspan="2">Operator-run release cut (model cannot auto-invoke): gather merged PRs, create a version candidate, merge it through the normal queue, tag the resulting <code>main</code> commit, validate the complete attested asset set on a draft GitHub Release, publish it as an immutable release, verify it, promote it to Latest, then run <code>/upgrade-larch</code>.</td></tr>
  </tbody>
</table>

### Non-skill entrypoints

<table>
  <thead>
    <tr><th>Name</th><th>Arguments</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><a href="docs/installation-and-setup.md#rust-executable-bootstrap"><code>scripts/larch.sh</code></a></td>
      <td><code>&lt;larch domain&gt; &lt;verb&gt; [arguments...]</code></td>
    </tr>
    <tr><td colspan="2">Verify and install the exact release-matched Rust executable when needed, then replace the shim process with it. Local <code>--plugin-dir</code> checkouts require an explicit <code>LARCH_BINARY</code>.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#relevant-checks-script"><code>python/cli.py checks run-relevant</code></a></td>
      <td><code>--site &lt;site&gt; [--tmpdir DIR] [--repo-root DIR] [--allow-skip]</code></td>
    </tr>
    <tr><td colspan="2">Consumer-provided validation entrypoint (not a SlashCommand skill). Orchestrators call it through <code>python/cli.py checks run-relevant --site &lt;site&gt; --tmpdir &lt;tmpdir&gt;</code>. <strong>Not part of the plugin surface; each consuming repo provides its own executable script.</strong></td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/skills.md#changed-path-rust-clippy-selector"><code>python/cli.py checks rust-clippy</code></a></td>
      <td><code>--repo-root DIR (--changed-from-git | PATH...)</code></td>
    </tr>
    <tr><td colspan="2">Bounded local Rust selector: maps changed paths to the smallest safe default-feature Clippy package or target set. It is used by the local pre-commit hook and <code>make rust-check</code>; CI owns exhaustive Rust checks.</td></tr>
    <tr><td colspan="2"><hr></td></tr>
    <tr>
      <td><a href="docs/migration-governance.md"><code>scripts/larch.sh issue migration-audit</code></a></td>
      <td><code>--repo owner/name --chief N [--output FILE] [--table-output stderr|stdout|none]</code></td>
    </tr>
    <tr><td colspan="2">Read-only aggregate for migration plans, blockers, owners, leases, command migration, clean-install coverage, and production runtime escape hatches. Emits stable JSON plus an optional count table.</td></tr>
  </tbody>
</table>

See [docs/skills.md](docs/skills.md) for full details on each skill.

## Aliases

Shortcut skills shipped with the plugin. Each alias forwards to an existing skill with preset flags.

| Alias | Equivalent |
|---|---|
| [`/im`](docs/skills.md#im) | `/implement --merge` (same public flags as `/implement`; requires positional `<issue-N>`) |
| [`/f`](docs/skills.md#f) | `/implement --force --self-review --self-implement` (same public flags as `/implement`; requires positional `<issue-N>`) |
| [`/fm`](docs/skills.md#fm) | `/implement --force --self-review --self-implement --merge` (same as `/f --merge`; requires positional `<issue-N>`) |
