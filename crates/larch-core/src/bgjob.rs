//! Durable background-job records, registry storage, and path validation.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fmt,
    fs::{self, OpenOptions},
    io::{Read as _, Seek as _, SeekFrom, Write as _},
    path::{Component, Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use sha2::{Digest as _, Sha256};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};

use crate::{
    DuplicatePolicy, KvDocument, ParseOptions, ProcessBirthIdentity, ProcessIdentityHost,
    ProcessIdentityValidationPolicy, RecordedProcessIdentity, identity_to_json,
    read_process_identity, validate_process_identity, validate_process_identity_with_policy,
};

/// Registry directory below the larch cache root.
pub const BGJOB_REGISTRY_DIRNAME: &str = "daemons";
/// Session-local subdirectory that holds bgjob wire files.
pub const BGJOB_TMP_SUBDIR: &str = "bgjob";
/// Completed-result suffix.
pub const BGJOB_RESULT_ENV_SUFFIX: &str = ".result.env";
/// Durable completion-transaction descriptor suffix.
pub const BGJOB_COMPLETION_ENV_SUFFIX: &str = ".completion.env";
/// Private staged result suffix used while a completion transaction is pending.
pub const BGJOB_COMPLETION_STAGE_SUFFIX: &str = ".completion-stage.env";
/// Daemon-startup marker suffix.
pub const BGJOB_STARTUP_ENV_SUFFIX: &str = ".startup.env";
/// Input-fingerprint sidecar suffix.
pub const BGJOB_INPUT_FP_SUFFIX: &str = ".input-fp";
/// Durable worker-completion witness suffix.
pub const BGJOB_WORKER_STATUS_SUFFIX: &str = ".worker-status.env";
/// Foreground-wait lease suffix. An active `bgjob wait` refreshes this file so
/// the daemon does not orphan a live job merely because the start-time session
/// owner PID exited (#8639).
pub const BGJOB_WAIT_LEASE_SUFFIX: &str = ".wait-lease.env";
/// Environment override for the durable registry root.
pub const ENV_BGJOB_REGISTRY_ROOT: &str = "LARCH_BGJOB_REGISTRY_ROOT";
/// Debug-build-only directory used by deterministic bgjob lifecycle tests.
///
/// A production release ignores this value. A private `<phase>.armed` marker
/// selects one test phase; that phase writes its PID to `<phase>.reached` and
/// waits for the matching `<phase>.release` file.
pub const ENV_TEST_BGJOB_PHASE_BARRIER_DIR: &str = "LARCH_TEST_BGJOB_PHASE_BARRIER_DIR";
/// Stable completion status key.
pub const BGJOB_STATUS_KEY: &str = "BGJOB_STATUS";
/// Stable started status token.
pub const BGJOB_STATUS_STARTED: &str = "STARTED";
/// Stable completed status token.
pub const BGJOB_STATUS_DONE: &str = "DONE";
/// Stable child result-code key.
pub const BGJOB_RC_KEY: &str = "BGJOB_RC";
/// Stable elapsed-seconds key.
pub const BGJOB_ELAPSED_KEY: &str = "BGJOB_ELAPSED_S";

const COMPLETION_STATE_PREPARED: &str = "PREPARED";
const COMPLETION_STATE_COMMITTED: &str = "COMMITTED";
const COMPLETION_STATE_KEY: &str = "STATE";
const COMPLETION_RESULT_SHA256_KEY: &str = "RESULT_SHA256";
const COMPLETION_SENTINEL_COUNT_KEY: &str = "SENTINEL_COUNT";
const COMPLETION_SENTINEL_PREFIX: &str = "SENTINEL_";
const REGISTRY_SENTINEL_COUNT_KEY: &str = "SENTINEL_COUNT";
const REGISTRY_SENTINEL_PREFIX: &str = "SENTINEL_";
const REGISTRY_MERGE_RESULT_ENV_KEY: &str = "MERGE_RESULT_ENV";
const REGISTRY_TERMINAL_STDOUT_KEY: &str = "TERMINAL_STDOUT_KEY";
const REGISTRY_RECOVERY_INPUTS_VERSION_KEY: &str = "RECOVERY_INPUTS_VERSION";
const REGISTRY_RECOVERY_INPUTS_VERSION: &str = "1";

const MAX_SLUG_LENGTH: usize = 97;
const MAX_RUN_ID_LENGTH: usize = 128;
const RECOVERY_LEASE_MALFORMED_STALE_AFTER: Duration = Duration::from_secs(5);
const TEST_PHASE_BARRIER_TIMEOUT: Duration = Duration::from_secs(30);
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A parsed recovery-lease path state while the claim lock is held.
enum RecoveryLeaseState {
    /// No lease is currently published.
    Missing,
    /// A complete claimant identity is published.
    Valid(RecordedProcessIdentity),
    /// A regular but incomplete or invalid lease, with its publication time.
    Malformed(SystemTime),
}

/// Short-lived advisory lock that serializes lease inspection and replacement.
///
/// The durable identity lease remains the recovery owner after this lock drops.
/// The operating system releases this lock if the claimant dies mid-reconciliation.
struct RecoveryClaimLock {
    _file: fs::File,
}

impl RecoveryClaimLock {
    fn acquire(path: &Path) -> Result<Option<Self>, BgjobError> {
        reject_unsafe_existing_file(path, "recovery claim lock is unsafe")?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }
        let file = options
            .open(path)
            .map_err(|error| BgjobError::Io(error.to_string()))?;
        #[cfg(unix)]
        {
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|error| BgjobError::Io(error.to_string()))?;
        }
        let _opened =
            checked_opened_regular_metadata(&file, path, "recovery claim lock is unsafe")?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(fs::TryLockError::WouldBlock) => Ok(None),
            Err(fs::TryLockError::Error(error)) => Err(BgjobError::Io(error.to_string())),
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecoveryLeaseFaultPhase {
    TemporaryCreated,
    PayloadWritten,
    PayloadSynced,
}

#[cfg(test)]
struct RecoveryLeaseFault {
    thread: std::thread::ThreadId,
    phase: RecoveryLeaseFaultPhase,
}

#[cfg(test)]
static RECOVERY_LEASE_FAULT: std::sync::Mutex<Option<RecoveryLeaseFault>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
fn set_recovery_lease_fault(phase: RecoveryLeaseFaultPhase) {
    let mut fault = RECOVERY_LEASE_FAULT
        .lock()
        .expect("recovery lease fault lock");
    *fault = Some(RecoveryLeaseFault {
        thread: std::thread::current().id(),
        phase,
    });
}

#[cfg(test)]
fn inject_recovery_lease_fault(phase: RecoveryLeaseFaultPhase) -> std::io::Result<()> {
    let injected = {
        let mut fault = RECOVERY_LEASE_FAULT
            .lock()
            .expect("recovery lease fault lock");
        let current = std::thread::current().id();
        if fault
            .as_ref()
            .is_some_and(|pending| pending.thread == current && pending.phase == phase)
        {
            *fault = None;
            true
        } else {
            false
        }
    };
    if injected {
        Err(std::io::Error::other(
            "injected recovery lease interruption",
        ))
    } else {
        Ok(())
    }
}

/// Identity of the session owner that started a background job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerIdentity {
    /// Captured process identity, when the caller supplied an owner.
    pub recorded: Option<RecordedProcessIdentity>,
}

/// One background-job launch request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobSpec {
    /// Stable workflow step slug.
    pub step: String,
    /// Owning session directory.
    pub tmpdir: PathBuf,
    /// Directory for daemon stdout and stderr logs.
    pub log_dir: PathBuf,
    /// Maximum job runtime in seconds.
    pub budget_s: i64,
    /// Child command and arguments.
    pub command: Vec<String>,
    /// Durable run identity.
    pub run_id: String,
    /// Identity of the session owner.
    pub owner: OwnerIdentity,
    /// Completion sentinels written by the Python daemon.
    pub sentinel_paths: Vec<PathBuf>,
    /// Optional child-to-daemon merge envelope.
    pub merge_result_env: Option<PathBuf>,
    /// Optional child stdout key that marks a terminal recovery envelope.
    ///
    /// A daemon that dies after its worker completes may recover this exact
    /// terminal block from the owned stdout log only when the launcher opted
    /// into one stable marker key.
    pub terminal_stdout_key: Option<String>,
    /// Rows seeded into the merge envelope before launch.
    pub initial_merge_rows: Vec<(String, String)>,
}

/// The durable state of a background-job completion transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionTransactionState {
    /// The exact result bytes and declared sentinel set are durable, but the
    /// caller must not treat the result as terminal yet.
    Prepared,
    /// The result and every declared sentinel have been published.
    Committed,
}

/// A confined completion transaction for one background job.
///
/// The descriptor remains beside the result after commit so readers that run
/// after the daemon removes its registry row can still prove the full output
/// set was published. The staged result remains private session state so a
/// recovery owner can idempotently finish an interrupted publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionTransaction {
    /// Current durable publication state.
    pub state: CompletionTransactionState,
    /// Session root that owns every output.
    pub tmpdir: PathBuf,
    /// Stable job step.
    pub step: String,
    /// Public-to-the-session completed result envelope.
    pub result_env: PathBuf,
    /// Private exact-byte source for publication and recovery.
    pub staged_result_env: PathBuf,
    /// Durable transaction descriptor.
    pub descriptor_env: PathBuf,
    /// Ordered set of completion sentinels that must exist before commit.
    pub sentinel_paths: Vec<PathBuf>,
    result_sha256: String,
}

/// One durable daemon registry row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryEntry {
    /// Stable workflow step slug.
    pub step: String,
    /// Durable run identity.
    pub run_id: String,
    /// Owning session directory.
    pub tmpdir: PathBuf,
    /// Directory for daemon stdout and stderr logs.
    pub log_dir: PathBuf,
    /// Clone from which the daemon was launched.
    pub clone_path: PathBuf,
    /// Daemon identity.
    pub daemon: RecordedProcessIdentity,
    /// Child identity.
    pub child: RecordedProcessIdentity,
    /// Whether the child may replace its wrapper command with `exec` while
    /// retaining its PID, process group, and start time.
    pub child_allows_exec: bool,
    /// Optional owner identity.
    pub owner: Option<RecordedProcessIdentity>,
    /// Epoch at launch.
    pub start_epoch: i64,
    /// Maximum job runtime in seconds.
    pub budget_s: i64,
    /// Child stdout log.
    pub stdout_log: PathBuf,
    /// Child stderr log.
    pub stderr_log: PathBuf,
    /// Completed result envelope.
    pub result_env: PathBuf,
    /// Whether this row records the complete recovery input set.
    ///
    /// Legacy rows lack this versioned contract and must not be reconstructed
    /// from a worker witness, because their merge and sentinel inputs were not
    /// durable registry state.
    pub recovery_inputs_recorded: bool,
    /// Optional child-to-daemon merge envelope retained for crash recovery.
    pub merge_result_env: Option<PathBuf>,
    /// Optional stdout terminal marker retained for crash recovery.
    pub terminal_stdout_key: Option<String>,
    /// Completion sentinels retained until the terminal transaction commits.
    pub sentinel_paths: Vec<PathBuf>,
}

/// An exclusive, durable claim held while recovering an abandoned registry row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryLease {
    entry_path: PathBuf,
    lease_path: PathBuf,
    claimant: RecordedProcessIdentity,
}

/// Result of attempting to claim an abandoned registry row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryClaim {
    /// This process exclusively owns recovery until it releases the lease.
    Acquired(RecoveryLease),
    /// Another live cleaner owns recovery.
    Busy,
    /// The row disappeared before it could be claimed.
    Gone,
}

/// Validated liveness result for a recorded process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LivenessVerdict {
    /// Whether the identity still names a live process.
    pub live: bool,
    /// Stable identity-validation reason.
    pub reason: String,
}

/// One `bgjob wait` result envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WaitResult {
    /// Stable wait status token.
    pub status: String,
    /// Caller-ordered result rows.
    pub rows: Vec<(String, String)>,
}

/// Caller-ordered completed result rows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResultEnvRows {
    /// Exact validated result rows.
    pub rows: Vec<(String, String)>,
}

/// Stable failure class for bgjob records and paths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BgjobError {
    /// A caller supplied an invalid scalar or path.
    Invalid(String),
    /// The filesystem could not satisfy a required operation.
    Io(String),
}

impl fmt::Display for BgjobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) | Self::Io(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for BgjobError {}

/// Validate a lowercase workflow slug used in durable filenames.
///
/// # Errors
///
/// Returns [`BgjobError::Invalid`] when `value` is not a safe workflow slug.
pub fn validate_slug(value: &str, label: &str) -> Result<String, BgjobError> {
    let bytes = value.as_bytes();
    let valid = !bytes.is_empty()
        && bytes.len() <= MAX_SLUG_LENGTH
        && bytes
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-');
    if value.contains("..") || value.contains(['/', '\\']) || !valid {
        return Err(BgjobError::Invalid(format!("invalid {label}: {value:?}")));
    }
    Ok(value.to_owned())
}

/// Validate a child stdout key that authorizes terminal-envelope recovery.
///
/// The key has the same closed grammar as a child merge row and cannot name a
/// daemon-owned completion field.
///
/// # Errors
///
/// Returns [`BgjobError::Invalid`] when `value` cannot safely identify a
/// child-authored terminal row.
pub fn validate_terminal_stdout_key(value: &str) -> Result<String, BgjobError> {
    let valid = value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_uppercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        && !matches!(value, BGJOB_RC_KEY | BGJOB_ELAPSED_KEY | "STEP");
    if !valid {
        return Err(BgjobError::Invalid(format!(
            "invalid terminal stdout key: {value:?}"
        )));
    }
    Ok(value.to_owned())
}

/// Validate a durable run id without consulting an active-run pointer.
///
/// # Errors
///
/// Returns [`BgjobError::Invalid`] when `value` is not a safe durable run id.
pub fn validate_run_id(value: &str) -> Result<String, BgjobError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_RUN_ID_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if matches!(value, "." | ".." | "current") || !valid {
        return Err(BgjobError::Invalid("invalid run ID".to_owned()));
    }
    Ok(value.to_owned())
}

/// Derive the legacy tmpdir-only fallback run id.
#[must_use]
pub fn default_run_id(tmpdir: &Path, _clone_path: &Path) -> String {
    let material = canonical_or_absolute(tmpdir);
    let digest = Sha256::digest(material.display().to_string().as_bytes());
    format!("{digest:x}")[..16].to_owned()
}

