//! Domain types, use cases, and effect-free service ports for larch.

mod admission;
mod architectural_guidelines;
mod attestation;
mod audit_umbrella;
mod bgjob;
mod bgjob_daemon;
mod ci_timing;
mod complete_umbrella;
mod config;
mod context;
/// Debate-domain protocol vocabulary, ledger grammar, and fingerprints (#8597).
pub mod debate;
/// Design-domain plan grammar and plan-quality analysis core (#8575).
pub mod design;
mod difficulty;
mod difficulty_calibration;
mod env_file;
mod error;
mod external_defaults;
mod fluff_analysis;
mod git;
mod github;
mod github_actions;
mod github_auth;
/// `/implement` dispatch shared helpers, recovery paths, and step-checks mapping (#8610).
pub mod implement;
mod issue;
mod issue_mutation;
mod logging_util;
mod main_cache;
mod message_error;
mod migration_audit;
mod migration_governance;
mod object_store;
mod ordered_json;
mod outcome;
mod plan_scope;
mod process;
mod process_identity;
mod progress;
mod rebalance_tests;
mod redaction;
/// Rejected-finding corpus and verdict-wire domain contract shared by migration leaves.
pub mod rejected_analysis;
mod repo_size;
pub mod report;
mod retry;
/// Review-domain contracts shared by Rust-owned selectors and migration consumers.
pub mod review;
mod review_dispatch;
mod run_log;
mod session_env;
mod session_state;
mod shell;
mod slack_announce;
mod stall_recovery;
mod storage;
mod telemetry;
mod test_shards;
mod text;
mod time;
mod upgrade_larch;
mod vendor;
mod vendor_diagnostics;
mod vendor_failure;
mod vendor_model;
mod vendor_usage;
mod voter_calibration;

pub use admission::{
    DESIGNED_PREFIX, GateDecision, MANAGED_PREFIXES, entry_gate, has_designed_prefix,
    has_managed_prefix, has_report_prefix, normal_issue, parse_prose_blockers, single_line,
};
pub use architectural_guidelines::{
    ArchitecturalKind, ArchitecturalKnowledge, ArchitecturalStatus, GUIDELINE_HEADING_RE,
    GUIDELINES_FILENAME, INVARIANT_HEADING_RE, INVARIANTS_FILENAME, entry_text, knowledge_block,
    parse_entries,
};
pub use attestation::{
    ArtifactAttestationRequest, AttestationInputError, AttestationInputErrorKind,
    ImmutableReleaseAttestationRequest, ReleaseAssetSubject, ReleaseSourceCommit, ReleaseTag,
    VerifiedArtifactAttestation, VerifiedReleaseAttestation,
};
pub use audit_umbrella::{
    AUDIT_LEDGER_VERSION, AUDIT_PROPOSAL_VERSION, AUDIT_SNAPSHOT_VERSION, AuditDependency,
    AuditDependencyNode, AuditGraphState, AuditIssue, AuditIssueFingerprint, AuditLeaf,
    AuditLeafDraft, AuditLeafState, AuditLedger, AuditLedgerEntry, AuditLedgerSummary,
    AuditLedgerViolation, AuditProposal, AuditProposalDraft, AuditSnapshot, AuditSource,
    AuditSourceItem, AuditUmbrellaRefusal, INVALID_AUDIT_LEDGER, INVALID_AUDIT_PROPOSAL,
    INVALID_AUDIT_SNAPSHOT, MAX_AUDIT_ARTIFACT_BYTES, MAX_AUDIT_LEAF_BODY_BYTES,
    MAX_AUDIT_LEAF_TITLE_BYTES, MAX_AUDIT_LEAVES, MAX_AUDIT_REQUIREMENTS, MAX_AUDIT_SOURCES,
    RequirementStatus, SECURITY_SENSITIVE_AUDIT, STALE_AUDIT_PROPOSAL, audit_issue_fingerprint,
    audit_leaf_identity, audit_leaf_prefix, audit_ledger_sha256, audit_proposal_existing_numbers,
    audit_snapshot_sha256, audit_source_items, build_audit_proposal, diagnose_audit_ledger,
    mark_audit_graph_in_flight, mark_audit_leaf_in_flight, mark_audit_proposal_complete,
    parse_audit_ledger, parse_audit_proposal, parse_audit_snapshot, record_audit_leaf_resolved,
    render_audit_ledger, render_audit_proposal, render_audit_snapshot,
    replace_audit_issue_fingerprints, validate_audit_ledger, validate_audit_proposal_binding,
};
pub use bgjob::{
    BGJOB_COMPLETION_ENV_SUFFIX, BGJOB_COMPLETION_STAGE_SUFFIX, BGJOB_ELAPSED_KEY,
    BGJOB_INPUT_FP_SUFFIX, BGJOB_RC_KEY, BGJOB_REGISTRY_DIRNAME, BGJOB_RESULT_ENV_SUFFIX,
    BGJOB_STARTUP_ENV_SUFFIX, BGJOB_STATUS_DONE, BGJOB_STATUS_KEY, BGJOB_STATUS_STARTED,
    BGJOB_TMP_SUBDIR, BGJOB_WAIT_LEASE_SUFFIX, BGJOB_WORKER_STATUS_SUFFIX, BgjobError,
    CompletionTransaction, CompletionTransactionState, ENV_BGJOB_REGISTRY_ROOT,
    ENV_TEST_BGJOB_PHASE_BARRIER_DIR, JobSpec, LivenessVerdict, OwnerIdentity, RecoveryClaim,
    RecoveryLease, RegistryEntry, ResultEnvRows, WaitResult, bgjob_dir, checked_dir,
    child_identity_policy, child_liveness, claim_recovery, clear_completion_residue,
    completion_env_path, completion_result_is_visible, completion_stage_env_path, daemon_liveness,
    default_run_id, ensure_under, entry_expired, epoch_now, finish_completion_transaction,
    has_live_entry, iter_entries, log_paths, phase_barrier, prepare_completion_transaction,
    private_atomic_write, read_completion_transaction, read_confined_regular_tail,
    read_confined_result_env, read_entry, read_for, read_merge_result_env,
    recovery_claim_entry_path, refresh_wait_lease, refresh_wait_lease_for_pid, registry_path,
    registry_root, reject_line_value, release_recovery_claim, resolve_run_id, result_env_path,
    startup_env_path, unlink_entry, validate_initial_merge_rows, validate_merge_result_env,
    validate_run_id, validate_slug, validate_terminal_stdout_key, wait_lease_is_fresh,
    wait_lease_path, worker_status_path, write_entry, write_entry_at,
};
pub use bgjob_daemon::{
    BGJOB_DAEMON_POLL_INTERVAL_S, BGJOB_LOG_TAIL_BYTES, BGJOB_OWNER_GRACE_S,
    BGJOB_OWNER_VALIDATION_FAILURE_THRESHOLD, BGJOB_RC_ORPHANED, BGJOB_RC_TIMEOUT,
    BGJOB_START_EPOCH_KEY, BGJOB_STARTUP_ACK_TIMEOUT_S, BGJOB_STARTUP_GRACE_S, BGJOB_STATUS_DEAD,
    BGJOB_STATUS_WAIT, BGJOB_STEP_KEY, BGJOB_WAIT_DEFAULT_CHUNK_S,
    BGJOB_WAIT_HARD_DEADLINE_GRACE_S, BGJOB_WAIT_LEASE_TTL_S, BGJOB_WAIT_MAX_CHUNK_S,
    ENV_BGJOB_OWNER_PID, ENV_CLAUDE_PID, ENV_LARCH_CLAUDE_PID,
    ENV_TEST_BGJOB_DAEMON_POLL_INTERVAL_S, ENV_TEST_BGJOB_OWNER_GRACE_S,
    ENV_TEST_BGJOB_STARTUP_ACK_TIMEOUT_S, OwnerValidationState, OwnerValidationStep,
    check_owner_validation, daemon_poll_interval_s, log_tail, merge_rows, ordered_rows,
    orphan_diagnostic, owner_grace_s, owner_pid_candidate, redact_outbound, render_rows,
    result_rows, startup_ack_timeout_s, startup_in_progress, startup_rows,
    timing_override_or_default, validate_timing_overrides,
};
pub use ci_timing::{
    CiTimingRunSelection, HarnessBootstrapRow, HarnessTimingReport, HarnessTimingRow,
    JobTimingReport, JobTimingRow, MAX_CI_TIMING_REQUIRED_TARGETS, MAX_CI_TIMING_RUNS,
    NodeidTiming, PytestTimingReport, PytestTimingRow, ShardTiming, TargetTiming,
    collect_harness_timing, collect_job_timing, collect_pytest_timing,
};
pub use complete_umbrella::{
    COMPLETE_UMBRELLA_CHILD_COMPLETE, COMPLETE_UMBRELLA_CHILD_FAILURE_NEEDS_DESIGN,
    COMPLETE_UMBRELLA_CHILD_FAILURE_TRANSIENT_API, COMPLETE_UMBRELLA_CHILD_NEEDS_DESIGN,
    COMPLETE_UMBRELLA_NON_CANDIDATE_PREFIXES, CompleteUmbrellaLeaf, CompleteUmbrellaNext,
    DESIGNING_PREFIX, complete_umbrella_child_prompt, complete_umbrella_done_title,
    complete_umbrella_leaf_non_candidate, complete_umbrella_relaunch_title,
    complete_umbrella_start_title, has_umbrella_proposal, select_complete_umbrella_leaf,
    umbrella_leaf_opening, umbrella_leaf_prefix, validate_complete_umbrella_leaf,
    validate_complete_umbrella_parent,
};
pub use config::{GIT_COMMIT_CO_AUTHORED_BY_TRAILER, env};

