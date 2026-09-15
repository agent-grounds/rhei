//! An existing directory with neither recognized manifest is a plan-shape
//! error, not a generic filesystem failure. §FS-rhei-errors.3.2

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

struct UnrecognizedDirectoryFixture {
    _root: TestDir,
    home: PathBuf,
    container: PathBuf,
    workspace: PathBuf,
    single_file: PathBuf,
}

fn fixture(prefix: &str) -> UnrecognizedDirectoryFixture {
    let root = unique_temp_dir(prefix);
    let container = root.join("panta");
    let workspace = container.join("workspace");
    fs::create_dir_all(workspace.join("tasks")).expect("create workspace fixture");
    fs::write(workspace.join("index.rhei.md"), "# Rhei: Workspace\n")
        .expect("write workspace manifest");
    fs::write(
        workspace.join("tasks/01-placeholder.md"),
        "### Task 1: Placeholder\n**State:** completed\n",
    )
    .expect("write inert workspace task");
    let single_file = write_fixture_file(
        &root,
        "single.rhei.md",
        "# Rhei: Single\n\n## Tasks\n\n### Task 1: Placeholder\n**State:** completed\n",
    );
    let home = root.join(".home");
    UnrecognizedDirectoryFixture { _root: root, home, container, workspace, single_file }
}

fn run_dry(home: &Path, input: &Path, extra: &[&str]) -> CliRun {
    let mut command = rhei_command(home);
    command.arg("run").arg(input).args(extra).args(["--dry-run", "--no-tui", "--no-callbacks"]);
    CliRun::from(&command.output().expect("rhei run should execute"))
}

#[cfg(unix)]
fn run_reported_headless(fixture: &UnrecognizedDirectoryFixture) -> CliRun {
    let output = rhei_command(&fixture.home)
        .arg("run")
        .arg("--headless")
        .arg(&fixture.container)
        .args(["--rhei", "workspace"])
        .output()
        .expect("rhei run --headless should execute");
    CliRun::from(&output)
}

fn validate(home: &Path, input: &Path) -> CliRun {
    let output = rhei_command(home)
        .arg("validate")
        .arg(input)
        .output()
        .expect("rhei validate should execute");
    CliRun::from(&output)
}

