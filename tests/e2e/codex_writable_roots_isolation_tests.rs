//! Fixture ownership is independent of the roots a launch grants.
//! §FS-rhei-agents.2.2.1 §REQ-test-isolation.4

use std::fs;
use std::process::{Command, Stdio};

use super::codex_writable_roots_support::*;
use super::*;

/// Port the enclosing-TMPDIR reproducer's HEAD signal, selecting existing
/// scenarios exactly so the child cannot select this regression recursively.
/// §FS-rhei-agents.2.2.1
#[test]
fn writable_roots_scenarios_leave_an_enclosing_tmpdir_repository_unchanged() {
    let dir = unique_temp_dir("codex-roots-isolation");
    let Some(outer) = git_repository(&dir.join("outer")) else { return };
    let tmp = outer.join("tmp");
    fs::create_dir_all(&tmp).unwrap();
    let before = git_ok(&outer, &["rev-parse", "HEAD"]);
    let scenarios = [
        "a_codex_workspace_launch_writes_its_plan_state_and_commit_through_its_roots",
        "a_codex_workspace_launch_from_a_panta_member_grants_the_project_too",
        "a_wrapper_that_writes_its_own_modes_still_inherits_its_roots",
        "changeset_review_composed_profiles_write_through_their_roots",
        "a_codex_workspace_launch_commits_in_a_linked_worktree",
        "a_codex_launch_with_no_mode_still_runs_yolo",
        "the_fake_codex_refuses_a_commit_without_the_git_dir_grant",
    ];
    let mut failures = Vec::new();
    let mut observations = Vec::new();
    for scenario in scenarios {
        let name = format!("codex_writable_roots_tests::{scenario}");
        let mut child = Command::new(std::env::current_exe().expect("e2e executable"));
        // Remove inherited discovery overrides and run identity, then set only
        // this child's temp directory. §REQ-test-isolation.3 §REQ-test-isolation.5
        super::git_env::scrub_git_repository_env(&mut child);
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("RHEI_") {
                child.env_remove(key);
            }
        }
        let prior = git_ok(&outer, &["rev-parse", "HEAD"]);
        let output = child
            .args([&name, "--exact", "--nocapture"])
            .current_dir(&dir)
            .env("TMPDIR", &tmp)
            .env("TMP", &tmp)
            .env("TEMP", &tmp)
            .stdin(Stdio::null())
            .output()
            .expect("start the existing e2e scenario");
        let stdout = stdout(&output);
        let stderr = stderr(&output);
        assert!(
            stdout.contains("running 1 test"),
            "selection/build failure for {name}, not isolation evidence:\n{stdout}\n{stderr}"
        );
        let after = git_ok(&outer, &["rev-parse", "HEAD"]);
        observations.push(format!(
            "{name}: status={}, outer HEAD {} -> {}",
            output.status,
            prior.trim(),
            after.trim()
        ));
        if !output.status.success() {
            failures.push(format!("{name}:\n{stdout}\n{stderr}"));
        }
    }
    let after = git_ok(&outer, &["rev-parse", "HEAD"]);
    let commits =
        git_ok(&outer, &["log", "--format=%h %s", &format!("{}..{}", before.trim(), after.trim())]);
    let evidence = format!(
        "{}\ncommits added to enclosing repository:\n{commits}\nchild failures:\n{}",
        observations.join("\n"),
        failures.join("\n")
    );
    assert_eq!(after, before, "fixture commits changed enclosing TMPDIR repository\n{evidence}");
    assert!(failures.is_empty(), "scenarios must also succeed under enclosing TMPDIR\n{evidence}");
}

/// Sufficient sandbox grants cannot authorize committing outside the shim's
/// own fixture, even when only a linked checkout is inside that fixture.
/// §FS-rhei-agents.2.2.1
#[test]
fn the_fake_codex_refuses_outside_repositories_despite_writable_grants() {
    let dir = unique_temp_dir("codex-roots-ownership");
    let fixture = dir.join("fixture");
    fs::create_dir_all(&fixture).unwrap();
    let Some(outside) = git_repository(&dir.join("outside")) else { return };
    let linked = fixture.join("linked");
    git_ok(
        &outside,
        &["worktree", "add", "-q", "-b", "outside-linked", &linked.display().to_string()],
    );
    // Keep the checkout and per-worktree admin directory inside the fixture,
    // with only the common directory outside, to exercise that boundary too.
    let split = fixture.join("split");
    git_ok(
        &outside,
        &["worktree", "add", "-q", "-b", "outside-common", &split.display().to_string()],
    );
    let admin = fixture.join("split-admin");
    fs::rename(outside.join(".git/worktrees/split"), &admin).unwrap();
    fs::write(split.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
    fs::write(admin.join("commondir"), format!("{}\n", outside.join(".git").display())).unwrap();
    let state = fixture.join("state");
    fs::create_dir_all(&state).unwrap();
    let log = fixture.join("shim.jsonl");
    let shim = write_roots_shim(&fixture, &log, &state, &[]);
    let mut violations = Vec::new();
    for checkout in [&outside, &linked, &split] {
        let before = git_ok(checkout, &["rev-parse", "HEAD"]);
        let metadata = git_ok(
            checkout,
            &["rev-parse", "--path-format=absolute", "--git-dir", "--git-common-dir"],
        );
        let mut command = Command::new(python_command());
        super::git_env::scrub_git_repository_env(&mut command);
        command.arg(&shim).arg("--add-dir").arg(checkout).arg("--add-dir").arg(&state);
        for path in metadata.lines() {
            command.arg("--add-dir").arg(path);
        }
        let output = command
            .args(["--sandbox", "workspace-write", "--"])
            .current_dir(checkout)
            .env_remove("RHEI_ATTEMPT")
            .stdin(Stdio::null())
            .output()
            .expect("launch the misdirected shim");
        let records = shim_records(&log);
        let record = records.last().expect("the shim must log its refusal");
        assert!(
            same_dir(std::path::Path::new(record["cwd"].as_str().unwrap()), checkout),
            "the shim did not run in the requested checkout: {record:#}"
        );
        let after = git_ok(checkout, &["rev-parse", "HEAD"]);
        if after != before
            || record["committed"] != false
            || record["refused"].as_array().is_none_or(|paths| paths.is_empty())
        {
            violations.push(format!(
                "{}: HEAD {} -> {}; record={record:#}; status={}\n{}",
                checkout.display(),
                before.trim(),
                after.trim(),
                output.status,
                stderr(&output)
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "fake Codex must refuse outside repository commits despite writable grants:\n{}",
        violations.join("\n")
    );
}
