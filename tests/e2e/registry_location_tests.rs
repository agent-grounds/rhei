//! A registry refusal says where to declare the name it refused, not only which
//! names already exist. §FS-rhei-errors.1.4
//!
//! The reported case is `rhei validate` on a state machine whose target selector
//! carries a mode the agent does not declare. The refusal listed `known modes:
//! yolo` and left the author to find the settings tree by grepping for the mode
//! (agent-grounds/rhei#197).

use std::fs;
use std::path::{Path, PathBuf};

use super::agent_grounds_support::{DEPRECATED, GROUNDS};
use super::*;

/// The ticket's own plan: one task in the state whose target is wrong.
const PLAN: &str = r#"# Rhei: Mode location fixture

## Tasks

### Task 1: Work
**State:** pending
"#;

/// The ticket's own machine: `pending` targets a mode `codex` does not declare,
/// so validation refuses the selector and has to say where `xhigh` would be
/// declared. §FS-rhei-states.1.3
const MACHINE: &str = r#"name: mode-location
version: 1
states:
  pending:
    initial: true
    description: Work the task
    target: codex[xhigh]:openai:gpt-5.6-luna
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: completed
"#;

/// A machine carrying no target, so the agent under test comes from the flags
/// rather than from a selector `validate` would refuse before the run starts.
const UNTARGETED_MACHINE: &str = r#"name: mode-location-flag
version: 1
states:
  pending:
    initial: true
    description: Work the task
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: completed
"#;

/// The issue #234 shape: the state selects `codex`, while merged settings
/// select its mode. §FS-rhei-agents.1.4.1
const SETTINGS_MODE_MACHINE: &str = r#"name: settings-mode
version: 1
states:
  pending:
    initial: true
    description: Work the task
    agent: codex
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: completed
"#;

const TASK_TARGET_PLAN: &str = r#"# Rhei: Task target shadow

## Tasks

### Task 1: Work
**State:** pending
**Target:** codex[yolo]:openai:gpt-5.6-luna
"#;

const MIXED_TASK_TARGET_PLAN: &str = r#"# Rhei: Mixed task target shadow

## Tasks

### Task 1: Targeted work
**State:** pending
**Target:** codex[yolo]:openai:gpt-5.6-luna

### Task 2: Settings-selected work
**State:** pending
"#;

fn settings_path(home: &str) -> String {
    format!("{home}/settings.json")
}

/// The clause §FS-rhei-errors.1.4 asks for: the key the registry entry is
/// written under, and the two files it may be written in. The global path is
/// static because the loader joins it under the home directory on every
/// platform rather than using a platform config directory. §REQ-cross-platform
fn location_clause(key: &str, project_settings: &str) -> String {
    format!("`{key}` in {project_settings} or ~/.config/rhei/settings.json")
}