pub use context::{RunId, RunIdError, RunIdErrorKind, RuntimeContext};
pub use design::{
    CANONICAL_TRAILER_ORDER, FIRM_HEADING_KINDS, FORCE_PLAN_CONTRACT_ERROR, HEADING_KINDS,
    HeadingEvent, HeadingKind, HeadingMatch, M1_DEFECT_TOKENS, M2_DEFECT_TOKENS,
    OPTIONAL_SIZE_TRAILER_KEYS, OVERSIZE_OVERRIDE_OPERATOR, OptionalMetadata,
    PLAN_COMMAND_TSV_HEADER, PLAN_DEFECT_ORDER, PLAN_SIZE_MAX_DIFF_ADDED, PLAN_SIZE_MAX_DIFF_LINES,
    PLAN_SIZE_MAX_FIRM_HEADINGS, PLAN_SIZE_MAX_PLAN_BODY_LINES, PLAN_SIZE_MAX_SURFACES,
    PlanCommandRow, PlanSizeAssessment, PlanTrailers, PlanValidationResult, TRAILER_KEYS,
    TrailerKey, TrailerMatch, TrailerValue, ValidationSummary, assess_plan_size,
    compose_plan_goals_test, compose_trailer_lines, drift_exceeds, drift_ratio_token,
    firm_heading_count, firm_heading_paths, grammar_prompt, is_fence_marker, iter_firm_headings,
    iter_heading_events, iter_plan_headings, iter_trailer_lines, match_heading, match_trailer_line,
    parse_final_trailers, parse_optional_metadata, parse_plan_commands, plan_surfaces,
    render_plan_command_tsv, set_oversize_override_text, validate_difficulty_metadata,
    validate_plan_contract,
};
pub use difficulty::{
    AUDIT_DENOMINATOR, BuildRecord, CODEX_MODEL_ROLE, CONFIDENCES, DifficultyFloor,
    DifficultyRating, FLOOR_MANIFEST_RELPATH, FloorMatch, FloorResult, HARD, MODERATE,
    MergeExplicit, RATIONALE_MAX_CHARS, RUBRIC, SCHEMA_VERSION, TIER_CEILING, TIERS, TRIVIAL,
    TierResolution, blank_merge_explicit, build_record, bump_for_confidence,
    codex_review_model_role, difficulty_line, dump_record, known_labels, label_for_tier,
    load_floor_manifest, load_record_data, match_floors, maybe_audit_upgrade,
    merge_existing_record_fields, next_tier, normalize_tier, panel_shape_for_tier, plan_difficulty,
    rating_from_tier, read_changed_paths, read_rating_file, refresh_existing_record,
    resolve_panel_tier, sanitize_rationale, threshold_panel_for_tier, tier_ceiling, tier_max,
    tier_rank, tier_valid, trailing_plan_difficulty, trailing_plan_metadata_lines,
    validate_rating_object, write_record_map,
};
pub use difficulty_calibration::difficulty_calibration_report;
pub use env_file::{
    CommentPolicy, CrStrip, DuplicateInputPolicy, DuplicatePolicy, EmptyKeyPolicy, EnvFile,
    KeyPolicy, KvDocument, KvError, KvErrorKind, KvRow, MalformedLinePolicy, ParseOptions,
    RenderOptions, WhitespacePolicy, kv_text, parse_allowlisted_env_line, parse_single_kv_row,
    select_kv_bytes, split_one_shell_token,
};
pub use error::{
    EnvironmentalFailure, ErrorCategory, FailureKind, InternalDefect, LarchError, OperatorError,
};
pub use external_defaults::{
    DebateSeat, ExternalDefaultError, ResolveResult, RoleDefault, RoleKind, debate_panel_seating,
    doc_rows, parse_bool_flag, resolve_vendor, role_default,
};
pub use fluff_analysis::{
    FluffOptions, PyTimestamp, fluff_analysis_report, parse_cutoff_text, parse_larch_version_tuple,
    parse_python_isoformat, sorted_paths_with_name_prefix,
};
pub use git::{
    Change, ChangeKind, ChangeSet, Commit, ConfigKey, ConfigScope, ConfigValue, ConflictKind,
    ConflictStage, GitMode, GitPath, Head, IgnoreKind, IgnoredEntry, IndexFlags, Object,
    ObjectHash, ObjectId, ObjectKind, RefFormat, RefName, Reference, ReferenceKind,
    ReferenceTarget, Remote, RepositoryError, RepositoryErrorKind, RepositoryLocation,
    RepositoryRead, RepositoryStatus, Revision, StatusOptions, TrackedEntry, UnmergedEntry,
    Upstream, Worktree,
};
pub use github::{
    ASSET_MEDIA_TYPE, AssetDigest, AssetStreamGuard, CheckBucket, CheckRun, GitHubActionsError,
    GitHubActionsErrorKind, GitHubActionsFuture, GitHubActionsService, GitHubCloseReason,
    GitHubComment, GitHubFailureInput, GitHubFuture, GitHubIssue, GitHubIssueBodyMode,
    GitHubIssueCreate, GitHubIssueEdit, GitHubIssueList, GitHubIssueListMode,
    GitHubIssueListResult, GitHubIssueListTimeoutScope, GitHubIssueScan, GitHubIssueSearch,
    GitHubIssueState, GitHubLabel, GitHubLabelCreate, GitHubListOutcome, GitHubListStop,
    GitHubMutationOutcome, GitHubOperationError, GitHubOperationErrorKind, GitHubRateLimitInputs,
    GitHubRepository, GitHubRepositoryRef, GitHubRequestKind, GitHubResponseLimits,
    GitHubRetryAction, GitHubService, GitHubTransportPolicy, ReconciledMutation, ReleaseDataError,
    ReleaseDataErrorKind, ReleaseState, RemoteAsset, TagObjectId, WorkflowDispatchRequest,
    WorkflowJob, WorkflowLogArchive, WorkflowRun, WorkflowRunFilters, classify_github_retry,
    reconcile_mutation, require_asset_content_type, resolve_issue_list, resolve_tag_object_id,
    select_release_for_staging, select_release_for_tag,
};
pub use github_actions::{RunLogsOutput, run_logs, run_logs_setup_failure, workflow_path};
pub use github_auth::{GitHubToken, GitHubTokenError, GitHubTokenErrorKind, acquire_github_token};
pub use implement::{
    CHECKS_TERMINAL_ACTIONS, DispatchState, RecoveryParse, RecoveryPorcelainInputs,
    STEP6_CHECKS_STEP, StepChecksSite, checks_step_for_site, clear_external_scout_paths,
    compute_recovery_paths, load_digest_map, manifest_legacy_fingerprint, parse_porcelain_z,
    path_under_submodule, public_args_for_site, rel_under_tmp, resolve_step_and_budget,
    resolve_step_name, resolve_tmpdir_path, sha256_file, tmpdir_rel_in_repo, write_bytes_atomic,
    write_digest_map,
};
pub use issue::{
    ACCEPTED_OOS_FILENAMES, BlockBoundary, CanonicalHeading, ConflictPlan, DispositionCounters,
    DispositionState, FILE_CONFLICT_DEFAULT_CLUSTER_CAP, FILE_CONFLICT_DEFAULT_GLOBAL_CAP,
    FileConflictEdge, FileConflictError, FileConflictRecord, HIGH_RISK_FOCUS_VALUES,
    INLINE_TRIAGE_MARKER, INLINE_TRIAGE_SOURCES, IssueCapError, ManifestObservation,
    OOS_CORRECTNESS_LABEL, OOS_CORRECTNESS_LABEL_COLOR, OOS_CORRECTNESS_LABEL_DESCRIPTION,
    OosBlock, OosDispositionCounts, OosItemKind, SerializedOos, analyze_run_dir, apply_issue_cap,
    count_filed_urls_strict_files, count_filed_urls_union_files, count_inline_triage_hits,
    count_inline_triage_occurrences, count_non_security_blocks, count_non_security_oos_blocks,
    count_rejected_oos_markers_from_ndjson, description_lines, existing_oos_titles,
    is_canonical_heading, is_high_risk_oos_block, is_oos_eligible_block, is_security_block_text,
    issue_number_from_url, issue_url_pattern, item_file_records, label_create_argv,
    next_oos_number, normalize_oos_block_header, normalize_title, observation_is_security,
    parse_canonical_heading, parse_conflict_cap, parse_intra_batch_deps, parse_oos_blocks,
    plan_file_conflict_deps, read_universal_newlines, render_deps_tsv, sanitize_public_text,
    serialize_accepted_oos, topological_create_order, validate_issue_cap_input,
};
pub use issue::{
    ADOPTED_UMBRELLA_SOURCE, AMBIGUOUS_IN_FLIGHT_RECOVERY, CLOSED_INPUT,
    COMPLETION_SENTINEL_VERSION, CandidateIssue, CompletionSentinel, DependencyEdge, ExpectedLeaf,
    INCOMPATIBLE_INPUT, INCOMPATIBLE_MANAGED_PARTITION, INCOMPATIBLE_UMBRELLA,
    INCOMPLETE_GRAPH_STATE, INVALID_COMPLETION_SENTINEL, INVALID_FINAL_UMBRELLA,
    INVALID_LEAF_NUMBER, INVALID_PREPARED_DEPENDENCIES, INVALID_PREPARED_PARTITION,
    INVALID_PROPOSAL_RECORD, INVALID_RESOLVED_LEAF, INVALID_UMBRELLA_NUMBER, LEAF_ALREADY_RESOLVED,
    LEAF_CAP_EXCEEDED, LeafState, MANAGED_PARTITION_PREFIXES, MAX_PREPARED_DEPS_BYTES,
    MAX_PREPARED_INPUT_BYTES, MAX_UMBRELLA_LEAVES, MIN_PREPARED_LEAVES, PREPARED_DEPENDENCY_CYCLE,
    PREPARED_PARTITION_TOO_LARGE, ProposalRecord, RemoteLeaf, ResolvedLeaf,
    STALE_COMPLETION_SENTINEL, STALE_PREPARED_PARTITION, UMBRELLA_PROPOSAL_TOKEN,
    UNKNOWN_LEAF_IDENTITY, UmbrellaRefusal, UmbrellaSnapshot, UmbrellaSourceKind, check_leaf_cap,
    classify_umbrella_source, completion_sentinel_for_record, expected_completion_sentinel,
    is_controlling_umbrella_title, is_managed_partition_title, is_umbrella_leaf_title,
    is_umbrella_title, leaf_identity, mark_leaf_in_flight, parse_proposal,
    prepare_proposal_from_batch, reconcile_in_flight, record_leaf_resolved, render_proposal,
    render_snapshot, umbrella_leaf_opening_text, validate_final_umbrella, verify_graph_state,
};
pub use issue::{
    ALLOWED_NAMED_BLOCK_MARKERS, ARCHIVAL_JQ_FILTER, BUG_PREFIX, BUG_TITLE_LIFECYCLE_PREFIXES,
    DESIGN_PAUSE_MARKER, DONE_PREFIX, IMPLEMENTING_PREFIX, LIFECYCLE_PREFIXES, MISSING_PLAN_BLOCK,
    MULTIPLE_PLAN_BLOCKS, NamedBlockDefect, NamedBlockError, NamedBlockSpan, NamedBlockWrite,
    NamedBlockWriteMode, OpenIssueRow, PLAN_MARKER, STALLED_PREFIX, UMBRELLA_PREFIX,
    bug_title_match, classify_named_block, compose_named_block, detect_lifecycle_prefix,
    insert_signal_marker, insert_tag_after_bug_prefix, is_valid_named_block_marker,
    issue_plan_marker_defect, leading_square_bracket_prefix, named_block_marker_allowed,
    neutralize_named_block_markers, normalize_title_prefix, open_issue_rows, parse_named_block,
    parse_open_issue_row, plan_named_block_write, redact_untrusted_stream, strip_lifecycle_prefix,
    strip_named_block, title_has_archival_report_prefix, title_is_archival,
    title_lifecycle_reject_marker, title_starts_with_brainstorm, untrusted_content_block,
    xml_escape_attr,
};
pub use issue::{
    AcceptedBlock, AcceptedSource, FiledIssue, LEGACY_PRIMARY_OOS_SOURCE, bare_oos_item_suffix,
    combined_block_count, dedupe_filed, is_capped_rollup_body, issue_covers_stable_id,
    ndjson_filed_evidence, normalized_title, priority_by_combined_item, priority_urls,
    render_blocks, render_oos_ndjson, render_recovery_evidence, render_sentinel, sentinel_urls,
    split_persisted_matches, split_to_github_limit, stable_identifier, stable_ids_by_combined_item,
    summarize_to_github_limit, working_batch, wrap_oos_body,
};
pub use issue::{
    BODY_CAP, CategoryCount, CategoryIndex, CategoryLabel, CategoryMode, ClassificationSource,
    CorpusFilter, CorpusScanStats, CoverageStats, EvidenceIndex, EvidenceOrdering, EvidenceSource,
    GateFailure, GroundTruthAnalysis, GroundTruthCorpusScan, GroundTruthEvidence, GroundTruthMode,
    GroundTruthOutcome, GroundTruthRow, GroundTruthStats, GroundTruthVoter, IncentiveEra,
    IssueCategory, IssueLifecycle, IssueSummary, LARGE_CORPUS_ROW_LIMIT, MatchProfile,
    NotLaterReason, OutcomeBucket, OutcomeDirection, PanelKind, PanelVerdict,
    STRIPPED_TITLE_PREFIXES, VerdictGateInputs, VoterBallot, VoterMetric, VoterSeverityMetric,
    accepted_finding_evidence, analyze_ground_truth, apply_verdict_gate, candidate_evidence,
    categorize, category_breakdown, category_pattern, classify_in_scope, coverage_stats,
    diagnostic_paths, distinctive_tokens, evidence_ordering, issue_evidence,
    normalize_diagnostic_path, parse_timestamp, percentile, realized_alignment_rate, run_dir_key,
    scan_ground_truth_corpus, strip_prefixes, strong_match, title_tokens, version_components,
    version_meets_floor,
};
pub use issue::{
    CANDIDATE_CAP, CandidateAllocation, CandidateRowDefect, CandidateRowDrop, ImplementationLease,
    InputMode, LeaseDefect, ParsedInput, ParsedItem, allocate_candidates,
    parse_implementation_lease, parse_issue_input, render_implementation_lease,
    upsert_implementation_lease,
};
pub use issue::{
    DEPS_GROUPS, DepsEdge, DepsFetchArtifacts, DepsFetchedIssue, DepsIssueMap, DepsLiveIssue,
    DepsMutation, DepsPartialAudit, DepsPlanInputs, GH_ERROR_CHARS, MUTATION_ERROR_CHARS,
    deps_compact_json, deps_edge_would_cycle, deps_explicit_refs, deps_failed_fetch,
    deps_fetch_artifacts, deps_flat_error, deps_group_for_title, deps_is_mutable_regular,
    deps_issue_map, deps_issue_title, deps_normal_edge, deps_partial_audit, deps_plan,
    deps_plan_writes_allowed, deps_pretty_json, deps_proposal_edges, deps_proposal_mutations,
    deps_revalidate_edge, deps_sanitize_outbound_body, deps_snapshot_numbers,
    deps_validate_snapshot_membership, deps_warning, json_positive_integer, parse_prose_blocks,
};
pub use issue::{
    MAX_TRIAGE_EVIDENCE_BYTES, MAX_TRIAGE_PROBE_BYTES, TRIAGE_MARKER_END, TRIAGE_MARKER_START,
    TRIAGE_PROBE_TIMEOUT_SECONDS, TRIAGE_TMP_PREFIX, TRIAGE_VERDICT_COMMENT_PREFIX, TRIAGED_TAG,
    TriageBlockDefect, TriageProbe, TriageProbeError, TriageSanitizeError,
    body_has_foreign_larch_marker, replace_triage_block, sanitize_triage_outbound,
    strip_triage_lifecycle_prefixes, triage_label_is_security, triage_probe_command,
    triage_text_is_security_sensitive, triage_title_has_lifecycle_prefix, triaged_title,
    validate_triage_evidence_path,
};
pub use issue_mutation::{
    CreatedIssue, IssueCreateRequest, IssueMutationError, IssueMutationField, IssueMutationLease,
    IssueMutationRequest, IssueMutationSnapshot, PlanReceiptIdentity, UMBRELLA_PROPOSAL_MARKER,
    VerifiedIssueMutation, mutation_postcondition, mutation_would_change,
    parse_plan_receipt_identity, redact_issue_create_request, redact_issue_mutation_request,
    redact_issue_text_outbound, same_mutation_identity, snapshot_is_strictly_newer,
    validate_issue_mutation_request, verify_authorized_body_change, verify_created_issue,
};
pub use logging_util::{emit_kv, sanitize_diagnostic_line};
pub use main_cache::{resolve_main_cache_merge_group_source, validate_main_cache_source_sha};
pub use migration_audit::{
    AggregateFinding, CommandAuditIssue, CommandAuditKey, DependencySnapshot, FindingCategory,
    IssueAuditEvidence, MIGRATION_AUDIT_COUNT_KEYS, MIGRATION_AUDIT_SCHEMA_VERSION,
    MigrationAuditDefect, MigrationAuditReport, MigrationAuditRequest, MigrationAuditSnapshot,
    MigrationIssueSnapshot, PLAN_DEFECT_TOKENS, PlanAuditEvidence, RepositoryAuditFinding,
    RepositoryFindingSource, RustLineBudgetDeviation, RustLineBudgetDeviationParse,
    build_command_audit_issue, build_migration_audit_report, parse_rust_line_budget_deviation,
    render_command_audit_input, render_migration_audit_json, render_migration_audit_table,
    validate_plan_facets,
};
pub use migration_governance::{
    BlockerSnapshotRow, FreshnessVerdict, GovernanceGateVerdict, GovernanceIssueSnapshot,
    LeaseAuditFinding, OwnerAdmissionRequest, OwnerAdmissionVerdict, OwnerBlock, OwnerBlockParse,
    OwnerKind, OwnerRow, ParityVerdict, PlanReceipt, PlanScopeDeclaration, PlanScopeKind,
    REASON_BLOCKER_READ_UNAVAILABLE, REASON_CLOSED_RETAINED, REASON_MISSING_NATIVE,
    REASON_MISSING_OWNER_BLOCK, REASON_OWNER_SCAN_UNAVAILABLE, REASON_REUSE_SOURCE_UNAVAILABLE,
    REASON_STALE_BLOCKER_SNAPSHOT, REASON_STALE_OWNER_SNAPSHOT, REASON_STALE_PLAN_BASE_SCOPE,
    REASON_STALE_PLAN_BODY, REASON_UNDOCUMENTED_NATIVE, ReceiptDefect, ReceiptFreshnessRequest,
    RepositoryName, RepositoryNameDefect, ScopeFile, ScopeFingerprintDefect, ScopeSnapshot,
    audit_stale_implementation_leases, compare_blocker_parity, compute_scope_fingerprint,
    declared_scope_paths, evaluate_governance_gate, evaluate_owner_admission, format_gate_refusal,
    hash_blocker_rows, hash_owner_rows, hash_plan_block, migration_requires_owner_block,
    normalize_state, owner_keys_from_rows, parse_native_blocker_refs, parse_owner_block,
    parse_receipt, plan_scope_declarations, receipt_marker_present, render_receipt, upsert_receipt,
    validate_receipt_freshness,
};
pub use object_store::{
    ObjectPage, ObjectStore, ObjectStoreError, ObjectStoreFuture, RemoteObject,
};
pub use ordered_json::OrderedJson;
pub use outcome::{ExitCode, WorkflowOutcome};
pub use plan_scope::{SCOPE_PATH_FALLBACK, extract_scope_paths};
pub use process::{
    ChildEnvironment, ExternalProcessRunner, ExternalProgram, GitCliOperation, GitHubCliOperation,
    HostUtilityProgram, LarchProgram, ProcessCancellation, ProcessError, ProcessErrorKind,
    ProcessEvent, ProcessEventKind, ProcessFuture, ProcessObserver, ProcessOutput, ProcessRequest,
    ProcessRequestError, ProcessRequestErrorKind, ProcessStatus, PythonVerbProgram, ScannerProgram,
    VendorProgram,
};
pub use process_identity::{
    COMMAND_LOG_LIMIT, DESIGN_STEP3_KILL_LOG_FILE, DESIGN_STEP3_LOOP_IDENTITY_FILE,
    DESIGN_STEP3_MISSING_PID_GRACE, DESIGN_STEP3_WRAPPER_DETACHED_FILE, FINALIZE_KILL_LOG_FILE,
    IMPLEMENT_STEP5_KILL_LOG_FILE, IMPLEMENT_STEP5_LOOP_IDENTITY_FILE,
    IMPLEMENT_STEP5_WRAPPER_DETACHED_FILE, IdentityProbeOutput, KillLogEvent, KillTargetSnapshot,
    PROCESS_IDENTITY_CAPTURE_ATTEMPTS, PROCESS_IDENTITY_CAPTURE_SLEEP, PROCESS_IDENTITY_PS_TIMEOUT,
    PS_LSTART_FIELD_COUNT, ProcessBirthIdentity, ProcessBirthIdentityProbeOutput,
    ProcessIdentityHost, ProcessIdentityProbeResult, ProcessIdentityValidationPolicy,
    RecordedProcessIdentity, TERMINATE_CONFIRM_POLL, TERMINATE_CONFIRM_TIMEOUT,
    TERMINATE_ESCALATION_SLEEP, TerminateSignal, TerminationResult, ValidationResult,
    append_kill_log, await_loop_identity, await_loop_poll, bounded_command, collect_descendants,
    collect_process_group_members, collect_process_group_members_checked,
    confirm_process_group_absent, identity_to_json, kill_session_background_processes,
    normalize_command_signature, parse_ps_identity, probe_process_identity, read_identity_record,
    read_process_identity, read_stable_process_identity, result_env_has_step3_status,
    teardown_loop_identity, terminate_validated_process_group,
    terminate_validated_process_group_and_confirm, terminate_validated_process_group_with_policy,
    validate_process_identity, validate_process_identity_with_policy, write_identity_record,
    write_loop_identity, write_step5_loop_identity,
};
pub use progress::{
    CURRENT_RUN_FILENAME, CURRENT_RUN_LOCK_FILENAME, DEFAULT_HIDE_AFTER_S, DEFAULT_STALE_AFTER_S,
    MAX_STATUSLINE_LINES, PROGRESS_DIRNAME, RESET_SESSION_SOURCES, RUN_BREADCRUMB_FILENAME,
    STATUSLINE_COMMAND_MARKER, STATUSLINE_DISABLE_ENV, STATUSLINE_LOCAL_SETTINGS,
    STATUSLINE_VERB_MARKER, StalenessDecision, apply_statusline, breadcrumb_line,
    chained_user_command, classify_staleness, install_payload_directory, is_breadcrumb_row,
    is_larch_statusline_command, positive_int, progress_clone_digest, progress_run_id_error,
    render_statusline_body, resets_active_run, settings_statusline_command, stale_suffix,
    statusline_launcher_text, statusline_payload_directory, truncate_columns,
    validate_progress_run_id,
};
pub use rebalance_tests::{
    REBALANCE_TESTS_SCHEMA_VERSION, RebalanceJsonResult, plan_json, verify_json,
};
pub use redaction::{
    RedactionResult, RuntimeRedactor, SafeText, contains_recognized_session_tmpdir_pointer, redact,
    redact_run_log_payload, redact_secrets, redact_secrets_only, redact_sensitive_paths,
};
pub use repo_size::{
    RepoSizeCategory, RepoSizeReport, count_newlines, is_rust_line_count_path, line_count_category,
    rust_line_split,
};
pub use report::{
    BLENDED_FALLBACK_WARNING, BlockMarkers, BlockMarkersError, BlockMarkersErrorKind,
    CODEX_IMPLEMENT_RAW_LABEL, CODEX_MINI_MODELS, CURSOR_COMPOSER_BASE_RATES, CURSOR_GROK_MODELS,
    CURSOR_IMPLEMENT_RAW_LABEL, CURSOR_TEAMS_TOKEN_RATE_SURCHARGE_PER_M, ClaudeCounts, CodexCounts,
    CursorCounts, EMPTY_GROUPS, IMPLEMENT_STEP2_LABEL, IMPLEMENT_STEP2_PREFIX, IssueDetail,
    IssueDetailGroups, IssueEvent, LoadResult, MAX_DEDUPE_KEY_LEN, MAX_DISPLAY_LEN,
    MAX_TRANSCRIPT_INPUT_BYTES, MAX_TRANSCRIPT_RECORD_BYTES, PathWarning, RATE_TABLE, RateRow,
    RenderedTranscript, RunCost, RunLogBatchArtifact, RunLogBatchMode, RunLogBatchSanitizer,
    RunLogBatchSpec, RunLogCorpus, RunLogCorpusEvent, RunLogCorpusIter, RunLogCorpusWarning,
    RunLogCorpusWarningKind, RunLogFileIter, RunLogManifest, RunLogRoundSort, RunLogRun,
    RunLogSelection, RunLogTimeWindow, RunLogTimeWindowError, TOKEN_VENDORS, TRANSCRIPT_POLICY,
    TRANSCRIPT_SCHEMA_VERSION, TokenCorpusScan, TokenCostError, TokenCostValues, TokenCounts,
    TokenObservation, TokenObservationKind, TokenObservations, TokenPhaseRow, TokenRates,
    TokenReportError, TokenReportInputs, TokenRunRecord, TokenScanEvent, TokenScanWarning,
    TokenScanWarningKind, TokenStepMark, TokenUsageRow, TokenVendor, TranscriptError,
    TranscriptWarnings, VendorTotals, WARN_CATEGORY, aggregate_vendor_tokens,
    build_issue_detail_section, build_report_from_ledgers, claude_effective_cache_create,
    claude_usage_rows, count_issue_groups, count_load_result, cursor_buckets_are_detailed,
    display_rates, effective_vendor_total, exact_rate_row, execution_issue_identity, fallback_cost,
    format_money, full_report, full_report_with_observations, ledger_step_marks,
    ledger_vendor_rows, load_issue_detail_groups, manifest_only_larch_version,
    manifest_only_started_at_text, normalize_body_for_hash, parse_epoch,
    parse_markdown_execution_issues, parse_preterminal_outcome_label, price_counts, price_run,
    python_round, rate_row, read_ledger, read_report_inputs, render_cost_kv, render_cost_line,
    render_issue_detail_block, render_session_transcript, render_token_report_buckets,
    render_token_report_json, render_token_report_markdown, render_token_report_summary_line,
    render_token_report_terse, replace_markdown_block, replace_markdown_block_with_warn,
    report_has_numeric_tokens, resolve_run_report, round_number_from_path, run_log_batch_spec,
    run_log_batch_specs, run_log_ledger_path, run_record, run_started_at_strict,
    run_started_at_without_manifest, safe_int, sanitize_diagram_capture, strip_diagram_sections,
    structured_body_dedupe_keys, summary_report, token_phase_rows, token_report_basename,
    transcript_sources, vendor_totals_from_report,
};
pub use retry::{
    AttemptOutcome, DeterministicJitter, Jitter, RetryClass, RetryDecision, RetryObservation,
    RetryPolicy, RetryPolicyError, StopReason,
};
pub use review_dispatch::{
    DiffMode, GENERATORS_TSV_COLUMNS, GeneratorsTsvError, ReviewerWaitConfig, ReviewerWaitHost,
    ReviewerWaitResult, ReviewerWaitRow, SUSPEND_REFUND_SECONDS,
    WAIT_DEFAULT_POLL_INTERVAL_SECONDS, WAIT_DEFAULT_TIMEOUT_SECONDS, classify_diff,
    parse_generated_paths, wait_for_reviewers, wait_max_polls,
};
pub use run_log::{
    AssessmentKind, BATCH_GUIDELINE_SHIP_OUTCOME, BATCH_INVARIANT_SHIP_OUTCOME, BatchInfo,
    BatchMode, BatchName, BatchPayloadError, CompletenessOutcome, EXECUTION_ISSUE_CATEGORIES,
    ExecutionIssueEntry, ExecutionIssueFormat, ExecutionIssueLedger, ExecutionIssueReadError,
    ExecutionIssueReadErrorKind, FAILURE_CATEGORIES, FailureEntry, LIFECYCLE_CONTEXT_BASENAME,
    LIFECYCLE_CONTEXT_SCHEMA_VERSION, LIFECYCLE_SCHEMA_VERSION, LifecycleContext, LifecycleError,
    LifecycleOutcome, ManifestDocument, ManifestFormatVersion, ManifestReadError,
    ManifestReadErrorKind, ManifestRecord, ManifestUpdate, ManifestV2Seed, ManifestWriteError,
    ReachabilityContext, RecordLabels, RedactionRefusal, ResidualSecretError, RoundNumber,
    RoundNumberError, RunLogLayout, RunLogSlug, RunLogSlugError, RunLogSlugErrorKind,
    SHIP_OUTCOME_CUTOVER_VERSION, Sanitizer, UNIVERSAL_EXECUTION_ISSUES, UNIVERSAL_FINAL_REPORT,
    UNIVERSAL_SESSION_TRANSCRIPT, batch_contains_all_sections, compose_execution_issue,
    compose_failure_entry, condition_reached, execution_issue_body_keys, execution_issue_chunks,
    execution_issue_records, execution_issue_sections, existing_execution_issue_keys,
    failure_retry_suffix, final_summary_terminal_heading, first_nonempty_line, glob_matches,
    implementation_plan_body, is_round_sidecar_file, lookup_batch, manifest_pr_evidence_matches,
    normalize_run_log_text, normalized_body_sha256, redact_batch_payload, round_artifact_included,
    scan_required_files, stage_round_artifact, stale_bail_heading_with_pr_evidence,
    terminal_bail_skip_signal, validate_batch_payload, validate_failure_counts,
    validate_run_log_slug, validate_ship_outcome_record,
};
pub use session_env::{
    DIFFICULTY_CHOICES, MAX_PATH_VALUE_LEN, RESTORE_FINALIZE_KEYS, RUN_FLAG_KEYS, RunParams,
    WRITE_DESIGN_ENV_KEYS, WRITE_ENV_KEYS, design_run_launcher_text, export_line, external_timeout,
    implement_run_launcher_text, is_bool, is_strict_run_id, is_valid_claude_pid,
    is_valid_plugin_root_value, is_valid_repo_value, is_valid_run_id, is_valid_session_id,
    parse_bool_arg, plugin_root_env_text, posix_path_display, restore_finalize_default,
    run_params_json, validate_no_newlines, validate_path_arg_value, validate_repo_root_value,
    validate_writer_keys,
};
pub use session_state::{
    IMPLEMENT_SENTINEL_RELATIVE_PATHS, IMPLEMENT_TMPDIR_PREFIX, IMPLEMENT_TMPDIR_TTL_SECONDS,
    allowed_session_roots, cleanup_cache_sessions_root, design_tmpdir_syntax_error,
    implement_session_roots, implement_tmpdir_ttl, prefers_implement_candidate,
    session_pointer_root,
};
pub use shell::shell_quote;
pub use slack_announce::{
    LARCH_SLACK_WEBHOOK_URL, SlackAnnounceInputs, SlackAnnounceStatus, SlackIssueAnnouncement,
    inputs_from_files, mark_posted, parent_issue_path, plan_slack_issue_announce,
    read_kv_from_text, redact_webhook_url, session_id_path, ship_state_path, transport_failure,
    webhook_scheme_allowed,
};
pub use stall_recovery::{
    BUG_TITLE_PREFIX, Classification, ClassifyTextInput, FileFailureReport, IssueNormalization,
    NormalizeOutcomeInput, artifact_prefix_valid, classification_signature, classify_text,
    failure_detail_sidecar_name, first_nonempty, implementation_bail_tokens,
    is_terminal_merge_result, normalize_file_failure_report, normalize_issue_output,
    normalize_outcome_values, public_text_is_sensitive, resume_hint_for, retry_policy,
    safe_bail_value, safe_dispatcher_value, safe_pattern_value, safe_phase, safe_phase_value,
    safe_step, safe_step_value, state as state_value, terminal_state_valid, token_valid, truthy,
};
pub use storage::{
    ENV_LARCH_LOGS_URI, ENV_LARCH_R2_ACCOUNT_ID, ENV_LARCH_R2_ENDPOINT, ENV_LARCH_STORAGE_BASE_URI,
    LARCH_TOOL_NAME, LOCAL_NAMESPACE_DOMAIN, RUN_LOGS_DATA_TYPE, RunLogStorageMode,
    RunLogStorageReason, RunLogStorageResolution, STORAGE_BASE_URI_FIELD, STORAGE_CONFIG_RELPATH,
    STORAGE_URI_SCHEMES, StorageBase, StorageConfigurationError, StoragePreflightError,
    ToolRepositoryStorage, format_preflight_stdout, injected_storage_resolution,
    local_namespace_id, parse_storage_base_uri, parse_tool_repository_uri,
    repository_leaf_from_remote, require_enabled_storage, resolve_run_log_storage,
    validate_client_repo,
};
pub use telemetry::{Breadcrumb, JournalRecord, RecordError, RecordErrorKind};
pub use test_shards::{
    TestShardMap, TestShardTiming, pack_test_shards, pack_test_shards_with_fixed_startup,
    read_makefile_shards, rewrite_makefile_shards,
};
pub use text::{
    balanced_fence_line_indices, bounded_ascii_identifier, ensure_ascii_json, fence_marker,
    file_line_regex, is_positive_decimal, is_python_whitespace, positive_integer, python_bigint,
    python_float, python_int, python_str, split_lines_keep_ends, split_text_lines, tail_lines,
    trim_python_whitespace, truncate_utf8_bytes, universal_newlines, unsigned_integer,
};
pub use time::{AsyncClock, BusinessClock, Deadline, MonotonicClock, MonotonicTime, Sleep};
pub use upgrade_larch::{
    ActiveRootState, InstalledVersionState, MarketplaceState, UpgradeDisposition,
    classify as classify_upgrade,
};
pub use vendor::{
    CAP_HIT_PAYLOAD, CLAUDE_DESCRIPTOR, CODEX_DESCRIPTOR, CODEX_DRAFTER_TRUSTED_INSTRUCTIONS,
    CODEX_POLICY_REJECTION_EXCERPT_BYTES, CODEX_POLICY_REJECTION_TAIL_BYTES,
    CODEX_PROBE_GATE_IMMEDIATE_TTL, COLLECTOR_NS_STRONG_HEADER, CURSOR_AUTH_MAX_ATTEMPTS,
    CURSOR_AUTH_RETRY_DELAY, CURSOR_DEGRADED_OUTPUT_TOKEN_FLOOR, CURSOR_DEGRADED_RESPONSE,
    CURSOR_DEGRADED_RESULT_BYTES_CEILING, CURSOR_DESCRIPTOR, CURSOR_EMPTY_RESPONSE,
    CURSOR_KEYCHAIN_ACCOUNT, CURSOR_KEYCHAIN_SERVICE, CURSOR_MODEL_LIST_ARGV,
    CURSOR_MODEL_LIST_HEADER, CURSOR_NO_ISSUES_JSON, CURSOR_NO_WORK_INPUT_TOKEN_FLOOR,
    CURSOR_PREFLIGHT_AUTH_RC, CURSOR_PREREAD_FAIL_MSG, CURSOR_PREREAD_FAIL_RC, CapHitArtifacts,
    CheckReviewersConfig, CheckReviewersResult, ClaudeEnvelopeStatus, CodexEnvAuth,
    CodexGateMessage, CodexProbeAttempt, CodexProbeLoop, CodexPromptSentinelRead,
    CodexPromptSidecarArgs, CodexReviewAuthPort, CodexSessionParseError,
    CursorCreateChatParseError, CursorCredential, CursorModelListOutcome, CursorPreflightFailure,
    CursorProbeLoop, CursorResultWrite, CursorReviewAuthPort, CursorStallRecord,
    DEFAULT_CURSOR_LAUNCH_JITTER_MS, DIALECTIC_RAW_PENDING_FILE, DRAFTER_REASON_ABSENT,
    DegradedToolsResult, DirtyBaselineCapturePlan, DirtyTreeBaselinePort, DrafterDialectic,
    DrafterDirtyTree, DrafterParse, DrafterParseError, DrafterScout, DrafterStatus,
    DrafterTimeoutError, ENV_CURSOR_LAUNCH_JITTER_MS, ENV_CURSOR_RETRY_EMPTY_RESULT,
    ENV_TOKEN_BUDGET_CAP_REVIEW, ENV_TRANSIENT_RETRY_DELAY,
    EXTERNAL_HEALTH_CHECK_TIMEOUT_DEFAULT_SEC, EXTERNAL_TOOL_NAMES, ExternalAuthVerdict,
    HostPlatform, LaunchTimingRecord, MAX_DRAFTER_TIMEOUT_SECONDS, MODEL_PINS_STATUS_LIST_FAILED,
    MODEL_PINS_STATUS_OK, MODEL_PINS_STATUS_SKIPPED, MODEL_PINS_STATUS_UNKNOWN_ID,
    MODEL_PINS_STATUS_UNPARSEABLE, MODEL_PINS_STATUS_UNVERIFIABLE, ModelPinsReport,
    NO_OPEN_BROWSER_ON, PROBE_AUTH_RETRY_RC, PROBE_NO_RETRY_RC, PROBE_TIMEOUT_EXIT_CODE,
    PROBE_TRANSIENT_RC, PinnedModel, ProbeConclusion, ProbeRetryLimits, ProbeStep, ProbeTtl,
    REQUIRED_CAPABILITIES, REVIEW_MAX_TRANSIENT_RETRIES, ResearchOutputValidator,
    RetryArtifactResetPlan, ReviewAuthVerdict, ReviewPreflightRefusal, SpecialistRenderPort,
    StreamResetPlan, SyncLauncherHooks, TOKEN_SIDECAR_ENV_UNSET, TierAttempt, TierLaunchError,
    TierLaunchInput, TimeoutStallRecord, TimingTaskKind, TimingTaskKindError, TokenSidecarIngest,
    VENDOR_DESCRIPTORS, VendorArgv, VendorArgvError, VendorArgvErrorKind, VendorCapCheckResult,
    VendorConfigurationGuard, VendorDescriptor, VendorDescriptorError, VendorDescriptorErrorKind,
    VendorFamilyHooks, VendorHookFuture, VendorLaunchContext, VendorLaunchError,
    VendorLaunchOutcome, VendorLaunchRequest, VendorLaunchStatus, VendorLifecycleHooks,
    VendorModelPinResult, VendorParsedResult, VendorPostHook, VendorProcessResult,
    VendorRetryClassification, VendorRetryPolicy, VendorSessionError, VendorSessionErrorKind,
    VendorSessionHandle, VendorSessionVendor, WaterfallResult, binary_on_path,
    build_check_budget_argv, build_claude_argv, build_codex_argv, build_codex_resume_argv,
    build_codex_session_argv, build_cursor_argv, build_cursor_create_chat_argv,
    build_cursor_resume_argv, build_launch_argv, build_record_launch_timing_argv,
    build_vendor_registry, check_token_budget_cap, codex_auth_args, codex_compact_sentinel_offset,
    codex_env_auth_from_key, codex_gate_detail_file_name, codex_pinned_model_declarations,
    codex_pinned_models, codex_policy_rejection_excerpt, codex_probe_identity,
    codex_probe_update_lock_file_name, cursor_child_environment, cursor_has_structured_findings,
    cursor_input_work_tokens, cursor_keychain_arguments, cursor_launch_jitter_ms,
    cursor_line_no_issues, cursor_normalize_no_issues, cursor_output_tokens,
    cursor_pinned_model_declarations, cursor_pinned_models, cursor_preflight_failure_message,
    cursor_preflight_refusal, cursor_result_is_no_issues, drafter_model_allowed,
    drafter_path_text_allowed, drafter_token_raw_label, effective_review_token_cap,
    elapsed_minute_message, external_auth_verdict, extract_model_from_argv, fresh_probe_verdict,
    ingest_launcher_token_sidecar, is_cursor_empty_result, is_transient_claude_api_error,
    keychain_credential, launch_tier, list_failed_detail, model_list_timeout_seconds, norm_bool,
    norm_tristate, ordered_tiers, outcome_exit_code, parse_claude_envelope,
    parse_codex_gate_detail, parse_codex_session_id, parse_cursor_create_chat_id,
    parse_cursor_model_list, parse_drafter_output, plan_capture_cursor_dirty_baseline,
    plan_contains_standalone_scout_manifest, plan_cursor_result_write, plan_retry_artifact_reset,
    plan_stream_reset, probe_attempt_rc, probe_cache_user, probe_stamp_contents,
    probe_stamp_file_name, python_json_dumps, read_codex_prompt_sentinel, render_cap_hit_artifacts,
    render_clean_readonly_dirty_tree, render_codex_gate_detail, render_codex_prompt_sidecar,
    render_cursor_degraded_diag, render_cursor_empty_response, render_cursor_no_work_diag,
    render_cursor_stall_json, render_drafter_dirty_tree, render_drafter_status,
    render_preflight_bundle, render_timeout_stall_json, render_unknown_dirty_tree,
    resolve_codex_model_pins, resolve_codex_review_model, resolve_cursor_model_pins_from_list,
    resolve_model_pins, resolve_probe_workdir, review_retry_delay_secs, run_codex_review_preflight,
    run_cursor_review_preflight, run_ready_launch, run_vendor_launch, run_waterfall,
    run_with_vendor_retries, sanitize_tool_label, state_phrase, strip_codex_config,
    terminal_diff_lines, terminal_plan_trailer_value, token_sidecar_ingest_env, tool_state,
    transient_probe_retries, trust_config_arg, validate_drafter_timeout,
    write_cursor_dirty_tree_from_baseline,
};
pub use vendor_diagnostics::{
    FAILED_AGENT_STDERR_TAIL_BYTE_CAP, FAILED_AGENT_STDERR_TAIL_LINES, FailureDiagSource,
    FailureDiagWrite, LauncherArtifactKind, LauncherArtifactPaths, StderrCaptureMode,
    VENDOR_FAILURE_DIAG_BYTE_CAP, VENDOR_FAILURE_DIAG_SECTION_LINES, compose_failure_diag,
    failed_agent_stderr_candidates, failure_diag_section_body, failure_diag_source_order,
    failure_diagnostic_source_candidates, plan_failure_diag_write, render_failed_agent_stderr_tail,
    stream_reset_history_entry,
};
pub use vendor_failure::{
    AuthVerdict, CodexGateDetail, CodexGateSignal, FailureClass, FailureReason, LaunchFailure,
    LaunchFailureInputs, LauncherArtifact, LauncherExitArtifacts, classify_launch_failure,
    detect_codex_cli_gate, effective_failure_class, is_quota_failure, is_transient_infra_failure,
    parse_launcher_exit_text, parse_launcher_failure_class, resolve_launcher_exit,
};
pub use vendor_model::{
    CLAUDE_FABLE_5_MODEL, CLAUDE_GLM_5_2_1M_MODEL, CLAUDE_GLM_5_2_MODEL, CLAUDE_HAIKU_4_5_MODEL,
    CLAUDE_OPUS_4_8_MODEL, CLAUDE_SONNET_4_6_1M_MODEL, CLAUDE_SONNET_4_6_MODEL,
    CODEX_DEFAULT_MODEL, CODEX_FIX_MODEL_DEFAULT, CODEX_REVIEW_MODEL_DEFAULT,
    CODEX_VOTE_MODEL_DEFAULT, CURSOR_DEFAULT_MODEL, CURSOR_GROK_4_6_HIGH_MODEL, CodexModelRole,
    DEBATE_CLAUDE_MODEL, DEBATE_CODEX_MODEL, DEBATE_CURSOR_MODEL, ModelArgError, ModelArgResult,
    ModelTool, canonicalize_glm_main_model, claude_model_from_transcript, claude_sub_default_model,
    is_posix_cntrl, normalize_claude_ledger_model, resolve_model_args,
    transcript_path_from_claude_source, validate_emitted_token,
};
pub use vendor_usage::{
    ClaudeUsageTotals, UsageParseError, UsageTotals, json_usage_number, parse_claude_usage,
    parse_codex_usage,
};
pub use voter_calibration::{
    EraBoundaryDisplay, VoterCalibrationCorpus, render_era_boundary_unavailable,
    render_voter_calibration_era_report, render_voter_calibration_report,
    voter_agreement_row_from_panel,
};