/// Resolve the run id a session process owns, without the active-run pointer.
///
/// An explicit value wins, then `LARCH_RUN_ID`, then the run id persisted in
/// the session env files, and finally the tmpdir-derived fallback.
#[must_use]
pub fn resolve_run_id(explicit: &str, tmpdir: &Path, clone_path: &Path) -> String {
    let mut candidates = vec![
        explicit.to_owned(),
        env::var("LARCH_RUN_ID").unwrap_or_default(),
    ];
    for name in ["session-env.sh", "source-env.sh"] {
        if let Ok(text) = fs::read_to_string(tmpdir.join(name)) {
            candidates.extend(text.lines().filter_map(run_id_from_line));
        }
    }
    candidates
        .into_iter()
        .find_map(|candidate| validate_run_id(&candidate).ok())
        .unwrap_or_else(|| default_run_id(tmpdir, clone_path))
}

fn run_id_from_line(line: &str) -> Option<String> {
    ["LARCH_RUN_ID=", "export LARCH_RUN_ID="]
        .iter()
        .find_map(|prefix| line.strip_prefix(prefix))
        .map(|raw| raw.trim().trim_matches(['\'', '"']).to_owned())
}

/// Resolve a directory, rejecting a symlink leaf and non-directory inputs.
///
/// # Errors
///
/// Returns an error when the directory is absent, unsafe, or cannot be resolved.
pub fn checked_dir(path: &Path, label: &str, must_exist: bool) -> Result<PathBuf, BgjobError> {
    let absolute = absolute_path(path)?;
    match fs::symlink_metadata(&absolute) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(BgjobError::Invalid(format!(
            "{label} must not be a symlink: {}",
            path.display()
        ))),
        Ok(metadata) if !metadata.is_dir() => Err(BgjobError::Invalid(format!(
            "{label} is not a directory: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !must_exist => Ok(absolute),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(BgjobError::Invalid(
            format!("{label} is not a directory: {}", path.display()),
        )),
        Err(error) => Err(BgjobError::Io(error.to_string())),
        Ok(_) => fs::canonicalize(&absolute).map_err(|error| BgjobError::Io(error.to_string())),
    }
}

