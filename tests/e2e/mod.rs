mod accounting_attempt_identity_roots_tests;
mod accounting_attempt_identity_tests;
mod accounting_contract_tests;
mod accounting_convention_tests;
mod accounting_duration_tests;
mod accounting_extraction_diagnostic_tests;
mod accounting_line_tests;
mod accounting_presentation_tests;
mod accounting_prices_tests;
mod accounting_support;
mod agent_environment_tests;
mod agent_grounds_settings_tests;
mod agent_grounds_support;
mod agent_grounds_templates_tests;
mod agent_prompt_path_tests;
mod agent_prompt_size_tests;
mod complete_result_input_tests;
mod completions_tests;
mod consumes_advisory_tests;
mod cost_accounting_tests;
mod cost_project_scope_support;
mod cost_project_scope_tests;
mod cost_selection_tests;
mod current_dir_target_tests;
mod dead_end_state_tests;
mod declared_exit_route_tests;
mod diagnostic_wrap_tests;
mod effort_invocation_tests;
mod effort_state_tests;
mod effort_support;
mod error_guidance_tests;
mod examples_tests;
mod exclusions_context_tests;
mod exclusions_regressions_tests;
mod exclusions_required_tests;
mod exclusions_tests;
mod exclusions_validation_tests;
mod handoff_tests;
mod headless_dead_supervisor_tests;
mod headless_recovery_tests;
mod headless_stop_ownership_tests;
mod headless_support;
mod headless_tests;
mod headless_undecided_tests;
mod install_skills_tests;
mod installed_state_machine_guidance_tests;
mod instantiate_output_default_tests;
mod laid_output_root_tests;
mod list_ready_tests;
mod live_member_admission_tests;
mod live_member_lock_tests;
mod live_member_program_tests;
mod loop_budget_refusal_tests;
mod member_publication_tests;
mod memory_map_tests;
mod memory_prompt_tests;
mod memory_supervisor_prompt_tests;
mod new_directory_adoption_tests;
mod new_guard_tests;
mod new_tests;
mod new_write_tests;
mod next_claim_atomicity_tests;
mod next_claim_boundary_tests;
mod next_node_kinds_tests;
mod next_tests;
mod parallel_target_override_tests;
mod poll_resume_program_tests;
mod poll_wait_outcome_tests;
mod provider_limit_cleanup_tests;
mod provider_limit_compatibility_tests;
mod provider_limit_parking_tests;
mod provider_limit_poll_tests;
mod provider_limit_scheduling_tests;
mod provider_limit_support;
mod registry_location_tests;
mod rendered_stderr;
mod roster_behavior_tests;
mod roster_json_tests;
mod roster_scope_tests;
mod roster_support;
mod run_lock_wait_tests;
mod run_shell_program_tests;
mod run_signals_tests;
mod run_tests;
mod runs_history_tests;
mod self_advancing_agent_tests;
mod snapshot_codex_session_tests;
mod snapshot_continue_root_tests;
mod snapshot_tests;
mod state_machine_placement_tests;
mod summary_tests;
mod supervised_delivery_tests;
mod supervision_appended_child_tests;
mod supervision_barrier_tests;
mod supervision_empty_visit_tests;
mod supervision_gated_finish_tests;
mod supervision_next_tests;
mod supervision_no_spawn_release_tests;
mod supervision_release_rule_tests;
mod supervision_surfaces_tests;
mod supervision_tests;
mod supervisor_validation_scope_tests;
mod supervisor_validation_tests;
mod template_ancestor_discovery_tests;
mod template_example_sync_tests;
mod template_transition_guard_tests;
mod templates_render_tests;
mod templates_tests;
mod terminal_result_attempt_tests;
mod terminal_result_fanout_tests;
mod terminal_result_redirect_tests;
mod terminal_result_stall_tests;
mod terminal_result_tests;
mod transition_tests;
mod unrecognized_input_directory_tests;
mod validate_retry_cache_tests;
mod validation_source_tests;
mod waiting_on_person_tests;
mod worker_capture_precondition_tests;

/// The plan and machine text most of them start from.
mod shared_fixtures;
pub use shared_fixtures::*;

// Shared with the `integration_markdown_plans` harness, which cannot see this
// module tree and `include!`s the same file.
#[path = "../support/binaries.rs"]
mod binaries;
#[path = "../support/python_fixture.rs"]
mod python_fixture;
#[path = "../support/test_dir.rs"]
mod test_dir;

pub use python_fixture::{
    fixture_command, fixture_command_line, python_command, write_python_agent,
};