/// Immutable metadata about the running larch build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildMetadata {
    version: &'static str,
    target: &'static str,
}

impl BuildMetadata {
    /// Create metadata for a compile-time version.
    #[must_use]
    pub const fn new(version: &'static str, target: &'static str) -> Self {
        Self { version, target }
    }

    /// Return the build version.
    #[must_use]
    pub const fn version(self) -> &'static str {
        self.version
    }

    /// Return the compilation target triple.
    #[must_use]
    pub const fn target(self) -> &'static str {
        self.target
    }
}

/// Render the machine-readable identity checked by the installation shim.
#[must_use]
pub fn bootstrap_self_check(metadata: BuildMetadata) -> String {
    format!(
        "{{\"schema_version\":1,\"version\":\"{}\",\"target\":\"{}\"}}",
        metadata.version(),
        metadata.target()
    )
}

/// Non-production use cases that prove command dispatch and library wiring.
pub mod example {
    /// Return a caller-owned message unchanged.
    #[must_use]
    pub const fn echo(message: &str) -> &str {
        message
    }
}

#[cfg(test)]
mod tests {
    use super::{BuildMetadata, bootstrap_self_check, example};

    #[test]
    fn build_metadata_preserves_the_version() {
        let metadata = BuildMetadata::new("1.2.3", "aarch64-apple-darwin");

        assert_eq!(metadata.version(), "1.2.3");
        assert_eq!(metadata.target(), "aarch64-apple-darwin");
    }

    #[test]
    fn bootstrap_self_check_is_compact_machine_readable_json() {
        let metadata = BuildMetadata::new("1.2.3", "x86_64-unknown-linux-gnu");

        assert_eq!(
            bootstrap_self_check(metadata),
            r#"{"schema_version":1,"version":"1.2.3","target":"x86_64-unknown-linux-gnu"}"#
        );
    }

    #[test]
    fn example_echo_preserves_the_message() {
        assert_eq!(example::echo("library wiring"), "library wiring");
    }
}
