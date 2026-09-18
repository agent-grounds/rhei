//! Implementation-owned boundary coverage for export-prior migration.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::terminal_result_tests::write_mock_agent_settings;
use super::*;

const MACHINE: &str = r#"name: migration-boundaries
version: 1
states:
  pending:
    initial: true
    description: Work
    agent: mock
    agent_timeout: 10s
  completed:
    final: true
    description: Done
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: pending
    to: completed
  - from: "*"
    to: cancelled
"#;

const AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
write(root / 'runtime' / ('spawned-' + task + '.txt'), agent_prompt())
result('done\n')
"#;

fn setup(prefix: &str, producer_state: &str) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        &format!(
            "# Rhei: boundaries\n**States:** migration-boundaries\n\n## Tasks\n\n\
             ### Task 1: producer\n**State:** {producer_state}\n**Provides:** x\n\n\
             ### Task 2: consumer\n**State:** pending\n**Consumes:** 1:x\n"
        ),
    );
    write_fixture_file(&dir, "states.yaml", MACHINE);
    let agent = write_python_agent(&dir, "consumer.py", AGENT);
    write_mock_agent_settings(&dir, &agent);
    (dir, plan)
}

fn migrate(home: &Path, target: Option<&Path>, dry_run: bool) -> CliRun {
    let mut command = rhei_command(home);
    command.arg("migrate").arg("export-priors");
    if dry_run {
        command.arg("--dry-run");
    }
    if let Some(target) = target {
        command.arg(target);
    }
    CliRun::from(&command.output().expect("migration command"))
}

/// Omitted-target discovery reaches the same complete operation, and shell
/// completion callbacks offer both command levels. §FS-rhei-migrate.1 §FS-rhei-migrate.6
#[test]
fn omitted_target_and_shell_completion_discover_export_prior_migration() {
    let (dir, plan) = setup("export-prior-discovery", "completed");
    let before = fs::read(&plan).unwrap();
    let mut preview_command = rhei_command(dir.join(".home"));
    preview_command.current_dir(&dir).arg("migrate").arg("export-priors").arg("--dry-run");
    let preview = CliRun::from(&preview_command.output().expect("discovered preview"));
    assert_success(&preview);
    assert!(preview.stdout.contains("Would add Task 1 to Task plan.2"));
    assert_eq!(fs::read(&plan).unwrap(), before);

    for (words, candidate) in
        [(&["rhei", "mig"][..], "migrate"), (&["rhei", "migrate", "ex"][..], "export-priors")]
    {
        let output = rhei_command(dir.join(".home"))
            .arg("--")
            .args(words)
            .current_dir(&dir)
            .env("COMPLETE", "fish")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_DATA_HOME")
            .output()
            .expect("dynamic fish completion callback");
        let completions = CliRun::from(&output);
        assert_success(&completions);
        assert!(
            completions.stdout.lines().any(|line| line.split('\t').next() == Some(candidate)),
            "missing {candidate} completion for {words:?}: {}",
            completions.stdout
        );
    }
}

/// The authored edge does not bypass ordinary readiness: nonterminal and
/// cancelled producers still leave the consumer blocked. §FS-rhei-migrate.2.1 §FS-rhei-migrate.5
#[test]
fn migrated_consumer_remains_blocked_by_nonterminal_or_cancelled_producer() {
    for (suffix, state) in [("pending", "pending"), ("cancelled", "cancelled")] {
        let (dir, plan) = setup(&format!("export-prior-{suffix}"), state);
        assert_success(&migrate(&dir.join(".home"), Some(&plan), false));
        let listed = run_cli_without_machine("list", &plan, &["--ready", "--json"]);
        assert_success(&listed);
        let tasks: serde_json::Value = serde_json::from_str(&listed.stdout).expect("list JSON");
        assert!(
            tasks.as_array().unwrap().iter().all(|task| task["id"] != "plan.2"),
            "consumer became ready behind {state}: {}",
            listed.stdout
        );
    }
}

/// Migration repairs authored ordering but neither synthesizes an export nor
/// weakens the pre-spawn nonblank check. §FS-rhei-migrate.1.2 §FS-rhei-migrate.5
#[test]
fn migrated_consumer_still_refuses_missing_or_blank_export_content() {
    for (suffix, body) in [("missing", None), ("blank", Some(" \n\t"))] {
        let (dir, plan) = setup(&format!("export-prior-{suffix}-content"), "completed");
        if let Some(body) = body {
            let export = dir.join("runtime/exports/plan.1/x.md");
            fs::create_dir_all(export.parent().unwrap()).unwrap();
            fs::write(export, body).unwrap();
        }
        assert_success(&migrate(&dir.join(".home"), Some(&plan), false));
        let run = run_cli_without_machine("run", &plan, &["--no-tui", "--no-callbacks"]);
        assert!(!run.status.success(), "{suffix} export unexpectedly ran");
        let output = format!("{}{}", run.stdout, run.stderr);
        assert!(output.contains("missing or blank consumed exports"), "{output}");
        assert!(!dir.join("runtime/spawned-plan.2.txt").exists());
    }
}

const DIAGNOSTIC_MACHINE: &str = r#"name: migration-diagnostics
version: 1
states:
  pending:
    initial: true
    description: Work
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: completed
"#;

const DIAGNOSTIC_PLAN: &str = r#"# Rhei: Migration diagnostics
**States:** migration-diagnostics