/// Resolve `path` and require it to remain inside `root`.
///
/// # Errors
///
/// Returns an error when either path is unsafe or `path` escapes `root`.
pub fn ensure_under(path: &Path, root: &Path, label: &str) -> Result<PathBuf, BgjobError> {
    let root = resolve_candidate(root)?;
    match fs::symlink_metadata(&root) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(BgjobError::Invalid(format!(
                "unsafe root: {}",
                root.display()
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(BgjobError::Io(error.to_string())),
        Ok(_) => {}
    }
    let resolved = resolve_candidate(path)?;
    if !resolved.starts_with(&root) {
        return Err(BgjobError::Invalid(format!(
            "{label} escapes {}: {}",
            root.display(),
            resolved.display()
        )));
    }
    Ok(resolved)
}

/// Return the session-local bgjob directory without creating it.
///
/// # Errors
///
/// Returns an error when the owning tmpdir or its bgjob child is unsafe.
pub fn bgjob_dir(tmpdir: &Path) -> Result<PathBuf, BgjobError> {
    let root = checked_dir(tmpdir, "tmpdir", true)?;
    let candidate = root.join(BGJOB_TMP_SUBDIR);
    match fs::symlink_metadata(&candidate) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(BgjobError::Invalid(format!(
            "bgjob dir must not be a symlink: {}",
            candidate.display()
        ))),
        Ok(metadata) if !metadata.is_dir() => Err(BgjobError::Invalid(format!(
            "bgjob dir is not a directory: {}",
            candidate.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(candidate),
        Err(error) => Err(BgjobError::Io(error.to_string())),
        Ok(_) => Ok(candidate),
    }
}

/// Return the completed-result path for `step`.
///
/// # Errors
///
/// Returns an error when `tmpdir` or `step` is unsafe.
pub fn result_env_path(tmpdir: &Path, step: &str) -> Result<PathBuf, BgjobError> {
    let step = validate_slug(step, "step")?;
    let root = bgjob_dir(tmpdir)?;
    ensure_under(
        &root.join(format!("{step}{BGJOB_RESULT_ENV_SUFFIX}")),
        &root,
        "result env",
    )
}

/// Return the worker-completion witness path for `step`.
///
/// The worker writes this only after its requested child has exited and its
/// process group is drained. It remains durable until terminal publication so
/// a dead-daemon recovery can distinguish a completed child from an abandoned
/// live group.
///
/// # Errors
///
/// Returns an error when `tmpdir` or `step` is unsafe.
pub fn worker_status_path(tmpdir: &Path, step: &str) -> Result<PathBuf, BgjobError> {
    let step = validate_slug(step, "step")?;
    let root = bgjob_dir(tmpdir)?;
    ensure_under(
        &root.join(format!("{step}{BGJOB_WORKER_STATUS_SUFFIX}")),
        &root,
        "worker status env",
    )
}

/// Return the startup-marker path for `step`.
///
/// # Errors
///
/// Returns an error when `tmpdir` or `step` is unsafe.
pub fn startup_env_path(tmpdir: &Path, step: &str) -> Result<PathBuf, BgjobError> {
    let step = validate_slug(step, "step")?;
    let root = bgjob_dir(tmpdir)?;
    ensure_under(
        &root.join(format!("{step}{BGJOB_STARTUP_ENV_SUFFIX}")),
        &root,
        "startup env",
    )
}

/// Return the foreground-wait lease path for `step`.
///
/// # Errors
///
/// Returns an error when `tmpdir` or `step` is unsafe.
pub fn wait_lease_path(tmpdir: &Path, step: &str) -> Result<PathBuf, BgjobError> {
    let step = validate_slug(step, "step")?;
    let root = bgjob_dir(tmpdir)?;
    ensure_under(
        &root.join(format!("{step}{BGJOB_WAIT_LEASE_SUFFIX}")),
        &root,
        "wait lease",
    )
}

/// Refresh the foreground-wait lease that keeps a live job from orphaning.
///
/// # Errors
///
/// Returns an error when the path is unsafe or the atomic write fails.
pub fn refresh_wait_lease(tmpdir: &Path, step: &str) -> Result<(), BgjobError> {
    refresh_wait_lease_for_pid(tmpdir, step, process::id())
}

/// Refresh the foreground-wait lease on behalf of one validated waiter PID.
///
/// Resume owners use this form to bind the lease to the rehydrated session
/// before its first foreground `bgjob wait` process exists.
///
/// # Errors
///
/// Returns an error when `waiter_pid` is zero, the path is unsafe, or the
/// atomic write fails.
pub fn refresh_wait_lease_for_pid(
    tmpdir: &Path,
    step: &str,
    waiter_pid: u32,
) -> Result<(), BgjobError> {
    if waiter_pid == 0 {
        return Err(BgjobError::Invalid(
            "waiter pid must be a positive integer".to_owned(),
        ));
    }
    let path = wait_lease_path(tmpdir, step)?;
    let root = bgjob_dir(tmpdir)?;
    let _ = fs::create_dir_all(&root);
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| BgjobError::Io(error.to_string()))?
        .as_secs();
    private_atomic_write(
        &path,
        &format!("REFRESH_EPOCH={epoch}\nWAITER_PID={waiter_pid}\n"),
        &root,
    )
}

/// Return whether `step` has a wait lease fresher than `ttl_s`.
#[must_use]
pub fn wait_lease_is_fresh(tmpdir: &Path, step: &str, ttl_s: f64) -> bool {
    if !ttl_s.is_finite() || ttl_s <= 0.0 {
        return false;
    }
    let Ok(path) = wait_lease_path(tmpdir, step) else {
        return false;
    };
    let Ok(text) = fs::read_to_string(&path) else {
        return false;
    };
    let Ok(document) = KvDocument::parse(&text, ParseOptions::legacy()) else {
        return false;
    };
    let Some(raw) = document
        .rows()
        .iter()
        .rev()
        .find(|row| row.key() == "REFRESH_EPOCH")
        .map(|row| row.value().to_owned())
    else {
        return false;
    };
    let Ok(epoch) = raw.parse::<u64>() else {
        return false;
    };
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return false;
    };
    let age = now.saturating_sub(Duration::from_secs(epoch));
    age <= Duration::from_secs_f64(ttl_s)
}

/// Return the durable completion-transaction descriptor path for `step`.
///
/// # Errors
///
/// Returns an error when `tmpdir` or `step` is unsafe.
pub fn completion_env_path(tmpdir: &Path, step: &str) -> Result<PathBuf, BgjobError> {
    let step = validate_slug(step, "step")?;
    let root = bgjob_dir(tmpdir)?;
    ensure_under(
        &root.join(format!("{step}{BGJOB_COMPLETION_ENV_SUFFIX}")),
        &root,
        "completion env",
    )
}

/// Return the private staged-result path for `step`.
///
/// # Errors
///
/// Returns an error when `tmpdir` or `step` is unsafe.
pub fn completion_stage_env_path(tmpdir: &Path, step: &str) -> Result<PathBuf, BgjobError> {
    let step = validate_slug(step, "step")?;
    let root = bgjob_dir(tmpdir)?;
    ensure_under(
        &root.join(format!("{step}{BGJOB_COMPLETION_STAGE_SUFFIX}")),
        &root,
        "completion stage env",
    )
}

/// Persist a completion transaction before publishing any observable output.
///
/// The staged result and descriptor are atomically written before the caller
/// can write the result envelope or a sentinel. Recovery can therefore either
/// replay this exact byte sequence or leave the job nonterminal.
///
/// # Errors
///
/// Returns an error when an owned path is unsafe or the intent cannot be
/// persisted privately.
pub fn prepare_completion_transaction(
    tmpdir: &Path,
    step: &str,
    sentinels: &[PathBuf],
    result_text: &str,
) -> Result<CompletionTransaction, BgjobError> {
    let tmpdir = checked_dir(tmpdir, "tmpdir", true)?;
    let root = bgjob_dir(&tmpdir)?;
    ensure_directory(&root, "bgjob dir")?;
    let step = validate_slug(step, "step")?;
    let sentinel_paths = sentinels
        .iter()
        .map(|path| ensure_under(path, &tmpdir, "completion sentinel"))
        .collect::<Result<Vec<_>, _>>()?;
    let transaction = CompletionTransaction {
        state: CompletionTransactionState::Prepared,
        result_env: result_env_path(&tmpdir, &step)?,
        staged_result_env: completion_stage_env_path(&tmpdir, &step)?,
        descriptor_env: completion_env_path(&tmpdir, &step)?,
        tmpdir,
        step,
        sentinel_paths,
        result_sha256: completion_digest(result_text),
    };
    phase_barrier("before-completion-intent")?;
    private_atomic_write(&transaction.staged_result_env, result_text, &root)?;
    phase_barrier("after-completion-stage")?;
    private_atomic_write(
        &transaction.descriptor_env,
        &render_completion_descriptor(&transaction)?,
        &root,
    )?;
    phase_barrier("after-completion-intent")?;
    Ok(transaction)
}

/// Replay the exact output set of a prepared or interrupted transaction.
///
/// The operation is idempotent. A prepared descriptor is atomically committed
/// only after the result and every declared sentinel have been published.
/// A committed descriptor may be replayed by recovery when same-user damage
/// removed an output after the commit, but readers remain nonterminal until
/// the full declared set is present again.
///
/// # Errors
///
/// Returns an error without committing when a staged result or any output path
/// cannot be safely read or written.
pub fn finish_completion_transaction(
    transaction: &CompletionTransaction,
) -> Result<(), BgjobError> {
    let Some(transaction) = read_completion_transaction(&transaction.tmpdir, &transaction.step)?
    else {
        return Err(BgjobError::Invalid(
            "completion transaction descriptor is missing".to_owned(),
        ));
    };
    let root = bgjob_dir(&transaction.tmpdir)?;
    let staged = read_confined_regular_text(
        &transaction.staged_result_env,
        &root,
        "completion staged result is unsafe",
    )?
    .ok_or_else(|| BgjobError::Invalid("completion staged result is missing".to_owned()))?;
    if completion_digest(&staged) != transaction.result_sha256 {
        return Err(BgjobError::Invalid(
            "completion staged result digest mismatch".to_owned(),
        ));
    }
    phase_barrier("before-result-publication")?;
    private_atomic_write(&transaction.result_env, &staged, &root)?;
    phase_barrier("after-result-publication")?;
    for (index, sentinel) in transaction.sentinel_paths.iter().enumerate() {
        phase_barrier(&format!("before-sentinel-{index}-publication"))?;
        private_atomic_write(sentinel, "", &transaction.tmpdir)?;
        phase_barrier(&format!("after-sentinel-{index}-publication"))?;
    }
    if transaction.state == CompletionTransactionState::Prepared {
        let mut committed = transaction;
        committed.state = CompletionTransactionState::Committed;
        phase_barrier("before-completion-commit")?;
        private_atomic_write(
            &committed.descriptor_env,
            &render_completion_descriptor(&committed)?,
            &root,
        )?;
        phase_barrier("after-completion-commit")?;
    }
    Ok(())
}

/// Read one completion transaction, if this job uses the committed protocol.
///
/// `Ok(None)` denotes a legacy result without a descriptor. A malformed or
/// unsafe descriptor is an error rather than permission to treat a result as
/// terminal.
///
/// # Errors
///
/// Returns an error when the descriptor, its declared outputs, or its owning
/// paths are malformed or unsafe.
pub fn read_completion_transaction(
    tmpdir: &Path,
    step: &str,
) -> Result<Option<CompletionTransaction>, BgjobError> {
    let tmpdir = checked_dir(tmpdir, "tmpdir", true)?;
    let step = validate_slug(step, "step")?;
    let root = bgjob_dir(&tmpdir)?;
    let descriptor_env = completion_env_path(&tmpdir, &step)?;
    let Some(text) =
        read_confined_regular_text(&descriptor_env, &root, "completion descriptor is unsafe")?
    else {
        return Ok(None);
    };
    let rows = parse_completion_descriptor(&text)?;
    let state = match completion_required(&rows, COMPLETION_STATE_KEY)?.as_str() {
        COMPLETION_STATE_PREPARED => CompletionTransactionState::Prepared,
        COMPLETION_STATE_COMMITTED => CompletionTransactionState::Committed,
        _ => {
            return Err(BgjobError::Invalid(
                "completion descriptor has an invalid state".to_owned(),
            ));
        }
    };
    if completion_required(&rows, "STEP")? != step {
        return Err(BgjobError::Invalid(
            "completion descriptor step does not match".to_owned(),
        ));
    }
    let result_sha256 = completion_required(&rows, COMPLETION_RESULT_SHA256_KEY)?;
    if !valid_completion_digest(&result_sha256) {
        return Err(BgjobError::Invalid(
            "completion descriptor has an invalid result digest".to_owned(),
        ));
    }
    let sentinel_count = completion_required(&rows, COMPLETION_SENTINEL_COUNT_KEY)?
        .parse::<usize>()
        .map_err(|_| {
            BgjobError::Invalid("completion descriptor has an invalid sentinel count".to_owned())
        })?;
    let expected_keys = 4_usize
        .checked_add(sentinel_count)
        .ok_or_else(|| BgjobError::Invalid("completion sentinel count is too large".to_owned()))?;
    if rows.len() != expected_keys {
        return Err(BgjobError::Invalid(
            "completion descriptor has unexpected fields".to_owned(),
        ));
    }
    let sentinel_paths = (0..sentinel_count)
        .map(|index| {
            let key = format!("{COMPLETION_SENTINEL_PREFIX}{index}");
            let raw = completion_required(&rows, &key)?;
            let path = Path::new(&raw);
            if !path.is_absolute() {
                return Err(BgjobError::Invalid(
                    "completion sentinel must be absolute".to_owned(),
                ));
            }
            ensure_under(path, &tmpdir, "completion sentinel")
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(CompletionTransaction {
        state,
        result_env: result_env_path(&tmpdir, &step)?,
        staged_result_env: completion_stage_env_path(&tmpdir, &step)?,
        descriptor_env,
        tmpdir,
        step,
        sentinel_paths,
        result_sha256,
    }))
}

/// Return whether `result_text` is visible as a terminal result.
///
/// Legacy jobs without a completion descriptor retain their existing result
/// grammar. New jobs require an atomically committed descriptor, a matching
/// result digest, and every declared regular sentinel.
///
/// # Errors
///
/// Returns an error when the completion descriptor or a declared output path
/// is unsafe.
pub fn completion_result_is_visible(
    tmpdir: &Path,
    step: &str,
    result_text: &str,
) -> Result<bool, BgjobError> {
    let Some(transaction) = read_completion_transaction(tmpdir, step)? else {
        return Ok(true);
    };
    if transaction.state != CompletionTransactionState::Committed
        || completion_digest(result_text) != transaction.result_sha256
    {
        return Ok(false);
    }
    transaction
        .sentinel_paths
        .iter()
        .try_fold(true, |complete, path| {
            regular_output_exists(path, &transaction.tmpdir).map(|exists| complete && exists)
        })
}

/// Remove result and transaction residue only through the confined bgjob root.
///
/// This intentionally leaves declared sentinels alone: callers may share a
/// sentinel with another state transition, while a missing commit descriptor
/// already prevents it from granting terminal-result visibility.
///
/// # Errors
///
/// Returns an error when a residue path is unsafe or cannot be removed.
pub fn clear_completion_residue(tmpdir: &Path, step: &str) -> Result<(), BgjobError> {
    let tmpdir = checked_dir(tmpdir, "tmpdir", true)?;
    let root = bgjob_dir(&tmpdir)?;
    for path in [
        result_env_path(&tmpdir, step)?,
        completion_env_path(&tmpdir, step)?,
        completion_stage_env_path(&tmpdir, step)?,
    ] {
        remove_confined_regular_file(&path, &root, "completion residue is unsafe")?;
    }
    Ok(())
}

/// Safely read one confined background-job result envelope, if present.
///
/// # Errors
///
/// Returns an error when the envelope or any parent is unsafe.
pub fn read_confined_result_env(path: &Path, tmpdir: &Path) -> Result<Option<String>, BgjobError> {
    let root = bgjob_dir(tmpdir)?;
    read_confined_regular_text(path, &root, "result env is unsafe")
}

/// Read the final bounded bytes of one confined regular background-job file.
///
/// The returned boolean is true when earlier bytes were omitted. A missing,
/// unsafe, or non-regular path is an error: callers recovering durable state
/// must retain their recovery inputs rather than treating absent bytes as an
/// empty terminal envelope.
///
/// # Errors
///
/// Returns an error when the root or file cannot be revalidated, opened, or
/// read safely.
pub fn read_confined_regular_tail(
    path: &Path,
    root: &Path,
    byte_limit: u64,
    message: &str,
) -> Result<(Vec<u8>, bool), BgjobError> {
    let Some(mut file) = open_confined_regular_file(path, root, message)? else {
        return Err(BgjobError::Invalid(message.to_owned()));
    };
    let length = file
        .metadata()
        .map_err(|error| BgjobError::Io(error.to_string()))?
        .len();
    let offset = length.saturating_sub(byte_limit);
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| BgjobError::Io(error.to_string()))?;
    let mut bytes = Vec::new();
    file.take(byte_limit)
        .read_to_end(&mut bytes)
        .map_err(|error| BgjobError::Io(error.to_string()))?;
    Ok((bytes, offset > 0))
}

/// Revalidate and read a merge-result envelope at the point of consumption.
///
/// # Errors
///
/// Returns an error when the envelope is no longer a confined regular file.
pub fn read_merge_result_env(path: &Path, tmpdir: &Path) -> Result<String, BgjobError> {
    let path = validate_merge_result_env(path, tmpdir)?;
    let root = checked_dir(tmpdir, "tmpdir", true)?;
    // A launch may legitimately reserve a merge path that the child elects
    // not to populate. Preserve that wire contract, while still revalidating
    // every existing file at the point of consumption.
    let text =
        read_confined_regular_text(&path, &root, "merge result env is unsafe")?.unwrap_or_default();
    if text.contains('\r') {
        return Ok(String::new());
    }
    Ok(text)
}

/// Stop at a named lifecycle phase when an integration test requests it.
///
/// The hook is deliberately unavailable in ordinary optimized builds. Coverage
/// instrumentation also enables it so the same lifecycle tests run under the
/// release-like CI test profile. Its directory is an existing, checked private
/// test root; phase names stay slug-validated and it never follows a marker
/// symlink. A missing release marker fails the current operation rather than
/// turning a test-only pause into an unbounded production wait.
///
/// # Errors
///
/// Returns an error when a requested debug-test barrier is unsafe or times out.
#[allow(unexpected_cfgs)] // `cargo llvm-cov` supplies the coverage-only test cfg.
pub fn phase_barrier(phase: &str) -> Result<(), BgjobError> {
    if !cfg!(any(debug_assertions, coverage)) {
        return Ok(());
    }
    let Some(raw_root) = env::var_os(ENV_TEST_BGJOB_PHASE_BARRIER_DIR) else {
        return Ok(());
    };
    let phase = validate_slug(phase, "phase barrier")?;
    let root = checked_dir(Path::new(&raw_root), "phase barrier root", true)?;
    let armed = ensure_under(&root.join(format!("{phase}.armed")), &root, "phase barrier")?;
    let reached = ensure_under(
        &root.join(format!("{phase}.reached")),
        &root,
        "phase barrier",
    )?;
    let release = ensure_under(
        &root.join(format!("{phase}.release")),
        &root,
        "phase barrier",
    )?;
    match fs::symlink_metadata(&armed) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Ok(metadata) if !metadata.file_type().is_symlink() && metadata.is_file() => {}
        Ok(_) => {
            return Err(BgjobError::Invalid(format!(
                "phase barrier arm is unsafe: {}",
                armed.display()
            )));
        }
        Err(error) => return Err(BgjobError::Io(error.to_string())),
    }
    private_atomic_write(&reached, &format!("PID={}\n", process::id()), &root)?;
    let deadline = Instant::now() + TEST_PHASE_BARRIER_TIMEOUT;
    loop {
        match fs::symlink_metadata(&release) {
            Ok(metadata) if !metadata.file_type().is_symlink() && metadata.is_file() => {
                return Ok(());
            }
            Ok(_) => {
                return Err(BgjobError::Invalid(format!(
                    "phase barrier release is unsafe: {}",
                    release.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(BgjobError::Io(error.to_string())),
        }
        if Instant::now() >= deadline {
            return Err(BgjobError::Invalid(format!(
                "phase barrier timed out: {phase}"
            )));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Validate a caller-selected merge envelope below the owned tmpdir.
///
/// # Errors
///
/// Returns an error when the envelope is not a safe regular file below `tmpdir`.
pub fn validate_merge_result_env(path: &Path, tmpdir: &Path) -> Result<PathBuf, BgjobError> {
    let root = checked_dir(tmpdir, "tmpdir", true)?;
    if !path.is_absolute() {
        return Err(BgjobError::Invalid(format!(
            "merge result env must be absolute: {}",
            path.display()
        )));
    }
    let resolved = ensure_under(path, &root, "merge result env")?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(BgjobError::Invalid(format!(
                "merge result env must be a regular file: {}",
                path.display()
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(BgjobError::Io(error.to_string())),
        Ok(_) => {}
    }
    validate_parent_chain(path, &root, "merge result env parent is unsafe")?;
    Ok(resolved)
}

fn render_completion_descriptor(transaction: &CompletionTransaction) -> Result<String, BgjobError> {
    let state = match transaction.state {
        CompletionTransactionState::Prepared => COMPLETION_STATE_PREPARED,
        CompletionTransactionState::Committed => COMPLETION_STATE_COMMITTED,
    };
    let mut rows = vec![
        (COMPLETION_STATE_KEY.to_owned(), state.to_owned()),
        ("STEP".to_owned(), transaction.step.clone()),
        (
            COMPLETION_RESULT_SHA256_KEY.to_owned(),
            transaction.result_sha256.clone(),
        ),
        (
            COMPLETION_SENTINEL_COUNT_KEY.to_owned(),
            transaction.sentinel_paths.len().to_string(),
        ),
    ];
    rows.extend(
        transaction
            .sentinel_paths
            .iter()
            .enumerate()
            .map(|(index, path)| {
                (
                    format!("{COMPLETION_SENTINEL_PREFIX}{index}"),
                    path.display().to_string(),
                )
            }),
    );
    render_rows(&rows)
}

fn parse_completion_descriptor(text: &str) -> Result<BTreeMap<String, String>, BgjobError> {
    let document = KvDocument::parse(text, ParseOptions::legacy())
        .map_err(|_| BgjobError::Invalid("completion descriptor is malformed".to_owned()))?;
    let mut rows = BTreeMap::new();
    for row in document.rows() {
        if rows
            .insert(row.key().to_owned(), row.value().to_owned())
            .is_some()
        {
            return Err(BgjobError::Invalid(
                "completion descriptor contains duplicate keys".to_owned(),
            ));
        }
    }
    Ok(rows)
}

fn completion_required(rows: &BTreeMap<String, String>, key: &str) -> Result<String, BgjobError> {
    rows.get(key)
        .cloned()
        .ok_or_else(|| BgjobError::Invalid(format!("completion descriptor is missing {key}")))
}

fn completion_digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn valid_completion_digest(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte.is_ascii_hexdigit())
        })
}

fn read_confined_regular_text(
    path: &Path,
    root: &Path,
    message: &str,
) -> Result<Option<String>, BgjobError> {
    let Some(mut file) = open_confined_regular_file(path, root, message)? else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| BgjobError::Io(error.to_string()))?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| BgjobError::Invalid(message.to_owned()))
}

fn open_confined_regular_file(
    path: &Path,
    root: &Path,
    message: &str,
) -> Result<Option<fs::File>, BgjobError> {
    let root = checked_dir(root, "bgjob root", true)?;
    let path = ensure_under(path, &root, message)?;
    validate_parent_chain(&path, &root, message)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    }
    let file = match options.open(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        #[cfg(unix)]
        Err(error) if error.raw_os_error() == Some(nix::libc::ELOOP) => {
            return Err(BgjobError::Invalid(message.to_owned()));
        }
        Err(error) => return Err(BgjobError::Io(error.to_string())),
        Ok(file) => file,
    };
    let _opened = checked_opened_regular_metadata(&file, &path, message)?;
    validate_parent_chain(&path, &root, message)?;
    Ok(Some(file))
}

fn regular_output_exists(path: &Path, root: &Path) -> Result<bool, BgjobError> {
    let path = ensure_under(path, root, "completion sentinel")?;
    validate_parent_chain(&path, root, "completion sentinel is unsafe")?;
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(BgjobError::Io(error.to_string())),
        Ok(metadata) => Ok(!metadata.file_type().is_symlink() && metadata.is_file()),
    }
}

fn remove_confined_regular_file(path: &Path, root: &Path, message: &str) -> Result<(), BgjobError> {
    let path = ensure_under(path, root, message)?;
    validate_parent_chain(&path, root, message)?;
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(BgjobError::Io(error.to_string())),
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(BgjobError::Invalid(message.to_owned()))
        }
        Ok(_) => fs::remove_file(&path).map_err(|error| BgjobError::Io(error.to_string())),
    }
}

/// Validate unique rows seeded into a merge envelope before child launch.
///
/// # Errors
///
/// Returns an error for duplicate, reserved, malformed, or multiline rows.
pub fn validate_initial_merge_rows(
    rows: &[(String, String)],
) -> Result<Vec<(String, String)>, BgjobError> {
    let mut seen = BTreeSet::new();
    let reserved = BTreeSet::from([BGJOB_RC_KEY, BGJOB_ELAPSED_KEY, "STEP"]);
    rows.iter()
        .map(|(key, value)| {
            let valid_key = key
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_uppercase())
                && key
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
            if !valid_key || !seen.insert(key.clone()) || reserved.contains(key.as_str()) {
                return Err(BgjobError::Invalid(format!(
                    "invalid initial merge row key: {key:?}"
                )));
            }
            Ok((
                key.clone(),
                reject_line_value(value, &format!("initial merge row {key}"))?,
            ))
        })
        .collect()
}

