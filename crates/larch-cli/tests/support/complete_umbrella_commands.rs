use super::*;
use larch_adapters::github::OctocrabGitHubService;
use larch_core::{is_transient_claude_api_error, parse_claude_envelope};
use larch_test_support::{GitFixture, GitRepository, IssueServiceExchange, IssueServiceStub};
use serde_json::{Value, json};
use std::collections::VecDeque;

const UMBRELLA: u64 = 40;
const LEAF: u64 = 41;
const GAP: u64 = 42;
const PROPOSAL_BODY: &str = "Requirements\n<!-- larch:umbrella-proposal v1 -->";
const BEFORE: &str = "2026-08-01T00:00:00Z";
const AFTER: &str = "2026-08-01T00:01:00Z";
const CLOSED_AT: &str = "2026-08-01T00:02:00Z";

fn service(exchanges: Vec<IssueServiceExchange>) -> (OctocrabGitHubService, IssueServiceStub) {
    let server = IssueServiceStub::start(exchanges).expect("start issue service stub");
    let client = octocrab::Octocrab::builder()
        .personal_token(String::from("test-token"))
        .base_uri(server.base_url())
        .expect("stub base URI")
        .upload_uri(server.base_url())
        .expect("stub upload URI")
        .build()
        .expect("stub client");
    (OctocrabGitHubService::with_test_client(client), server)
}

fn response(status: u16, body: impl AsRef<[u8]>) -> IssueServiceExchange {
    IssueServiceExchange::any_json(status, body.as_ref().to_vec())
        .expect("valid issue service response")
}

fn issue_json(
    number: u64,
    id: u64,
    title: &str,
    body: &str,
    state: &str,
    updated_at: &str,
) -> String {
    let mut issue: Value = serde_json::from_str(include_str!(
        "../../../larch-adapters/fixtures/github_issue.json"
    ))
    .expect("valid issue fixture");
    issue["id"] = json!(id);
    issue["number"] = json!(number);
    issue["title"] = json!(title);
    issue["body"] = json!(body);
    issue["state"] = json!(state);
    issue["updated_at"] = json!(updated_at);
    issue["url"] = json!(format!("https://api.github.com/repos/o/r/issues/{number}"));
    issue["html_url"] = json!(format!("https://github.com/o/r/issues/{number}"));
    issue["labels"] = json!([]);
    issue.to_string()
}

fn refs(values: &[(u64, u64, &str)]) -> String {
    Value::Array(
        values
            .iter()
            .map(|(number, id, state)| json!({ "number": number, "id": id, "state": state }))
            .collect(),
    )
    .to_string()
}

fn repository() -> GitHubRepositoryRef {
    GitHubRepositoryRef::new("o", "r").expect("valid repository")
}

fn run_pointer(tmpdir: &Path, session_pid: u32) -> CompleteUmbrellaRunPointer {
    CompleteUmbrellaRunPointer {
        current_leaf: Some(LEAF),
        current_step: RunPointerStep::Launch,
        ..CompleteUmbrellaRunPointer::initial(
            String::from("o/r"),
            UMBRELLA,
            fs::canonicalize(tmpdir).expect("canonical run tmpdir"),
            session_pid,
        )
    }
}

fn closed_graph(parent: &str, leaf: &str) -> Vec<IssueServiceExchange> {
    closed_graph_with_parent_blockers(parent, leaf, &[(LEAF, 410, "closed")])
}

fn closed_graph_with_parent_blockers(
    parent: &str,
    leaf: &str,
    parent_blockers: &[(u64, u64, &str)],
) -> Vec<IssueServiceExchange> {
    vec![
        response(200, parent),
        response(200, refs(&[(LEAF, 410, "closed")])),
        response(200, refs(parent_blockers)),
        response(200, leaf),
        response(200, "[]"),
    ]
}

fn open_graph(parent: &str, leaf: &str) -> Vec<IssueServiceExchange> {
    vec![
        response(200, parent),
        response(200, refs(&[(LEAF, 410, "open")])),
        response(200, refs(&[(LEAF, 410, "open")])),
        response(200, leaf),
        response(200, "[]"),
        response(200, "[]"),
    ]
}

fn attachment_exchanges(
    parent: &str,
    leaf: &str,
    leaf_ref: &str,
    parent_ref: &str,
) -> Vec<IssueServiceExchange> {
    vec![
        response(200, parent),
        response(200, "[]"),
        response(200, leaf),
        response(200, "[]"),
        response(404, "{}"),
        response(200, "[]"),
        response(201, "{}"),
        response(200, leaf_ref),
        response(200, parent),
        response(200, "[]"),
        response(201, "{}"),
        response(200, leaf_ref),
        response(200, parent),
        response(200, leaf_ref),
        response(200, leaf_ref),
        response(200, leaf),
        response(200, "[]"),
        response(200, "[]"),
        response(200, leaf_ref),
        response(200, parent),
        response(200, "[]"),
        response(200, leaf),
        response(200, "[]"),
        response(200, parent_ref),
        response(200, leaf_ref),
        response(200, parent),
        response(200, leaf_ref),
        response(200, parent),
        response(200, leaf_ref),
        response(200, leaf_ref),
        response(200, leaf),
        response(200, "[]"),
        response(200, "[]"),
        response(200, leaf_ref),
    ]
}

#[test]
fn repository_and_positive_issue_inputs_fail_closed() {
    assert!(parse_repository("owner/repo").is_ok());
    assert!(parse_repository("../repo").is_err());
    assert!(require_issue(1, "--issue").is_ok());
    assert!(require_issue(0, "--issue").is_err());
    assert!(require_operator(true).is_ok());
    assert!(require_operator(false).is_err());
    assert!(temporary_root(Path::new("/"), "root").is_err());
}

#[test]
fn command_dispatch_rejects_invalid_inputs_before_remote_work() {
    let files = || GapFileArguments {
        root: PathBuf::from("relative"),
        title_file: PathBuf::from("title"),
        body_file: PathBuf::from("body"),
    };
    let leaf = || LeafArguments {
        repository: String::from("o/r"),
        umbrella: UMBRELLA,
        leaf: LEAF,
    };
    let commands = [
        CompleteUmbrellaCommand::Start(StartArguments {
            repository: String::from("o/r"),
            issue: UMBRELLA,
            tmpdir: PathBuf::from("missing"),
            claude_pid: 1,
            operator_invoked: false,
        }),
        CompleteUmbrellaCommand::Resume(ResumeArguments {
            repository: String::from("o/r"),
            issue: 0,
            claude_pid: Some(1),
            operator_invoked: false,
        }),
        CompleteUmbrellaCommand::ClearPointer(ClearPointerArguments {
            repository: String::from("o/r"),
            issue: 0,
            tmpdir: PathBuf::from("missing"),
        }),
        CompleteUmbrellaCommand::Next(NextArguments {
            repository: String::from("o/r"),
            issue: 0,
            output_root: PathBuf::from("relative"),
            output: PathBuf::from("snapshot"),
        }),
        CompleteUmbrellaCommand::RunLeaves(RunLeavesArguments {
            repository: String::from("o/r"),
            repo_root: PathBuf::from("missing"),
            umbrella: UMBRELLA,
            model: String::from("unknown"),
            output_root: PathBuf::from("relative"),
            output: PathBuf::from("snapshot"),
            result_env: PathBuf::from("result"),
            operator_invoked: true,
        }),
        CompleteUmbrellaCommand::RunChild(RunChildArguments {
            repository: String::from("o/r"),
            repo_root: PathBuf::from("missing"),
            umbrella: UMBRELLA,
            leaf: LEAF,
            transient_attempt_count: 0,
            model: String::from("unknown"),
            output_root: PathBuf::from("relative"),
            output: PathBuf::from("output"),
            result_env: PathBuf::from("result"),
        }),
        CompleteUmbrellaCommand::VerifyChild(LeafArguments {
            umbrella: 0,
            ..leaf()
        }),
        CompleteUmbrellaCommand::RecoverOrphanedChild(RecoverOrphanedChildArguments {
            leaf: leaf(),
            root: PathBuf::from("relative"),
            result_env: PathBuf::from("result.env"),
        }),
        CompleteUmbrellaCommand::ResetLeaf(ResetLeafArguments {
            leaf: LeafArguments {
                umbrella: 0,
                ..leaf()
            },
            operator_invoked: false,
        }),
        CompleteUmbrellaCommand::ValidateGap(ValidateGapArguments {
            umbrella: 0,
            files: files(),
        }),
        CompleteUmbrellaCommand::AttachLeaf(AttachLeafArguments {
            leaf: leaf(),
            operator_invoked: false,
            files: files(),
        }),
        CompleteUmbrellaCommand::Finish(FinishArguments {
            repository: String::from("o/r"),
            issue: UMBRELLA,
            operator_invoked: false,
        }),
    ];

    for command in commands {
        assert_eq!(run(command), ExitCode::FAILURE);
    }
}

#[test]
fn child_terminal_status_accepts_only_complete_success_marker() {
    assert_eq!(
        child_terminal_status("summary\nCOMPLETE_UMBRELLA_CHILD_STATUS=complete\n"),
        Some(ChildResultStatus::Complete)
    );
    assert_eq!(
        child_terminal_status("summary\nCOMPLETE_UMBRELLA_CHILD_STATUS=needs-design\n"),
        Some(ChildResultStatus::NeedsDesign)
    );
    assert_eq!(
        child_terminal_status(
            "summary\nCOMPLETE_UMBRELLA_CHILD_STATUS=needs-orchestrator-finalize\n"
        ),
        None
    );
    assert!(ChildResultStatus::Complete.envelope_complete());
    assert!(!ChildResultStatus::NeedsDesign.envelope_complete());
    assert!(!ChildResultStatus::Failed.envelope_complete());
    assert_eq!(
        child_terminal_status("COMPLETE_UMBRELLA_CHILD_STATUS=failed\n"),
        None
    );
    assert_eq!(child_terminal_status("summary\n"), None);
}

