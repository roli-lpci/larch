"""Structure pins for complete-umbrella phase isolation."""

from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
REFERENCES = REPO_ROOT / "skills" / "complete-umbrella" / "references"
PHASES = (
    "recon-design.md",
    "implement.md",
    "adversarial-review.md",
    "ship.md",
)


def _read(name: str) -> str:
    return (REFERENCES / name).read_text(encoding="utf-8")


def test_every_primary_phase_loads_the_shared_context_economy_contract() -> None:
    for phase in PHASES:
        assert (
            "Read `phase-common.md` in this directory in full before acting."
            in _read(phase)
        )

    common = _read("phase-common.md")
    assert "Set `head_limit` on every `Grep` call." in common
    assert "`sed -n`, `grep -n`, or `grep -rn`" in common
    assert "Put independent tool calls in one assistant message." in common
    assert "Put any handoff over 2,000 tokens" in common
    assert "tail -20" in common


def test_implement_and_review_inputs_are_phase_scoped() -> None:
    implementation = _read("implement.md")
    assert (
        "Read only `$SESSION_TMPDIR/design-brief.md` and `$SESSION_TMPDIR/leaf-issue.md`"
        in implementation
    )
    assert "Do not repeat broad repository exploration." in implementation

    review = _read("adversarial-review.md")
    assert "Start from only `$SESSION_TMPDIR/design-brief.md`" in review
    assert "repository-wide stale-caller sweep" in review
    assert "asserts a real success path executed" in review


def test_ship_is_deterministic_and_fixer_is_failure_only() -> None:
    ship = _read("ship.md")
    assert "complete-umbrella ship-leaf" in ship
    assert "refresh CI once every 300 seconds" in ship
    assert "`ci_failed`" in ship
    assert "needs-orchestrator-finalize" not in ship
    assert "`needs_conflict_fix`" in ship
    assert "conflict-fix.md" in ship
    assert "Do not spawn a CI fixer when checks are pending or green." in ship
    assert (
        "Do not spawn a conflict fixer unless the driver returned `needs_conflict_fix`."
        in ship
    )
    assert "The driver's persisted state enforces the fix-attempt cap." in ship
    assert "The driver's persisted state enforces the conflict-fix attempt cap." in ship

    conflict = _read("conflict-fix.md")
    assert "SHIP_STATUS=needs_conflict_fix" in conflict
    assert "MODE=conflict" in conflict
    assert "caller_kind=ship_pr_pre_push" in conflict
    assert "larch:ci-fixer" in conflict


def test_managed_leaf_phases_persist_and_verify_gate_evidence() -> None:
    recon = _read("recon-design.md")
    assert "valid durable plan" in recon
    assert "named-block write" in recon
    assert "--marker plan" in recon
    assert "before it adds `[IMPLEMENTING]`" in recon

    review = _read("adversarial-review.md")
    assert "--mode line-budget" in review
    assert "RUST_LINE_BUDGET_STATUS=over-limit" in review
    assert "automatic continue-with-warning path" in review
    assert "do not edit or publish the" in review


def test_recon_routes_large_or_malformed_plans_before_implementation() -> None:
    recon = _read("recon-design.md")
    assert "plan-block read" in recon
    assert "Do not replace or republish it." in recon
    assert "MALFORMED=<reason>" in recon
    assert "SHIP_STATUS=needs-design" in recon
    assert "PHASE_STATUS=needs-design" in recon
    assert "HANDOFF_FILE=needs-design.md" in recon
    assert "Never add `oversize_override: operator`" in recon

    skill = (REPO_ROOT / "skills" / "complete-umbrella" / "SKILL.md").read_text(
        encoding="utf-8"
    )
    needs_design = skill.index("CHILD_FAILURE_CLASS=needs-design")
    reset = skill.index("resets only a stale active leaf title", needs_design)
    assert needs_design < reset


def test_top_level_continues_after_an_over_limit_budget_warning() -> None:
    skill = (REPO_ROOT / "skills" / "complete-umbrella" / "SKILL.md").read_text(
        encoding="utf-8"
    )
    assert "needs-orchestrator-finalize" not in skill
    assert "finalize-budget-deviation" not in skill
    assert "independently measured advisory" in skill
    assert "continues through the ordinary merge path" in skill