/// Create the requested log directory and return stdout/stderr log paths.
///
/// # Errors
///
/// Returns an error when the directory or workflow step is unsafe.
pub fn log_paths(
    tmpdir: &Path,
    log_dir: Option<&Path>,
    step: &str,
) -> Result<(PathBuf, PathBuf, PathBuf), BgjobError> {
    let step = validate_slug(step, "step")?;
    let root = match log_dir {
        Some(path) => ensure_directory(path, "log-dir")?,
        None => ensure_directory(&bgjob_dir(tmpdir)?, "log-dir")?,
    };
    let stdout = ensure_under(
        &root.join(format!("{step}.stdout.log")),
        &root,
        "stdout log",
    )?;
    let stderr = ensure_under(
        &root.join(format!("{step}.stderr.log")),
        &root,
        "stderr log",
    )?;
    Ok((root, stdout, stderr))
}

/// Return the durable registry root, creating it with private permissions.
///
/// # Errors
///
/// Returns an error when the configured registry root cannot be created safely.
pub fn registry_root() -> Result<PathBuf, BgjobError> {
    let root = env::var_os(ENV_BGJOB_REGISTRY_ROOT).map_or_else(
        || {
            env::var_os("HOME")
                .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from)
                .join(".cache/larch")
                .join(BGJOB_REGISTRY_DIRNAME)
        },
        PathBuf::from,
    );
    ensure_directory(&root, "registry root")
}

/// Derive the durable registry filename for one run and step.
///
/// # Errors
///
/// Returns an error when the run, step, or registry root is unsafe.
pub fn registry_path(run_id: &str, step: &str, root: Option<&Path>) -> Result<PathBuf, BgjobError> {
    let run_id = validate_run_id(run_id)?;
    let step = validate_slug(step, "step")?;
    let root = match root {
        Some(path) => ensure_directory(path, "registry-root")?,
        None => registry_root()?,
    };
    ensure_under(
        &root.join(format!("{run_id}-{step}.env")),
        &root,
        "registry path",
    )
}

/// Reject a value that would forge a line-oriented `KEY=value` record.
///
/// # Errors
///
/// Returns [`BgjobError::Invalid`] when `value` contains a line break.
pub fn reject_line_value(value: &str, label: &str) -> Result<String, BgjobError> {
    if value.contains(['\n', '\r']) {
        return Err(BgjobError::Invalid(format!(
            "{label} contains a newline or carriage return"
        )));
    }
    Ok(value.to_owned())
}

/// Persist one registry entry atomically with private permissions.
///
/// # Errors
///
/// Returns an error when the entry cannot be validated or written safely.
pub fn write_entry(entry: &RegistryEntry) -> Result<PathBuf, BgjobError> {
    write_entry_at(entry, None)
}

/// Persist one registry entry under an explicit root for an isolated caller or test.
///
/// # Errors
///
/// Returns an error when the entry cannot be validated or written safely.
pub fn write_entry_at(entry: &RegistryEntry, root: Option<&Path>) -> Result<PathBuf, BgjobError> {
    if !entry.recovery_inputs_recorded
        && (entry.merge_result_env.is_some()
            || entry.terminal_stdout_key.is_some()
            || !entry.sentinel_paths.is_empty())
    {
        return Err(BgjobError::Invalid(
            "legacy registry entries cannot carry recovery inputs".to_owned(),
        ));
    }
    let path = registry_path(&entry.run_id, &entry.step, root)?;
    let mut rows = vec![
        ("STEP".to_owned(), entry.step.clone()),
        ("RUN_ID".to_owned(), entry.run_id.clone()),
        ("TMPDIR".to_owned(), entry.tmpdir.display().to_string()),
        ("LOG_DIR".to_owned(), entry.log_dir.display().to_string()),
        (
            "CLONE_PATH".to_owned(),
            entry.clone_path.display().to_string(),
        ),
        ("START_EPOCH".to_owned(), entry.start_epoch.to_string()),
        ("BUDGET_S".to_owned(), entry.budget_s.to_string()),
        (
            "STDOUT_LOG".to_owned(),
            entry.stdout_log.display().to_string(),
        ),
        (
            "STDERR_LOG".to_owned(),
            entry.stderr_log.display().to_string(),
        ),
        (
            "RESULT_ENV".to_owned(),
            entry.result_env.display().to_string(),
        ),
    ];
    if entry.recovery_inputs_recorded {
        rows.extend([
            (
                REGISTRY_RECOVERY_INPUTS_VERSION_KEY.to_owned(),
                REGISTRY_RECOVERY_INPUTS_VERSION.to_owned(),
            ),
            (
                REGISTRY_SENTINEL_COUNT_KEY.to_owned(),
                entry.sentinel_paths.len().to_string(),
            ),
        ]);
        if let Some(path) = entry.merge_result_env.as_ref() {
            rows.push((
                REGISTRY_MERGE_RESULT_ENV_KEY.to_owned(),
                path.display().to_string(),
            ));
        }
        if let Some(key) = entry.terminal_stdout_key.as_ref() {
            rows.push((
                REGISTRY_TERMINAL_STDOUT_KEY.to_owned(),
                validate_terminal_stdout_key(key)?,
            ));
        }
        rows.extend(
            entry
                .sentinel_paths
                .iter()
                .enumerate()
                .map(|(index, path)| {
                    (
                        format!("{REGISTRY_SENTINEL_PREFIX}{index}"),
                        path.display().to_string(),
                    )
                }),
        );
    }
    rows.extend(identity_rows("DAEMON", Some(&entry.daemon)));
    rows.extend(identity_rows("CHILD", Some(&entry.child)));
    rows.push((
        "CHILD_ALLOW_COMMAND_TRANSITION".to_owned(),
        entry.child_allows_exec.to_string(),
    ));
    rows.extend(identity_rows("OWNER", entry.owner.as_ref()));
    let text = render_rows(&rows)?;
    let parent = path
        .parent()
        .ok_or_else(|| BgjobError::Invalid("registry path has no parent".to_owned()))?;
    private_atomic_write(&path, &text, parent)?;
    Ok(path)
}

/// Decode one registry entry, returning `None` for malformed or unsafe rows.
#[must_use]
pub fn read_entry(path: &Path) -> Option<RegistryEntry> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    let document = KvDocument::parse(&text, ParseOptions::legacy()).ok()?;
    let rows = document.select(DuplicatePolicy::First);
    let daemon = parse_identity(&rows, "DAEMON")?;
    let child = parse_identity(&rows, "CHILD")?;
    let tmpdir = resolved_directory(rows.get("TMPDIR")?)?;
    let log_dir = resolved_directory(rows.get("LOG_DIR")?)?;
    let stdout_log = validated_path(Path::new(rows.get("STDOUT_LOG")?), &log_dir)?;
    let stderr_log = validated_path(Path::new(rows.get("STDERR_LOG")?), &log_dir)?;
    let bgjob_root = bgjob_dir(&tmpdir).ok()?;
    let result_env = validated_result_output_path(Path::new(rows.get("RESULT_ENV")?), &bgjob_root)?;
    let recovery_inputs_recorded = parse_recovery_inputs_version(&rows)?;
    let (merge_result_env, terminal_stdout_key, sentinel_paths) = if recovery_inputs_recorded {
        let merge_result_env = match rows.get(REGISTRY_MERGE_RESULT_ENV_KEY) {
            Some(raw) => Some(validated_merge_result_path(Path::new(raw), &tmpdir)?),
            None => None,
        };
        let terminal_stdout_key = rows
            .get(REGISTRY_TERMINAL_STDOUT_KEY)
            .map(|value| validate_terminal_stdout_key(value))
            .transpose()
            .ok()?;
        let sentinel_paths = parse_registry_sentinel_paths(&rows, &tmpdir)?;
        (merge_result_env, terminal_stdout_key, sentinel_paths)
    } else {
        (None, None, Vec::new())
    };
    let clone_raw = rows.get("CLONE_PATH").map_or(".", String::as_str);
    Some(RegistryEntry {
        step: validate_slug(rows.get("STEP")?, "step").ok()?,
        run_id: validate_run_id(rows.get("RUN_ID")?).ok()?,
        tmpdir,
        log_dir,
        clone_path: resolve_candidate(Path::new(clone_raw)).ok()?,
        daemon,
        child,
        // Legacy rows predate the explicit marker but were launched through
        // the same wrapper boundary, so preserve exec-capable recovery for
        // them. An explicit non-true marker opts into exact command matching.
        child_allows_exec: rows
            .get("CHILD_ALLOW_COMMAND_TRANSITION")
            .is_none_or(|value| value == "true"),
        owner: parse_identity(&rows, "OWNER"),
        start_epoch: rows.get("START_EPOCH")?.parse().ok()?,
        budget_s: rows.get("BUDGET_S")?.parse().ok()?,
        stdout_log,
        stderr_log,
        result_env,
        recovery_inputs_recorded,
        merge_result_env,
        terminal_stdout_key,
        sentinel_paths,
    })
}

/// Read the registry row for the requested run and step.
///
/// An omitted run id resolves through [`resolve_run_id`], as the Python owner did.
///
/// # Errors
///
/// Returns an error when the requested tmpdir, run id, or registry root is unsafe.
pub fn read_for(
    tmpdir: &Path,
    step: &str,
    run_id: Option<&str>,
) -> Result<(PathBuf, Option<RegistryEntry>), BgjobError> {
    let tmpdir = checked_dir(tmpdir, "tmpdir", true)?;
    let clone_path = env::current_dir().map_err(|error| BgjobError::Io(error.to_string()))?;
    let run_id = resolve_run_id(run_id.unwrap_or_default(), &tmpdir, &clone_path);
    let path = registry_path(&run_id, step, None)?;
    Ok((path.clone(), read_entry(&path)))
}

/// Iterate durable entries in deterministic filename order.
#[must_use]
pub fn iter_entries() -> Vec<(PathBuf, Option<RegistryEntry>)> {
    let Ok(root) = registry_root() else {
        return Vec::new();
    };
    iter_entries_at(&root)
}

fn iter_entries_at(root: &Path) -> Vec<(PathBuf, Option<RegistryEntry>)> {
    let Ok(read_dir) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut paths = read_dir
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "env"))
        .filter(|path| {
            fs::symlink_metadata(path)
                .is_ok_and(|metadata| !metadata.file_type().is_symlink() && metadata.is_file())
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let entry = read_entry(&path);
            (path, entry)
        })
        .collect()
}

/// Remove a regular registry entry, leaving symlinks untouched.
pub fn unlink_entry(path: &Path) {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return;
    }
    let _ = fs::remove_file(path);
}

/// Claim a registry row for foreground recovery without racing another cleaner.
///
/// The lease records the cleaner's full process identity. A later cleaner may
/// remove the lease only when that identity is absent or mismatched, so a
/// cancelled foreground `wait` cannot strand recovery forever.
///
/// # Errors
///
/// Returns an error when the registry row, lease path, or claimant identity is
/// unsafe or cannot be validated.
pub fn claim_recovery(
    host: &dyn ProcessIdentityHost,
    entry_path: &Path,
) -> Result<RecoveryClaim, BgjobError> {
    if fs::symlink_metadata(entry_path).is_err() {
        return Ok(RecoveryClaim::Gone);
    }
    let parent = entry_path
        .parent()
        .ok_or_else(|| BgjobError::Invalid("registry path has no parent".to_owned()))?;
    let parent_metadata =
        fs::symlink_metadata(parent).map_err(|error| BgjobError::Io(error.to_string()))?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err(BgjobError::Invalid(
            "registry parent is unsafe for recovery claim".to_owned(),
        ));
    }
    let entry_name = entry_path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| {
            Path::new(value)
                .extension()
                .is_some_and(|extension| extension == "env")
        })
        .ok_or_else(|| BgjobError::Invalid("registry filename is unsafe".to_owned()))?;
    let lease_path = parent.join(format!(".{entry_name}.recovery"));
    let lock_path = parent.join(format!(".{entry_name}.recovery.lock"));
    let claimant = read_process_identity(host, host.current_pid(), "").ok_or_else(|| {
        BgjobError::Invalid("could not capture recovery claimant identity".to_owned())
    })?;
    let Some(_lock) = RecoveryClaimLock::acquire(&lock_path)? else {
        return Ok(RecoveryClaim::Busy);
    };

    for _ in 0..2 {
        match write_recovery_lease(&lease_path, &claimant) {
            Ok(()) => {
                if read_entry(entry_path).is_none() {
                    release_recovery_claim(&RecoveryLease {
                        entry_path: entry_path.to_path_buf(),
                        lease_path,
                        claimant,
                    });
                    return Ok(RecoveryClaim::Gone);
                }
                return Ok(RecoveryClaim::Acquired(RecoveryLease {
                    entry_path: entry_path.to_path_buf(),
                    lease_path,
                    claimant,
                }));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                match inspect_recovery_lease(&lease_path)? {
                    RecoveryLeaseState::Missing => continue,
                    RecoveryLeaseState::Valid(existing) => {
                        let validation = validate_process_identity(host, &existing);
                        if validation.ok {
                            return Ok(RecoveryClaim::Busy);
                        }
                        if matches!(
                            validation.reason.as_str(),
                            "identity-probe-error"
                                | "identity-probe-timeout"
                                | "pgid-changed-during-identity-probe"
                                | "process-birth-identity-probe-error"
                                | "process-birth-identity-unsupported"
                                | "process-birth-identity-unstable"
                        ) {
                            return Err(BgjobError::Invalid(format!(
                                "could not validate recovery claimant: {}",
                                validation.reason
                            )));
                        }
                        if matches!(
                            validation.reason.as_str(),
                            "missing-process-birth-identity" | "invalid-process-birth-identity"
                        ) {
                            // An older claimant row can prove neither that a
                            // live PID is its original owner nor that it is a
                            // recycled process. Retain that lease until the
                            // PID is absent rather than letting another
                            // cleaner overlap its recovery mutation.
                            return Ok(RecoveryClaim::Busy);
                        }
                    }
                    RecoveryLeaseState::Malformed(modified) => {
                        if !malformed_recovery_lease_is_stale(modified)? {
                            return Ok(RecoveryClaim::Busy);
                        }
                    }
                }
                if !remove_regular_file(&lease_path) {
                    return Err(BgjobError::Io(
                        "could not remove stale recovery lease".to_owned(),
                    ));
                }
            }
            Err(error) => return Err(BgjobError::Io(error.to_string())),
        }
    }
    Ok(RecoveryClaim::Busy)
}

/// Return the claimed row path for an acquired recovery lease.
#[must_use]
pub fn recovery_claim_entry_path(claim: &RecoveryLease) -> &Path {
    &claim.entry_path
}

