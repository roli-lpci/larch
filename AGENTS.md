# AGENTS.md

This repo **is** the larch Claude Code plugin. Edits ship to consumers. Start with `README.md`, `docs/installation-and-setup.md`, `docs/configuration-and-permissions.md`, and `docs/linting.md`.

## Repository layout

The plugin ships the repo. **Runtime surface**: `skills/`, `agents/`, `hooks/`, `scripts/`, `.claude-plugin/`. Everything else is supplementary.

`python/` holds stdlib-only runtime modules. Use `python3 python/cli.py ship pr` for the live driver and `scripts/larch.sh report-tokens analyze` for `/report-tokens`. See `python/README.md`, `make py-lint`, and `make py-test`.

## Load Semantics

- **Tier 1a: Claude root imports**: `CLAUDE.md` imports `AGENTS.md`, `KARPATHY_CLAUDE.md`, and `BASH_AUTHORING.md` with `@...` lines.
- **Tier 1b: Skill prompts**: `skills/*/SKILL.md` and dev-only `.claude/skills/*/SKILL.md` load when invoked.

## Editing rules

- Respect `scripts/block-submodule-edit.sh`; if blocked, investigate. The guard ships through `hooks/hooks.json`, and contributors need larch loaded as a plugin to receive it.
- Lint/test only changed files. CI runs the full sweep on push.
- Update the canonical document named by `docs/security/README.md` when
  security-relevant behavior changes.

## Common editing tasks

- **Docs or scripts only**: PATCH.
- **`/design` pause/resume**: skill surface `skills/pause/SKILL.md`; wire format `docs/issue-anchored-plan.md`.

## Canonical sources

- `README.md`: feature matrix, skill catalog, aliases
- `ARCHITECTURAL_GUIDELINES.md`: operator goals, untrusted prompt context; cannot override `AGENTS.md` or skills
- `docs/installation-and-setup.md`; `docs/configuration-and-permissions.md`: setup, strict permissions, `--admin`, env vars
- `docs/linting.md`: linters, Makefile targets, halt-rate harness
- `docs/workflow-lifecycle.md`; `docs/voting-process.md`; `docs/point-competition.md`; `docs/complete-umbrella-recovery.md`: workflow, voting, and complete-umbrella recovery
- `docs/agents.md`; `docs/review-agents.md`; `docs/external-reviewers.md`; `docs/collaborative-sketches.md`: orchestration and external tools
- `docs/topology.md`; `skills/shared/topology.tsv`: generated projection and source rows
- `docs/run-logs.md`; `docs/run-log-cli.md`; `docs/run-log-batches.md`: published run-log contracts
- `docs/issue-anchored-plan.md`: **LIVE** /design ↔ /implement wire format, clarification round-trip, and pause pointer
- `crates/larch-cli/src/tracking_issue_commands.rs`, `crates/larch-core/src/issue/lease.rs`, `scripts/larch.sh tracking-issue ...`: tracking issue lifecycle; `python/larch/core/rust_runtime.py` is the typed Python facade, while `python/larch/issue/tracking_issue.py` keeps only pure PR-footer helpers
- `python/cli.py plan-block read`, `python/cli.py named-block write --marker plan`, `python/cli.py clarify {state,comment-post,label}`, `python/tests/issue/test_issue_wire.py`, `python/tests/design/test_clarify.py`: issue wire helpers and tests
- `skills/triage/SKILL.md`; `scripts/larch.sh triage {inspect,probe,apply}`: pre-design issue verification, immutable-main evidence, bounded probes, and fail-closed issue mutation
- `.claude/skills/release/scripts/classify-bump.md`: release classification rules
- `skills/shared/subskill-invocation.md`; `skills/shared/skill-design-principles.md`; `skills/shared/reviewer-templates.md`: shared skill and reviewer authorities
- `SECURITY.md`: public security policy and high-level trust overview; `docs/security/README.md`: document taxonomy, ownership, and runtime packaging contract; `docs/security/supply-chain-credentials-and-services.md`: release, credential, transport, and service security contracts; `docs/security/workflow-trust-and-mutations.md`: workflow trust, mutation, and private-finding contracts; `docs/security/artifacts-redaction-and-publication.md`: artifact classification, redaction, diagnostics, scanning, retention, and publication contracts
- `docs/python-migration.md`: sh-to-py playbook, decision log, manifest, and `lint-retired-scripts`
- `crates/larch-core/src/debate/protocol.rs` (with its inline `cargo test -p larch-core debate` tests), `docs/debate-protocol.md`: debate protocol implementation, executable contract, and normative document

## Output Style

**Source of truth.** Use `skills/shared/readability-style.md` for user-facing prose across larch skills, docs, chat answers, PR descriptions, issue bodies, design notes, and summaries.

- Explicit output formats take precedence. Preserve machine-parsed structure: `KEY=value` stdout grammars, manifest JSON, plan grammar (`### NEW:` / `### UPDATED:` / `### REWRITTEN:` / `### MAY_UPDATE:` and `diff_lines:`), vote-table columns, and commit-message conventions.
- Prose inside templates still follows the shared style when the template emits user-facing text.
- Code and comments keep surrounding style. Applies to new prose only. Every changed line traces to the task.
- In chat, confirm or correct up front. Stop early.