#[test]
fn orphaned_child_recovery_requires_the_exact_transport_and_leaf_identity() {
    let valid =
        format!("BGJOB_RC=orphaned\nSTEP=complete-umbrella-leaf-{LEAF}\nCHILD_ISSUE={LEAF}\n");
    assert!(validate_orphaned_child_result(&valid, LEAF).is_ok());
    assert!(
        validate_orphaned_child_result(
            &format!("BGJOB_RC=timeout\nSTEP=complete-umbrella-leaf-{LEAF}\n"),
            LEAF,
        )
        .is_err()
    );
    assert!(
        validate_orphaned_child_result(
            &format!("BGJOB_RC=orphaned\nSTEP=complete-umbrella-leaf-{GAP}\n"),
            LEAF,
        )
        .is_err()
    );
    assert!(
        validate_orphaned_child_result(
            &format!("BGJOB_RC=orphaned\nSTEP=complete-umbrella-leaf-{LEAF}\nCHILD_ISSUE={GAP}\n"),
            LEAF,
        )
        .is_err()
    );
    assert!(
        validate_orphaned_child_result(
            &format!("BGJOB_RC=orphaned\nBGJOB_RC=0\nSTEP=complete-umbrella-leaf-{LEAF}\n"),
            LEAF,
        )
        .is_err()
    );

    let driver = format!(
        "BGJOB_RC=orphaned\nSTEP={RUN_LEAVES_STEP}\nNEXT_ACTION=verify\nCURRENT_LEAF={LEAF}\n"
    );
    assert!(validate_orphaned_child_result(&driver, LEAF).is_ok());
    assert!(
        validate_orphaned_child_result(
            &format!(
                "BGJOB_RC=orphaned\nSTEP={RUN_LEAVES_STEP}\nNEXT_ACTION=audit\nCURRENT_LEAF={LEAF}\n"
            ),
            LEAF,
        )
        .is_err()
    );
    assert!(
        validate_orphaned_child_result(
            &format!(
                "BGJOB_RC=orphaned\nSTEP={RUN_LEAVES_STEP}\nNEXT_ACTION=launch\nCURRENT_LEAF={GAP}\n"
            ),
            LEAF,
        )
        .is_err()
    );
}

#[test]
fn run_pointer_round_trip_rebind_and_ambiguity_fail_closed() {
    let pointer_root = tempfile::tempdir().expect("pointer root");
    let run_root = tempfile::tempdir().expect("run root");
    let store = RunPointerStore::at(pointer_root.path());
    let record = store
        .create(run_pointer(run_root.path(), 123))
        .expect("create run pointer");
    assert!(record.path.ends_with("current-complete-umbrella-123.env"));
    assert_eq!(
        CompleteUmbrellaRunPointer::parse(
            &fs::read_to_string(&record.path).expect("run pointer text")
        )
        .expect("parse run pointer"),
        record.state
    );
    let inconsistent = fs::read_to_string(&record.path)
        .expect("run pointer text")
        .replace("CURRENT_STEP=launch", "CURRENT_STEP=audit");
    assert!(CompleteUmbrellaRunPointer::parse(&inconsistent).is_err());

    let rebound = store.rebind(&record, 456).expect("rebind run pointer");
    assert!(rebound.path.ends_with("current-complete-umbrella-456.env"));
    assert!(!record.path.exists());
    assert_eq!(
        store
            .resume_candidate("o/r", UMBRELLA)
            .expect("resume candidate"),
        Some(rebound.clone())
    );
    assert!(store.resume_candidate("other/r", UMBRELLA).is_err());
    store.remove(&rebound).expect("remove run pointer");
    assert!(
        store
            .resume_candidate("o/r", UMBRELLA)
            .expect("missing candidate")
            .is_none()
    );

    let other_root = tempfile::tempdir().expect("other run root");
    store
        .create(run_pointer(run_root.path(), 111))
        .expect("first ambiguous pointer");
    let mut other = run_pointer(other_root.path(), 222);
    other.repository = String::from("other/r");
    store.create(other).expect("second ambiguous pointer");
    assert!(store.resume_candidate("o/r", UMBRELLA).is_err());
}

#[test]
fn resume_live_child_rebinds_the_wait_lease_to_the_new_session() {
    let pointer_root = tempfile::tempdir().expect("pointer root");
    let run_root = tempfile::tempdir().expect("run root");
    let store = RunPointerStore::at(pointer_root.path());
    let record = store
        .create(run_pointer(run_root.path(), 123))
        .expect("create run pointer");
    let rebound = store.rebind(&record, 456).expect("rebind run pointer");
    refresh_wait_lease_for_pid(
        &rebound.state.tmpdir,
        &rebound.state.bgjob_step,
        rebound.state.session_pid,
    )
    .expect("refresh rehydrated wait lease");
    let lease = fs::read_to_string(
        larch_core::wait_lease_path(&rebound.state.tmpdir, &rebound.state.bgjob_step)
            .expect("wait lease path"),
    )
    .expect("wait lease text");
    assert!(lease.contains("WAITER_PID=456\n"));
    assert_eq!(rebound.state.bgjob_step, RUN_LEAVES_STEP);
}

#[test]
fn resume_dead_child_result_preserves_its_failure_class_and_retry_count() {
    let run_root = tempfile::tempdir().expect("run root");
    let mut pointer = run_pointer(run_root.path(), 123);
    let text = format!(
        "CHILD_STATUS=failed\nCHILD_ISSUE={LEAF}\nCHILD_ENVELOPE_COMPLETE=false\nCHILD_TRANSIENT_ATTEMPT_COUNT=0\nCHILD_FAILURE_CLASS={COMPLETE_UMBRELLA_CHILD_FAILURE_TRANSIENT_API}\n"
    );
    let result = parse_durable_child_result(&text, LEAF, 0)
        .expect("durable child result")
        .expect("current attempt result");
    assert_eq!(result, DurableChildResult::TransientApi);
    let stale = parse_durable_child_result(&text, LEAF, 1).expect("stale result is valid");
    assert_eq!(stale, None);
    assert!(parse_durable_child_result(&text, LEAF, 2).is_err());
    pointer.transient_attempt_count = 1;
    let after_retry_checkpoint =
        decide_resume_recovery(&pointer, stale, Some(ResumeLeafState::Idle));
    assert_eq!(after_retry_checkpoint.action, ResumeAction::Reselect);
    assert_eq!(after_retry_checkpoint.transient_attempt_count, 1);

    pointer.transient_attempt_count = 0;
    let retry = decide_resume_recovery(&pointer, Some(result), Some(ResumeLeafState::Active));
    assert_eq!(retry.action, ResumeAction::Reselect);
    assert!(retry.reset_active_leaf);
    assert_eq!(retry.transient_attempt_count, 1);

    pointer.transient_attempt_count = MAX_TRANSIENT_CHILD_RETRIES;
    let exhausted = decide_resume_recovery(&pointer, Some(result), Some(ResumeLeafState::Active));
    assert_eq!(exhausted.action, ResumeAction::Failed);
    assert!(exhausted.reset_active_leaf);
    assert_eq!(
        result.failure_class(),
        Some(COMPLETE_UMBRELLA_CHILD_FAILURE_TRANSIENT_API)
    );
}

#[test]
fn resume_ignores_completed_bgjob_results_after_a_new_selection_checkpoint() {
    let run_root = tempfile::tempdir().expect("run root");
    let mut pointer = run_pointer(run_root.path(), 123);
    pointer.current_step = RunPointerStep::Select;
    assert_eq!(
        completed_bgjob_state(&pointer, true),
        ResumeBgjobState::None
    );
    pointer.current_step = RunPointerStep::Launch;
    assert_eq!(
        completed_bgjob_state(&pointer, true),
        ResumeBgjobState::Completed
    );

    let open_graph = driver_graph(&[(LEAF, GitHubIssueState::Open, false)]);
    assert!(audit_graph_requires_reselection(&open_graph).expect("active graph"));
    let closed_graph = driver_graph(&[(LEAF, GitHubIssueState::Closed, false)]);
    assert!(!audit_graph_requires_reselection(&closed_graph).expect("finished leaf graph"));

    let pointer_root = tempfile::tempdir().expect("pointer root");
    let store = RunPointerStore::at(pointer_root.path());
    let record = store
        .create(pointer.clone())
        .expect("create audit-loop pointer");
    checkpoint_reselection(&store, "o/r", UMBRELLA, &pointer.tmpdir)
        .expect("checkpoint new selection");
    let checkpoint = store
        .resume_candidate("o/r", UMBRELLA)
        .expect("selection candidate")
        .expect("selection pointer");
    assert_eq!(checkpoint.state.current_step, RunPointerStep::Select);
    assert_eq!(checkpoint.state.current_leaf, None);
    assert_eq!(checkpoint.state.transient_attempt_count, 0);
    assert_ne!(checkpoint.state, record.state);
}

#[test]
fn resume_stuck_title_reselects_and_reuses_the_original_handoff_root() {
    let pointer_root = tempfile::tempdir().expect("pointer root");
    let run_root = tempfile::tempdir().expect("run root");
    let handoff = run_root
        .path()
        .join(format!("complete-umbrella-leaf-{LEAF}"));
    fs::create_dir(&handoff).expect("leaf handoff root");
    fs::write(handoff.join("design-brief.md"), "durable work\n").expect("durable handoff");
    let store = RunPointerStore::at(pointer_root.path());
    let record = store
        .create(run_pointer(run_root.path(), 123))
        .expect("create run pointer");
    let rebound = store.rebind(&record, 456).expect("rebind run pointer");
    let decision = decide_resume_recovery(&rebound.state, None, Some(ResumeLeafState::Active));
    assert_eq!(decision.action, ResumeAction::Reselect);
    assert!(decision.reset_active_leaf);
    assert_eq!(
        rebound
            .state
            .tmpdir
            .join(format!("complete-umbrella-leaf-{LEAF}")),
        fs::canonicalize(handoff).expect("canonical handoff root")
    );
}

#[derive(Default)]
struct FakeRunLeavesOperations {
    graphs: VecDeque<Result<GraphState, String>>,
    syncs: VecDeque<Result<(), String>>,
    children: VecDeque<ChildAttempt>,
    reset_results: VecDeque<Result<(), String>>,
    result_writes: VecDeque<Result<(), String>>,
    snapshot_error: Option<String>,
    snapshots: usize,
    child_leaves: Vec<u64>,
    resets: Vec<u64>,
    results: Vec<RunLeavesEnvelope>,
    pointer_leaf: Option<u64>,
    pointer_transient_attempt_count: u8,
}

impl RunLeavesOperations for FakeRunLeavesOperations {
    fn read_graph(&mut self) -> Result<GraphState, String> {
        self.graphs
            .pop_front()
            .unwrap_or_else(|| Err("unexpected graph read".to_owned()))
    }

    fn write_snapshot(&mut self, _graph: &GraphState) -> Result<(), String> {
        if let Some(error) = self.snapshot_error.take() {
            return Err(error);
        }
        self.snapshots += 1;
        Ok(())
    }

    fn sync_main(&mut self) -> Result<(), String> {
        self.syncs
            .pop_front()
            .unwrap_or_else(|| Err("unexpected main synchronization".to_owned()))
    }

    fn run_child(&mut self, leaf: u64) -> ChildAttempt {
        self.child_leaves.push(leaf);
        self.children
            .pop_front()
            .unwrap_or_else(|| ChildAttempt::Failed("unexpected child launch".to_owned()))
    }

    fn reset_leaf(&mut self, leaf: u64) -> Result<(), String> {
        self.resets.push(leaf);
        self.reset_results.pop_front().unwrap_or(Ok(()))
    }

