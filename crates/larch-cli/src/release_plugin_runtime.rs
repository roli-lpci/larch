//! Generation and validation of the runtime-only Claude plugin projection.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use larch_adapters::{GixRepository, PathIntent, RepositoryRoot, read_utf8};
use larch_core::{GitPath, StatusOptions};

const DIRECT_FILES: &[&str] = &[
    ".claude-plugin/plugin.json",
    "ARCHITECTURE.md",
    "LICENSE",
    "SECURITY.md",
    "docs/analysis-state.md",
    "docs/ci-latency-evidence.md",
    "docs/complete-umbrella-recovery.md",
    "docs/configuration-and-permissions.md",
    "docs/dev-hook-audit.md",
    "docs/difficulty-floor-globs.tsv",
    "docs/external-reviewers.md",
    "docs/git-operation-inventory.md",
    "docs/github-service-inventory.md",
    "docs/google-service-inventory.md",
    "docs/installation-and-setup.md",
    "docs/issue-anchored-plan.md",
    "docs/linting.md",
    "docs/progress-reporting.md",
    "docs/python-migration.md",
    "docs/review-agents.md",
    "docs/run-log-archive.md",
    "docs/run-log-batches.md",
    "docs/run-log-cli.md",
    "docs/run-log-corpus-audit-2026-08-09.md",
    "docs/run-logs-required-files.tsv",
    "docs/run-logs.md",
    "docs/rust-async-runtime.md",
    "docs/rust-parity-harness.md",
    "docs/rust-ci-selection-observation.md",
    "docs/rust-testing.md",
    "docs/security/README.md",
    "docs/skills.md",
    "python/cli.py",
    "python/stall-recovery-report.md",
    "python/stall-recovery-report-allowlists.tsv",
    "scripts/block-submodule-edit.sh",
    "scripts/check-stale-plugin.sh",
    "scripts/cleanup-sessionstart.sh",
    "scripts/deny-edit-write.sh",
    "scripts/dry-runnable-scripts.tsv",
    "scripts/file-failure-report-cross-repo.sh",
    "scripts/generators.tsv",
    "scripts/hook-anti-read-poll.sh",
    "scripts/hook-deny-run-in-background.sh",
    "scripts/larch.sh",
    "scripts/read-result-env.sh",
    "scripts/resolve-upstream-larch-repo.sh",
    "scripts/sessionstart-health.sh",
    "scripts/sessionstart-statusline.sh",
    "scripts/sleep-seconds.sh",
];

const DEV_ONLY_PYTHON: &[&str] = &[];

const INDEX_ERROR: &str = "plugin runtime projection requires a readable git index";
const ROOT_ERROR: &str = "plugin runtime projection requires the larch repository root";

pub fn run(check: bool, check_worktree: bool) -> Result<(), String> {
    let current = std::env::current_dir().map_err(|_| ROOT_ERROR.to_owned())?;
    let root = RepositoryRoot::resolve(Some(&current)).map_err(|_| ROOT_ERROR.to_owned())?;
    let paths = runtime_paths(&root)?;

    if check {
        let errors = projection_errors(&root, &paths)?;
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
    }
    if !check {
        validate_root(&root)?;
        sync(&root, &paths)?;
    }
    if check_worktree {
        verify_projection_worktree(&root)?;
    }
    Ok(())
}

fn verify_projection_worktree(root: &RepositoryRoot) -> Result<(), String> {
    let repository = GixRepository::open(root.path()).map_err(|_| INDEX_ERROR.to_owned())?;
    let status = repository
        .local_status(&StatusOptions {
            pathspecs: vec![GitPath::new(b"plugin".to_vec())],
            include_untracked: true,
            include_ignored: false,
        })
        .map_err(|_| "unable to inspect runtime projection worktree".to_owned())?;
    if status.is_dirty() {
        return Err("runtime projection changed tracked or untracked files".to_owned());
    }
    Ok(())
}