/// A `cli:` callback that runs one line of Python, spelled as a YAML scalar.
///
/// A callback is a command line for the platform's own shell, and the two
/// shells share almost no vocabulary — `printf` is `sh`, not `cmd`. The code
/// goes inside one pair of double quotes and quotes its own strings with
/// `'…'`, which both shells hand through unchanged, and `serde_json` spells
/// the result as a YAML double-quoted scalar.

// §FS-rhei-programs.1.1
pub fn python_callback_yaml(code: &str) -> String {
    serde_json::to_string(&format!("cli:{} -c \"{code}\"", python_command()))
        .expect("callback should serialize")
}
pub use binaries::rhei_binary;
pub use test_dir::TestDir;

/// The product's own quoting, so a test builds an expected command line the way
/// the product built it — POSIX quotes on Unix, `cmd`'s on Windows — instead of
/// dropping the assertion to a substring because the two platforms disagree.
// §FS-rhei-errors.2
pub use rhei_core::platform::shell_quote;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub struct CliRun {
    pub status: std::process::ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

/// A finished process read the way an assertion should read it.
///
/// Every test that spawns its own command builds its `CliRun` here rather than
/// decoding the captured bytes itself, so the reading of stderr stays one
/// decision instead of sixty. §FS-rhei-errors.2
impl From<&Output> for CliRun {
    fn from(out: &Output) -> Self {
        CliRun { status: out.status, stdout: stdout(out), stderr: stderr(out) }
    }
}

pub fn unique_temp_dir(prefix: &str) -> TestDir {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    TestDir::create(std::env::temp_dir().join(format!("rhei-integ-{prefix}-{nanos}")))
}

pub fn unique_scratchpad_dir(prefix: &str) -> TestDir {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    TestDir::create(repo_root().join("scratchpad").join(format!("rhei-integ-{prefix}-{nanos}")))
}

/// Compare existing paths by filesystem identity rather than spelling. macOS
/// exposes temporary directories through both `/var` and `/private/var`.
pub fn assert_same_path(actual: &Path, expected: &Path) {
    assert_eq!(
        fs::canonicalize(actual).expect("actual path should resolve"),
        fs::canonicalize(expected).expect("expected path should resolve")
    );
}

pub fn write_fixture_file(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).expect("fixture file should be written");
    path
}

/// Set up a single-file test: returns (temp_dir, plan_path, machine_path).
pub fn setup_single_file(prefix: &str, plan: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    (dir, plan_path, machine_path)
}

/// Set up a directory workspace. Returns (temp_dir, workspace_root,
/// machine_path); the workspace lives inside the temp directory, so the first
/// element is what has to stay bound for the tree to outlive the setup call.
pub fn create_workspace(
    prefix: &str,
    index: &str,
    task_files: &[(&str, &str)],
) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let ws = dir.join("workspace");
    let tasks_dir = ws.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(ws.join("index.rhei.md"), index).expect("write index");
    for (name, content) in task_files {
        fs::write(tasks_dir.join(name), content).expect("write task file");
    }
    let machine_path = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    (dir, ws, machine_path)
}

pub fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name)
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate should have workspace parent")
        .parent()
        .expect("workspace root should exist")
        .to_path_buf()
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create fixture directory");

    for entry in fs::read_dir(src).expect("read fixture directory") {
        let entry = entry.expect("fixture entry");
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let file_type = entry.file_type().expect("fixture file type");

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
            continue;
        }

        fs::copy(&src_path, &dst_path).expect("copy fixture file");
        let permissions = fs::metadata(&src_path).expect("fixture metadata").permissions();
        fs::set_permissions(&dst_path, permissions).expect("fixture permissions");
    }
}

pub fn copy_workspace_fixture(prefix: &str, fixture_name: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_scratchpad_dir(prefix);
    let workspace_path = dir.join(fixture_name);
    copy_dir_recursive(&fixture_path(fixture_name), &workspace_path);
    let machine_path = workspace_path.join("team-states.yaml");
    // A fixture's `cli:` callbacks name their interpreter as
    // `RHEI_FIXTURE_PYTHON`, because which of `python3` and `python` exists is
    // a fact about the host and not about the fixture. The committed machine
    // is the template; the copy is what runs.
    let machine = fs::read_to_string(&machine_path).expect("fixture state machine");
    fs::write(&machine_path, machine.replace("RHEI_FIXTURE_PYTHON", python_command()))
        .expect("fixture state machine with the host's interpreter");
    (dir, workspace_path, machine_path)
}

/// Every `rhei` an end-to-end test spawns, with **both** state locations pinned
/// under `home`.
///
/// The run registry is machine-wide and every non-dry run publishes into it, so
/// a `HOME` alone leaves a test writing entries into the developer's own
/// `$XDG_STATE_HOME/rhei/runs` wherever their environment sets that variable.