## Tasks

### Task 1: Producer
**State:** completed
**Provides:** handoff

### Task 2: Consumer
**State:** pending
**Consumes:** 1:handoff
"#;

struct ChildGuard(Option<Child>);

impl ChildGuard {
    fn child(&mut self) -> &mut Child {
        self.0.as_mut().expect("guarded child")
    }

    fn stop(mut self) {
        if let Some(mut child) = self.0.take() {
            child.kill().expect("stop watch");
            child.wait().expect("reap watch");
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn diagnostic_case(prefix: &str) -> (TestDir, PathBuf) {
    let root = unique_temp_dir(prefix);
    let directory = root.join(
        "a deliberately long migration diagnostic path with spaces and an apostrophe's target",
    );
    fs::create_dir_all(&directory).expect("diagnostic fixture directory");
    let plan = write_fixture_file(&directory, "plan.rhei.md", DIAGNOSTIC_PLAN);
    write_fixture_file(&directory, "states.yaml", DIAGNOSTIC_MACHINE);
    (root, plan)
}

fn expected_help(target: &Path) -> String {
    format!("help: rhei migrate export-priors {}", shell_quote(&target.display().to_string()))
}

/// The target an omitted plan discovers is whatever the OS reports as the
/// current directory, which on macOS resolves `/var`'s symlink to
/// `/private/var`; canonicalize the same way so the expected line matches the
/// rendered one instead of the fixture's pre-resolution spelling.
fn discovered_target(plan: &Path) -> PathBuf {
    rhei_core::platform::canonical_path(plan).unwrap_or_else(|_| plan.to_path_buf())
}

fn assert_complete_help_line(rendered: &str, target: &Path) {
    let expected = expected_help(target);
    assert!(
        rendered.lines().any(|line| {
            line.find("help: rhei migrate export-priors ")
                .is_some_and(|start| line[start..] == expected)
        }),
        "expected one complete physical help line {expected:?}; got:\n{rendered}"
    );
    assert_eq!(
        rendered.matches("rhei migrate export-priors").count(),
        1,
        "the recovery command should be rendered exactly once:\n{rendered}"
    );
}

fn assert_authored_unchanged(plan: &Path, before: &[u8]) {
    assert_eq!(fs::read(plan).expect("plan after refusal"), before);
}

/// Explicit validate and run-preview targets retain one shell-safe physical
/// recovery line and remain read-only. §FS-rhei-migrate.5 §FS-rhei-errors.1.2
#[test]
fn explicit_validate_and_run_dry_run_render_copyable_migration_help() {
    let (_root, plan) = diagnostic_case("migration-diagnostic-explicit");
    let before = fs::read(&plan).expect("plan before refusal");

    for (command, extras) in [("validate", &[][..]), ("run", &["--dry-run", "--no-tui"][..])] {
        let output = rhei_command(isolated_home_for(&plan))
            .arg(command)
            .arg(&plan)
            .args(extras)
            .output()
            .expect("diagnostic command");
        assert!(!output.status.success(), "{command} unexpectedly succeeded");
        assert_complete_help_line(&raw_stderr(&output), &plan);
        assert_authored_unchanged(&plan, &before);
    }
}

/// Omitted discovery feeds the complete discovered target through watch's
/// initial pass without mutation. §FS-rhei-migrate.5 §FS-rhei-errors.1.2
#[test]
fn omitted_validate_watch_renders_copyable_migration_help() {
    let (_root, plan) = diagnostic_case("migration-diagnostic-watch");
    let before = fs::read(&plan).expect("plan before watch");
    let directory = plan.parent().expect("plan directory");
    let stderr_path = directory.join("watch-stderr.txt");
    let stderr_file = fs::File::create(&stderr_path).expect("watch stderr");
    let child = rhei_command(isolated_home_for(&plan))
        .current_dir(directory)
        .args(["validate", "--watch"])
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .expect("watch command");
    let mut child = ChildGuard(Some(child));

    let deadline = Instant::now() + Duration::from_secs(10);
    let rendered = loop {
        let rendered = raw_stderr_from_file(&stderr_path);
        if rendered.contains("rhei migrate export-priors") {
            break rendered;
        }
        assert!(Instant::now() < deadline, "watch did not render migration help:\n{rendered}");
        if let Some(status) = child.child().try_wait().expect("inspect watch") {
            panic!("watch exited early with {status}:\n{rendered}");
        }
        thread::sleep(Duration::from_millis(25));
    };
    child.stop();

    assert_complete_help_line(&rendered, &discovered_target(&plan));
    assert_authored_unchanged(&plan, &before);
}

/// The detached startup path preserves the omitted discovered target when it
/// relays the child's refusal. §FS-rhei-migrate.5 §FS-rhei-errors.1.2
#[cfg(unix)]
#[test]
fn omitted_headless_startup_renders_copyable_migration_help() {
    let (_root, plan) = diagnostic_case("migration-diagnostic-headless");
    let before = fs::read(&plan).expect("plan before headless run");
    let output = rhei_command(isolated_home_for(&plan))
        .current_dir(plan.parent().expect("plan directory"))
        .args(["run", "--headless"])
        .output()
        .expect("headless command");

    assert!(!output.status.success(), "invalid headless run unexpectedly started");
    assert_complete_help_line(&raw_stderr(&output), &discovered_target(&plan));
    assert_authored_unchanged(&plan, &before);
}