    fn write_result(&mut self, envelope: &RunLeavesEnvelope) -> Result<(), String> {
        if let RunLeavesEnvelope::Progress {
            leaf,
            transient_attempt_count,
            ..
        } = envelope
        {
            self.pointer_leaf = Some(*leaf);
            self.pointer_transient_attempt_count = *transient_attempt_count;
        }
        self.results.push(envelope.clone());
        self.result_writes.pop_front().unwrap_or(Ok(()))
    }

    fn transient_attempt_count(&self, leaf: u64) -> u8 {
        if self.pointer_leaf == Some(leaf) {
            self.pointer_transient_attempt_count
        } else {
            0
        }
    }
}

fn driver_graph(leaves: &[(u64, GitHubIssueState, bool)]) -> GraphState {
    let parent = GitHubIssue {
        id: 400,
        number: UMBRELLA,
        title: String::from("[IMPLEMENTING] [UMBRELLA] Ship it"),
        body: String::from(PROPOSAL_BODY),
        state: GitHubIssueState::Open,
        state_reason: String::new(),
        url: String::from("https://github.com/o/r/issues/40"),
        author: String::from("author"),
        labels: Vec::new(),
        comments: 0,
        created_at: String::from(BEFORE),
        closed_at: String::new(),
        updated_at: String::from(BEFORE),
        is_pull_request: false,
    };
    GraphState {
        parent: parent.clone(),
        leaves: leaves
            .iter()
            .map(|(number, state, implementing)| {
                let lifecycle = if *state == GitHubIssueState::Closed {
                    "[DONE] "
                } else if *implementing {
                    "[IMPLEMENTING] "
                } else {
                    ""
                };
                LeafState {
                    issue: GitHubIssue {
                        id: number * 10,
                        number: *number,
                        title: format!("{lifecycle}[LEAF OF {UMBRELLA}] Leaf {number}"),
                        state: *state,
                        ..parent.clone()
                    },
                    open_blockers: Vec::new(),
                }
            })
            .collect(),
        open_orphan_blockers: Vec::new(),
    }
}

#[test]
fn run_leaves_verifies_and_selects_from_one_fresh_graph_per_iteration() {
    let mut operations = FakeRunLeavesOperations {
        graphs: VecDeque::from([
            Ok(driver_graph(&[
                (LEAF, GitHubIssueState::Open, false),
                (GAP, GitHubIssueState::Open, false),
            ])),
            Ok(driver_graph(&[
                (LEAF, GitHubIssueState::Closed, false),
                (GAP, GitHubIssueState::Open, false),
            ])),
            Ok(driver_graph(&[
                (LEAF, GitHubIssueState::Closed, false),
                (GAP, GitHubIssueState::Closed, false),
            ])),
        ]),
        syncs: VecDeque::from([Ok(()), Ok(()), Ok(()), Ok(())]),
        children: VecDeque::from([ChildAttempt::Complete, ChildAttempt::Complete]),
        ..FakeRunLeavesOperations::default()
    };

    assert_eq!(execute_run_leaves(&mut operations), Ok(2));
    assert!(operations.graphs.is_empty());
    assert!(operations.syncs.is_empty());
    assert_eq!(operations.snapshots, 3);
    assert_eq!(operations.child_leaves, vec![LEAF, GAP]);
    assert_eq!(
        operations.results,
        vec![
            RunLeavesEnvelope::Progress {
                action: "launch",
                leaf: LEAF,
                completed: 0,
                transient_attempt_count: 0,
            },
            RunLeavesEnvelope::Progress {
                action: "verify",
                leaf: LEAF,
                completed: 0,
                transient_attempt_count: 0,
            },
            RunLeavesEnvelope::Progress {
                action: "launch",
                leaf: GAP,
                completed: 1,
                transient_attempt_count: 0,
            },
            RunLeavesEnvelope::Progress {
                action: "verify",
                leaf: GAP,
                completed: 1,
                transient_attempt_count: 0,
            },
            RunLeavesEnvelope::Audit { completed: 2 },
        ]
    );
}

#[test]
fn run_leaves_stops_on_child_or_remote_verification_failure() {
    let mut child_failure = FakeRunLeavesOperations {
        graphs: VecDeque::from([Ok(driver_graph(&[(LEAF, GitHubIssueState::Open, false)]))]),
        syncs: VecDeque::from([Ok(())]),
        children: VecDeque::from([ChildAttempt::Failed("ship failed".to_owned())]),
        ..FakeRunLeavesOperations::default()
    };
    let failure = execute_run_leaves(&mut child_failure).expect_err("child failure");
    assert_eq!(failure.step, "run-child");
    assert_eq!(failure.leaf, Some(LEAF));
    assert_eq!(child_failure.snapshots, 1);

    let mut verification_failure = FakeRunLeavesOperations {
        graphs: VecDeque::from([
            Ok(driver_graph(&[(LEAF, GitHubIssueState::Open, false)])),
            Ok(driver_graph(&[(LEAF, GitHubIssueState::Open, true)])),
        ]),
        syncs: VecDeque::from([Ok(()), Ok(())]),
        children: VecDeque::from([ChildAttempt::Complete]),
        ..FakeRunLeavesOperations::default()
    };
    let failure = execute_run_leaves(&mut verification_failure)
        .expect_err("remote lifecycle must verify before another selection");
    assert_eq!(failure.step, "verify-child");
    assert_eq!(failure.leaf, Some(LEAF));
    assert_eq!(verification_failure.snapshots, 1);
    assert_eq!(verification_failure.child_leaves, vec![LEAF]);
}

#[test]
fn run_leaves_reports_the_exact_sync_step_before_launch() {
    let mut operations = FakeRunLeavesOperations {
        graphs: VecDeque::from([Ok(driver_graph(&[(LEAF, GitHubIssueState::Open, false)]))]),
        syncs: VecDeque::from([Err("working tree is dirty".to_owned())]),
        children: VecDeque::from([ChildAttempt::Complete]),
        ..FakeRunLeavesOperations::default()
    };

    let failure = execute_run_leaves(&mut operations).expect_err("sync failure");
    assert_eq!(failure.step, "sync-before-child");
    assert_eq!(failure.leaf, Some(LEAF));
    assert_eq!(failure.reason, "working tree is dirty");
    assert!(operations.child_leaves.is_empty());
    assert_eq!(
        operations.results.last(),
        Some(&RunLeavesEnvelope::Failure(failure))
    );
}

#[test]
fn run_leaves_fails_closed_on_graph_and_selection_boundaries() {
    let mut graph_failure = FakeRunLeavesOperations {
        graphs: VecDeque::from([Err("graph unavailable".to_owned())]),
        ..FakeRunLeavesOperations::default()
    };
    let failure = execute_run_leaves(&mut graph_failure).expect_err("graph failure");
    assert_eq!(failure.step, "read-graph");
    assert_eq!(failure.leaf, None);

    let mut snapshot_failure = FakeRunLeavesOperations {
        graphs: VecDeque::from([Ok(driver_graph(&[(LEAF, GitHubIssueState::Open, false)]))]),
        snapshot_error: Some("snapshot refused".to_owned()),
        ..FakeRunLeavesOperations::default()
    };
    let failure = execute_run_leaves(&mut snapshot_failure).expect_err("snapshot failure");
    assert_eq!(failure.step, "write-snapshot");
    assert_eq!(failure.leaf, Some(LEAF));

    let mut orphan_graph = driver_graph(&[]);
    orphan_graph.open_orphan_blockers.push(GAP);
    let mut orphan = FakeRunLeavesOperations {
        graphs: VecDeque::from([Ok(orphan_graph)]),
        ..FakeRunLeavesOperations::default()
    };
    let failure = execute_run_leaves(&mut orphan).expect_err("orphan blocker");
    assert_eq!(failure.step, "select-leaf");
    assert!(failure.reason.contains("42"));

    let mut blocked_graph = driver_graph(&[(LEAF, GitHubIssueState::Open, false)]);
    blocked_graph.leaves[0].open_blockers.push(GAP);
    let mut deadlocked = FakeRunLeavesOperations {
        graphs: VecDeque::from([Ok(blocked_graph)]),
        ..FakeRunLeavesOperations::default()
    };
    let failure = execute_run_leaves(&mut deadlocked).expect_err("deadlock");
    assert_eq!(failure.leaf, Some(LEAF));
    assert!(failure.reason.contains("all open leaves"));
}

#[test]
fn run_leaves_preserves_result_write_failures() {
    let mut audit_write_failure = FakeRunLeavesOperations {
        graphs: VecDeque::from([Ok(driver_graph(&[(LEAF, GitHubIssueState::Closed, false)]))]),
        result_writes: VecDeque::from([Err("audit result refused".to_owned())]),
        ..FakeRunLeavesOperations::default()
    };
    let failure = execute_run_leaves(&mut audit_write_failure).expect_err("audit write failure");
    assert_eq!(failure.step, "write-result");
    assert_eq!(failure.leaf, None);
    assert_eq!(failure.reason, "audit result refused");

    let mut failure_write_failure = FakeRunLeavesOperations {
        graphs: VecDeque::from([Err("graph unavailable".to_owned())]),
        result_writes: VecDeque::from([Err("failure result refused".to_owned())]),
        ..FakeRunLeavesOperations::default()
    };
    let failure =
        execute_run_leaves(&mut failure_write_failure).expect_err("failure write failure");
    assert_eq!(failure.step, "write-result");
    assert!(failure.reason.contains("failure result refused"));
    assert!(failure.reason.contains("original failure"));
}

#[test]
fn run_leaves_retries_only_the_same_transient_leaf_and_resets_needs_design() {
    let mut transient = FakeRunLeavesOperations {
        graphs: VecDeque::from([
            Ok(driver_graph(&[(LEAF, GitHubIssueState::Open, false)])),
            Ok(driver_graph(&[(LEAF, GitHubIssueState::Closed, false)])),
        ]),
        syncs: VecDeque::from([Ok(()), Ok(()), Ok(()), Ok(())]),
        children: VecDeque::from([
            ChildAttempt::TransientApi("network one".to_owned()),
            ChildAttempt::TransientApi("network two".to_owned()),
            ChildAttempt::Complete,
        ]),
        ..FakeRunLeavesOperations::default()
    };
    assert_eq!(execute_run_leaves(&mut transient), Ok(1));
    assert_eq!(transient.child_leaves, vec![LEAF, LEAF, LEAF]);
    assert_eq!(transient.resets, vec![LEAF, LEAF]);
    assert!(transient.syncs.is_empty());

    let mut needs_design = FakeRunLeavesOperations {
        graphs: VecDeque::from([Ok(driver_graph(&[(LEAF, GitHubIssueState::Open, false)]))]),
        syncs: VecDeque::from([Ok(())]),
        children: VecDeque::from([ChildAttempt::NeedsDesign]),
        ..FakeRunLeavesOperations::default()
    };
    let failure = execute_run_leaves(&mut needs_design).expect_err("needs design");
    assert_eq!(failure.next_action, "needs-design");
    assert_eq!(failure.step, "run-child");
    assert_eq!(needs_design.resets, vec![LEAF]);
    assert_eq!(
        needs_design.results.last(),
        Some(&RunLeavesEnvelope::Failure(failure))
    );
}