// §FS-rhei-run-headless.2
pub fn rhei_command(home: impl AsRef<Path>) -> Command {
    let home = home.as_ref();
    let _ = fs::create_dir_all(home.join("state"));
    let mut cmd = rhei_process();
    cmd.env("HOME", home);
    cmd.env("XDG_STATE_HOME", home.join("state"));
    cmd
}

/// Every spawn of the built binary, with what decides how a diagnostic is
/// *rendered* taken out of the operator's hands.
///
/// miette asks `supports-color` before it asks whether stderr is a tty, and
/// `FORCE_COLOR` or `CLICOLOR_FORCE` answers yes for a pipe as well. On a
/// machine that sets either, every gutter arrives wrapped in ANSI escapes, no
/// block matches, `undo_soft_wrap` hands the text back untouched, and every
/// assertion this suite unwraps for fails there and nowhere else — the
/// machine-dependent failure the unwrapping exists to remove, wearing a
/// different variable. §FS-rhei-errors.2
pub fn rhei_process() -> Command {
    rhei_process_at(rhei_binary())
}

/// [`rhei_process`] for a test that runs a *copy* of the binary from elsewhere.
pub fn rhei_process_at(bin: impl AsRef<Path>) -> Command {
    let mut cmd = Command::new(bin.as_ref());
    cmd.env_remove("FORCE_COLOR");
    cmd.env_remove("CLICOLOR_FORCE");
    cmd
}

/// Captured stdout, as the process wrote it. Nothing renders stdout, so there
/// is nothing between the bytes and the assertion.
pub fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Captured stderr exactly as the process wrote it — miette's gutter, its soft
/// wrap and all.
///
/// Only a test whose subject is the *rendering* wants this. A test asserting
/// what the binary said reads `stderr`, because where a line happened to break
/// is a property of the terminal width, not of the message. §FS-rhei-errors.2
pub fn raw_stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Captured stderr as an assertion should read it: the sentences the binary
/// printed, with the renderer's soft wrap undone and nothing else changed.
///
/// This is the one seam between captured stderr bytes and a test's `contains`,
/// so that no assertion's outcome turns on which space the wrap fell at — a
/// column that moves with a pid, a temporary path or a run id, and so decides
/// on one machine what it decides differently on the next.
/// §FS-rhei-errors.2
pub fn stderr(out: &Output) -> String {
    rendered_stderr::undo_soft_wrap(&raw_stderr(out))
}

/// Captured stderr a run redirected into a file, read the way an assertion
/// should read it.
///
/// A `rhei run` under test writes its own stderr to a file rather than to a
/// pipe, and miette lays a diagnostic out to the same column there, so the
/// seam has to reach that door too. Empty when the file is not there yet,
/// because the callers poll for the run to write one. §FS-rhei-errors.2
pub fn stderr_from_file(path: impl AsRef<Path>) -> String {
    rendered_stderr::undo_soft_wrap(&fs::read_to_string(path).unwrap_or_default())
}

/// Run an arbitrary rhei subcommand.
pub fn run_cli(
    subcommand: &str,
    plan_path: &Path,
    machine_path: &Path,
    extra_args: &[&str],
) -> CliRun {
    let mut cmd = rhei_command(isolated_home_for(plan_path));
    cmd.arg("--state-machine").arg(machine_path).arg(subcommand).arg(plan_path);
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("rhei command should run");
    CliRun::from(&output)
}

/// Run an arbitrary rhei subcommand without passing `--state-machine`.
pub fn run_cli_without_machine(subcommand: &str, plan_path: &Path, extra_args: &[&str]) -> CliRun {
    let mut cmd = rhei_command(isolated_home_for(plan_path));
    cmd.arg(subcommand).arg(plan_path);
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("rhei command should run");
    CliRun::from(&output)
}

fn isolated_home_for(plan_path: &Path) -> PathBuf {
    plan_path.parent().unwrap_or_else(|| Path::new(".")).join(".home")
}

/// Run `rhei transition`.
pub fn run_transition(
    plan_path: &Path,
    machine_path: &Path,
    task: &str,
    from: &str,
    to: &str,
) -> CliRun {
    run_cli(
        "transition",
        plan_path,
        machine_path,
        &["--task", task, "--from", from, "--to", to, "--no-callbacks"],
    )
}

