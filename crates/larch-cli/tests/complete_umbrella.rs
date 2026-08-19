//! Executable-boundary coverage for the complete-umbrella child harness.

use assert_cmd::Command;
use predicates::prelude::*;
use std::{env, fs, path::Path};
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;

fn larch() -> Command {
    Command::cargo_bin("larch").expect("larch binary should build")
}

fn write(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write fixture");
}

#[cfg(unix)]
fn fixture(body: &str) -> (TempDir, Command) {
    let root = TempDir::new().expect("fixture");
    let bin = root.path().join("bin");
    fs::create_dir(&bin).expect("bin directory");
    let claude = bin.join("claude");
    write(&claude, body);
    fs::set_permissions(&claude, fs::Permissions::from_mode(0o755))
        .expect("Claude fixture permissions");
    let inherited = env::var_os("PATH").unwrap_or_default();
    let path = env::join_paths(std::iter::once(bin).chain(env::split_paths(&inherited)))
        .expect("fixture PATH");
    let mut command = larch();
    command
        .current_dir(root.path())
        .env("PATH", path)
        .env("CLAUDE_PLUGIN_ROOT", root.path())
        .env("GH_CONFIG_DIR", root.path().join("gh-config"))
        .env("XDG_CONFIG_HOME", root.path().join("xdg-config"));
    (root, command)
}

#[cfg(unix)]
fn child_arguments(root: &Path) -> Vec<String> {
    vec![
        "complete-umbrella".to_owned(),
        "run-child".to_owned(),
        "--repository".to_owned(),
        "owner/repo".to_owned(),
        "--repo-root".to_owned(),
        root.display().to_string(),
        "--umbrella".to_owned(),
        "40".to_owned(),
        "--leaf".to_owned(),
        "42".to_owned(),
        "--model".to_owned(),
        "claude-test-model".to_owned(),
        "--output-root".to_owned(),
        root.display().to_string(),
        "--output".to_owned(),
        root.join("child.json").display().to_string(),
        "--result-env".to_owned(),
        root.join("child.env").display().to_string(),
    ]
}