fn runtime_paths(root: &RepositoryRoot) -> Result<BTreeSet<String>, String> {
    let repository = GixRepository::open(root.path()).map_err(|_| INDEX_ERROR.to_owned())?;
    let tracked = repository
        .tracked_paths()
        .map_err(|_| INDEX_ERROR.to_owned())?;
    let mut selected = BTreeSet::new();

    for tracked_path in tracked {
        let path = String::from_utf8(tracked_path.as_bytes().to_vec())
            .map_err(|_| INDEX_ERROR.to_owned())?;
        if path.is_empty()
            || path.starts_with("plugin/")
            || DEV_ONLY_PYTHON.contains(&path.as_str())
            || Path::new(&path)
                .components()
                .any(|part| part.as_os_str() == "__pycache__")
        {
            continue;
        }
        if DIRECT_FILES.contains(&path.as_str())
            || path.starts_with("agents/")
            || (path.starts_with("docs/security/") && is_markdown_path(&path))
            || path.starts_with("hooks/")
            || (path.starts_with("skills/") && !is_test_path(&path))
            || is_runtime_python_path(&path)
        {
            selected.insert(path);
        }
    }

    let missing: Vec<&str> = DIRECT_FILES
        .iter()
        .copied()
        .filter(|path| !selected.contains(*path))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "plugin runtime projection inputs are missing: {}",
            missing.join(", ")
        ));
    }

    let unsafe_paths: Vec<&str> = selected
        .iter()
        .filter(|path| root.confine(path, PathIntent::Read).is_err())
        .map(String::as_str)
        .collect();
    if !unsafe_paths.is_empty() {
        return Err(format!(
            "plugin runtime projection inputs are unsafe: {}",
            unsafe_paths.join(", ")
        ));
    }
    validate_skill_security_references(root, &selected)?;
    Ok(selected)
}

fn validate_skill_security_references(
    root: &RepositoryRoot,
    selected: &BTreeSet<String>,
) -> Result<(), String> {
    let mut missing = BTreeSet::new();
    for skill in selected
        .iter()
        .filter(|path| path.starts_with("skills/") && is_markdown_path(path))
    {
        let source = root
            .confine(skill, PathIntent::Read)
            .map_err(|_| format!("plugin runtime projection inputs are unsafe: {skill}"))?;
        let contents = read_utf8(&source)
            .map_err(|_| format!("plugin runtime projection inputs are unreadable: {skill}"))?;
        for reference in focused_security_references(&contents) {
            if !selected.contains(&reference) {
                missing.insert(format!("{skill} -> {reference}"));
            }
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "shipped skill security references are missing: {}",
        missing.into_iter().collect::<Vec<_>>().join(", ")
    ))
}

fn focused_security_references(contents: &str) -> BTreeSet<String> {
    const PREFIX: &str = "docs/security/";
    let mut references = BTreeSet::new();
    let mut remainder = contents;
    while let Some(offset) = remainder.find(PREFIX) {
        let candidate = &remainder[offset..];
        let end = candidate
            .find(|character: char| {
                !character.is_ascii_alphanumeric() && !matches!(character, '/' | '-' | '_' | '.')
            })
            .unwrap_or(candidate.len());
        let candidate = candidate[..end].trim_end_matches('.');
        if candidate.len() > PREFIX.len() {
            references.insert(candidate.to_owned());
        }
        remainder = &remainder[offset + PREFIX.len()..];
    }
    references
}

fn is_markdown_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn is_test_path(path: &str) -> bool {
    Path::new(path).components().any(|part| {
        let part = part.as_os_str().to_string_lossy();
        part == "fixtures" || part.starts_with("test-") || part.starts_with("test_")
    })
}

fn is_runtime_python_path(path: &str) -> bool {
    let Some(remainder) = path.strip_prefix("python/larch/") else {
        return false;
    };
    if is_test_path(path) {
        return false;
    }
    let Some(package) = remainder.split('/').next() else {
        return false;
    };
    !matches!(package, "lint" | "release")
}

fn validate_root(root: &RepositoryRoot) -> Result<(), String> {
    let manifest = root
        .confine(".claude-plugin/plugin.json", PathIntent::Read)
        .map_err(|_| ROOT_ERROR.to_owned())?;
    let payload: serde_json::Value =
        serde_json::from_str(&read_utf8(&manifest).map_err(|_| ROOT_ERROR.to_owned())?)
            .map_err(|_| ROOT_ERROR.to_owned())?;
    if payload.get("name").and_then(serde_json::Value::as_str) != Some("larch")
        || !root.path().join(".git").exists()
    {
        return Err(ROOT_ERROR.to_owned());
    }
    Ok(())
}