/// Run `rhei transition --result`, which every move into a `final: true` state
/// needs unless the ticket already has a result on disk. §FS-rhei-states.3.3
pub fn run_transition_with_result(
    plan_path: &Path,
    machine_path: &Path,
    task: &str,
    from: &str,
    to: &str,
    result: &str,
) -> CliRun {
    run_cli(
        "transition",
        plan_path,
        machine_path,
        &["--task", task, "--from", from, "--to", to, "--result", result, "--no-callbacks"],
    )
}

/// Render the plan as JSON via `rhei render --format json --pretty` and return
/// the parsed JSON. All state assertions go through the CLI this way.
pub fn render_json(plan_path: &Path, machine_path: &Path) -> serde_json::Value {
    let result = run_cli("render", plan_path, machine_path, &["--format", "json", "--pretty"]);
    assert_success(&result);
    serde_json::from_str(&result.stdout).expect("render JSON should parse")
}

/// Assert that every task in the plan has the given state, verified via CLI.
pub fn assert_all_tasks_in_state(plan_path: &Path, machine_path: &Path, expected: &str) {
    let json = render_json(plan_path, machine_path);
    let tasks = json["tasks"].as_array().expect("tasks array");
    assert!(!tasks.is_empty(), "plan should have tasks");
    for task in tasks {
        let id = &task["id"];
        let state = task["state"].as_str().expect("state field");
        assert_eq!(state, expected, "Task {} should be '{}', got '{}'", id, expected, state);
    }
}

/// Assert a single task has the expected state, verified via CLI.
/// `task_id` can be a number (e.g. "1") or a name (e.g. "setup").
pub fn assert_task_state(plan_path: &Path, machine_path: &Path, task_id: &str, expected: &str) {
    let json = render_json(plan_path, machine_path);
    let tasks = json["tasks"].as_array().expect("tasks array");
    let task = tasks
        .iter()
        .find(|t| {
            // JSON id now has shape { "path": "...", "segments": [...] }, with the
            // path qualified by the implicit Panta rhei id (e.g. "plan.1"). Match
            // either the exact path or the local id after the rhei qualifier.
            t["id"]["path"].as_str().is_some_and(|path| {
                path == task_id || path.split_once('.').is_some_and(|(_, local)| local == task_id)
            })
        })
        .unwrap_or_else(|| panic!("Task {} not found in rendered JSON", task_id));
    let state = task["state"].as_str().expect("state field");
    assert_eq!(state, expected, "Task {} should be '{}', got '{}'", task_id, expected, state);
}

/// Assert that stderr contains `expected`, exactly as the binary printed it.
///
/// `CliRun.stderr` already has the renderer's wrap undone, so there is nothing
/// left here for the assertion to forgive. It used to collapse both sides to
/// their ASCII-graphic characters, which defeats the wrap by defeating every
/// space — and so also matched a phrase spanning a message and its `help:`
/// block, and healed a token the renderer broke mid-word. Both are phrases
/// nobody printed, and a false pass costs more than the false failure it
/// replaces. §FS-rhei-errors.2
pub fn assert_stderr_contains(result: &CliRun, expected: &str) {
    assert!(
        result.stderr.contains(expected),
        "expected stderr to contain {:?}; got:\n{}",
        expected,
        result.stderr
    );
}

/// `<TIME>` text that is not a time. Each one reaches the parser as raw bytes:
/// a trailing multi-byte character, a bare one, nothing at all, and two that
/// are the right *length* for an instant or a date without being one.
/// §FS-rhei-cost-accounting.8.2
pub const HOSTILE_TIME_TEXTS: &[&str] = &[
    // What pasting `--since 7d ` out of a rendered document actually gives you.
    "7d\u{a0}",
    "7\u{e9}",
    "30\u{65e5}",
    "\u{e9}",
    "",
    // Ten bytes, so the bare-date branch takes it; twenty, so the instant
    // branch does — neither of them on a character boundary its fields expect.
    "2026-09-\u{e9}",
    "202\u{e9}-09-01T00:00:0Z",
];

/// Hold one `<TIME>`-taking flag to the refusal an unreadable value earns: a
/// usage error naming what could not be read, and never a panic.
/// §FS-rhei-cost-accounting.8.2
pub fn assert_refuses_time_text(result: &CliRun, flag: &str, text: &str) {
    assert!(
        !result.status.success(),
        "{flag} {text:?} must be refused, not answered; got:\n{}",
        result.stdout
    );
    assert!(
        !result.stderr.contains("panicked"),
        "{flag} {text:?} panicked instead of refusing it:\n{}",
        result.stderr
    );
    assert_stderr_contains(result, &format!("could not read '{text}' as a time for {flag}"));
}

pub fn assert_success(result: &CliRun) {
    assert!(
        result.status.success(),
        "command should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}
