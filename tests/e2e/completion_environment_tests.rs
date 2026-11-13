//! Inherited completion requests must explain their environment cause.
//! §FS-rhei-completions.5.1

use std::fs;
use std::path::Path;
use std::process::Output;

use super::roster_support::tree_snapshot;
use super::{raw_stderr, rhei_command, stderr, unique_temp_dir, write_fixture_file, TestDir};

fn fixture() -> TestDir {
    let dir = unique_temp_dir("completion-environment");
    write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Build\n\n## Tasks\n\n### Task 1: Build\n**State:** pending\n\n\
         ### Task 2: Build docs\n**State:** pending\n",
    );
    fs::create_dir_all(dir.join("home/state")).expect("create isolated home");
    dir
}

fn run(dir: &Path, complete: Option<&str>, args: &[&str]) -> Output {
    let home = dir.join("home");
    let mut cmd = rhei_command(&home);
    cmd.env_clear()
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("XDG_STATE_HOME", home.join("state"))
        .env("NO_COLOR", "1")
        .current_dir(dir)
        .args(args);
    for name in ["PATH", "SystemRoot"] {
        if let Some(value) = std::env::var_os(name) {
            cmd.env(name, value);
        }
    }
    if let Some(value) = complete {
        cmd.env("COMPLETE", value);
    }
    cmd.output().expect("rhei subprocess should run")
}

#[track_caller]
fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command should succeed: {output:?}\nstderr: {}",
        stderr(output)
    );
}

#[track_caller]
fn assert_environment_refusal(dir: &Path, value: &str, args: &[&str]) {
    let before = tree_snapshot(dir);
    let output = run(dir, Some(value), args);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty(), "refusal must leave stdout empty: {output:?}");
    assert_eq!(tree_snapshot(dir), before, "refused command must not mutate any fixture file");

    let rendered = raw_stderr(&output);
    let stderr = stderr(&output);
    let lower = stderr.to_lowercase();
    let shows_value = stderr.match_indices(value).any(|(offset, _)| {
        !stderr[..offset].ends_with(char::is_alphanumeric)
            && !stderr[offset + value.len()..].starts_with(char::is_alphanumeric)
    });
    let checks = [
        (stderr.contains("COMPLETE"), "names COMPLETE"),
        (shows_value, "shows the full supplied value"),
        (lower.contains("completion"), "explains the shell completion trigger"),
        (
            stderr.contains("unset COMPLETE") || stderr.contains("env -u COMPLETE"),
            "teaches how to unset COMPLETE",
        ),
        (lower.contains("retry") || lower.contains("rerun"), "instructs retrying the command"),
        (rendered.ends_with('\n'), "terminates with a newline"),
    ];
    let missing: Vec<_> = checks.iter().filter(|(ok, _)| !ok).map(|(_, claim)| *claim).collect();
    assert!(
        missing.is_empty(),
        "COMPLETE={value:?}, args={args:?}: missing diagnostic requirements: {missing:?}; \
         stderr={stderr:?}"
    );
}

#[test]
#[ignore = "red until #708: list identifies the COMPLETE environment cause; implement removes this"]
fn invalid_complete_environment_list_diagnostic() {
    let dir = fixture();
    let control = run(&dir, None, &["list", "plan.rhei.md"]);
    assert_success(&control);
    assert!(String::from_utf8_lossy(&control.stdout).contains("plan.1"));
    assert_environment_refusal(&dir, "x", &["list", "plan.rhei.md"]);
}

#[test]
#[ignore = "red until #708: complete reports the full COMPLETE value and recovery; implement removes this"]
fn invalid_complete_environment_complete_diagnostic() {
    let dir = fixture();
    assert_environment_refusal(&dir, "plan.1 plan.2", &["complete", "plan.1", "--result", "done"]);
}

#[test]
#[ignore = "red until #708: reset reports the full COMPLETE value and recovery; implement removes this"]
fn invalid_complete_environment_reset_diagnostic() {
    let dir = fixture();
    assert_success(&run(&dir, None, &["complete", "plan.1", "--result", "done"]));
    assert!(dir.join("runtime/results/plan.1.md").is_file(), "reset fixture has a result to lose");
    assert_environment_refusal(&dir, "plan.1 plan.2", &["reset", "plan.rhei.md", "--yes"]);
}

/// The same controls as the upstream reproducer, including both off values.
/// §FS-rhei-templates.1.3
#[test]
fn complete_environment_off_values_preserve_ordinary_commands() {
    for value in [None, Some(""), Some("0")] {
        let dir = fixture();
        let listed = run(&dir, value, &["list", "plan.rhei.md"]);
        assert_success(&listed);
        let stdout = String::from_utf8_lossy(&listed.stdout);
        assert!(stdout.contains("plan.1") && stdout.contains("plan.2"), "{stdout}");

        assert_success(&run(&dir, value, &["complete", "plan.1", "--result", "done"]));
        let result = dir.join("runtime/results/plan.1.md");
        assert!(fs::read_to_string(&result).expect("completion result").contains("done"));
        let completed = fs::read_to_string(dir.join("plan.rhei.md")).expect("completed plan");
        assert!(completed.contains("**State:** completed"));

        assert_success(&run(&dir, value, &["reset", "plan.rhei.md", "--yes"]));
        assert!(!result.exists(), "ordinary reset removes runtime output");
        let reset = fs::read_to_string(dir.join("plan.rhei.md")).expect("reset plan");
        assert_eq!(reset.matches("**State:** pending").count(), 2);
    }
}

/// Accepted path and extension forms use the library's shell interpretation.
/// §FS-rhei-completions.5.1
#[test]
fn complete_environment_preserves_registration_and_shell_path_callbacks() {
    let dir = fixture();
    let before = fs::read(dir.join("plan.rhei.md")).expect("read plan");
    let registration = run(&dir, Some("bash"), &["list", "plan.rhei.md"]);
    assert_success(&registration);
    let script = String::from_utf8_lossy(&registration.stdout);
    assert!(
        script.contains("_clap_complete_rhei") && script.contains("COMPLETE=\"bash\""),
        "{script}"
    );
    assert!(!script.contains("Task plan.1"), "COMPLETE=bash still requests completion");
    assert!(registration.stderr.is_empty());

    for shell in ["fish", "/usr/bin/fish", "/usr/bin/fish.exe"] {
        let candidates = run(&dir, Some(shell), &["--", "rhei", "complete", "--task", "plan."]);
        assert_success(&candidates);
        let stdout = String::from_utf8_lossy(&candidates.stdout);
        assert!(stdout.contains("plan.1") && stdout.contains("plan.2"), "{shell}: {stdout}");
        assert!(candidates.stderr.is_empty(), "{candidates:?}");
    }
    assert_eq!(fs::read(dir.join("plan.rhei.md")).expect("read plan"), before);
    assert!(!dir.join("runtime").exists(), "completion must not execute the ordinary command");
}