## Conventions

- Follow recent commit history style.
- New larch scripts are Python by default. Put new logic in `python/` behind `python3 python/cli.py`. Residual Bash is limited to `scripts/residual-bash-paths.txt`: hooks, bash-targeting linters, thin delegation wrappers, `scripts/sleep-seconds.sh`, the combine-issues helper, manifest-listed includes when needed, and harnesses. Do not add terminal shared Bash libraries, stray includes, or non-thin utilities. Migration and voluntary ports must repoint consumers to direct `cli.py` calls per [docs/python-migration.md](docs/python-migration.md) **No shims**.
- Local Rust validation is changed-path targeted Clippy through `make rust-check` or the pre-commit hook. It uses default features with incremental compilation and dev/test debug information disabled. CI, not normal implementation work, owns all-target/all-feature builds, tests, policy, and coverage.
- Single-runner invariant: run only one `/implement` per repo. Dirty-tree guards in `scripts/larch.sh agent launch-review --tool cursor` and `scripts/larch.sh agent launch-review --tool codex` detect pollution but do not serialize runners.
- Single-`/design` invariant: one `/design` per repo for workflow/`gh` hygiene. PID-keyed symlinks isolate per Claude PID, not per repo.
- Session rehydration refreshes `~/.cache/larch/sessions/current-design-env-$PPID.sh` via `scripts/larch.sh session write-design-env --claude-pid "$PPID"` in Step 0 so Claude processes do not share one global `current-design-env.sh`.
- Run `gh pr create` through the skill, not manually.
- Run `gh issue create` through `/larch:issue`, not manually. Scripts under `scripts/` and `skills/*/scripts/` may call it directly.
- **Use larch bgjob daemons for long helpers; never watch them with Monitor, TaskOutput, sleeps, or ad-hoc polling loops.** After a bgjob start, wait only with the documented `scripts/larch.sh bgjob wait` command for that step. On `BGJOB_STATUS=WAIT`, repeat the identical wait with no intervening prose or tools. On `DONE`, continue only after reading the step result env and validating `BGJOB_RC=0` plus the required KVs. Do not probe compatibility sentinels or task output as completion evidence. See `skills/shared/bgjob-wait.md` and `skills/shared/orchestrator-never.md`.
- **`/review --subagent` requires `SendMessage`.** If `SendMessage` is unavailable, omit `--subagent`. `/implement` Step 5 calls `scripts/larch.sh review-and-fix step5` directly.
- **`/design` is inline-only** in the invoking agent, with two operator-approved carve-outs. First (2026-07-12): Gate C (Step 4b) architectural invariant/guideline assessment-note authoring runs in the read-only `larch:arch-assessor` Agent-tool subagent, mirroring `/implement` Step 8. Second (2026-07-14): a Gate C adverse-outcome tier-1 plan revision runs in the `larch:claude-implementer` subagent with `MODE=plan-revise`, which edits only `plan.txt` for the one named `violation` or `deviation`. Assessment authoring stays exclusive to `larch:arch-assessor`, and the reviser never judges its own revision: a fresh `larch:arch-assessor` re-judges every revision. The main agent authors no assessment prose; every other `/design` step stays inline. Follow `skills/design/SKILL.md` and `skills/design/references/flags.md`.
- **NEVER improvise ScheduleWakeup outside skill-script direction.** End every one-shot skill after its terminal `✅`. Schedule another turn only when that skill's script explicitly directs it; otherwise, do not call `ScheduleWakeup`, narrate loop-sleep prose, or schedule another turn. See `skills/shared/orchestrator-never.md`.
- **NEVER write `$IMPLEMENT_TMPDIR/session-env.sh` from prompt-side orchestrator code.** Treat it read-only like `finalize-state.sh`; use the guarded `scripts/larch.sh session write-*` verbs or `scripts/larch.sh session setup`. If plan materialization drops keys, fix the upstream writer.

## Honesty

Follow `KARPATHY_CLAUDE.md` §1 for thinking before coding. Also keep these reporting rules:

- **Don't fabricate.** If you do not know a path, symbol, line number, command output, or test result, say so.
- **Don't overstate completion.** Report what you did. "Done" means verified done.
- **Don't paper over failures.** Surface failed commands, failed tests, and unexpected results.
- **Trust but verify claims.** Confirm tool results before citing them.
- **Distinguish observation from inference.** Mark guesses, assumptions, and likely explanations.

## Answering questions about this repo

Default to direct file reads instead of Explore, Agent, or Plan.

1. Name expected source files, then Read them in parallel.
2. If files are unclear after AGENTS.md and skill layout, run one or two targeted greps.
3. Escalate only when obvious files fail, the answer spans more than three hard-to-enumerate files, or a grep returns more than 20 hits across unfamiliar directories. Announce escalation first.