#[test]
fn run_leaves_reports_retry_and_post_child_boundaries() {
    let mut exhausted = FakeRunLeavesOperations {
        graphs: VecDeque::from([Ok(driver_graph(&[(LEAF, GitHubIssueState::Open, false)]))]),
        syncs: VecDeque::from([Ok(()), Ok(()), Ok(())]),
        children: VecDeque::from([
            ChildAttempt::TransientApi("network one".to_owned()),
            ChildAttempt::TransientApi("network two".to_owned()),
            ChildAttempt::TransientApi("network three".to_owned()),
        ]),
        ..FakeRunLeavesOperations::default()
    };
    let failure = execute_run_leaves(&mut exhausted).expect_err("retry exhaustion");
    assert_eq!(failure.step, "run-child");
    assert!(failure.reason.contains("after 3 attempts"));
    assert_eq!(exhausted.resets, vec![LEAF, LEAF, LEAF]);

    let mut retry_sync_failure = FakeRunLeavesOperations {
        graphs: VecDeque::from([Ok(driver_graph(&[(LEAF, GitHubIssueState::Open, false)]))]),
        syncs: VecDeque::from([Ok(()), Err("retry sync refused".to_owned())]),
        children: VecDeque::from([ChildAttempt::TransientApi("network".to_owned())]),
        ..FakeRunLeavesOperations::default()
    };
    let failure = execute_run_leaves(&mut retry_sync_failure).expect_err("retry sync failure");
    assert_eq!(failure.step, "sync-before-retry");

    let mut after_child_failure = FakeRunLeavesOperations {
        graphs: VecDeque::from([Ok(driver_graph(&[(LEAF, GitHubIssueState::Open, false)]))]),
        syncs: VecDeque::from([Ok(()), Err("post-child sync refused".to_owned())]),
        children: VecDeque::from([ChildAttempt::Complete]),
        ..FakeRunLeavesOperations::default()
    };
    let failure = execute_run_leaves(&mut after_child_failure).expect_err("post-child sync");
    assert_eq!(failure.step, "sync-after-child");
}

#[test]
fn run_leaves_reports_reset_and_progress_write_failures() {
    let mut reset_failure = FakeRunLeavesOperations {
        graphs: VecDeque::from([Ok(driver_graph(&[(LEAF, GitHubIssueState::Open, false)]))]),
        syncs: VecDeque::from([Ok(())]),
        children: VecDeque::from([ChildAttempt::NeedsDesign]),
        reset_results: VecDeque::from([Err("reset refused".to_owned())]),
        ..FakeRunLeavesOperations::default()
    };
    let failure = execute_run_leaves(&mut reset_failure).expect_err("needs-design reset");
    assert_eq!(failure.step, "reset-leaf");
    assert!(failure.reason.contains("needs-design reset failed"));

    let mut transient_reset_failure = FakeRunLeavesOperations {
        graphs: VecDeque::from([Ok(driver_graph(&[(LEAF, GitHubIssueState::Open, false)]))]),
        syncs: VecDeque::from([Ok(())]),
        children: VecDeque::from([ChildAttempt::TransientApi("network".to_owned())]),
        reset_results: VecDeque::from([Err("reset refused".to_owned())]),
        ..FakeRunLeavesOperations::default()
    };
    let failure =
        execute_run_leaves(&mut transient_reset_failure).expect_err("transient reset failure");
    assert_eq!(failure.step, "reset-leaf");
    assert!(failure.reason.contains("transient child reset failed"));

    let mut progress_failure = FakeRunLeavesOperations {
        graphs: VecDeque::from([Ok(driver_graph(&[(LEAF, GitHubIssueState::Open, false)]))]),
        result_writes: VecDeque::from([Err("progress refused".to_owned()), Ok(())]),
        ..FakeRunLeavesOperations::default()
    };
    let failure = execute_run_leaves(&mut progress_failure).expect_err("progress write failure");
    assert_eq!(failure.step, "write-result");
    assert_eq!(failure.leaf, Some(LEAF));
}

#[test]
fn run_leaves_envelopes_and_child_results_are_exact() {
    let failure = RunLeavesFailure::failed("run-child", Some(LEAF), "first\nsecond");
    assert_eq!(failure.reason, "first second");
    assert_eq!(
        RunLeavesEnvelope::Failure(failure)
            .render()
            .expect("failure envelope"),
        "FAILED_LEAF=41\nFAILED_STEP=run-child\nFAILURE_REASON=first second\nNEXT_ACTION=failed\n"
    );

    let complete = format!(
        "CHILD_STATUS=complete\nCHILD_ISSUE={LEAF}\nCHILD_ENVELOPE_COMPLETE=true\nCHILD_TRANSIENT_ATTEMPT_COUNT=0\n"
    );
    assert_eq!(
        classify_child_attempt(LEAF, 0, Ok(()), Ok(complete)),
        ChildAttempt::Complete
    );
    let stale_attempt = format!(
        "CHILD_STATUS=complete\nCHILD_ISSUE={LEAF}\nCHILD_ENVELOPE_COMPLETE=true\nCHILD_TRANSIENT_ATTEMPT_COUNT=1\n"
    );
    assert_eq!(
        classify_child_attempt(LEAF, 0, Ok(()), Ok(stale_attempt)),
        ChildAttempt::Failed("child result carries another transient-attempt identity".to_owned())
    );
    let transient = format!(
        "CHILD_STATUS=failed\nCHILD_ISSUE={LEAF}\nCHILD_ENVELOPE_COMPLETE=false\nCHILD_TRANSIENT_ATTEMPT_COUNT=0\nCHILD_FAILURE_CLASS={COMPLETE_UMBRELLA_CHILD_FAILURE_TRANSIENT_API}\n"
    );
    assert_eq!(
        classify_child_attempt(
            LEAF,
            0,
            Err("temporary API failure".to_owned()),
            Ok(transient),
        ),
        ChildAttempt::TransientApi("temporary API failure".to_owned())
    );

    assert_eq!(
        RunLeavesEnvelope::Progress {
            action: "verify",
            leaf: LEAF,
            completed: 2,
            transient_attempt_count: 0,
        }
        .render()
        .expect("progress envelope"),
        "COMPLETED_LEAF_COUNT=2\nCURRENT_LEAF=41\nNEXT_ACTION=verify\n"
    );
    assert_eq!(
        RunLeavesEnvelope::Audit { completed: 3 }
            .render()
            .expect("audit envelope"),
        "COMPLETED_LEAF_COUNT=3\nNEXT_ACTION=audit\nOPEN_LEAF_COUNT=0\nSNAPSHOT_WRITTEN=true\n"
    );
    let unspecified = RunLeavesFailure::failed("read-graph", None, "");
    assert_eq!(unspecified.reason, "unspecified failure");
    assert_eq!(
        unspecified.diagnostic(),
        "failed at read-graph: unspecified failure"
    );
}

#[test]
fn run_leaves_classifies_malformed_and_nonterminal_child_results() {
    let needs_design = format!(
        "CHILD_STATUS=needs-design\nCHILD_ISSUE={LEAF}\nCHILD_ENVELOPE_COMPLETE=false\nCHILD_TRANSIENT_ATTEMPT_COUNT=0\nCHILD_FAILURE_CLASS={COMPLETE_UMBRELLA_CHILD_FAILURE_NEEDS_DESIGN}\n"
    );
    assert_eq!(
        classify_child_attempt(LEAF, 0, Ok(()), Ok(needs_design)),
        ChildAttempt::NeedsDesign
    );

    for attempt in [
        classify_child_attempt(LEAF, 0, Ok(()), Err("missing".to_owned())),
        classify_child_attempt(
            LEAF,
            0,
            Err("process failed".to_owned()),
            Err("missing".to_owned()),
        ),
        classify_child_attempt(LEAF, 0, Ok(()), Ok("not-an-environment".to_owned())),
        classify_child_attempt(
            LEAF,
            0,
            Err("process failed".to_owned()),
            Ok("not-an-environment".to_owned()),
        ),
    ] {
        assert!(matches!(attempt, ChildAttempt::Failed(_)));
    }

    let wrong_leaf = format!(
        "CHILD_STATUS=complete\nCHILD_ISSUE={GAP}\nCHILD_ENVELOPE_COMPLETE=true\nCHILD_TRANSIENT_ATTEMPT_COUNT=0\n"
    );
    assert_eq!(
        classify_child_attempt(LEAF, 0, Ok(()), Ok(wrong_leaf)),
        ChildAttempt::Failed("child result carries another leaf identity".to_owned())
    );
    let hard_failure = format!(
        "CHILD_STATUS=failed\nCHILD_ISSUE={LEAF}\nCHILD_ENVELOPE_COMPLETE=false\nCHILD_TRANSIENT_ATTEMPT_COUNT=0\n"
    );
    assert_eq!(
        classify_child_attempt(
            LEAF,
            0,
            Err("permanent failure".to_owned()),
            Ok(hard_failure),
        ),
        ChildAttempt::Failed("permanent failure".to_owned())
    );
    let invalid_success = format!(
        "CHILD_STATUS=failed\nCHILD_ISSUE={LEAF}\nCHILD_ENVELOPE_COMPLETE=false\nCHILD_TRANSIENT_ATTEMPT_COUNT=0\n"
    );
    assert!(matches!(
        classify_child_attempt(LEAF, 0, Ok(()), Ok(invalid_success)),
        ChildAttempt::Failed(reason) if reason.contains("invalid success shape")
    ));
    let invalid_failure = format!(
        "CHILD_STATUS=complete\nCHILD_ISSUE={LEAF}\nCHILD_ENVELOPE_COMPLETE=true\nCHILD_TRANSIENT_ATTEMPT_COUNT=0\n"
    );
    assert!(matches!(
        classify_child_attempt(
            LEAF,
            0,
            Err("process failed".to_owned()),
            Ok(invalid_failure),
        ),
        ChildAttempt::Failed(reason) if reason.contains("invalid failure shape")
    ));
}