/// Release a recovery claim only when this process still owns the exact lease.
pub fn release_recovery_claim(claim: &RecoveryLease) {
    if matches!(
        inspect_recovery_lease(&claim.lease_path),
        Ok(RecoveryLeaseState::Valid(existing)) if existing == claim.claimant
    ) {
        let _removed = remove_regular_file(&claim.lease_path);
    }
}

/// Validate the recorded child identity against a host process table.
#[must_use]
pub fn child_liveness(host: &dyn ProcessIdentityHost, entry: &RegistryEntry) -> LivenessVerdict {
    let validation =
        validate_process_identity_with_policy(host, &entry.child, child_identity_policy(entry));
    LivenessVerdict {
        live: validation.ok,
        reason: if validation.ok {
            "ok".to_owned()
        } else {
            validation.reason
        },
    }
}

/// Return the validation policy persisted with a job child identity.
#[must_use]
pub const fn child_identity_policy(entry: &RegistryEntry) -> ProcessIdentityValidationPolicy {
    if entry.child_allows_exec {
        ProcessIdentityValidationPolicy::AllowCommandTransition
    } else {
        ProcessIdentityValidationPolicy::ExactCommand
    }
}

/// Validate the recorded daemon identity against a host process table.
#[must_use]
pub fn daemon_liveness(host: &dyn ProcessIdentityHost, entry: &RegistryEntry) -> LivenessVerdict {
    liveness(host, &entry.daemon)
}

/// Return whether an entry exceeded its configured runtime budget.
#[must_use]
pub fn entry_expired(entry: &RegistryEntry) -> bool {
    epoch_now().saturating_sub(entry.start_epoch) > entry.budget_s
}

/// Return whether a live, in-budget registry entry belongs to a run and clone.
#[must_use]
pub fn has_live_entry(host: &dyn ProcessIdentityHost, repo_root: &Path, run_id: &str) -> bool {
    let Ok(root) = registry_root() else {
        return false;
    };
    has_live_entry_at(host, repo_root, run_id, &root)
}

fn has_live_entry_at(
    host: &dyn ProcessIdentityHost,
    repo_root: &Path,
    run_id: &str,
    registry_root: &Path,
) -> bool {
    let Ok(run_id) = validate_run_id(run_id) else {
        return false;
    };
    let Ok(repo_root) = fs::canonicalize(repo_root) else {
        return false;
    };
    iter_entries_at(registry_root)
        .into_iter()
        .any(|(_, entry)| {
            let Some(entry) = entry else {
                return false;
            };
            entry.run_id == run_id
                && !entry_expired(&entry)
                && fs::canonicalize(&entry.clone_path).is_ok_and(|clone| clone == repo_root)
                && (child_liveness(host, &entry).live || daemon_liveness(host, &entry).live)
        })
}

/// Atomically write a regular session-owned wire file with mode `0600`.
///
/// # Errors
///
/// Returns an error when the target is unsafe or the atomic write fails.
pub fn private_atomic_write(path: &Path, text: &str, root: &Path) -> Result<(), BgjobError> {
    let path = ensure_under(path, root, "wire file")?;
    let parent = path
        .parent()
        .ok_or_else(|| BgjobError::Invalid("wire file has no parent".to_owned()))?;
    validate_parent_chain(&path, root, "wire file parent is unsafe")?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(BgjobError::Invalid(format!(
                "wire file is unsafe: {}",
                path.display()
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(BgjobError::Io(error.to_string())),
        Ok(_) => {}
    }
    let temporary = temporary_path(parent, &path)?;
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|error| BgjobError::Io(error.to_string()))?;
        file.write_all(text.as_bytes())
            .map_err(|error| BgjobError::Io(error.to_string()))?;
        file.sync_all()
            .map_err(|error| BgjobError::Io(error.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
                .map_err(|error| BgjobError::Io(error.to_string()))?;
        }
        fs::rename(&temporary, &path).map_err(|error| BgjobError::Io(error.to_string()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn liveness(host: &dyn ProcessIdentityHost, identity: &RecordedProcessIdentity) -> LivenessVerdict {
    let validation = validate_process_identity(host, identity);
    LivenessVerdict {
        live: validation.ok,
        reason: if validation.ok {
            "ok".to_owned()
        } else {
            validation.reason
        },
    }
}

fn write_recovery_lease(path: &Path, claimant: &RecordedProcessIdentity) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "recovery lease path has no parent",
        )
    })?;
    let temporary =
        temporary_path(parent, path).map_err(|error| std::io::Error::other(error.to_string()))?;
    let mut temporary_created = false;
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        temporary_created = true;
        #[cfg(test)]
        inject_recovery_lease_fault(RecoveryLeaseFaultPhase::TemporaryCreated)?;
        file.write_all(identity_to_json(claimant, None).as_bytes())?;
        #[cfg(test)]
        inject_recovery_lease_fault(RecoveryLeaseFaultPhase::PayloadWritten)?;
        file.sync_all()?;
        #[cfg(test)]
        inject_recovery_lease_fault(RecoveryLeaseFaultPhase::PayloadSynced)?;
        drop(file);
        fs::hard_link(&temporary, path)
    })();
    if result.is_err() {
        if temporary_created {
            let _removed = fs::remove_file(&temporary);
        }
        return result;
    }
    // The link is the durable claim; a failed best-effort temporary cleanup
    // must not make a caller believe its complete final lease was not acquired.
    let _removed = fs::remove_file(&temporary);
    Ok(())
}

fn inspect_recovery_lease(path: &Path) -> Result<RecoveryLeaseState, BgjobError> {
    reject_unsafe_existing_file(path, "recovery lease is malformed or unsafe")?;
    let mut options = OpenOptions::new();
    options.read(true);
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RecoveryLeaseState::Missing);
        }
        Err(error) => return Err(BgjobError::Io(error.to_string())),
    };
    let opened =
        checked_opened_regular_metadata(&file, path, "recovery lease is malformed or unsafe")?;
    let mut bytes = Vec::new();
    let mut file = file;
    file.read_to_end(&mut bytes)
        .map_err(|error| BgjobError::Io(error.to_string()))?;
    serde_json::from_slice(&bytes).map_or_else(
        |_| {
            opened
                .modified()
                .map(RecoveryLeaseState::Malformed)
                .map_err(|error| BgjobError::Io(error.to_string()))
        },
        |identity| Ok(RecoveryLeaseState::Valid(identity)),
    )
}

fn reject_unsafe_existing_file(path: &Path, message: &str) -> Result<(), BgjobError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return if error.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(BgjobError::Io(error.to_string()))
            };
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(BgjobError::Invalid(message.to_owned()));
    }
    Ok(())
}

fn checked_opened_regular_metadata(
    file: &fs::File,
    path: &Path,
    message: &str,
) -> Result<fs::Metadata, BgjobError> {
    let opened = file
        .metadata()
        .map_err(|error| BgjobError::Io(error.to_string()))?;
    let visible = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(BgjobError::Invalid(message.to_owned()));
        }
        Err(error) => return Err(BgjobError::Io(error.to_string())),
    };
    if visible.file_type().is_symlink() || !visible.is_file() || !opened.is_file() {
        return Err(BgjobError::Invalid(message.to_owned()));
    }
    #[cfg(unix)]
    if opened.dev() != visible.dev() || opened.ino() != visible.ino() {
        return Err(BgjobError::Invalid(message.to_owned()));
    }
    Ok(opened)
}

fn malformed_recovery_lease_is_stale(modified: SystemTime) -> Result<bool, BgjobError> {
    SystemTime::now()
        .duration_since(modified)
        .map(|age| age >= RECOVERY_LEASE_MALFORMED_STALE_AFTER)
        .map_err(|_| BgjobError::Invalid("recovery lease timestamp is in the future".to_owned()))
}

#[cfg(test)]
fn read_recovery_lease(path: &Path) -> Option<RecordedProcessIdentity> {
    match inspect_recovery_lease(path) {
        Ok(RecoveryLeaseState::Valid(identity)) => Some(identity),
        Ok(RecoveryLeaseState::Missing | RecoveryLeaseState::Malformed(_)) | Err(_) => None,
    }
}

fn remove_regular_file(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_symlink() && metadata.is_file() => {
            fs::remove_file(path).is_ok()
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Ok(_) | Err(_) => false,
    }
}

fn identity_rows(
    prefix: &str,
    identity: Option<&RecordedProcessIdentity>,
) -> Vec<(String, String)> {
    identity.map_or_else(Vec::new, |identity| {
        vec![
            (format!("{prefix}_PID"), identity.pid.to_string()),
            (format!("{prefix}_PGID"), identity.pgid.to_string()),
            (format!("{prefix}_START_TIME"), identity.start_time.clone()),
            (
                format!("{prefix}_BIRTH_IDENTITY"),
                identity
                    .birth_identity
                    .as_ref()
                    .map_or_else(String::new, ProcessBirthIdentity::wire_value),
            ),
            (
                format!("{prefix}_COMMAND"),
                identity.command_signature.clone(),
            ),
            (
                format!("{prefix}_EXPECTED"),
                identity.expected_signature.clone(),
            ),
        ]
    })
}

fn parse_identity(
    rows: &BTreeMap<String, String>,
    prefix: &str,
) -> Option<RecordedProcessIdentity> {
    Some(RecordedProcessIdentity {
        pid: rows.get(&format!("{prefix}_PID"))?.parse().ok()?,
        pgid: rows.get(&format!("{prefix}_PGID"))?.parse().ok()?,
        start_time: rows.get(&format!("{prefix}_START_TIME"))?.clone(),
        birth_identity: rows
            .get(&format!("{prefix}_BIRTH_IDENTITY"))
            .and_then(|value| ProcessBirthIdentity::parse_wire_value(value)),
        command_signature: rows.get(&format!("{prefix}_COMMAND"))?.clone(),
        expected_signature: rows
            .get(&format!("{prefix}_EXPECTED"))
            .cloned()
            .unwrap_or_default(),
    })
}

fn render_rows(rows: &[(String, String)]) -> Result<String, BgjobError> {
    let mut rendered = String::new();
    for (key, value) in rows {
        let value = reject_line_value(value, key)?;
        rendered.push_str(key);
        rendered.push('=');
        rendered.push_str(&value);
        rendered.push('\n');
    }
    Ok(rendered)
}

fn resolved_directory(raw: &str) -> Option<PathBuf> {
    let path = Path::new(raw);
    let metadata = fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return None;
    }
    fs::canonicalize(path).ok()
}

fn validated_path(path: &Path, root: &Path) -> Option<PathBuf> {
    let metadata = fs::symlink_metadata(path).ok();
    if metadata.is_some_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_file()) {
        return None;
    }
    let resolved = resolve_candidate(path).ok()?;
    let root = fs::canonicalize(root).ok()?;
    resolved.starts_with(root).then_some(resolved)
}

/// Validate a confined result-output path while retaining a safe I/O obstacle.
///
/// A regular result is the ordinary state. A same-user directory at that
/// precise leaf cannot be consumed as a result, but keeping the registry row
/// readable lets recovery retain its prepared transaction and retry after the
/// obstacle is removed. Symlinks, special files, unsafe parents, and escaped
/// paths remain invalid registry state.
fn validated_result_output_path(path: &Path, root: &Path) -> Option<PathBuf> {
    let resolved = ensure_under(path, root, "result env").ok()?;
    validate_parent_chain(&resolved, root, "result env parent is unsafe").ok()?;
    match fs::symlink_metadata(&resolved) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(resolved),
        Ok(metadata)
            if !metadata.file_type().is_symlink() && (metadata.is_file() || metadata.is_dir()) =>
        {
            Some(resolved)
        }
        Ok(_) | Err(_) => None,
    }
}

/// Validate a confined merge-input path while retaining a safe I/O obstacle.
///
/// A registry row must remain readable when a same-user directory replaces a
/// merge leaf: recovery then reports the failed durable input rather than
/// discarding the child completion. Symlinks, special files, unsafe parents,
/// and escaped paths still invalidate the row.
fn validated_merge_result_path(path: &Path, tmpdir: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let resolved = ensure_under(path, tmpdir, "merge result env").ok()?;
    validate_parent_chain(&resolved, tmpdir, "merge result env parent is unsafe").ok()?;
    match fs::symlink_metadata(&resolved) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(resolved),
        Ok(metadata)
            if !metadata.file_type().is_symlink() && (metadata.is_file() || metadata.is_dir()) =>
        {
            Some(resolved)
        }
        Ok(_) | Err(_) => None,
    }
}

fn parse_registry_sentinel_paths(
    rows: &BTreeMap<String, String>,
    tmpdir: &Path,
) -> Option<Vec<PathBuf>> {
    let Some(raw_count) = rows.get(REGISTRY_SENTINEL_COUNT_KEY) else {
        return (!rows
            .keys()
            .any(|key| key.starts_with(REGISTRY_SENTINEL_PREFIX)))
        .then(Vec::new);
    };
    let count = raw_count.parse::<usize>().ok()?;
    // Each persisted sentinel consumes one row, so a count above the full
    // decoded row set cannot be legitimate and must not drive an allocation.
    if count > rows.len() {
        return None;
    }
    let expected_keys = (0..count)
        .map(|index| format!("{REGISTRY_SENTINEL_PREFIX}{index}"))
        .collect::<BTreeSet<_>>();
    if rows.keys().any(|key| {
        key != REGISTRY_SENTINEL_COUNT_KEY
            && key.starts_with(REGISTRY_SENTINEL_PREFIX)
            && !expected_keys.contains(key)
    }) {
        return None;
    }
    expected_keys
        .iter()
        .map(|key| {
            let path = Path::new(rows.get(key)?);
            path.is_absolute()
                .then(|| ensure_under(path, tmpdir, "registry sentinel"))?
                .ok()
        })
        .collect()
}

fn parse_recovery_inputs_version(rows: &BTreeMap<String, String>) -> Option<bool> {
    let has_recovery_input_field = rows.contains_key(REGISTRY_SENTINEL_COUNT_KEY)
        || rows.contains_key(REGISTRY_MERGE_RESULT_ENV_KEY)
        || rows.contains_key(REGISTRY_TERMINAL_STDOUT_KEY)
        || rows
            .keys()
            .any(|key| key.starts_with(REGISTRY_SENTINEL_PREFIX));
    match rows.get(REGISTRY_RECOVERY_INPUTS_VERSION_KEY) {
        Some(value) if value == REGISTRY_RECOVERY_INPUTS_VERSION => rows
            .contains_key(REGISTRY_SENTINEL_COUNT_KEY)
            .then_some(true),
        Some(_) => None,
        None if has_recovery_input_field => None,
        None => Some(false),
    }
}