def test_top_level_recovers_only_an_exact_remote_done_orphan() -> None:
    skill = (REPO_ROOT / "skills" / "complete-umbrella" / "SKILL.md").read_text(
        encoding="utf-8"
    )
    assert "complete-umbrella recover-orphaned-child" in skill
    assert "BGJOB_RC=orphaned" in skill
    assert "CHILD_RECOVERED=true" in skill
    assert "already closed with its exact `[DONE]`" in skill
    assert "Do not wait, sleep, or retry the recovery." in skill


def test_phase_return_contract_uses_basename_handoffs() -> None:
    common = _read("phase-common.md")
    assert "Prefer `HANDOFF_FILE=<basename under $SESSION_TMPDIR>`" in common
    assert "Do not echo driver stdout, `SHIP_STATUS`" in common

    recon = _read("recon-design.md")
    assert "HANDOFF_FILE=design-brief.md" in recon
    assert "Do not echo `SHIP_STATUS`" in recon
    assert "<absolute path to design-brief.md>" not in recon

    for phase, basename in (
        ("implement.md", "implementation-summary.md"),
        ("adversarial-review.md", "review-summary.md"),
        ("ship.md", "ship-summary.md"),
        ("ci-fix.md", "ci-fix-round-<N>.md"),
        ("conflict-fix.md", "conflict-fix-round-<N>.md"),
    ):
        text = _read(phase)
        assert f"HANDOFF_FILE={basename}" in text
        assert "<absolute path" not in text

    ship = _read("ship.md")
    assert "ignore surrounding narration and cosmetic `HANDOFF_FILE` path slips" in ship
    assert "re-spawn that fixer in a fresh context up to two additional times" in ship


def test_whole_leaf_loop_bgjob_binds_one_durable_session_owner() -> None:
    skill = (REPO_ROOT / "skills" / "complete-umbrella" / "SKILL.md").read_text(
        encoding="utf-8"
    )
    start = (
        'LARCH_CLAUDE_PID="$COMPLETE_UMBRELLA_OWNER_PID" '
        '"${CLAUDE_PLUGIN_ROOT}/scripts/larch.sh" bgjob start'
    )
    assert skill.count(start) == 1
    assert "STEP=complete-umbrella-leaves" in skill
    assert "complete-umbrella run-leaves" in skill
    assert "complete-umbrella next" not in skill
    assert "complete-umbrella run-child" not in skill
    assert "complete-umbrella verify-child" not in skill
    assert "uses that same graph to verify the prior child and select the next leaf" in skill
    for key in ("FAILED_STEP", "FAILED_LEAF", "FAILURE_REASON"):
        assert key in skill
    assert "wait lease" in skill
    assert "--max-wait-s 7200" in skill
    assert "run_in_background: true" in skill
    assert "330000" not in skill
    wait_contract = (
        REPO_ROOT / "skills" / "shared" / "bgjob-wait.md"
    ).read_text(encoding="utf-8")
    assert 'LARCH_CLAUDE_PID="$PPID"' in wait_contract
    assert "wait lease" in wait_contract


def test_top_level_rehydrates_the_durable_run_pointer_before_start() -> None:
    skill = (REPO_ROOT / "skills" / "complete-umbrella" / "SKILL.md").read_text(
        encoding="utf-8"
    )
    resume = skill.index("complete-umbrella resume")
    setup = skill.index("session setup", resume)
    start = skill.index("complete-umbrella start", setup)
    assert resume < setup < start
    assert '--claude-pid "$COMPLETE_UMBRELLA_OWNER_PID"' in skill
    assert "RESUME_ACTION=wait" in skill
    assert "RESUME_ACTION=reselect" in skill
    assert "without truncating a file or starting another bgjob" in skill
    assert "A resumed `wait` has no new start marker" in skill
    assert "Set `RESUME_ACTION=reselect`, then return immediately to Step 1" in skill
    assert "complete-umbrella clear-pointer" in skill
    assert "POINTER_CLEARED=true" in skill