fn projection_errors(
    root: &RepositoryRoot,
    expected: &BTreeSet<String>,
) -> Result<Vec<String>, String> {
    let projection = root.path().join("plugin");
    match fs::symlink_metadata(&projection) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Ok(vec![
                "runtime projection root must be a real directory".to_owned(),
            ]);
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("unable to inspect runtime projection".to_owned()),
    }

    let actual = if projection.is_dir() {
        projection_paths(&projection)?
    } else {
        BTreeSet::new()
    };
    let mut errors: Vec<String> = expected
        .difference(&actual)
        .map(|path| format!("missing runtime projection: {path}"))
        .collect();
    errors.extend(
        actual
            .difference(expected)
            .map(|path| format!("unexpected runtime projection: {path}")),
    );
    for path in expected.intersection(&actual) {
        let source = root
            .confine(path, PathIntent::Read)
            .map_err(|_| format!("runtime projection differs from its source: {path}"))?;
        let copy = projection.join(path);
        let matching = fs::symlink_metadata(&copy)
            .ok()
            .is_some_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
            && fs::read(source.path()).ok() == fs::read(&copy).ok();
        if !matching {
            errors.push(format!(
                "runtime projection differs from its source: {path}"
            ));
        }
    }
    Ok(errors)
}

fn projection_paths(projection: &Path) -> Result<BTreeSet<String>, String> {
    let mut paths = BTreeSet::new();
    collect_projection_paths(projection, projection, &mut paths)?;
    Ok(paths)
}

fn collect_projection_paths(
    root: &Path,
    directory: &Path,
    paths: &mut BTreeSet<String>,
) -> Result<(), String> {
    for entry in
        fs::read_dir(directory).map_err(|_| "unable to inspect runtime projection".to_owned())?
    {
        let entry = entry.map_err(|_| "unable to inspect runtime projection".to_owned())?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| "unable to inspect runtime projection".to_owned())?;
        if metadata.file_type().is_symlink() || metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "unable to inspect runtime projection".to_owned())?;
            paths.insert(
                relative
                    .to_str()
                    .ok_or_else(|| "unable to inspect runtime projection".to_owned())?
                    .replace(std::path::MAIN_SEPARATOR, "/"),
            );
        } else if metadata.is_dir() {
            collect_projection_paths(root, &path, paths)?;
        }
    }
    Ok(())
}

fn sync(root: &RepositoryRoot, paths: &BTreeSet<String>) -> Result<(), String> {
    let projection = root.path().join("plugin");
    if let Ok(metadata) = fs::symlink_metadata(&projection) {
        if metadata.file_type().is_symlink()
            || !metadata.is_dir()
            || projection.parent() != Some(root.path())
        {
            return Err("refusing to replace an unsafe plugin projection path".to_owned());
        }
        let confined = root
            .confine(&projection, PathIntent::Cleanup)
            .map_err(|_| "refusing to replace an unsafe plugin projection path".to_owned())?;
        ensure_real_tree(&projection)?;
        confined
            .revalidate()
            .map_err(|_| "refusing to replace an unsafe plugin projection path".to_owned())?;
        fs::remove_dir_all(confined.path())
            .map_err(|_| "refusing to replace an unsafe plugin projection path".to_owned())?;
    }
    for path in paths {
        copy_runtime_path(root, path)?;
    }
    Ok(())
}

fn ensure_real_tree(directory: &Path) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|_| "refusing to replace an unsafe plugin projection path".to_owned())?
    {
        let entry =
            entry.map_err(|_| "refusing to replace an unsafe plugin projection path".to_owned())?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| "refusing to replace an unsafe plugin projection path".to_owned())?;
        if metadata.file_type().is_symlink() {
            return Err("refusing to replace an unsafe plugin projection path".to_owned());
        }
        if metadata.is_dir() {
            ensure_real_tree(&path)?;
        } else if !metadata.is_file() {
            return Err("refusing to replace an unsafe plugin projection path".to_owned());
        }
    }
    Ok(())
}