fn ensure_directory(path: &Path, label: &str) -> Result<PathBuf, BgjobError> {
    let absolute = absolute_path(path)?;
    if fs::symlink_metadata(&absolute).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(BgjobError::Invalid(format!(
            "{label} must not be a symlink: {}",
            path.display()
        )));
    }
    fs::create_dir_all(&absolute).map_err(|error| BgjobError::Io(error.to_string()))?;
    checked_dir(&absolute, label, true)
}

fn validate_parent_chain(path: &Path, root: &Path, message: &str) -> Result<(), BgjobError> {
    let root = resolve_candidate(root)?;
    let resolved_path = resolve_candidate(path)?;
    let mut parent = resolved_path
        .parent()
        .ok_or_else(|| BgjobError::Invalid(message.to_owned()))?;
    loop {
        let metadata =
            fs::symlink_metadata(parent).map_err(|_| BgjobError::Invalid(message.to_owned()))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(BgjobError::Invalid(message.to_owned()));
        }
        if parent == root {
            return Ok(());
        }
        parent = parent
            .parent()
            .ok_or_else(|| BgjobError::Invalid(message.to_owned()))?;
    }
}

fn resolve_candidate(path: &Path) -> Result<PathBuf, BgjobError> {
    let absolute = absolute_path(path)?;
    if absolute
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(BgjobError::Invalid(format!(
            "unsafe path: {}",
            path.display()
        )));
    }
    let mut ancestor = absolute.as_path();
    let mut tail = Vec::new();
    while !ancestor.exists() {
        let name = ancestor
            .file_name()
            .ok_or_else(|| BgjobError::Invalid(format!("unsafe path: {}", path.display())))?;
        tail.push(name.to_owned());
        ancestor = ancestor
            .parent()
            .ok_or_else(|| BgjobError::Invalid(format!("unsafe path: {}", path.display())))?;
    }
    let mut resolved =
        fs::canonicalize(ancestor).map_err(|error| BgjobError::Io(error.to_string()))?;
    for name in tail.iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

fn absolute_path(path: &Path) -> Result<PathBuf, BgjobError> {
    let path = expand_home(path);
    if path.is_absolute() {
        Ok(path)
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|error| BgjobError::Io(error.to_string()))
    }
}

fn expand_home(path: &Path) -> PathBuf {
    let Some(raw) = path.to_str() else {
        return path.to_path_buf();
    };
    let Some(tail) = raw.strip_prefix("~/") else {
        return if raw == "~" {
            env::var_os("HOME").map_or_else(|| PathBuf::from("~"), PathBuf::from)
        } else {
            path.to_path_buf()
        };
    };
    env::var_os("HOME")
        .map(PathBuf::from)
        .map_or_else(|| path.to_path_buf(), |home| home.join(tail))
}

fn canonical_or_absolute(path: &Path) -> PathBuf {
    fs::canonicalize(path)
        .unwrap_or_else(|_| absolute_path(path).unwrap_or_else(|_| path.to_path_buf()))
}

fn temporary_path(parent: &Path, target: &Path) -> Result<PathBuf, BgjobError> {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| BgjobError::Invalid("wire file name is unsafe".to_owned()))?;
    for _ in 0..32 {
        let serial = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".{name}.tmp.{}.{}", process::id(), serial));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(BgjobError::Io(
        "cannot allocate atomic-write temporary file".to_owned(),
    ))
}