fn assert_unrecognized_directory(result: &CliRun, container: &Path, invocation: &str) {
    assert!(
        !result.status.success(),
        "{invocation} must reject the unrecognized directory\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let diagnostic = &result.stderr;
    assert!(
        diagnostic.contains(&container.display().to_string())
            && diagnostic.contains("not a recognized")
            && diagnostic.contains("Panta Project")
            && diagnostic.contains("Directory Workspace"),
        "{invocation} must classify '{}' as an unrecognized input directory; command exit: {:?}; got:\n{}",
        container.display(),
        result.status.code(),
        diagnostic
    );
    assert!(
        diagnostic.contains("index.panta.md") && diagnostic.contains("index.rhei.md"),
        "{invocation} must name the alternative project and workspace manifests; got:\n{}",
        diagnostic
    );
    assert!(
        diagnostic.contains("pass the actual plan or workspace path"),
        "{invocation} must explain how to address an existing plan or workspace; got:\n{}",
        diagnostic
    );
    for misleading in ["permission", "writable", "free space"] {
        assert!(
            !diagnostic.to_ascii_lowercase().contains(misleading),
            "{invocation} must not give {misleading:?} advice for a missing manifest; got:\n{}",
            diagnostic
        );
    }
}

fn expected_shell_command(arguments: &[String]) -> String {
    arguments.iter().map(|argument| shell_quote(argument)).collect::<Vec<_>>().join(" ")
}

/// Read the correction through ordinary help wrapping and the headless
/// launcher's nested diagnostic. Strip only known line prefixes, preserving
/// quotes, internal spaces, and literal gutter characters. §FS-rhei-errors.1.2
fn rendered_correction(diagnostic: &str) -> String {
    let mut lines = diagnostic.lines();
    let start = lines
        .find_map(|line| line.split_once("Run that workspace directly with:"))
        .expect("diagnostic must offer a direct workspace command");
    let nested = start.0.starts_with("  │ ");
    let mut command = start.1.strip_prefix(' ').unwrap_or(start.1).to_string();
    for line in lines {
        let continuation = if nested {
            // Inner help indentation, outer rewrap indentation, then its gutter.
            line.strip_prefix("  │           ")
                .or_else(|| line.strip_prefix("  │  "))
                .or_else(|| line.strip_prefix("  │ "))
        } else {
            line.strip_prefix("        ")
        };
        let Some(continuation) = continuation else { break };
        if !command.is_empty() {
            command.push(' ');
        }
        command.push_str(continuation);
    }
    command
}

fn assert_corrected_run_command(result: &CliRun, arguments: &[String], invocation: &str) {
    let expected = expected_shell_command(arguments);
    assert_eq!(
        rendered_correction(&result.stderr),
        expected,
        "{invocation} must preserve the complete invocation while correcting only its plan path; expected:\n{expected}\ngot:\n{}",
        result.stderr
    );
}

/// Validation shares the plan input boundary but retains its collect-errors
/// path for a real Single-File Plan. §FS-rhei-errors.3.2
#[test]
fn validate_classifies_an_unrecognized_directory_and_keeps_file_validation() {
    let fixture = fixture("unrecognized-directory-validate");

    let file_control = validate(&fixture.home, &fixture.single_file);
    assert!(
        file_control.status.success(),
        "Single-File Plan validation must retain its behavior\nstdout:\n{}\nstderr:\n{}",
        file_control.stdout,
        file_control.stderr
    );

    let rejected = validate(&fixture.home, &fixture.container);
    assert_unrecognized_directory(&rejected, &fixture.container, "rhei validate DIRECTORY");
}

/// The ordinary run path rejects the reported container while preserving all
/// three recognized input shapes. Every control is dry-run and cannot spawn an
/// agent or begin orchestration. §FS-rhei-errors.3.2
#[test]
fn run_classifies_an_unrecognized_directory_and_points_to_its_workspace() {
    let fixture = fixture("unrecognized-directory-run");

    let workspace_control = run_dry(&fixture.home, &fixture.workspace, &[]);
    let single_file_control = run_dry(&fixture.home, &fixture.single_file, &[]);
    let rejected = run_dry(&fixture.home, &fixture.container, &["--rhei", "workspace"]);

    fs::write(fixture.container.join("index.panta.md"), "# Panta: Recognized\n")
        .expect("write project manifest for the final control");
    let project_control = run_dry(&fixture.home, &fixture.container, &["--rhei", "workspace"]);

    for (name, result) in [
        ("directly addressed Directory Workspace", &workspace_control),
        ("Single-File Plan", &single_file_control),
        ("Panta Project", &project_control),
    ] {
        assert!(
            result.status.success(),
            "recognized {name} control must retain its behavior\nstdout:\n{}\nstderr:\n{}",
            result.stdout,
            result.stderr
        );
    }

    assert_unrecognized_directory(
        &rejected,
        &fixture.container,
        "rhei run DIRECTORY --rhei workspace --dry-run",
    );
    assert_corrected_run_command(
        &rejected,
        &[
            "rhei".to_string(),
            "run".to_string(),
            fixture.workspace.display().to_string(),
            "--dry-run".to_string(),
            "--no-callbacks".to_string(),
            "--rhei".to_string(),
            "workspace".to_string(),
            "--no-tui".to_string(),
        ],
        "rhei run DIRECTORY --rhei workspace --dry-run --no-tui --no-callbacks",
    );
}

/// A pasteable correction carries option values and quotes shell-sensitive
/// paths and values, changing only the wrong-level plan path. §FS-rhei-errors.1.2
#[test]
fn run_correction_preserves_supplied_flags_and_shell_sensitive_values() {
    let fixture = fixture("unrecognized directory's options");
    let state_machine = fixture._root.join("state machine's.yaml");
    let prices = fixture._root.join("price book's.json");
    let agent = "agent's choice";
    let mode = "mode with spaces";
    let model = "provider:model with spaces";
    let timeout = "2 minutes";
    let output = rhei_command(&fixture.home)
        .arg("run")
        .arg(&fixture.container)
        .arg("--state-machine")
        .arg(&state_machine)
        .args(["--rhei", "workspace", "--dry-run", "--no-tui", "--no-callbacks"])
        .args(["--continue-on-error", "--parallel", "3"])
        .arg("--prices")
        .arg(&prices)
        .args(["--agent", agent, "--agent-mode", mode, "--model", model])
        .args(["--no-program", "--program-timeout", timeout])
        .output()
        .expect("rhei run should execute");
    let rejected = CliRun::from(&output);

    assert_unrecognized_directory(
        &rejected,
        &fixture.container,
        "rhei run with supplied flags and shell-sensitive values",
    );
    assert_corrected_run_command(
        &rejected,
        &[
            "rhei".to_string(),
            "run".to_string(),
            fixture.workspace.display().to_string(),
            "--state-machine".to_string(),
            state_machine.display().to_string(),
            "--dry-run".to_string(),
            "--no-callbacks".to_string(),
            "--continue-on-error".to_string(),
            "--parallel".to_string(),
            "3".to_string(),
            "--prices".to_string(),
            prices.display().to_string(),
            "--rhei".to_string(),
            "workspace".to_string(),
            "--no-tui".to_string(),
            "--agent".to_string(),
            agent.to_string(),
            "--agent-mode".to_string(),
            mode.to_string(),
            "--model".to_string(),
            model.to_string(),
            "--no-program".to_string(),
            "--program-timeout".to_string(),
            timeout.to_string(),
        ],
        "rhei run with supplied flags and shell-sensitive values",
    );
}

/// Execute the displayed correction with the binary under test. Accepted
/// equals-form values must remain values, and dry-run must remain a preview.
/// The fixture has only a completed task, even if a flag is lost. §FS-rhei-errors.1.2
#[cfg(unix)]
#[test]
fn run_correction_accepts_leading_hyphen_values_when_pasted() {
    let fixture = fixture("unrecognized directory's pasted correction");
    let captured_args = fixture._root.join("pasted-arguments");
    for model in ["-weird", "-model's $HOME │ choice"] {
        let model_arg = format!("--model={model}");
        let rejected =
            run_dry(&fixture.home, &fixture.container, &["--rhei", "workspace", &model_arg]);
        assert_unrecognized_directory(
            &rejected,
            &fixture.container,
            "rhei run with --model=-VALUE",
        );
        assert_eq!(
            rejected.status.code(),
            Some(1),
            "the original invocation must pass CLI parsing"
        );
        let correction = rendered_correction(&rejected.stderr);

        // The shell consumes the actual suggestion. The function only records
        // argv and selects our binary; it adds or repairs no run arguments.
        let script = format!(
            "rhei() {{\n  printf '%s\\0' \"$@\" > \"$RHEI_CORRECTION_ARGS\"\n  \
             \"$RHEI_CORRECTION_BINARY\" \"$@\"\n}}\n{correction}"
        );
        let output = rhei_process_at("sh")
            .args(["-c", &script])
            .current_dir(&fixture.workspace)
            .env("HOME", &fixture.home)
            .env("XDG_STATE_HOME", fixture.home.join("state"))
            .env("RHEI_CORRECTION_ARGS", &captured_args)
            .env("RHEI_CORRECTION_BINARY", rhei_binary())
            .output()
            .expect("execute the rendered correction through the shell");
        let pasted = CliRun::from(&output);
        assert!(
            pasted.status.success() && pasted.stdout.contains("Dry run complete"),
            "the pasted correction must succeed as a dry-run: {correction}\nstdout:\n{}\nstderr:\n{}",
            pasted.stdout,
            pasted.stderr
        );
        let recorded = fs::read_to_string(&captured_args).expect("read the shell's actual argv");
        assert_eq!(
            recorded.split_terminator('\0').collect::<Vec<_>>(),
            [
                "run",
                fixture.workspace.to_str().unwrap(),
                "--dry-run",
                "--no-callbacks",
                "--rhei",
                "workspace",
                "--no-tui",
                &model_arg,
            ],
            "pasting must preserve ordered arguments and literal shell-sensitive content"
        );
    }
}

/// The invocation from agent-grounds/rhei#264 must report the startup failure
/// synchronously instead of leaving the useful diagnosis in a detached log.
/// §FS-rhei-errors.3.2 §FS-rhei-run-headless.1.1
#[cfg(unix)]
#[test]
fn headless_run_propagates_the_unrecognized_directory_diagnosis() {
    let fixture = fixture("unrecognized-directory-headless");
    let rejected = run_reported_headless(&fixture);

    assert_unrecognized_directory(
        &rejected,
        &fixture.container,
        "rhei run --headless DIRECTORY --rhei workspace",
    );
    assert_corrected_run_command(
        &rejected,
        &[
            "rhei".to_string(),
            "run".to_string(),
            fixture.workspace.display().to_string(),
            "--rhei".to_string(),
            "workspace".to_string(),
            "--headless".to_string(),
        ],
        "rhei run --headless DIRECTORY --rhei workspace",
    );
}