fn copy_runtime_path(root: &RepositoryRoot, path: &str) -> Result<(), String> {
    let source = root
        .confine(path, PathIntent::Read)
        .map_err(|_| format!("plugin runtime projection inputs are unsafe: {path}"))?;
    let destination = root.path().join("plugin").join(path);
    let parent = destination
        .parent()
        .ok_or_else(|| "refusing to write an unsafe plugin projection path".to_owned())?;
    create_real_directories(root.path(), parent)?;
    let destination = root
        .confine(&destination, PathIntent::Write)
        .map_err(|_| "refusing to write an unsafe plugin projection path".to_owned())?;
    fs::copy(source.path(), destination.path())
        .map_err(|_| "refusing to write an unsafe plugin projection path".to_owned())?;
    destination
        .revalidate()
        .map_err(|_| "refusing to write an unsafe plugin projection path".to_owned())?;
    Ok(())
}

fn create_real_directories(root: &Path, destination: &Path) -> Result<(), String> {
    let relative = destination
        .strip_prefix(root)
        .map_err(|_| "refusing to write an unsafe plugin projection path".to_owned())?;
    let mut current = PathBuf::from(root);
    for component in relative.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)
                .map_err(|_| "refusing to write an unsafe plugin projection path".to_owned())?,
            Ok(_) | Err(_) => {
                return Err("refusing to write an unsafe plugin projection path".to_owned());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DEV_ONLY_PYTHON, DIRECT_FILES, ROOT_ERROR, focused_security_references, projection_errors,
        runtime_paths, sync, validate_root, verify_projection_worktree,
    };
    use larch_adapters::RepositoryRoot;
    use std::{collections::BTreeSet, fs, path::Path, process::Command};
    use tempfile::TempDir;

    #[test]
    fn generates_and_validates_the_complete_runtime_projection() {
        let fixture = fixture();
        let root = repository_root(fixture.path());
        let paths = runtime_paths(&root).expect("runtime path selection");

        sync(&root, &paths).expect("projection generation");

        assert!(
            projection_errors(&root, &paths)
                .expect("projection validation")
                .is_empty()
        );
        assert!(paths.contains("agents/reviewer.md"));
        assert!(paths.contains("ARCHITECTURE.md"));
        assert!(paths.contains("SECURITY.md"));
        assert!(paths.contains("docs/git-operation-inventory.md"));
        assert!(paths.contains("docs/github-service-inventory.md"));
        assert!(paths.contains("docs/google-service-inventory.md"));
        assert!(paths.contains("docs/dev-hook-audit.md"));
        assert!(paths.contains("docs/progress-reporting.md"));
        assert!(paths.contains("docs/run-log-corpus-audit-2026-08-09.md"));
        assert!(paths.contains("docs/rust-async-runtime.md"));
        assert!(paths.contains("docs/rust-parity-harness.md"));
        assert!(paths.contains("docs/rust-ci-selection-observation.md"));
        assert!(paths.contains("docs/rust-testing.md"));
        assert!(paths.contains("docs/security/README.md"));
        assert!(paths.contains("docs/security/workflow.md"));
        assert!(paths.contains("hooks/hooks.json"));
        assert!(paths.contains("skills/implement/SKILL.md"));
        assert!(paths.contains("python/larch/core/runtime.py"));
        assert!(!paths.contains("skills/implement/test-helper.md"));
        assert!(!paths.contains("python/larch/release/runtime.py"));
        assert!(!paths.contains("python/larch/lint/runtime.py"));
        for path in DEV_ONLY_PYTHON {
            assert!(!paths.contains(*path));
        }
    }

    #[test]
    fn includes_complete_umbrella_recovery_runbook_in_runtime_projection() {
        let fixture = fixture();
        let root = repository_root(fixture.path());

        let paths = runtime_paths(&root).expect("runtime path selection");

        assert!(paths.contains("docs/complete-umbrella-recovery.md"));
    }

    #[test]
    fn includes_ci_latency_evidence_in_runtime_projection() {
        let fixture = fixture();
        let root = repository_root(fixture.path());

        let paths = runtime_paths(&root).expect("runtime path selection");

        assert!(paths.contains("docs/ci-latency-evidence.md"));
    }

    #[test]
    fn includes_stall_recovery_contract_in_runtime_projection() {
        let fixture = fixture();
        let root = repository_root(fixture.path());

        let paths = runtime_paths(&root).expect("runtime path selection");

        assert!(paths.contains("python/stall-recovery-report-allowlists.tsv"));
    }

    #[test]
    fn detects_missing_unexpected_and_changed_projection_files() {
        let fixture = fixture();
        let root = repository_root(fixture.path());
        let paths = runtime_paths(&root).expect("runtime path selection");
        sync(&root, &paths).expect("projection generation");
        fs::remove_file(fixture.path().join("plugin/LICENSE")).expect("remove projection file");
        fs::write(
            fixture.path().join("plugin/agents/reviewer.md"),
            "changed\n",
        )
        .expect("change projection file");
        fs::write(fixture.path().join("plugin/unexpected.txt"), "unexpected\n")
            .expect("write unexpected projection file");

        let errors = projection_errors(&root, &paths).expect("projection validation");

        assert!(errors.contains(&"missing runtime projection: LICENSE".to_owned()));
        assert!(errors.contains(&"unexpected runtime projection: unexpected.txt".to_owned()));
        assert!(errors.contains(
            &"runtime projection differs from its source: agents/reviewer.md".to_owned()
        ));
    }

    #[test]
    fn rejects_tracked_or_untracked_projection_worktree_changes() {
        let fixture = fixture();
        let root = repository_root(fixture.path());

        assert_eq!(verify_projection_worktree(&root), Ok(()));
        fs::write(fixture.path().join("plugin/ignored.txt"), "changed\n")
            .expect("tracked projection file");

        assert_eq!(
            verify_projection_worktree(&root),
            Err("runtime projection changed tracked or untracked files".to_owned())
        );
        fs::write(fixture.path().join("plugin/ignored.txt"), "ignored\n")
            .expect("restore tracked projection file");
        fs::write(fixture.path().join("plugin/untracked.txt"), "unexpected\n")
            .expect("untracked projection file");

        assert_eq!(
            verify_projection_worktree(&root),
            Err("runtime projection changed tracked or untracked files".to_owned())
        );
    }

    #[test]
    fn rejects_a_malformed_manifest_before_generation() {
        let fixture = fixture();
        let root = repository_root(fixture.path());
        fs::write(
            fixture.path().join(".claude-plugin/plugin.json"),
            "not json\n",
        )
        .expect("malform manifest");

        assert_eq!(validate_root(&root), Err(ROOT_ERROR.to_owned()));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_projection_inputs_and_destinations() {
        use std::os::unix::fs::symlink;

        let fixture = fixture();
        let root = repository_root(fixture.path());
        let outside = fixture.path().join("outside");
        fs::write(&outside, "outside\n").expect("write outside target");
        let source = fixture.path().join("LICENSE");
        fs::remove_file(&source).expect("remove source file");
        symlink(&outside, &source).expect("symlink source");
        assert_eq!(
            runtime_paths(&root),
            Err("plugin runtime projection inputs are unsafe: LICENSE".to_owned())
        );

        fs::remove_file(&source).expect("remove source symlink");
        fs::write(&source, "LICENSE\n").expect("restore source file");
        let paths = runtime_paths(&root).expect("runtime path selection");
        fs::remove_dir_all(fixture.path().join("plugin")).expect("remove fixture plugin directory");
        symlink(&outside, fixture.path().join("plugin")).expect("symlink projection");
        assert_eq!(
            projection_errors(&root, &paths),
            Ok(vec![
                "runtime projection root must be a real directory".to_owned()
            ])
        );
        assert_eq!(
            sync(&root, &paths),
            Err("refusing to replace an unsafe plugin projection path".to_owned())
        );
    }

    #[test]
    fn rejects_missing_required_tracked_inputs() {
        let fixture = fixture();
        run_git(fixture.path(), ["rm", "--cached", "LICENSE"]);
        let root = repository_root(fixture.path());

        assert_eq!(
            runtime_paths(&root),
            Err("plugin runtime projection inputs are missing: LICENSE".to_owned())
        );
    }

    #[test]
    fn rejects_missing_required_security_entry_points() {
        let fixture = fixture();
        run_git(
            fixture.path(),
            ["rm", "--cached", "SECURITY.md", "docs/security/README.md"],
        );
        let root = repository_root(fixture.path());

        assert_eq!(
            runtime_paths(&root),
            Err(
                "plugin runtime projection inputs are missing: SECURITY.md, docs/security/README.md"
                    .to_owned()
            )
        );
    }

    #[test]
    fn rejects_missing_required_security_supporting_documents() {
        let fixture = fixture();
        run_git(
            fixture.path(),
            [
                "rm",
                "--cached",
                "docs/ci-latency-evidence.md",
                "docs/dev-hook-audit.md",
                "docs/progress-reporting.md",
                "docs/rust-async-runtime.md",
                "docs/rust-parity-harness.md",
                "docs/rust-ci-selection-observation.md",
                "docs/rust-testing.md",
            ],
        );
        let root = repository_root(fixture.path());

        assert_eq!(
            runtime_paths(&root),
            Err(
                "plugin runtime projection inputs are missing: docs/ci-latency-evidence.md, docs/dev-hook-audit.md, docs/progress-reporting.md, docs/rust-async-runtime.md, docs/rust-parity-harness.md, docs/rust-ci-selection-observation.md, docs/rust-testing.md"
                    .to_owned()
            )
        );
    }

    #[test]
    fn rejects_missing_focused_security_references_from_shipped_skills() {
        let fixture = fixture();
        fs::write(
            fixture.path().join("skills/implement/SKILL.md"),
            "Read `${CLAUDE_PLUGIN_ROOT}/docs/security/missing.md` first.\n",
        )
        .expect("write missing security reference");
        let root = repository_root(fixture.path());

        assert_eq!(
            runtime_paths(&root),
            Err(
                "shipped skill security references are missing: skills/implement/SKILL.md -> docs/security/missing.md"
                    .to_owned()
            )
        );
    }

    #[test]
    fn extracts_focused_security_references_with_links_and_anchors() {
        assert_eq!(
            focused_security_references(
                "Read [the policy](../../docs/security/workflow.md#agents), docs/security/artifacts.md, and docs/security/case.MD."
            ),
            BTreeSet::from([
                "docs/security/artifacts.md".to_owned(),
                "docs/security/case.MD".to_owned(),
                "docs/security/workflow.md".to_owned(),
            ])
        );
    }

    #[test]
    fn preserves_invalid_suffixes_for_missing_reference_validation() {
        assert_eq!(
            focused_security_references("Read docs/security/workflow.md.backup."),
            BTreeSet::from(["docs/security/workflow.md.backup".to_owned()])
        );
    }

    fn fixture() -> TempDir {
        let fixture = tempfile::tempdir().expect("fixture directory");
        for path in DIRECT_FILES {
            let contents = if *path == ".claude-plugin/plugin.json" {
                "{\"name\":\"larch\"}\n"
            } else {
                "runtime\n"
            };
            write(fixture.path(), path, contents);
        }
        for (path, contents) in [
            ("agents/reviewer.md", "agent\n"),
            ("hooks/hooks.json", "{}\n"),
            ("skills/implement/SKILL.md", "skill\n"),
            ("skills/implement/test-helper.md", "test\n"),
            ("docs/security/workflow.md", "focused security\n"),
            ("python/larch/core/runtime.py", "runtime\n"),
            ("python/larch/release/runtime.py", "release\n"),
            ("python/larch/lint/runtime.py", "lint\n"),
            ("plugin/ignored.txt", "ignored\n"),
        ] {
            write(fixture.path(), path, contents);
        }
        for path in DEV_ONLY_PYTHON {
            write(fixture.path(), path, "development-only\n");
        }
        run_git(fixture.path(), ["init", "--quiet"]);
        run_git(
            fixture.path(),
            ["config", "user.email", "test@example.invalid"],
        );
        run_git(fixture.path(), ["config", "user.name", "Larch test"]);
        run_git(fixture.path(), ["add", "--all"]);
        run_git(fixture.path(), ["commit", "--quiet", "-m", "fixture"]);
        fixture
    }

    fn repository_root(path: &Path) -> RepositoryRoot {
        RepositoryRoot::resolve(Some(path)).expect("fixture repository root")
    }

    fn write(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture parent");
        fs::write(path, contents).expect("write fixture file");
    }

    fn run_git<const N: usize>(root: &Path, arguments: [&str; N]) {
        let status = Command::new("git") // lint-subprocess-via-runner: ok test-only Git fixture creates the indexed projection inputs
            .arg("-C")
            .arg(root)
            .args(arguments)
            .output()
            .expect("run fixture git");
        assert!(
            status.status.success(),
            "fixture git command failed: {}",
            status.status
        );
    }
}