/// Return whole seconds since the Unix epoch, saturating on a clock fault.
#[must_use]
pub fn epoch_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::{
        BGJOB_RESULT_ENV_SUFFIX, BGJOB_STARTUP_ENV_SUFFIX, BgjobError, CompletionTransactionState,
        RECOVERY_LEASE_MALFORMED_STALE_AFTER, REGISTRY_SENTINEL_PREFIX, RecoveryClaim,
        RecoveryLeaseFaultPhase, RegistryEntry, absolute_path, bgjob_dir, checked_dir,
        child_liveness, claim_recovery, completion_result_is_visible, daemon_liveness,
        default_run_id, ensure_directory, ensure_under, entry_expired, epoch_now, expand_home,
        finish_completion_transaction, has_live_entry_at, identity_rows, iter_entries_at,
        log_paths, parse_identity, prepare_completion_transaction, private_atomic_write,
        read_completion_transaction, read_entry, read_recovery_lease, refresh_wait_lease,
        refresh_wait_lease_for_pid, registry_path, reject_line_value, release_recovery_claim,
        render_rows, resolve_candidate, resolve_run_id, resolved_directory, result_env_path,
        set_recovery_lease_fault, startup_env_path, temporary_path, unlink_entry,
        validate_initial_merge_rows, validate_merge_result_env, validate_parent_chain,
        validate_run_id, validate_slug, validate_terminal_stdout_key, validated_path,
        wait_lease_is_fresh, wait_lease_path, write_entry_at,
    };
    use crate::{
        IdentityProbeOutput, ProcessBirthIdentity, ProcessBirthIdentityProbeOutput,
        ProcessIdentityHost, RecordedProcessIdentity, TerminateSignal,
    };
    use std::{
        collections::BTreeMap,
        env, fs,
        path::{Path, PathBuf},
        sync::{Arc, Barrier},
        thread,
        time::{Duration, SystemTime},
    };

    fn identity(pid: i32) -> RecordedProcessIdentity {
        RecordedProcessIdentity {
            pid,
            pgid: pid,
            start_time: format!("start-{pid}"),
            birth_identity: Some(ProcessBirthIdentity::Darwin {
                seconds: 1,
                microseconds: u64::try_from(pid).unwrap_or_default(),
            }),
            command_signature: format!("command-{pid}"),
            expected_signature: String::new(),
        }
    }

    fn entry(tmpdir: &Path, owner: Option<RecordedProcessIdentity>) -> RegistryEntry {
        let tmpdir = checked_dir(tmpdir, "tmpdir", true).expect("tmpdir");
        let log_dir = tmpdir.join("logs");
        fs::create_dir_all(&log_dir).expect("logs");
        fs::create_dir_all(tmpdir.join("bgjob")).expect("bgjob");
        RegistryEntry {
            step: "demo-step".to_owned(),
            run_id: "run-1".to_owned(),
            tmpdir: tmpdir.clone(),
            log_dir: log_dir.clone(),
            clone_path: env::current_dir().expect("cwd"),
            daemon: identity(11),
            child: identity(12),
            child_allows_exec: false,
            owner,
            start_epoch: 1,
            budget_s: 30,
            stdout_log: log_dir.join("demo-step.stdout.log"),
            stderr_log: log_dir.join("demo-step.stderr.log"),
            result_env: tmpdir.join("bgjob/demo-step.result.env"),
            recovery_inputs_recorded: true,
            merge_result_env: None,
            terminal_stdout_key: None,
            sentinel_paths: Vec::new(),
        }
    }

    struct LivenessHost {
        live: bool,
        current_pid: i32,
        probe_error_pid: Option<i32>,
    }

    impl ProcessIdentityHost for LivenessHost {
        fn get_pgid(&self, pid: i32) -> Option<i32> {
            self.live.then_some(pid)
        }

        fn probe_ps_identity(&self, pid: i32) -> IdentityProbeOutput {
            if self.probe_error_pid == Some(pid) {
                return IdentityProbeOutput::Error;
            }
            if self.live {
                IdentityProbeOutput::Stdout("Fri Jul 3 17:01:02 2026 worker".to_owned())
            } else {
                IdentityProbeOutput::Missing
            }
        }

        fn probe_process_birth_identity(&self, pid: i32) -> ProcessBirthIdentityProbeOutput {
            self.live
                .then_some(ProcessBirthIdentity::Darwin {
                    seconds: 1,
                    microseconds: u64::try_from(pid).unwrap_or_default(),
                })
                .map_or(
                    ProcessBirthIdentityProbeOutput::Missing,
                    ProcessBirthIdentityProbeOutput::Identity,
                )
        }

        fn pgrep_children(&self, _pid: i32) -> Vec<i32> {
            Vec::new()
        }

        fn pgrep_group(&self, _pgid: i32) -> Vec<i32> {
            Vec::new()
        }

        fn signal_process(&self, _pid: i32, _signal: TerminateSignal) -> bool {
            false
        }

        fn signal_group(&self, _pgid: i32, _signal: TerminateSignal) -> bool {
            false
        }

        fn sleep(&self, _duration: Duration) {}

        fn monotonic_now(&self) -> Duration {
            Duration::ZERO
        }

        fn wall_time_secs(&self) -> f64 {
            0.0
        }

        fn current_pid(&self) -> i32 {
            self.current_pid
        }

        fn parent_pid(&self) -> i32 {
            0
        }

        fn parent_of(&self, _pid: i32) -> Option<i32> {
            None
        }

        fn list_processes(&self) -> Vec<(i32, String)> {
            Vec::new()
        }

        fn resolve_path(&self, path: &str) -> String {
            path.to_owned()
        }

        fn append_kill_log_line(&self, _path: &Path, _line: &str) {}

        fn write_identity_file(&self, _path: &Path, _text: &str) -> Result<(), String> {
            Ok(())
        }

        fn read_identity_file(&self, _path: &Path) -> Option<String> {
            None
        }

        fn remove_file(&self, _path: &Path) {}

        fn is_regular_file(&self, _path: &Path) -> bool {
            false
        }

        fn file_mtime_ns(&self, _path: &Path) -> Option<u64> {
            None
        }

        fn read_text_lossy(&self, _path: &Path) -> Option<String> {
            None
        }
    }

    fn live_identity(pid: i32) -> RecordedProcessIdentity {
        RecordedProcessIdentity {
            pid,
            pgid: pid,
            start_time: "Fri Jul 3 17:01:02 2026".to_owned(),
            birth_identity: Some(ProcessBirthIdentity::Darwin {
                seconds: 1,
                microseconds: u64::try_from(pid).unwrap_or_default(),
            }),
            command_signature: "worker".to_owned(),
            expected_signature: String::new(),
        }
    }

    fn recovery_lease_path(entry_path: &Path) -> PathBuf {
        let parent = entry_path.parent().expect("registry parent");
        let entry_name = entry_path
            .file_name()
            .expect("registry name")
            .to_string_lossy();
        parent.join(format!(".{entry_name}.recovery"))
    }

    fn age_recovery_lease(path: &Path) {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("recovery lease");
        let old = SystemTime::now() - RECOVERY_LEASE_MALFORMED_STALE_AFTER - Duration::from_secs(1);
        file.set_times(fs::FileTimes::new().set_modified(old))
            .expect("age recovery lease");
    }

    #[test]
    fn session_env_run_ids_win_over_the_tmpdir_fallback() {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let tmpdir = checked_dir(sandbox.path(), "tmpdir", true).expect("tmpdir");
        let clone = env::current_dir().expect("cwd");
        assert_eq!(
            resolve_run_id("run-explicit", &tmpdir, &clone),
            "run-explicit"
        );
        assert_eq!(
            resolve_run_id("", &tmpdir, &clone),
            default_run_id(&tmpdir, &clone)
        );
        fs::write(
            tmpdir.join("session-env.sh"),
            "OTHER=value\nexport LARCH_RUN_ID='run-persisted'\n",
        )
        .expect("session env");
        assert_eq!(resolve_run_id("", &tmpdir, &clone), "run-persisted");
        fs::write(
            tmpdir.join("session-env.sh"),
            "LARCH_RUN_ID=\"run-quoted\"\n",
        )
        .expect("session env");
        assert_eq!(resolve_run_id("bad run id", &tmpdir, &clone), "run-quoted");
    }

    #[test]
    fn record_paths_reject_unsafe_slugs_and_preserve_tmpdir_run_id_fallback() {
        let sandbox = tempfile::tempdir().expect("tempdir");
        assert!(validate_slug("bad/step", "step").is_err());
        assert!(result_env_path(sandbox.path(), "bad/step").is_err());
        assert_eq!(
            default_run_id(sandbox.path(), PathBuf::from("/tmp/left").as_path()),
            default_run_id(sandbox.path(), PathBuf::from("/tmp/right").as_path())
        );
        assert!(
            result_env_path(sandbox.path(), "demo-step")
                .expect("path")
                .ends_with(format!("demo-step{BGJOB_RESULT_ENV_SUFFIX}"))
        );
    }

    #[test]
    fn wait_lease_refresh_is_fresh_until_ttl_expires() {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let tmpdir = sandbox.path();
        assert!(!wait_lease_is_fresh(tmpdir, "lease-step", 30.0));
        refresh_wait_lease(tmpdir, "lease-step").expect("refresh");
        assert!(wait_lease_is_fresh(tmpdir, "lease-step", 30.0));
        assert!(
            wait_lease_path(tmpdir, "lease-step")
                .expect("lease path")
                .ends_with("lease-step.wait-lease.env")
        );
        assert!(!wait_lease_is_fresh(tmpdir, "lease-step", 0.0));

        refresh_wait_lease_for_pid(tmpdir, "lease-step", 4242).expect("rebind");
        let lease = fs::read_to_string(
            wait_lease_path(tmpdir, "lease-step").expect("lease path after rebind"),
        )
        .expect("lease text");
        assert!(lease.contains("WAITER_PID=4242\n"));
        assert!(refresh_wait_lease_for_pid(tmpdir, "lease-step", 0).is_err());
    }

    #[test]
    fn completion_transaction_hides_partial_outputs_and_replays_exact_bytes() {
        let sandbox = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(sandbox.path().join("bgjob")).expect("bgjob dir");
        let sentinels = vec![
            sandbox.path().join("first.sentinel"),
            sandbox.path().join("second.sentinel"),
        ];
        let result = "BGJOB_RC=0\nBGJOB_ELAPSED_S=1\nSTEP=demo-step\nCUSTOM=ordered\n";
        let prepared =
            prepare_completion_transaction(sandbox.path(), "demo-step", &sentinels, result)
                .expect("prepare completion transaction");

        assert_eq!(prepared.state, CompletionTransactionState::Prepared);
        assert!(
            !completion_result_is_visible(sandbox.path(), "demo-step", result)
                .expect("prepared visibility"),
            "a durable intent must not publish a terminal result"
        );
        assert!(
            !prepared.result_env.exists(),
            "preparing must not publish the result envelope"
        );

        let unexpected = sandbox.path().join("unexpected.sentinel");
        let mut caller_copy = prepared;
        caller_copy.sentinel_paths.push(unexpected.clone());
        finish_completion_transaction(&caller_copy).expect("finish completion transaction");
        let committed = read_completion_transaction(sandbox.path(), "demo-step")
            .expect("read completion transaction")
            .expect("completion descriptor");
        assert_eq!(committed.state, CompletionTransactionState::Committed);
        assert_eq!(
            fs::read_to_string(&committed.result_env).expect("result bytes"),
            result,
            "publication must preserve the exact result envelope bytes"
        );
        assert!(
            completion_result_is_visible(sandbox.path(), "demo-step", result)
                .expect("committed visibility")
        );
        for sentinel in &sentinels {
            assert!(
                sentinel.is_file(),
                "missing sentinel: {}",
                sentinel.display()
            );
        }
        assert!(
            !unexpected.exists(),
            "publication must use the durable descriptor rather than caller memory"
        );

        fs::remove_file(&sentinels[1]).expect("remove one sentinel");
        assert!(
            !completion_result_is_visible(sandbox.path(), "demo-step", result)
                .expect("incomplete committed visibility"),
            "a removed sentinel must revoke terminal visibility"
        );
        finish_completion_transaction(&committed).expect("replay committed transaction");
        assert!(
            completion_result_is_visible(sandbox.path(), "demo-step", result)
                .expect("replayed visibility")
        );
        assert_eq!(
            fs::read_to_string(&committed.result_env).expect("replayed result bytes"),
            result,
            "recovery must replay the original envelope rather than synthesize one"
        );
    }

    #[test]
    fn recovery_claim_is_exclusive_and_releases_only_its_owner() {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let tmpdir = sandbox.path().join("session");
        fs::create_dir_all(&tmpdir).expect("session");
        let entry = entry(&tmpdir, None);
        let registry = sandbox.path().join("registry");
        fs::create_dir_all(&registry).expect("registry");
        let path = write_entry_at(&entry, Some(&registry)).expect("registry row");
        let host = LivenessHost {
            live: true,
            current_pid: 77,
            probe_error_pid: None,
        };

        let first = match claim_recovery(&host, &path).expect("first claim") {
            RecoveryClaim::Acquired(claim) => claim,
            other => panic!("unexpected first claim: {other:?}"),
        };
        assert!(matches!(
            claim_recovery(&host, &path).expect("contending claim"),
            RecoveryClaim::Busy
        ));
        release_recovery_claim(&first);
        assert!(matches!(
            claim_recovery(&host, &path).expect("released claim"),
            RecoveryClaim::Acquired(_)
        ));
    }

    #[test]
    fn recovery_claim_keeps_a_live_lease_when_its_identity_probe_fails() {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let tmpdir = sandbox.path().join("session");
        fs::create_dir_all(&tmpdir).expect("session");
        let entry = entry(&tmpdir, None);
        let registry = sandbox.path().join("registry");
        fs::create_dir_all(&registry).expect("registry");
        let path = write_entry_at(&entry, Some(&registry)).expect("registry row");
        let holder = LivenessHost {
            live: true,
            current_pid: 77,
            probe_error_pid: None,
        };
        let claim = match claim_recovery(&holder, &path).expect("claim") {
            RecoveryClaim::Acquired(claim) => claim,
            other => panic!("unexpected claim: {other:?}"),
        };
        let contender = LivenessHost {
            live: true,
            current_pid: 88,
            probe_error_pid: Some(77),
        };
        assert!(claim_recovery(&contender, &path).is_err());
        assert!(
            claim.lease_path.exists(),
            "a probe failure must not reclaim another cleaner's live lease"
        );
        release_recovery_claim(&claim);
    }

    #[test]
    fn recovery_claim_keeps_a_live_legacy_lease_without_birth_identity() {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let tmpdir = sandbox.path().join("session");
        fs::create_dir_all(&tmpdir).expect("session");
        let entry = entry(&tmpdir, None);
        let registry = sandbox.path().join("registry");
        fs::create_dir_all(&registry).expect("registry");
        let path = write_entry_at(&entry, Some(&registry)).expect("registry row");
        let holder = LivenessHost {
            live: true,
            current_pid: 77,
            probe_error_pid: None,
        };
        let claim = match claim_recovery(&holder, &path).expect("claim") {
            RecoveryClaim::Acquired(claim) => claim,
            other => panic!("unexpected claim: {other:?}"),
        };
        let mut legacy: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&claim.lease_path).expect("recovery lease"))
                .expect("recovery lease JSON");
        legacy
            .as_object_mut()
            .expect("recovery lease object")
            .remove("birth_identity");
        fs::write(
            &claim.lease_path,
            serde_json::to_string(&legacy).expect("legacy recovery lease"),
        )
        .expect("write legacy recovery lease");

        let contender = LivenessHost {
            live: true,
            current_pid: 88,
            probe_error_pid: None,
        };
        assert!(matches!(
            claim_recovery(&contender, &path).expect("legacy lease response"),
            RecoveryClaim::Busy
        ));
        assert!(
            claim.lease_path.exists(),
            "a live legacy claimant must not be reclaimed from weak identity fields"
        );
        fs::remove_file(&claim.lease_path).expect("legacy recovery lease cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn recovery_claim_rejects_symlinked_lease_and_lock_paths() {
        use std::os::unix::fs::symlink;

        let sandbox = tempfile::tempdir().expect("tempdir");
        let tmpdir = sandbox.path().join("session");
        fs::create_dir_all(&tmpdir).expect("session");
        let registry = sandbox.path().join("registry");
        fs::create_dir_all(&registry).expect("registry");
        let path = write_entry_at(&entry(&tmpdir, None), Some(&registry)).expect("registry row");
        let outside = sandbox.path().join("outside");
        fs::write(&outside, "outside\n").expect("outside target");
        let host = LivenessHost {
            live: true,
            current_pid: 77,
            probe_error_pid: None,
        };
        let lease_path = recovery_lease_path(&path);

        symlink(&outside, &lease_path).expect("lease symlink");
        assert!(claim_recovery(&host, &path).is_err());
        fs::remove_file(&lease_path).expect("lease symlink cleanup");

        let lock_path = registry.join(format!(
            ".{}.recovery.lock",
            path.file_name()
                .expect("registry filename")
                .to_string_lossy()
        ));
        fs::remove_file(&lock_path).expect("claim lock cleanup");
        symlink(&outside, &lock_path).expect("claim lock symlink");
        assert!(claim_recovery(&host, &path).is_err());
    }

    #[test]
    fn interrupted_recovery_lease_publication_leaves_no_blocking_final_path() {
        for phase in [
            RecoveryLeaseFaultPhase::TemporaryCreated,
            RecoveryLeaseFaultPhase::PayloadWritten,
            RecoveryLeaseFaultPhase::PayloadSynced,
        ] {
            let sandbox = tempfile::tempdir().expect("tempdir");
            let tmpdir = sandbox.path().join("session");
            fs::create_dir_all(&tmpdir).expect("session");
            let registry = sandbox.path().join("registry");
            fs::create_dir_all(&registry).expect("registry");
            let path =
                write_entry_at(&entry(&tmpdir, None), Some(&registry)).expect("registry row");
            let lease_path = recovery_lease_path(&path);
            let host = LivenessHost {
                live: true,
                current_pid: 77,
                probe_error_pid: None,
            };

            set_recovery_lease_fault(phase);
            assert!(claim_recovery(&host, &path).is_err(), "{phase:?}");
            assert!(
                !lease_path.exists(),
                "{phase:?} exposed a partial recovery lease"
            );
            let temporary_exists = fs::read_dir(&registry)
                .expect("registry entries")
                .filter_map(Result::ok)
                .map(|entry| entry.file_name())
                .any(|name| name.to_string_lossy().contains(".recovery.tmp."));
            assert!(
                !temporary_exists,
                "{phase:?} retained an interrupted recovery-lease temporary"
            );

            let claim = match claim_recovery(&host, &path).expect("retry claim") {
                RecoveryClaim::Acquired(claim) => claim,
                other => panic!("{phase:?} retry did not acquire: {other:?}"),
            };
            assert_eq!(
                read_recovery_lease(&lease_path),
                Some(claim.claimant.clone()),
                "{phase:?} retry did not publish a complete identity"
            );
            release_recovery_claim(&claim);
        }
    }

    #[test]
    fn malformed_recovery_lease_waits_for_publication_window_then_reconciles() {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let tmpdir = sandbox.path().join("session");
        fs::create_dir_all(&tmpdir).expect("session");
        let registry = sandbox.path().join("registry");
        fs::create_dir_all(&registry).expect("registry");
        let path = write_entry_at(&entry(&tmpdir, None), Some(&registry)).expect("registry row");
        let lease_path = recovery_lease_path(&path);
        fs::write(&lease_path, "{\"pid\":").expect("malformed lease");
        let host = LivenessHost {
            live: true,
            current_pid: 77,
            probe_error_pid: None,
        };

        assert!(matches!(
            claim_recovery(&host, &path).expect("fresh malformed lease"),
            RecoveryClaim::Busy
        ));
        assert!(lease_path.exists(), "fresh lease was removed");

        age_recovery_lease(&lease_path);
        let claim = match claim_recovery(&host, &path).expect("stale malformed lease") {
            RecoveryClaim::Acquired(claim) => claim,
            other => panic!("stale malformed lease did not reconcile: {other:?}"),
        };
        assert_eq!(
            read_recovery_lease(&lease_path),
            Some(claim.claimant.clone())
        );
        release_recovery_claim(&claim);
    }

    #[test]
    fn concurrent_claimants_reconcile_one_malformed_lease_to_one_live_owner() {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let tmpdir = sandbox.path().join("session");
        fs::create_dir_all(&tmpdir).expect("session");
        let registry = sandbox.path().join("registry");
        fs::create_dir_all(&registry).expect("registry");
        let path = write_entry_at(&entry(&tmpdir, None), Some(&registry)).expect("registry row");
        let lease_path = recovery_lease_path(&path);
        fs::write(&lease_path, "interrupted publication").expect("malformed lease");
        age_recovery_lease(&lease_path);

        let barrier = Arc::new(Barrier::new(3));
        let first_path = path.clone();
        let first_barrier = Arc::clone(&barrier);
        let first = thread::spawn(move || {
            first_barrier.wait();
            claim_recovery(
                &LivenessHost {
                    live: true,
                    current_pid: 77,
                    probe_error_pid: None,
                },
                &first_path,
            )
        });
        let second_path = path;
        let second_barrier = Arc::clone(&barrier);
        let second = thread::spawn(move || {
            second_barrier.wait();
            claim_recovery(
                &LivenessHost {
                    live: true,
                    current_pid: 88,
                    probe_error_pid: None,
                },
                &second_path,
            )
        });
        barrier.wait();

        let mut claims = Vec::new();
        for result in [
            first.join().expect("first claimant"),
            second.join().expect("second claimant"),
        ] {
            match result.expect("claim result") {
                RecoveryClaim::Acquired(claim) => claims.push(claim),
                RecoveryClaim::Busy => {}
                RecoveryClaim::Gone => panic!("registry row disappeared"),
            }
        }
        assert_eq!(
            claims.len(),
            1,
            "concurrent claimants both acquired recovery"
        );
        assert_eq!(
            read_recovery_lease(&lease_path),
            Some(claims[0].claimant.clone()),
            "the winner did not retain its own lease"
        );
        release_recovery_claim(&claims.pop().expect("winning claim"));
    }

    #[test]
    fn scalar_and_path_validation_rejects_unsafe_inputs() {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let missing = sandbox.path().join("missing");
        let regular = sandbox.path().join("regular");
        fs::write(&regular, "not a directory").expect("regular file");

        assert!(validate_slug("demo-step-2", "step").is_ok());
        for invalid in ["", "UPPER", "two--dots..", "bad/path", "bad\\path"] {
            assert!(validate_slug(invalid, "step").is_err(), "{invalid}");
        }
        assert!(validate_run_id("run_1.2-3").is_ok());
        for invalid in ["", ".", "..", "current", "bad/path", "bad\nrun"] {
            assert!(validate_run_id(invalid).is_err(), "{invalid:?}");
        }
        assert!(checked_dir(&missing, "missing", true).is_err());
        assert!(checked_dir(&missing, "missing", false).is_ok());
        assert!(checked_dir(&regular, "regular", true).is_err());
        assert!(ensure_under(&sandbox.path().join("nested/file"), sandbox.path(), "wire").is_ok());
        assert!(ensure_under(&sandbox.path().join("../escape"), sandbox.path(), "wire").is_err());
        assert!(reject_line_value("one\ntwo", "row").is_err());
        assert!(reject_line_value("one\rtwo", "row").is_err());
        assert_eq!(reject_line_value("one", "row").expect("line"), "one");
        assert_eq!(
            validate_terminal_stdout_key("NEXT_ACTION").expect("terminal marker"),
            "NEXT_ACTION"
        );
        for invalid in [
            "",
            "next_action",
            "1ACTION",
            "BGJOB_RC",
            "BGJOB_ELAPSED_S",
            "STEP",
        ] {
            assert!(
                validate_terminal_stdout_key(invalid).is_err(),
                "accepted unsafe terminal marker {invalid:?}"
            );
        }

        assert!(
            startup_env_path(sandbox.path(), "demo-step")
                .expect("startup path")
                .ends_with(format!("demo-step{BGJOB_STARTUP_ENV_SUFFIX}"))
        );
        assert!(
            result_env_path(sandbox.path(), "demo-step")
                .expect("result path")
                .ends_with(format!("demo-step{BGJOB_RESULT_ENV_SUFFIX}"))
        );

        fs::write(sandbox.path().join("bgjob"), "not a directory").expect("bgjob file");
        assert!(bgjob_dir(sandbox.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_leaves_are_rejected_by_directory_and_merge_validators() {
        use std::os::unix::fs::symlink;

        let sandbox = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("outside");
        let directory_link = sandbox.path().join("directory-link");
        symlink(outside.path(), &directory_link).expect("directory link");
        assert!(checked_dir(&directory_link, "directory", true).is_err());

        let merge_link = sandbox.path().join("merge.env");
        let target = outside.path().join("merge.env");
        fs::write(&target, "OUTCOME=continue\n").expect("target");
        symlink(&target, &merge_link).expect("merge link");
        assert!(validate_merge_result_env(&merge_link, sandbox.path()).is_err());
        assert!(private_atomic_write(&merge_link, "OUTCOME=stop\n", sandbox.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn registry_reader_retains_a_confined_result_directory_for_retry_but_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let sandbox = tempfile::tempdir().expect("tempdir");
        let tmpdir = sandbox.path().join("session");
        fs::create_dir_all(&tmpdir).expect("session");
        let entry = entry(&tmpdir, None);
        let registry = sandbox.path().join("registry");
        fs::create_dir_all(&registry).expect("registry");
        let row = write_entry_at(&entry, Some(&registry)).expect("registry row");
        fs::create_dir(&entry.result_env).expect("result directory obstacle");
        assert!(
            read_entry(&row).is_some(),
            "a confined result I/O obstacle must remain recoverable"
        );

        fs::remove_dir(&entry.result_env).expect("remove result obstacle");
        let outside = tempfile::tempdir().expect("outside");
        let target = outside.path().join("result.env");
        fs::write(&target, "BGJOB_RC=0\n").expect("symlink target");
        symlink(&target, &entry.result_env).expect("result symlink");
        assert!(
            read_entry(&row).is_none(),
            "a symlinked result path must invalidate the registry row"
        );
    }

    #[test]
    fn merge_log_and_atomic_write_paths_cover_fresh_and_existing_files() {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let (log_dir, stdout, stderr) = log_paths(sandbox.path(), None, "demo-step").expect("logs");
        assert!(log_dir.is_dir());
        assert!(stdout.ends_with("demo-step.stdout.log"));
        assert!(stderr.ends_with("demo-step.stderr.log"));
        let custom = sandbox.path().join("custom-logs");
        assert_eq!(
            log_paths(sandbox.path(), Some(&custom), "demo-step")
                .expect("custom logs")
                .0,
            custom.canonicalize().expect("canonical custom logs")
        );

        let merge = sandbox.path().join("merge.env");
        assert_eq!(
            validate_merge_result_env(&merge, sandbox.path()).expect("merge path"),
            sandbox
                .path()
                .canonicalize()
                .expect("canonical sandbox")
                .join("merge.env")
        );
        private_atomic_write(&merge, "OUTCOME=continue\n", sandbox.path()).expect("write merge");
        assert_eq!(
            fs::read_to_string(&merge).expect("merge text"),
            "OUTCOME=continue\n"
        );
        private_atomic_write(&merge, "OUTCOME=stop\n", sandbox.path()).expect("replace merge");
        assert_eq!(
            fs::read_to_string(&merge).expect("replaced text"),
            "OUTCOME=stop\n"
        );
        assert!(validate_merge_result_env(Path::new("relative.env"), sandbox.path()).is_err());
        assert!(
            validate_merge_result_env(
                &tempfile::tempdir().expect("outside").path().join("x"),
                sandbox.path()
            )
            .is_err()
        );

        assert!(
            validate_initial_merge_rows(&[
                ("OUTCOME".to_owned(), "continue".to_owned()),
                ("DETAIL".to_owned(), "two".to_owned()),
            ])
            .is_ok()
        );
        for rows in [
            vec![("STEP".to_owned(), "x".to_owned())],
            vec![
                ("OUTCOME".to_owned(), "one".to_owned()),
                ("OUTCOME".to_owned(), "two".to_owned()),
            ],
            vec![("OUTCOME".to_owned(), "one\ntwo".to_owned())],
        ] {
            assert!(validate_initial_merge_rows(&rows).is_err());
        }
    }

    #[test]
    fn registry_round_trip_rejects_escaped_result_and_validates_seed_rows() {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let tmpdir = checked_dir(sandbox.path(), "tmpdir", true).expect("tmpdir");
        let registry = tmpdir.join("registry");
        let mut entry = entry(&tmpdir, Some(identity(13)));
        entry.merge_result_env = Some(tmpdir.join("bgjob/merge.env"));
        entry.terminal_stdout_key = Some("NEXT_ACTION".to_owned());
        entry.sentinel_paths = vec![tmpdir.join("done.sentinel")];
        let mut incomplete = entry.clone();
        incomplete.recovery_inputs_recorded = false;
        assert!(
            write_entry_at(&incomplete, Some(&registry)).is_err(),
            "a legacy wire row must not silently discard recovery inputs"
        );
        let mut invalid_marker = entry.clone();
        invalid_marker.terminal_stdout_key = Some("next_action".to_owned());
        assert!(
            write_entry_at(&invalid_marker, Some(&registry)).is_err(),
            "the registry writer must reject an unusable terminal marker"
        );
        let path = write_entry_at(&entry, Some(&registry)).expect("write registry");
        assert_eq!(read_entry(&path), Some(entry));
        assert!(
            fs::read_to_string(&path)
                .expect("registry text")
                .contains("CHILD_BIRTH_IDENTITY=darwin:1:12\n")
        );
        let legacy = fs::read_to_string(&path)
            .expect("registry text")
            .replace("CHILD_ALLOW_COMMAND_TRANSITION=false\n", "")
            .lines()
            .filter(|line| {
                !line.ends_with("_BIRTH_IDENTITY=darwin:1:11")
                    && !line.ends_with("_BIRTH_IDENTITY=darwin:1:12")
                    && !line.ends_with("_BIRTH_IDENTITY=darwin:1:13")
                    && !line.starts_with("RECOVERY_INPUTS_VERSION=")
                    && !line.starts_with("MERGE_RESULT_ENV=")
                    && !line.starts_with("TERMINAL_STDOUT_KEY=")
                    && !line.starts_with(REGISTRY_SENTINEL_PREFIX)
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, format!("{legacy}\n")).expect("legacy registry text");
        let legacy_entry = read_entry(&path).expect("legacy registry entry");
        assert!(
            legacy_entry.child_allows_exec,
            "a markerless legacy row must retain exec-capable recovery"
        );
        assert!(legacy_entry.child.birth_identity.is_none());
        assert!(legacy_entry.daemon.birth_identity.is_none());
        assert!(legacy_entry.merge_result_env.is_none());
        assert!(legacy_entry.terminal_stdout_key.is_none());
        assert!(legacy_entry.sentinel_paths.is_empty());
        assert!(!legacy_entry.recovery_inputs_recorded);
        assert!(
            legacy_entry
                .owner
                .expect("legacy owner")
                .birth_identity
                .is_none()
        );
        fs::write(&path, format!("{legacy}\nSENTINEL_COUNT=0\n")).expect("partial recovery inputs");
        assert!(
            read_entry(&path).is_none(),
            "unversioned recovery inputs must not authorize reconstruction"
        );
        fs::write(
            &path,
            format!("{legacy}\nRECOVERY_INPUTS_VERSION=2\nSENTINEL_COUNT=0\n"),
        )
        .expect("unknown recovery input version");
        assert!(
            read_entry(&path).is_none(),
            "unknown recovery input version must fail closed"
        );
        assert!(validate_initial_merge_rows(&[("GOOD".to_owned(), "value".to_owned())]).is_ok());
        assert!(validate_initial_merge_rows(&[("bad".to_owned(), "value".to_owned())]).is_err());
        assert!(
            registry_path("run-1", "demo-step", Some(&registry))
                .expect("registry path")
                .starts_with(&registry)
        );
        assert!(bgjob_dir(&tmpdir).is_ok());
    }

    #[test]
    fn registry_reading_unlink_and_expiry_handle_invalid_records() {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let tmpdir = checked_dir(sandbox.path(), "tmpdir", true).expect("tmpdir");
        let registry = tmpdir.join("registry");
        let mut record = entry(&tmpdir, None);
        record.start_epoch = i64::MAX;
        assert!(!entry_expired(&record));
        record.start_epoch = 0;
        record.budget_s = -1;
        assert!(entry_expired(&record));

        let path = write_entry_at(&record, Some(&registry)).expect("write registry");
        assert!(read_entry(&path).is_some());
        unlink_entry(&path);
        assert!(!path.exists());
        unlink_entry(&path);

        fs::create_dir_all(&registry).expect("registry");
        fs::write(&path, "STEP=demo-step\nRUN_ID=run-1\n").expect("malformed registry");
        assert!(read_entry(&path).is_none());
        fs::write(&path, "not=a-valid-record\n").expect("invalid registry");
        assert!(read_entry(&path).is_none());
        assert!(registry_path("current", "demo-step", Some(&registry)).is_err());
        assert!(registry_path("run-1", "bad/step", Some(&registry)).is_err());
    }

    #[test]
    fn internal_registry_helpers_preserve_identity_and_liveness_rules() {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let tmpdir = checked_dir(sandbox.path(), "tmpdir", true).expect("tmpdir");
        let registry = tmpdir.join("registry");
        let mut record = entry(&tmpdir, None);
        record.daemon = live_identity(11);
        record.child = live_identity(12);
        record.start_epoch = epoch_now();
        record.budget_s = 60;
        let path = write_entry_at(&record, Some(&registry)).expect("registry entry");

        let live_host = LivenessHost {
            live: true,
            current_pid: 0,
            probe_error_pid: None,
        };
        let missing_host = LivenessHost {
            live: false,
            current_pid: 0,
            probe_error_pid: None,
        };
        assert!(daemon_liveness(&live_host, &record).live);
        assert!(child_liveness(&live_host, &record).live);
        assert_eq!(child_liveness(&missing_host, &record).reason, "missing-pid");
        assert!(has_live_entry_at(
            &live_host,
            &env::current_dir().expect("cwd"),
            &record.run_id,
            &registry,
        ));
        assert!(!has_live_entry_at(
            &live_host,
            &tmpdir.join("missing"),
            &record.run_id,
            &registry
        ));
        assert!(!has_live_entry_at(
            &live_host, &tmpdir, "bad/run", &registry
        ));
        let entries = iter_entries_at(&registry);
        assert_eq!(entries, vec![(path, Some(record.clone()))]);

        let rows = identity_rows("TEST", Some(&record.daemon));
        assert_eq!(rows.len(), 6);
        assert!(identity_rows("TEST", None).is_empty());
        let values = rows.into_iter().collect::<BTreeMap<_, _>>();
        assert_eq!(
            values.get("TEST_BIRTH_IDENTITY"),
            Some(&"darwin:1:11".to_owned())
        );
        assert_eq!(parse_identity(&values, "TEST"), Some(record.daemon));
        let mut legacy_values = values;
        legacy_values.remove("TEST_BIRTH_IDENTITY");
        assert!(
            parse_identity(&legacy_values, "TEST")
                .expect("legacy identity")
                .birth_identity
                .is_none()
        );
        assert_eq!(
            render_rows(&[("ONE".to_owned(), "one".to_owned())]).expect("rendered rows"),
            "ONE=one\n"
        );
        assert!(render_rows(&[("ONE".to_owned(), "one\ntwo".to_owned())]).is_err());
        assert_eq!(
            format!("{}", BgjobError::Invalid("invalid".to_owned())),
            "invalid"
        );

        let directory = ensure_directory(&tmpdir.join("nested/dir"), "nested").expect("directory");
        let child = directory.join("child.env");
        assert!(validate_parent_chain(&child, &tmpdir, "unsafe").is_ok());
        assert_eq!(
            resolved_directory(directory.to_str().expect("utf8 directory")),
            Some(directory.clone())
        );
        assert_eq!(validated_path(&child, &directory), Some(child.clone()));
        assert_eq!(
            resolve_candidate(&tmpdir.join("nested/missing.env")).expect("candidate"),
            tmpdir.join("nested/missing.env")
        );
        assert!(resolve_candidate(&tmpdir.join("nested/../escape")).is_err());
        assert!(
            absolute_path(Path::new("relative.env"))
                .expect("absolute path")
                .is_absolute()
        );
        assert!(!expand_home(Path::new("~")).as_os_str().is_empty());
        let temporary = temporary_path(&directory, &child).expect("temporary path");
        assert_eq!(temporary.parent(), Some(directory.as_path()));
    }

    #[cfg(unix)]
    #[test]
    fn filesystem_guard_branches_reject_nonregular_and_symlinked_records() {
        use std::os::unix::fs::symlink;

        let sandbox = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("outside");
        let root_file = sandbox.path().join("root-file");
        fs::write(&root_file, "not a root\n").expect("root file");
        assert!(ensure_under(&sandbox.path().join("child"), &root_file, "child").is_err());

        let tmpdir = sandbox.path().join("session");
        fs::create_dir(&tmpdir).expect("session");
        let bgjob_link = tmpdir.join("bgjob");
        symlink(outside.path(), &bgjob_link).expect("bgjob symlink");
        assert!(bgjob_dir(&tmpdir).is_err());

        let regular_log_dir = sandbox.path().join("regular-log-dir");
        fs::write(&regular_log_dir, "not a directory\n").expect("regular log dir");
        assert!(log_paths(&tmpdir, Some(&regular_log_dir), "demo-step").is_err());
        let merge_dir = sandbox.path().join("merge-dir");
        fs::create_dir(&merge_dir).expect("merge directory");
        assert!(validate_merge_result_env(&merge_dir, sandbox.path()).is_err());

        assert!(read_entry(&merge_dir).is_none());
        let target = outside.path().join("entry.env");
        fs::write(&target, "STEP=demo-step\n").expect("target registry");
        let entry_link = sandbox.path().join("entry.env");
        symlink(&target, &entry_link).expect("registry symlink");
        assert!(read_entry(&entry_link).is_none());
        unlink_entry(&entry_link);
        assert!(entry_link.exists());

        let write_dir = sandbox.path().join("write-dir");
        fs::create_dir(&write_dir).expect("write directory");
        assert!(private_atomic_write(&write_dir, "STATE=ready\n", sandbox.path()).is_err());
        let parent_link = sandbox.path().join("parent-link");
        symlink(outside.path(), &parent_link).expect("parent symlink");
        assert!(
            validate_parent_chain(&parent_link.join("wire.env"), sandbox.path(), "unsafe").is_err()
        );
        assert!(validated_path(&entry_link, sandbox.path()).is_none());
        assert!(resolved_directory(entry_link.to_str().expect("utf8 path")).is_none());
        assert!(ensure_directory(&parent_link, "link").is_err());
        assert!(iter_entries_at(&sandbox.path().join("missing-registry")).is_empty());
    }
}
