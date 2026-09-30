//! The timeout requirement is a preflight, not only a guard at the spawn.
//!
//! A state that resolves to an agent invocation with no finite `agent_timeout`
//! is refused by `rhei validate` and by `rhei run --dry-run` with the sentence
//! `rhei run` already gives it, so the two commands run before an unattended
//! launch stop predicting a spawn the launch refuses one line later.
//! §FS-rhei-agents.3.2.2 §FS-rhei-validate.4 §FS-rhei-run.4

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

/// The report's own plan: one task, one state that names a target.
const PLAN: &str = "# Rhei: Timeout preflight

## Tasks

### Task 1: Edit something
**State:** edit
";

const MACHINE: &str = r#"name: timeout-preflight
version: 1
states:
  edit:
    target: cld[yolo]:anthropic:claude-opus-5
    instructions: |
      Make the edit.
  completed:
    final: true
transitions:
  - from: edit
    to: completed
"#;

/// The state the fixture offends in, and the agent it resolves — both have to
/// appear in the refusal for it to be actionable.
const STATE: &str = "edit";
const AGENT: &str = "cld";

fn write_settings(root: &Path, body: &str) {
    let dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&dir).expect("create settings directory");
    write_fixture_file(&dir, "settings.json", body);
}

/// The report's fixture: an agent profile that sets no `timeout`, no model
/// binding, and — unless `defaults` is given — no `defaults.agent_timeout`, so
/// nothing in the four-level chain resolves.
///
/// `defaults` is the whole JSON member, including its leading comma, because
/// the only difference the tests draw between the broken plan and the fixed
/// one is the single line the error message tells a user to write.
fn fixture(prefix: &str, defaults: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let agent = write_python_agent(&dir, "agent.py", "result('done')\n");
    write_settings(
        &dir,
        &format!(
            r#"{{"agents":{{"{AGENT}":{{"command":{},"stdin_prompt":true,"model_flag":"--model","modes":{{"yolo":["--yolo"]}}}}}}{defaults}}}"#,
            fixture_command(&agent)
        ),
    );
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", MACHINE);
    (dir, plan, machine)
}

/// No `defaults.agent_timeout`: the chain resolves nothing.
const UNBOUNDED: &str = "";

/// The one line the refusal tells a user to add, and nothing else.
const BOUNDED: &str = r#","defaults":{"agent_timeout":"30m"}"#;

/// Hold one surface to the refusal: non-zero, and a message a reader can act
/// on — the offending state, the agent that resolved no timeout, and the
/// setting whose absence is the cause.
fn assert_refuses_missing_timeout(result: &CliRun, surface: &str) {
    assert!(
        !result.status.success(),
        "{surface} must refuse a state that resolves no agent_timeout\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let said = format!("{}{}", result.stdout, result.stderr);
    for expected in ["agent_timeout", STATE, AGENT] {
        assert!(
            said.contains(expected),
            "{surface} must name {expected:?} in its refusal; it said:\n{said}"
        );
    }
}

/// `rhei validate` decides this, so a plan it accepts is a plan whose every
/// orchestrator-driven agent state is bounded. §FS-rhei-validate.4
#[test]
fn validate_refuses_a_state_that_resolves_no_agent_timeout() {
    let (_dir, plan, machine) = fixture("timeout-validate", UNBOUNDED);

    let result = run_cli("validate", &plan, &machine, &[]);

    assert_refuses_missing_timeout(&result, "rhei validate");
}

/// A dry run predicts the real run, including its exit status, so it may not
/// print `Would spawn:` for an invocation admission refuses. §FS-rhei-run.4
#[test]
fn dry_run_refuses_a_state_that_resolves_no_agent_timeout() {
    let (_dir, plan, machine) = fixture("timeout-dry-run", UNBOUNDED);

    let result = run_cli("run", &plan, &machine, &["--dry-run", "--no-tui"]);

    assert_refuses_missing_timeout(&result, "rhei run --dry-run");
    assert!(
        !result.stdout.contains("Would spawn:"),
        "a dry run must not project a spawn the run refuses; it said:\n{}",
        result.stdout
    );
}

/// The refusal the report already relies on, pinned so moving it from the
/// first pass to admission cannot lose it. §FS-rhei-agents.3.2.2
#[test]
fn run_refuses_a_state_that_resolves_no_agent_timeout() {
    let (_dir, plan, machine) = fixture("timeout-run", UNBOUNDED);

    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);

    assert_refuses_missing_timeout(&result, "rhei run");
}

/// The line the error tells a user to write is the line that fixes it — on
/// every surface, with nothing else changed. §FS-rhei-agents.7.1
#[test]
fn one_defaults_line_satisfies_every_surface() {
    let (_dir, plan, machine) = fixture("timeout-bounded", BOUNDED);

    assert_success(&run_cli("validate", &plan, &machine, &[]));
    assert_success(&run_cli("run", &plan, &machine, &["--dry-run", "--no-tui"]));
    assert_success(&run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]));
}

/// `--no-agent` resolves no agent invocation, so it resolves nothing to
/// reject: the unfixed fixture still runs to its terminal state.
/// §FS-rhei-validate.4
#[test]
fn no_agent_runs_a_plan_that_resolves_no_agent_timeout() {
    let (_dir, plan, machine) = fixture("timeout-no-agent", UNBOUNDED);

    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-agent", "--no-callbacks"]);

    assert_success(&result);
    assert_task_state(&plan, &machine, "plan.1", "completed");
}