#[test]
fn live_run_leaves_confines_state_and_writes_caller_owned_files() {
    let repository_fixture = GitRepository::builder(GitFixture::Refs)
        .build()
        .expect("Git fixture");
    let output_root = tempfile::tempdir().expect("temporary output root");
    let arguments = RunLeavesArguments {
        repository: String::from("o/r"),
        repo_root: repository_fixture.root().to_path_buf(),
        umbrella: UMBRELLA,
        model: String::from("claude-opus-4-1"),
        output_root: output_root.path().to_path_buf(),
        output: output_root.path().join("snapshot.json"),
        result_env: output_root.path().join("run-leaves.env"),
        operator_invoked: true,
    };
    let pointer_root = tempfile::tempdir().expect("pointer root");
    let pointer_store = RunPointerStore::at(pointer_root.path());
    pointer_store
        .create(CompleteUmbrellaRunPointer {
            current_step: RunPointerStep::Select,
            ..CompleteUmbrellaRunPointer::initial(
                String::from("o/r"),
                UMBRELLA,
                fs::canonicalize(output_root.path()).expect("canonical output root"),
                123,
            )
        })
        .expect("run pointer");
    let mut operations = LiveRunLeavesOperations::new_with_store(&arguments, pointer_store.clone())
        .expect("live operations");
    let child = operations.child_arguments(LEAF);
    assert_eq!(child.umbrella, UMBRELLA);
    assert_eq!(child.leaf, LEAF);
    assert!(child.output.starts_with(&operations.driver_root));
    assert!(child.result_env.starts_with(&operations.driver_root));

    operations
        .write_snapshot(&driver_graph(&[(LEAF, GitHubIssueState::Open, false)]))
        .expect("caller-owned snapshot");
    let snapshot = fs::read_to_string(&arguments.output).expect("snapshot text");
    assert!(snapshot.contains("\"repository\": \"o/r\""));
    assert!(snapshot.contains("\"number\": 41"));
    operations
        .write_result(&RunLeavesEnvelope::Progress {
            action: "launch",
            leaf: LEAF,
            completed: 0,
            transient_attempt_count: 0,
        })
        .expect("caller-owned result");
    assert_eq!(
        fs::read_to_string(&arguments.result_env).expect("result text"),
        "COMPLETED_LEAF_COUNT=0\nCURRENT_LEAF=41\nNEXT_ACTION=launch\n"
    );

    fs::create_dir(&child.result_env).expect("blocking child-result directory");
    assert!(matches!(
        operations.run_child(LEAF),
        ChildAttempt::Failed(reason) if reason.contains("could not clear the prior child result")
    ));
    drop(operations);

    let same_file = output_root.path().join("same.env");
    let same_arguments = RunLeavesArguments {
        output: same_file.clone(),
        result_env: same_file,
        ..arguments
    };
    let Err(error) = LiveRunLeavesOperations::new_with_store(&same_arguments, pointer_store) else {
        panic!("identical caller-owned outputs must fail");
    };
    assert!(error.contains("must be different files"));
}

#[test]
fn live_run_leaves_rejects_outputs_inside_its_private_state_root() {
    let repository_fixture = GitRepository::builder(GitFixture::Refs)
        .build()
        .expect("Git fixture");
    let output_root = tempfile::tempdir().expect("temporary output root");
    fs::create_dir(output_root.path().join("complete-umbrella-run-leaves"))
        .expect("private state root fixture");
    let arguments = RunLeavesArguments {
        repository: String::from("o/r"),
        repo_root: repository_fixture.root().to_path_buf(),
        umbrella: UMBRELLA,
        model: String::from("claude-opus-4-1"),
        output_root: output_root.path().to_path_buf(),
        output: output_root
            .path()
            .join("complete-umbrella-run-leaves/snapshot.json"),
        result_env: output_root.path().join("run-leaves.env"),
        operator_invoked: true,
    };
    let Err(error) = LiveRunLeavesOperations::new(&arguments) else {
        panic!("private state overlap must fail");
    };
    assert!(error.contains("must not overlap run-leaves state"));
}

#[test]
fn run_leaves_main_sync_fast_forwards_and_rejects_another_branch() {
    let repository = GitRepository::builder(GitFixture::Refs)
        .build()
        .expect("Git fixture");
    let remote = repository.workspace_root().join("remote.git");
    let remote_text = remote.to_str().expect("UTF-8 fixture path");
    for arguments in [
        vec!["init", "--quiet", "--bare", remote_text],
        vec!["remote", "add", "origin", remote_text],
        vec!["push", "--quiet", "origin", "main"],
    ] {
        let output = repository.git(arguments).expect("fixture Git command");
        assert!(output.success(), "fixture Git command failed: {output:?}");
    }
    repository
        .write("remote.txt", b"remote commit\n")
        .expect("remote fixture file");
    for arguments in [
        ["add", "--", "remote.txt"].as_slice(),
        ["commit", "--quiet", "-m", "remote"].as_slice(),
        ["push", "--quiet", "origin", "main"].as_slice(),
        ["reset", "--quiet", "--hard", "HEAD^"].as_slice(),
    ] {
        let output = repository.git(arguments).expect("fixture Git command");
        assert!(output.success(), "fixture Git command failed: {output:?}");
    }

    synchronize_main(repository.root()).expect("fast-forward main");
    let head = repository
        .git(["rev-parse", "HEAD"])
        .expect("local revision");
    let origin = repository
        .git(["rev-parse", "origin/main"])
        .expect("remote revision");
    assert_eq!(head.stdout, origin.stdout);

    repository
        .write("conflict.txt", b"remote side\n")
        .expect("remote conflict fixture");
    for arguments in [
        ["add", "--", "conflict.txt"].as_slice(),
        ["commit", "--quiet", "-m", "remote conflict"].as_slice(),
        ["push", "--quiet", "origin", "main"].as_slice(),
        ["reset", "--quiet", "--hard", "HEAD^"].as_slice(),
    ] {
        let output = repository.git(arguments).expect("remote conflict command");
        assert!(
            output.success(),
            "remote conflict command failed: {output:?}"
        );
    }
    repository
        .write("conflict.txt", b"local side\n")
        .expect("local conflict fixture");
    for arguments in [
        ["add", "--", "conflict.txt"].as_slice(),
        ["commit", "--quiet", "-m", "local conflict"].as_slice(),
    ] {
        let output = repository.git(arguments).expect("local conflict command");
        assert!(
            output.success(),
            "local conflict command failed: {output:?}"
        );
    }
    let error = synchronize_main(repository.root()).expect_err("conflicting rebase");
    assert!(error.contains("git rebase origin/main failed"));
    assert!(error.contains("rebase aborted"));

    let checkout = repository
        .git(["checkout", "--quiet", "topic"])
        .expect("topic checkout");
    assert!(checkout.success(), "topic checkout failed: {checkout:?}");
    assert!(
        synchronize_main(repository.root())
            .expect_err("non-main checkout")
            .contains("not on branch main")
    );
}

#[test]
fn run_leaves_main_sync_rejects_dirty_detached_and_nonrepositories() {
    let dirty = GitRepository::builder(GitFixture::Refs)
        .build()
        .expect("dirty Git fixture");
    dirty
        .write("untracked.txt", b"dirty\n")
        .expect("dirty fixture file");
    assert!(
        synchronize_main(dirty.root())
            .expect_err("dirty worktree")
            .contains("working tree is not clean")
    );

    let detached = GitRepository::builder(GitFixture::Refs)
        .build()
        .expect("detached Git fixture");
    let checkout = detached
        .git(["checkout", "--quiet", "--detach"])
        .expect("detached checkout");
    assert!(checkout.success(), "detached checkout failed: {checkout:?}");
    assert!(
        synchronize_main(detached.root())
            .expect_err("detached worktree")
            .contains("not on branch main")
    );

    let not_repository = tempfile::tempdir().expect("non-repository directory");
    assert!(
        synchronize_main(not_repository.path())
            .expect_err("non-repository")
            .contains("cannot open repository")
    );
}

#[test]
fn transient_claude_api_envelopes_classify_from_terminal_reason_or_connectivity() {
    let api_error = parse_claude_envelope(
        r#"{"is_error":true,"terminal_reason":"api_error","result":"API Error: Can't reach the API server — check your internet or DNS (ENOTFOUND)"}"#,
    );
    assert!(is_transient_claude_api_error(&api_error));
    assert_eq!(api_error.terminal_reason, "api_error");
    assert!(api_error.is_error);

    let connectivity = parse_claude_envelope(
        r#"{"is_error":true,"result":"API Error: Can't reach the API server — check your internet or DNS (ENOTFOUND)"}"#,
    );
    assert!(is_transient_claude_api_error(&connectivity));

    let permanent = parse_claude_envelope(r#"{"is_error":true,"result":"policy refusal"}"#);
    assert!(!is_transient_claude_api_error(&permanent));

    let ok = parse_claude_envelope(
        "{\"result\":\"verified\\nCOMPLETE_UMBRELLA_CHILD_STATUS=complete\"}",
    );
    assert!(!is_transient_claude_api_error(&ok));
}

#[test]
fn expected_paths_and_leaf_files_fail_closed() {
    let directory = tempfile::tempdir().expect("temporary root");
    let root = temporary_root(directory.path(), "root").expect("trusted root");
    let file = directory.path().join("file");
    fs::write(&file, "content").expect("fixture");

    assert!(canonical_directory(&file, "directory").is_err());
    assert!(temporary_root(Path::new("relative"), "root").is_err());
    assert!(
        confine_session_path(
            Path::new("relative"),
            directory.path(),
            &root,
            PathIntent::Read,
            "file",
        )
        .is_err()
    );
    assert!(
        confine_session_path(
            directory.path(),
            directory.path(),
            &root,
            PathIntent::Read,
            "file",
        )
        .is_err()
    );

    let title_file = directory.path().join("title");
    let body_file = directory.path().join("body");
    fs::write(&title_file, "Close the gap\n").expect("title fixture");
    fs::write(&body_file, "").expect("empty body fixture");
    let arguments = GapFileArguments {
        root: directory.path().to_path_buf(),
        title_file,
        body_file: body_file.clone(),
    };
    assert!(read_expected_audit_leaf(UMBRELLA, &arguments).is_err());
    fs::write(&body_file, "Wrong umbrella opening\n").expect("body fixture");
    assert!(read_expected_audit_leaf(UMBRELLA, &arguments).is_err());
}

#[tokio::test]
async fn start_remote_applies_only_the_active_title_transition() {
    let original = issue_json(
        UMBRELLA,
        400,
        "[UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let active = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        AFTER,
    );
    let (client, server) = service(vec![
        // read_graph runnability pre-check (zero leaves -> audit-runnable).
        response(200, &original),
        response(200, "[]"),
        response(200, "[]"),
        // read_snapshot, then apply (read, PATCH, read-back).
        response(200, &original),
        response(200, &original),
        response(200, &active),
        response(200, &active),
    ]);

    start_remote(&client, &Cancellation::new(), &repository(), UMBRELLA)
        .await
        .expect("active transition");

    let requests = server.finish().expect("stub completed");
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.method == "PATCH")
            .count(),
        1
    );
    let patch = requests
        .iter()
        .find(|request| request.method == "PATCH")
        .expect("one title PATCH");
    assert!(
        String::from_utf8_lossy(&patch.body.bytes).contains("[IMPLEMENTING] [UMBRELLA] Ship it")
    );
}