/// Miette wraps a long message across gutter-prefixed lines, so a clause that
/// reads as one sentence is not one line. Flatten before matching: the renderer
/// breaks only at spaces, so no token is split. §FS-rhei-errors.2
fn flattened(result: &CliRun) -> String {
    let raw = format!("{}\n{}", result.stdout, result.stderr);
    raw.lines()
        .flat_map(|line| {
            line.trim_start_matches(|c: char| c.is_whitespace() || "×│╭╰─".contains(c))
                .split_whitespace()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn write_project(prefix: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", MACHINE);
    (dir, plan, machine)
}

/// A project settings file that declares something unrelated, so the merge
/// resolves *this* home without teaching `codex` the mode under test.
fn write_settings(dir: &Path, home: &str) {
    let settings_dir = dir.join(home);
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    write_fixture_file(
        &settings_dir,
        "settings.json",
        r#"{
  "agents": {
    "local-agent": { "command": ["local-agent"], "modes": { "yolo": [] } }
  }
}
"#,
    );
}

fn write_default_mode_settings(dir: &Path, mode: &str, default_agent: bool) {
    let settings_dir = dir.join(GROUNDS);
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    let agent = if default_agent { r#", "agent": "codex""# } else { "" };
    write_fixture_file(
        &settings_dir,
        "settings.json",
        &format!(r#"{{ "defaults": {{ "agent_mode": "{mode}"{agent} }} }}"#),
    );
}

fn assert_refuses_the_mode(output: &str) {
    assert!(
        output.contains("unknown target mode 'xhigh'"),
        "the undeclared mode must still be refused; output was:\n{output}"
    );
    assert!(
        output.contains("known modes: yolo"),
        "the refusal must still list what the agent declares; output was:\n{output}"
    );
}

/// The reported run: no project settings file at all, so the agent and its one
/// mode come from the built-in registry and there is no file to report. The
/// refusal still has to say where `xhigh` would be declared, which is the new
/// home rhei writes. §FS-rhei-errors.1.4 §FS-rhei-agents.1.1
#[test]
fn a_refused_mode_names_the_key_and_both_settings_files() {
    let (_dir, plan, machine) = write_project("registry-location-mode");

    let result = run_cli("validate", &plan, &machine, &[]);
    let output = flattened(&result);

    assert!(
        !result.status.success(),
        "an undeclared mode is still a refusal; output was:\n{output}"
    );
    assert_refuses_the_mode(&output);
    assert!(
        output.contains(&location_clause("agents.codex.modes", &settings_path(GROUNDS))),
        "the refusal must name where a mode is declared; output was:\n{output}"
    );
}

/// Validation resolves the settings-selected mode before execution and gives
/// the same declaration guidance as other registry refusals.
/// §FS-rhei-validate.4 §FS-rhei-agents.1.4.1
#[test]
fn validate_refuses_undeclared_effective_defaults_agent_mode() {
    let dir = unique_temp_dir("registry-location-default-mode-invalid");
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", SETTINGS_MODE_MACHINE);
    write_default_mode_settings(&dir, "bogus", false);

    let result = run_cli("validate", &plan, &machine, &[]);
    let output = flattened(&result);

    assert!(!result.status.success(), "the undeclared effective mode must fail: {output}");
    assert!(
        output.contains("agent 'codex' has no mode 'bogus'"),
        "the refusal must identify the effective agent and mode: {output}"
    );
    assert!(
        output.contains(&location_clause("agents.codex.modes", &settings_path(GROUNDS))),
        "the refusal must name where the effective mode is declared: {output}"
    );
}

/// A declared merged-settings mode remains a valid static selection.
/// §FS-rhei-validate.4 §FS-rhei-agents.1.4.1
#[test]
fn validate_accepts_declared_effective_defaults_agent_mode() {
    let dir = unique_temp_dir("registry-location-default-mode-valid");
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", SETTINGS_MODE_MACHINE);
    write_default_mode_settings(&dir, "yolo", false);

    let result = run_cli("validate", &plan, &machine, &[]);

    assert!(
        result.status.success(),
        "a declared effective mode must validate; stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(result.stdout.contains("Validation succeeded"));
}

/// A task selector owns its agent and mode, so a fallback it bypasses is not
/// rejected merely because it exists. §FS-rhei-agents.1.4.1
#[test]
fn validate_task_target_shadows_undeclared_defaults_agent_mode() {
    let dir = unique_temp_dir("registry-location-task-target-shadow");
    let plan = write_fixture_file(&dir, "plan.rhei.md", TASK_TARGET_PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", UNTARGETED_MACHINE);
    write_default_mode_settings(&dir, "bogus", true);

    let result = run_cli("validate", &plan, &machine, &[]);

    assert!(
        result.status.success(),
        "task target must bypass the settings mode; stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}

/// One task's target cannot hide the invalid fallback still used by another
/// task on the same machine. §FS-rhei-validate.4 §FS-rhei-plan-language.3.11
#[test]
fn validate_mixed_task_targets_still_refuses_undeclared_default_agent_mode() {
    let dir = unique_temp_dir("registry-location-mixed-task-target-shadow");
    let plan = write_fixture_file(&dir, "plan.rhei.md", MIXED_TASK_TARGET_PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", UNTARGETED_MACHINE);
    write_default_mode_settings(&dir, "bogus", true);

    let result = run_cli("validate", &plan, &machine, &[]);
    let output = flattened(&result);

    assert!(!result.status.success(), "the untargeted task must keep the refusal: {output}");
    assert!(
        output.contains("agent 'codex' has no mode 'bogus'"),
        "the refusal must identify the unshadowed settings selection: {output}"
    );
}

/// A project still on the deprecated home is sent to the file rhei read, never
/// to the one rhei writes: following the write path would create a file that
/// shadows the registry this very error just listed. §FS-rhei-agents.1.1
#[test]
fn a_refused_mode_names_the_deprecated_home_when_that_is_what_was_read() {
    let (dir, plan, machine) = write_project("registry-location-deprecated");
    write_settings(&dir, DEPRECATED);

    let result = run_cli("validate", &plan, &machine, &[]);
    let output = flattened(&result);

    assert!(
        !result.status.success(),
        "an undeclared mode is still a refusal; output was:\n{output}"
    );
    assert_refuses_the_mode(&output);
    assert!(
        output.contains(&location_clause("agents.codex.modes", &settings_path(DEPRECATED))),
        "the refusal must name the project settings file rhei read; output was:\n{output}"
    );
    assert!(
        !output.contains(&location_clause("agents.codex.modes", &settings_path(GROUNDS))),
        "naming the write path here shadows the registry just listed; output was:\n{output}"
    );
}

/// `validate` is not the only surface that refuses a mode against the registry.
/// A mode named on the command line is refused as the run starts, and
/// §FS-rhei-errors.6 asks the category to have one wording wherever it is
/// reached — so this refusal owes the same clause as the selector above, and
/// owes it in the branch that lists what the agent does declare rather than
/// only when it declares nothing. §FS-rhei-errors.1.4
#[test]
fn a_mode_refused_as_the_run_starts_names_the_key_and_both_settings_files() {
    let dir = unique_temp_dir("registry-location-flag");
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", UNTARGETED_MACHINE);
    let settings_dir = dir.join(GROUNDS);
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    write_fixture_file(
        &settings_dir,
        "settings.json",
        r#"{ "agents": { "fake": { "command": ["/bin/true"], "modes": { "safe": [] } } } }"#,
    );

    let result = run_cli(
        "run",
        &plan,
        &machine,
        &["--agent", "fake", "--agent-mode", "bogus", "--no-dashboard"],
    );
    let output = flattened(&result);

    assert!(
        output.contains("agent 'fake' has no mode 'bogus'"),
        "the undeclared mode must still be refused; output was:\n{output}"
    );
    assert!(
        output.contains("safe"),
        "the refusal must still list what the agent declares; output was:\n{output}"
    );
    assert!(
        output.contains(&location_clause("agents.fake.modes", &settings_path(GROUNDS))),
        "the refusal must name where a mode is declared; output was:\n{output}"
    );
}