#[test]
fn gap_preflight_validates_files_before_issue_creation() {
    let root = TempDir::new().expect("fixture");
    let title = root.path().join("gap-title.txt");
    let body = root.path().join("gap-body.md");
    write(&title, "Finish the integration\n");
    write(
        &body,
        "This is a leaf of umbrella #40. Read the umbrella in full before acting.\n\nAcceptance criteria.\n",
    );

    larch()
        .args([
            "complete-umbrella",
            "validate-gap",
            "--umbrella",
            "40",
            "--expected-root",
            &root.path().display().to_string(),
            "--expected-title-file",
            &title.display().to_string(),
            "--expected-body-file",
            &body.display().to_string(),
        ])
        .assert()
        .success()
        .stdout("GAP_VALID=true\nUMBRELLA_ISSUE=40\n");

    write(&title, "--operator-invoked\n");
    larch()
        .args([
            "complete-umbrella",
            "validate-gap",
            "--umbrella",
            "40",
            "--expected-root",
            &root.path().display().to_string(),
            "--expected-title-file",
            &title.display().to_string(),
            "--expected-body-file",
            &body.display().to_string(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("prefix-free, option-safe"));
}

#[test]
fn run_leaves_is_exposed_and_rejects_an_unresolved_model_before_remote_work() {
    let root = TempDir::new().expect("fixture");
    larch()
        .args([
            "complete-umbrella",
            "run-leaves",
            "--repository",
            "owner/repo",
            "--repo-root",
            &root.path().display().to_string(),
            "--umbrella",
            "40",
            "--model",
            "unknown",
            "--output-root",
            &root.path().display().to_string(),
            "--output",
            &root.path().join("snapshot.json").display().to_string(),
            "--result-env",
            &root.path().join("result.env").display().to_string(),
            "--operator-invoked",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--model must be one resolved non-whitespace token",
        ));
}

#[test]
fn resume_accepts_the_documented_repository_and_issue_signature() {
    let home = TempDir::new().expect("isolated home");
    larch()
        .env("HOME", home.path())
        .args([
            "complete-umbrella",
            "resume",
            "--repository",
            "owner/repo",
            "--issue",
            "40",
        ])
        .assert()
        .success()
        .stdout("RESUME_FOUND=false\n");
}

#[cfg(unix)]
#[test]
fn child_harness_pins_model_disables_skills_and_requires_completion_marker() {
    let (root, mut command) = fixture(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > claude.argv\nprintf 'GH_CONFIG_DIR=%s\\nXDG_CONFIG_HOME=%s\\nSESSION_TMPDIR=%s\\nCLAUDE_PROJECT_DIR=%s\\n' \"$GH_CONFIG_DIR\" \"$XDG_CONFIG_HOME\" \"$SESSION_TMPDIR\" \"$CLAUDE_PROJECT_DIR\" > claude.env\ncat > claude.prompt\nprintf '%s' '{\"result\":\"verified\\nCOMPLETE_UMBRELLA_CHILD_STATUS=complete\"}'\n",
    );
    command
        .args(child_arguments(root.path()))
        .assert()
        .success()
        .stdout(predicate::str::contains("CHILD_STATUS=complete\n"));

    let canonical = fs::canonicalize(root.path()).expect("canonical fixture");
    assert_eq!(
        fs::read_to_string(root.path().join("claude.argv")).expect("Claude argv"),
        format!(
            "--print\n--output-format\njson\n--model\nclaude-test-model\n--add-dir\n{}\n--allowedTools\nBash,Read,Edit,Write,Glob,Grep,Agent\n--permission-mode\ndontAsk\n--disable-slash-commands\n--no-session-persistence\n",
            canonical.display()
        )
    );
    let prompt = fs::read_to_string(root.path().join("claude.prompt")).expect("Claude prompt");
    assert!(prompt.contains("leaf issue #42 of umbrella #40"));
    assert!(prompt.contains("without using any larch skills"));
    assert!(prompt.contains("exactly four primary general-purpose Agent subagents"));
    assert!(prompt.contains("Do not personally call Read, Grep, Glob, Edit, or Write"));
    assert!(prompt.contains(&format!(
        "HANDOFF_ROOT={}",
        canonical.join("complete-umbrella-leaf-42").display()
    )));
    assert!(prompt.contains("exact value of $SESSION_TMPDIR"));
    assert_eq!(
        fs::read_to_string(root.path().join("claude.env")).expect("Claude environment"),
        format!(
            "GH_CONFIG_DIR={}\nXDG_CONFIG_HOME={}\nSESSION_TMPDIR={}\nCLAUDE_PROJECT_DIR={}\n",
            root.path().join("gh-config").display(),
            root.path().join("xdg-config").display(),
            canonical.join("complete-umbrella-leaf-42").display(),
            canonical.display()
        )
    );
    assert!(canonical.join("complete-umbrella-leaf-42").is_dir());
    assert_eq!(
        fs::read_to_string(root.path().join("child.env")).expect("result env"),
        "CHILD_STATUS=complete\nCHILD_ISSUE=42\nCHILD_ENVELOPE_COMPLETE=true\nCHILD_TRANSIENT_ATTEMPT_COUNT=0\n"
    );
    assert!(root.path().join("child.json").is_file());
}

#[cfg(unix)]
#[test]
fn child_harness_hard_fails_a_noncompletion_envelope() {
    let (root, mut command) = fixture(
        "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"result\":\"could not finish\\nCOMPLETE_UMBRELLA_CHILD_STATUS=failed\"}'\n",
    );
    command
        .args(child_arguments(root.path()))
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "child did not return a complete, bounded success envelope",
        ));
    assert_eq!(
        fs::read_to_string(root.path().join("child.env")).expect("failure result env"),
        "CHILD_STATUS=failed\nCHILD_ISSUE=42\nCHILD_ENVELOPE_COMPLETE=false\nCHILD_TRANSIENT_ATTEMPT_COUNT=0\n"
    );
}

#[cfg(unix)]
#[test]
fn child_harness_classifies_a_transient_claude_api_envelope() {
    let (root, mut command) = fixture(
        "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"is_error\":true,\"terminal_reason\":\"api_error\",\"result\":\"API Error: ENOTFOUND\"}'\n",
    );
    command
        .args(child_arguments(root.path()))
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "child ended on a transient Claude API failure",
        ));
    assert_eq!(
        fs::read_to_string(root.path().join("child.env")).expect("transient result env"),
        "CHILD_STATUS=failed\nCHILD_ISSUE=42\nCHILD_ENVELOPE_COMPLETE=false\nCHILD_TRANSIENT_ATTEMPT_COUNT=0\nCHILD_FAILURE_CLASS=transient-api\n"
    );
}

#[cfg(unix)]
#[test]
fn child_harness_preserves_a_bounded_needs_design_handoff() {
    let (root, mut command) = fixture(
        "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"result\":\"scoped handoff\\nCOMPLETE_UMBRELLA_CHILD_STATUS=needs-design\"}'\n",
    );
    command
        .args(child_arguments(root.path()))
        .assert()
        .success()
        .stdout(predicate::str::contains("CHILD_STATUS=needs-design\n"));
    assert_eq!(
        fs::read_to_string(root.path().join("child.env")).expect("needs-design result env"),
        "CHILD_STATUS=needs-design\nCHILD_ISSUE=42\nCHILD_ENVELOPE_COMPLETE=false\nCHILD_TRANSIENT_ATTEMPT_COUNT=0\nCHILD_FAILURE_CLASS=needs-design\n"
    );
}