#[tokio::test]
async fn start_refuses_an_open_non_leaf_parent_blocker_without_renaming() {
    let original = issue_json(
        UMBRELLA,
        400,
        "[UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let leaf = issue_json(
        LEAF,
        410,
        "[LEAF OF 40] Implement it",
        &format!("{}\n\nWork remains.", umbrella_leaf_opening(UMBRELLA)),
        "open",
        BEFORE,
    );
    let (client, server) = service(vec![
        response(200, &original),
        response(200, refs(&[(LEAF, 410, "open")])),
        response(200, refs(&[(LEAF, 410, "open"), (GAP, 420, "open")])),
        response(200, &leaf),
        response(200, "[]"),
        response(200, "[]"),
    ]);

    let error = start_remote(&client, &Cancellation::new(), &repository(), UMBRELLA)
        .await
        .expect_err("open non-leaf parent blocker must refuse start");
    assert_eq!(
        error,
        "cannot start while open non-leaf parent blockers remain: 42"
    );
    let requests = server.finish().expect("stub completed");
    assert!(requests.iter().all(|request| request.method != "PATCH"));
}

#[tokio::test]
async fn start_refuses_a_deadlocked_umbrella_without_renaming() {
    let original = issue_json(
        UMBRELLA,
        400,
        "[UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let leaf = issue_json(
        LEAF,
        410,
        "[LEAF OF 40] Implement it",
        &format!("{}\n\nWork remains.", umbrella_leaf_opening(UMBRELLA)),
        "open",
        BEFORE,
    );
    let (client, server) = service(vec![
        response(200, &original),
        response(200, refs(&[(LEAF, 410, "open")])),
        response(200, refs(&[(LEAF, 410, "open")])),
        response(200, &leaf),
        response(200, "[]"),
        response(200, refs(&[(99, 990, "open")])),
    ]);

    let error = start_remote(&client, &Cancellation::new(), &repository(), UMBRELLA)
        .await
        .expect_err("a fully blocked leaf graph must refuse start");
    assert_eq!(
        error,
        "cannot start a deadlocked umbrella while every open leaf is blocked: 41"
    );
    let requests = server.finish().expect("stub completed");
    assert!(requests.iter().all(|request| request.method != "PATCH"));
}

#[tokio::test]
async fn reset_leaf_remote_strips_only_the_active_prefix() {
    let parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let body = format!("{}\n\nTask.", umbrella_leaf_opening(UMBRELLA));
    let active_leaf = issue_json(
        LEAF,
        410,
        "[IMPLEMENTING] [LEAF OF 40] Task",
        &body,
        "open",
        BEFORE,
    );
    let idle_leaf = issue_json(LEAF, 410, "[LEAF OF 40] Task", &body, "open", AFTER);
    let mut exchanges = open_graph(&parent, &active_leaf);
    exchanges.extend([
        response(200, &active_leaf),
        response(200, &active_leaf),
        response(200, &idle_leaf),
        response(200, &idle_leaf),
    ]);
    let (client, server) = service(exchanges);
    let arguments = LeafArguments {
        repository: String::from("o/r"),
        umbrella: UMBRELLA,
        leaf: LEAF,
    };

    reset_leaf_remote(&client, &Cancellation::new(), &repository(), &arguments)
        .await
        .expect("idle transition");

    let requests = server.finish().expect("stub completed");
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.method == "PATCH")
            .count(),
        1
    );
}

#[tokio::test]
async fn reset_leaf_remote_is_idempotent_for_an_idle_leaf() {
    let parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let body = format!("{}\n\nTask.", umbrella_leaf_opening(UMBRELLA));
    let idle_leaf = issue_json(LEAF, 410, "[LEAF OF 40] Task", &body, "open", BEFORE);
    let (client, server) = service(open_graph(&parent, &idle_leaf));
    let arguments = LeafArguments {
        repository: String::from("o/r"),
        umbrella: UMBRELLA,
        leaf: LEAF,
    };

    reset_leaf_remote(&client, &Cancellation::new(), &repository(), &arguments)
        .await
        .expect("already idle");

    let requests = server.finish().expect("stub completed");
    assert!(requests.iter().all(|request| request.method != "PATCH"));
}

#[tokio::test]
async fn reset_leaf_remote_preserves_a_designed_leaf_for_design_reentry() {
    let parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let body = format!("{}\n\nTask.", umbrella_leaf_opening(UMBRELLA));
    let designed_leaf = issue_json(
        LEAF,
        410,
        "[DESIGNED] [LEAF OF 40] Task",
        &body,
        "open",
        BEFORE,
    );
    let (client, server) = service(open_graph(&parent, &designed_leaf));
    let arguments = LeafArguments {
        repository: String::from("o/r"),
        umbrella: UMBRELLA,
        leaf: LEAF,
    };

    reset_leaf_remote(&client, &Cancellation::new(), &repository(), &arguments)
        .await
        .expect("designed leaf remains design-admissible");

    let requests = server.finish().expect("stub completed");
    assert!(requests.iter().all(|request| request.method != "PATCH"));
}

#[tokio::test]
async fn attachment_is_idempotent_and_replays_both_native_edges_with_exact_read_back() {
    let parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let opening = umbrella_leaf_opening(UMBRELLA);
    let body = format!("{opening}\n\nClose the audited gap.");
    let leaf = issue_json(
        GAP,
        420,
        "[LEAF OF 40] Close the gap",
        &body,
        "open",
        BEFORE,
    );
    let leaf_ref = refs(&[(GAP, 420, "open")]);
    let parent_ref = json!({ "number": UMBRELLA, "id": 400, "state": "open" }).to_string();
    let (service, server) = service(attachment_exchanges(&parent, &leaf, &leaf_ref, &parent_ref));
    let arguments = LeafArguments {
        repository: String::from("o/r"),
        umbrella: UMBRELLA,
        leaf: GAP,
    };
    let expected = ExpectedAuditLeaf {
        remote_title: String::from("[LEAF OF 40] Close the gap"),
        body,
    };

    attach_leaf_remote(
        &service,
        &Cancellation::new(),
        &repository(),
        &arguments,
        &expected,
    )
    .await
    .expect("verified attachment");
    attach_leaf_remote(
        &service,
        &Cancellation::new(),
        &repository(),
        &arguments,
        &expected,
    )
    .await
    .expect("idempotent attachment");

    let requests = server.finish().expect("stub completed");
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.method == "POST")
            .count(),
        2
    );
    assert!(
        requests
            .iter()
            .any(|request| request.path.ends_with("/sub_issues"))
    );
    assert!(
        requests
            .iter()
            .any(|request| request.path.ends_with("/dependencies/blocked_by"))
    );
}

#[tokio::test]
async fn attachment_stops_when_the_parent_finishes_between_edges() {
    let active_parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let done_parent = issue_json(
        UMBRELLA,
        400,
        "[DONE] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        AFTER,
    );
    let body = format!(
        "{}\n\nClose the audited gap.",
        umbrella_leaf_opening(UMBRELLA)
    );
    let leaf = issue_json(
        GAP,
        420,
        "[LEAF OF 40] Close the gap",
        &body,
        "open",
        BEFORE,
    );
    let leaf_ref = refs(&[(GAP, 420, "open")]);
    let (service, server) = service(vec![
        response(200, &active_parent),
        response(200, "[]"),
        response(200, &leaf),
        response(200, "[]"),
        response(404, "{}"),
        response(200, "[]"),
        response(201, "{}"),
        response(200, &leaf_ref),
        response(200, &done_parent),
    ]);
    let arguments = LeafArguments {
        repository: String::from("o/r"),
        umbrella: UMBRELLA,
        leaf: GAP,
    };
    let expected = ExpectedAuditLeaf {
        remote_title: String::from("[LEAF OF 40] Close the gap"),
        body,
    };

    assert!(
        attach_leaf_remote(
            &service,
            &Cancellation::new(),
            &repository(),
            &arguments,
            &expected,
        )
        .await
        .is_err()
    );

    let requests = server.finish().expect("stub completed");
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.method == "POST")
            .count(),
        1
    );
    assert!(requests[6].path.ends_with("/sub_issues"));
}

#[tokio::test]
async fn finish_remote_proves_closed_leaves_and_ignores_closed_historical_extra_blockers() {
    let active_parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let done_parent = issue_json(
        UMBRELLA,
        400,
        "[DONE] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        AFTER,
    );
    let closed_parent = issue_json(
        UMBRELLA,
        400,
        "[DONE] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "closed",
        CLOSED_AT,
    );
    let leaf = issue_json(
        LEAF,
        410,
        "[DONE] [LEAF OF 40] Implement it",
        &format!("{}\n\nDone.", umbrella_leaf_opening(UMBRELLA)),
        "closed",
        BEFORE,
    );
    let parent_blockers = [(LEAF, 410, "closed"), (GAP, 420, "closed")];
    let mut exchanges = closed_graph_with_parent_blockers(&active_parent, &leaf, &parent_blockers);
    exchanges.extend([
        response(200, &active_parent),
        response(200, &active_parent),
        response(200, &done_parent),
        response(200, &done_parent),
    ]);
    exchanges.extend(closed_graph_with_parent_blockers(
        &done_parent,
        &leaf,
        &parent_blockers,
    ));
    exchanges.push(response(200, &closed_parent));
    exchanges.extend(closed_graph_with_parent_blockers(
        &closed_parent,
        &leaf,
        &parent_blockers,
    ));
    let (service, server) = service(exchanges);

    finish_remote(&service, &Cancellation::new(), &repository(), UMBRELLA)
        .await
        .expect("verified finish");

    let requests = server.finish().expect("stub completed");
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.method == "PATCH")
            .count(),
        2
    );
    assert!(requests.iter().any(|request| {
        request.method == "PATCH"
            && String::from_utf8_lossy(&request.body.bytes).contains("completed")
    }));
}

#[tokio::test]
async fn finish_refuses_an_open_non_leaf_parent_blocker() {
    let active_parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let leaf = issue_json(
        LEAF,
        410,
        "[DONE] [LEAF OF 40] Implement it",
        &format!("{}\n\nDone.", umbrella_leaf_opening(UMBRELLA)),
        "closed",
        BEFORE,
    );
    let (client, server) = service(closed_graph_with_parent_blockers(
        &active_parent,
        &leaf,
        &[(LEAF, 410, "closed"), (GAP, 420, "open")],
    ));

    let error = finish_remote(&client, &Cancellation::new(), &repository(), UMBRELLA)
        .await
        .expect_err("open non-leaf parent blocker must refuse completion");
    assert_eq!(
        error,
        "cannot finish while open non-leaf parent blockers remain: 42"
    );
    let requests = server.finish().expect("stub completed");
    assert!(requests.iter().all(|request| request.method != "PATCH"));
}

#[tokio::test]
async fn next_graph_reports_an_open_non_leaf_parent_blocker_separately() {
    let parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let leaf = issue_json(
        LEAF,
        410,
        "[LEAF OF 40] Implement it",
        &format!("{}\n\nWork remains.", umbrella_leaf_opening(UMBRELLA)),
        "open",
        BEFORE,
    );
    let (client, server) = service(vec![
        response(200, &parent),
        response(200, refs(&[(LEAF, 410, "open")])),
        response(200, refs(&[(LEAF, 410, "open"), (GAP, 420, "open")])),
        response(200, &leaf),
        response(200, "[]"),
        response(200, "[]"),
    ]);

    let graph = read_graph(&client, &Cancellation::new(), &repository(), UMBRELLA)
        .await
        .expect("valid direct leaf with a separately reported parent blocker");
    assert_eq!(graph.open_orphan_blockers, vec![GAP]);
    let selection = select_complete_umbrella_leaf(
        &selection_leaves(&graph.leaves),
        &graph.open_orphan_blockers,
    );
    assert_eq!(
        next_action_fields(&selection),
        vec![
            ("NEXT_ACTION", "orphan-blocker".to_owned()),
            ("ORPHAN_BLOCKERS", "42".to_owned()),
        ]
    );
    server.join().expect("orphan graph stub completed");
}

#[tokio::test]
async fn next_excludes_closed_leaves_with_stale_or_arbitrary_titles() {
    let parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let stale_implementing = issue_json(
        LEAF,
        410,
        "[IMPLEMENTING] [LEAF OF 40] Already shipped",
        &format!("{}\n\nDone.", umbrella_leaf_opening(UMBRELLA)),
        "closed",
        BEFORE,
    );
    let arbitrary_closed = issue_json(
        GAP,
        420,
        "Finished without a lifecycle prefix",
        "Wrong first line\n\nStill closed.",
        "closed",
        BEFORE,
    );
    let open_leaf = issue_json(
        43,
        430,
        "[LEAF OF 40] Still open",
        &format!("{}\n\nWork remains.", umbrella_leaf_opening(UMBRELLA)),
        "open",
        BEFORE,
    );
    let (client, server) = service(vec![
        response(200, &parent),
        response(
            200,
            refs(&[
                (LEAF, 410, "closed"),
                (GAP, 420, "closed"),
                (43, 430, "open"),
            ]),
        ),
        response(
            200,
            refs(&[
                (LEAF, 410, "closed"),
                (GAP, 420, "closed"),
                (43, 430, "open"),
            ]),
        ),
        response(200, &stale_implementing),
        response(200, "[]"),
        response(200, &arbitrary_closed),
        response(200, "[]"),
        response(200, &open_leaf),
        response(200, "[]"),
        response(200, "[]"),
    ]);

    let graph = read_graph(&client, &Cancellation::new(), &repository(), UMBRELLA)
        .await
        .expect("closed title drift must not abort next enumeration");
    assert_eq!(graph.leaves.len(), 3);
    let selection = select_complete_umbrella_leaf(
        &selection_leaves(&graph.leaves),
        &graph.open_orphan_blockers,
    );
    assert_eq!(
        next_action_fields(&selection),
        vec![
            ("NEXT_ACTION", "launch".to_owned()),
            ("NEXT_LEAF", "43".to_owned()),
        ]
    );
    server
        .join()
        .expect("stale closed leaf graph stub completed");
}

#[tokio::test]
async fn next_skips_open_designing_designed_and_done_without_aborting() {
    let parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let designing = issue_json(
        LEAF,
        410,
        "[DESIGNING] [LEAF OF 40] Planning",
        &format!("{}\n\nWork remains.", umbrella_leaf_opening(UMBRELLA)),
        "open",
        BEFORE,
    );
    let designed = issue_json(
        GAP,
        420,
        "[DESIGNED] [LEAF OF 40] Planned",
        &format!("{}\n\nWork remains.", umbrella_leaf_opening(UMBRELLA)),
        "open",
        BEFORE,
    );
    let open_done = issue_json(
        43,
        430,
        "[DONE] [LEAF OF 40] Drifted open",
        &format!("{}\n\nWork remains.", umbrella_leaf_opening(UMBRELLA)),
        "open",
        BEFORE,
    );
    let idle = issue_json(
        44,
        440,
        "[LEAF OF 40] Still open",
        &format!("{}\n\nWork remains.", umbrella_leaf_opening(UMBRELLA)),
        "open",
        BEFORE,
    );
    let (client, server) = service(vec![
        response(200, &parent),
        response(
            200,
            refs(&[
                (LEAF, 410, "open"),
                (GAP, 420, "open"),
                (43, 430, "open"),
                (44, 440, "open"),
            ]),
        ),
        response(
            200,
            refs(&[
                (LEAF, 410, "open"),
                (GAP, 420, "open"),
                (43, 430, "open"),
                (44, 440, "open"),
            ]),
        ),
        response(200, &designing),
        response(200, "[]"),
        response(200, "[]"),
        response(200, &designed),
        response(200, "[]"),
        response(200, "[]"),
        response(200, &open_done),
        response(200, "[]"),
        response(200, "[]"),
        response(200, &idle),
        response(200, "[]"),
        response(200, "[]"),
    ]);

    let graph = read_graph(&client, &Cancellation::new(), &repository(), UMBRELLA)
        .await
        .expect("in-flight and open-DONE leaves must not abort next enumeration");
    assert_eq!(graph.leaves.len(), 4);
    let selection = select_complete_umbrella_leaf(
        &selection_leaves(&graph.leaves),
        &graph.open_orphan_blockers,
    );
    assert_eq!(
        next_action_fields(&selection),
        vec![
            ("NEXT_ACTION", "launch".to_owned()),
            ("NEXT_LEAF", "44".to_owned()),
        ]
    );
    server
        .join()
        .expect("non-candidate open leaf graph stub completed");
}

#[tokio::test]
async fn next_deadlocks_when_only_non_candidate_open_leaves_remain() {
    let parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let open_done = issue_json(
        LEAF,
        410,
        "[DONE] [LEAF OF 40] Still open",
        &format!("{}\n\nWork remains.", umbrella_leaf_opening(UMBRELLA)),
        "open",
        BEFORE,
    );
    let (client, server) = service(vec![
        response(200, &parent),
        response(200, refs(&[(LEAF, 410, "open")])),
        response(200, refs(&[(LEAF, 410, "open")])),
        response(200, &open_done),
        response(200, "[]"),
        response(200, "[]"),
    ]);
    let graph = read_graph(&client, &Cancellation::new(), &repository(), UMBRELLA)
        .await
        .expect("open DONE drift must enumerate then deadlock");
    let selection = select_complete_umbrella_leaf(
        &selection_leaves(&graph.leaves),
        &graph.open_orphan_blockers,
    );
    assert_eq!(
        next_action_fields(&selection),
        vec![
            ("NEXT_ACTION", "deadlock".to_owned()),
            ("BLOCKED_LEAVES", "41".to_owned()),
        ]
    );
    server.join().expect("open DONE deadlock stub completed");
}

#[tokio::test]
async fn graph_diagnostics_still_name_open_leaf_lifecycle_failures() {
    let parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let invalid_title = issue_json(
        LEAF,
        410,
        "[STALLED] [LEAF OF 40] Still open",
        &format!("{}\n\nWork remains.", umbrella_leaf_opening(UMBRELLA)),
        "open",
        BEFORE,
    );
    let (client, server) = service(vec![
        response(200, &parent),
        response(200, refs(&[(LEAF, 410, "open")])),
        response(200, refs(&[(LEAF, 410, "open")])),
        response(200, &invalid_title),
    ]);
    let Err(error) = read_graph(&client, &Cancellation::new(), &repository(), UMBRELLA).await
    else {
        panic!("unsupported open leaf title must remain exact");
    };
    assert_eq!(
        error,
        "direct leaf #41 violates the exact lifecycle-title invariant"
    );
    server.join().expect("open title stub completed");

    let invalid_body = issue_json(
        LEAF,
        410,
        "[LEAF OF 40] Still open",
        "Wrong first line\n\nWork remains.",
        "open",
        BEFORE,
    );
    let (client, server) = service(vec![
        response(200, &parent),
        response(200, refs(&[(LEAF, 410, "open")])),
        response(200, refs(&[(LEAF, 410, "open")])),
        response(200, &invalid_body),
    ]);
    let Err(error) = read_graph(&client, &Cancellation::new(), &repository(), UMBRELLA).await
    else {
        panic!("open leaf body must retain the exact opening");
    };
    assert_eq!(
        error,
        "direct leaf #41 violates the exact first-line body invariant"
    );
    server.join().expect("open body stub completed");
}

#[tokio::test]
async fn finish_accepts_closed_leaves_with_stale_implementing_titles() {
    let active_parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let done_parent = issue_json(
        UMBRELLA,
        400,
        "[DONE] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        AFTER,
    );
    let closed_parent = issue_json(
        UMBRELLA,
        400,
        "[DONE] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "closed",
        CLOSED_AT,
    );
    let stale_leaf = issue_json(
        LEAF,
        410,
        "[IMPLEMENTING] [LEAF OF 40] Implement it",
        &format!("{}\n\nDone.", umbrella_leaf_opening(UMBRELLA)),
        "closed",
        BEFORE,
    );
    let mut exchanges = closed_graph(&active_parent, &stale_leaf);
    exchanges.extend([
        response(200, &active_parent),
        response(200, &active_parent),
        response(200, &done_parent),
        response(200, &done_parent),
    ]);
    exchanges.extend(closed_graph(&done_parent, &stale_leaf));
    exchanges.push(response(200, &closed_parent));
    exchanges.extend(closed_graph(&closed_parent, &stale_leaf));
    let (service, server) = service(exchanges);

    finish_remote(&service, &Cancellation::new(), &repository(), UMBRELLA)
        .await
        .expect("stale closed leaf titles must not block finish");

    let requests = server.finish().expect("stub completed");
    assert!(requests.iter().any(|request| {
        request.method == "PATCH"
            && String::from_utf8_lossy(&request.body.bytes).contains("completed")
    }));
}

#[tokio::test]
async fn child_verification_reads_the_fresh_closed_graph() {
    let parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let leaf = issue_json(
        LEAF,
        410,
        "[DONE] [LEAF OF 40] Implement it",
        &format!("{}\n\nDone.", umbrella_leaf_opening(UMBRELLA)),
        "closed",
        BEFORE,
    );
    let (service, server) = service(closed_graph(&parent, &leaf));
    let arguments = LeafArguments {
        repository: String::from("o/r"),
        umbrella: UMBRELLA,
        leaf: LEAF,
    };

    verify_child_remote(&service, &Cancellation::new(), &repository(), &arguments)
        .await
        .expect("verified child");
    server.join().expect("stub completed");
}

#[tokio::test]
async fn orphaned_child_recovery_accepts_an_exact_result_after_remote_completion() {
    let parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let leaf = issue_json(
        LEAF,
        410,
        "[DONE] [LEAF OF 40] Implement it",
        &format!("{}\n\nDone.", umbrella_leaf_opening(UMBRELLA)),
        "closed",
        BEFORE,
    );
    let (service, server) = service(closed_graph(&parent, &leaf));
    let arguments = LeafArguments {
        repository: String::from("o/r"),
        umbrella: UMBRELLA,
        leaf: LEAF,
    };
    let result = format!("BGJOB_RC=orphaned\nSTEP=complete-umbrella-leaf-{LEAF}\n");

    recover_orphaned_child_remote(
        &service,
        &Cancellation::new(),
        &repository(),
        &arguments,
        &result,
    )
    .await
    .expect("remote completion recovers the orphaned transport");
    server.join().expect("stub completed");
}

#[tokio::test]
async fn child_verification_still_requires_exact_done_title_on_the_shipped_leaf() {
    let parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let stale_leaf = issue_json(
        LEAF,
        410,
        "[IMPLEMENTING] [LEAF OF 40] Implement it",
        &format!("{}\n\nDone.", umbrella_leaf_opening(UMBRELLA)),
        "closed",
        BEFORE,
    );
    let (service, server) = service(closed_graph(&parent, &stale_leaf));
    let arguments = LeafArguments {
        repository: String::from("o/r"),
        umbrella: UMBRELLA,
        leaf: LEAF,
    };

    let error = verify_child_remote(&service, &Cancellation::new(), &repository(), &arguments)
        .await
        .expect_err("verify-child must keep the exact [DONE] assertion");
    assert_eq!(
        error,
        "child must be closed with the exact [DONE] leaf prefix"
    );
    server.join().expect("stale verify-child stub completed");
}

#[tokio::test]
async fn remote_graph_checks_reject_incomplete_or_nested_lifecycles() {
    let active_parent = issue_json(
        UMBRELLA,
        400,
        "[IMPLEMENTING] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let done_parent = issue_json(
        UMBRELLA,
        400,
        "[DONE] [UMBRELLA] Ship it",
        PROPOSAL_BODY,
        "closed",
        CLOSED_AT,
    );
    let open_leaf = issue_json(
        LEAF,
        410,
        "[LEAF OF 40] Implement it",
        &format!("{}\n\nWork remains.", umbrella_leaf_opening(UMBRELLA)),
        "open",
        BEFORE,
    );
    let closed_leaf = issue_json(
        LEAF,
        410,
        "[DONE] [LEAF OF 40] Implement it",
        &format!("{}\n\nDone.", umbrella_leaf_opening(UMBRELLA)),
        "closed",
        BEFORE,
    );

    let nested_child = issue_json(
        99,
        990,
        "[UMBRELLA] Nested child",
        PROPOSAL_BODY,
        "open",
        BEFORE,
    );
    let (client, server) = service(vec![
        response(200, &active_parent),
        response(200, refs(&[(99, 990, "open")])),
        response(200, refs(&[(99, 990, "open")])),
        response(200, &nested_child),
    ]);
    let Err(nested_error) =
        read_graph(&client, &Cancellation::new(), &repository(), UMBRELLA).await
    else {
        panic!("umbrella children must refuse");
    };
    assert_eq!(nested_error, "nested umbrellas are not supported");
    server.join().expect("nested graph stub completed");

    let (client, server) = service(vec![
        response(200, &active_parent),
        response(200, refs(&[(LEAF, 410, "open")])),
        response(200, "[]"),
    ]);
    assert!(
        read_graph(&client, &Cancellation::new(), &repository(), UMBRELLA,)
            .await
            .is_err()
    );
    server.join().expect("missing edge stub completed");

    let (client, server) = service(open_graph(&active_parent, &open_leaf));
    assert!(
        finish_remote(&client, &Cancellation::new(), &repository(), UMBRELLA,)
            .await
            .is_err()
    );
    server.join().expect("open leaf stub completed");

    let (client, server) = service(closed_graph(&done_parent, &closed_leaf));
    finish_remote(&client, &Cancellation::new(), &repository(), UMBRELLA)
        .await
        .expect("closed done graph is idempotent");
    server.join().expect("closed graph stub completed");

    let (client, server) = service(vec![
        response(200, &active_parent),
        response(200, "[]"),
        response(200, "[]"),
    ]);
    let arguments = LeafArguments {
        repository: String::from("o/r"),
        umbrella: UMBRELLA,
        leaf: GAP,
    };
    assert!(
        verify_child_remote(&client, &Cancellation::new(), &repository(), &arguments)
            .await
            .is_err()
    );
    server.join().expect("missing child stub completed");
}

#[test]
fn next_snapshot_covers_launch_audit_orphan_deadlock_and_inactive_refusal() {
    let directory = tempfile::tempdir().expect("temporary root");
    let parent = GitHubIssue {
        id: 400,
        number: UMBRELLA,
        title: String::from("[IMPLEMENTING] [UMBRELLA] Ship it"),
        body: String::from(PROPOSAL_BODY),
        state: GitHubIssueState::Open,
        state_reason: String::new(),
        url: String::from("https://github.com/o/r/issues/40"),
        author: String::from("author"),
        labels: Vec::new(),
        comments: 0,
        created_at: String::from(BEFORE),
        closed_at: String::new(),
        updated_at: String::from(BEFORE),
        is_pull_request: false,
    };
    let leaf = |number, state, open_blockers| LeafState {
        issue: GitHubIssue {
            number,
            state,
            title: format!("[LEAF OF 40] Leaf {number}"),
            ..parent.clone()
        },
        open_blockers,
    };
    let arguments = |name: &str| NextArguments {
        repository: String::from("o/r"),
        issue: UMBRELLA,
        output_root: directory.path().to_path_buf(),
        output: directory.path().join(name),
    };

    let mut active = leaf(42, GitHubIssueState::Open, Vec::new());
    active.issue.title = format!("{IMPLEMENTING_PREFIX}[LEAF OF 40] Leaf 42");
    let active_input = selection_leaves(&[active]);
    let active_selection = select_complete_umbrella_leaf(&active_input, &[]);
    assert_eq!(active_selection, CompleteUmbrellaNext::Deadlocked(vec![42]));
    assert_eq!(
        next_action_fields(&active_selection),
        vec![
            ("NEXT_ACTION", "deadlock".to_owned()),
            ("BLOCKED_LEAVES", "42".to_owned()),
        ]
    );

    emit_next(
        &arguments("launch.json"),
        &GraphState {
            parent: parent.clone(),
            leaves: vec![leaf(43, GitHubIssueState::Open, Vec::new())],
            open_orphan_blockers: Vec::new(),
        },
    )
    .expect("launch snapshot");
    emit_next(
        &arguments("audit.json"),
        &GraphState {
            parent: parent.clone(),
            leaves: Vec::new(),
            open_orphan_blockers: Vec::new(),
        },
    )
    .expect("audit snapshot");
    emit_next(
        &arguments("deadlock.json"),
        &GraphState {
            parent: parent.clone(),
            leaves: vec![leaf(44, GitHubIssueState::Open, vec![99])],
            open_orphan_blockers: Vec::new(),
        },
    )
    .expect("deadlock snapshot");
    emit_next(
        &arguments("orphan-blocker.json"),
        &GraphState {
            parent: parent.clone(),
            leaves: Vec::new(),
            open_orphan_blockers: vec![99],
        },
    )
    .expect("orphan-blocker snapshot");
    let mut inactive = parent;
    inactive.state = GitHubIssueState::Closed;
    assert!(
        emit_next(
            &arguments("inactive.json"),
            &GraphState {
                parent: inactive,
                leaves: Vec::new(),
                open_orphan_blockers: Vec::new(),
            },
        )
        .is_err()
    );

    let snapshot =
        fs::read_to_string(directory.path().join("launch.json")).expect("written snapshot");
    assert!(snapshot.contains("\"repository\": \"o/r\""));
    assert_eq!(join_numbers(&[3, 5, 8]), "3 5 8");
    assert_eq!(state_token(GitHubIssueState::All), "all");
}

#[test]
fn next_action_fields_preserve_valid_order_and_distinguish_orphan_blockers() {
    assert_eq!(
        next_action_fields(&CompleteUmbrellaNext::Launch(41)),
        vec![
            ("NEXT_ACTION", "launch".to_owned()),
            ("NEXT_LEAF", "41".to_owned()),
        ]
    );
    assert_eq!(
        next_action_fields(&CompleteUmbrellaNext::Audit),
        vec![("NEXT_ACTION", "audit".to_owned())]
    );
    assert_eq!(
        next_action_fields(&CompleteUmbrellaNext::Deadlocked(vec![43, 44])),
        vec![
            ("NEXT_ACTION", "deadlock".to_owned()),
            ("BLOCKED_LEAVES", "43 44".to_owned()),
        ]
    );
    assert_eq!(
        next_action_fields(&CompleteUmbrellaNext::OrphanBlocked(vec![42, 99])),
        vec![
            ("NEXT_ACTION", "orphan-blocker".to_owned()),
            ("ORPHAN_BLOCKERS", "42 99".to_owned()),
        ]
    );
}

#[cfg(unix)]
#[test]
fn expected_files_reject_symlinks_and_oversize_content() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("temporary root");
    let root = temporary_root(directory.path(), "root").expect("trusted root");
    let real = directory.path().join("real.txt");
    fs::write(&real, "four").expect("fixture");
    let link = directory.path().join("link.txt");
    symlink(&real, &link).expect("symlink fixture");
    let real_directory = directory.path().join("real-directory");
    fs::create_dir(&real_directory).expect("directory fixture");
    fs::write(real_directory.join("nested.txt"), "four").expect("nested fixture");
    let linked_directory = directory.path().join("linked-directory");
    symlink(&real_directory, &linked_directory).expect("directory symlink fixture");
    let nested_link = linked_directory.join("nested.txt");

    assert!(read_expected_file(&link, directory.path(), &root, "file", 8).is_err());
    assert!(read_expected_file(&nested_link, directory.path(), &root, "file", 8).is_err());
    assert!(write_private_file(&link, "changed", directory.path(), &root).is_err());
    assert!(read_expected_file(&real, directory.path(), &root, "file", 3).is_err());
    assert_eq!(
        read_expected_file(&real, directory.path(), &root, "file", 4)
            .expect("bounded regular file"),
        "four"
    );
}
