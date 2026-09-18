use std::path::Path;

use super::session_continuation_support::{continuation_agent, continuation_settings};
use super::*;

fn validation_fixture(prefix: &str) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let agent = continuation_agent(&dir);
    continuation_settings(&dir, &agent, "supported");
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Session Validation\n\n## Tasks\n\n### Task 1: Work\n**State:** work\n",
    );
    (dir, plan)
}

fn validate_machine(dir: &Path, plan: &Path, name: &str, machine: &str) -> CliRun {
    let path = write_fixture_file(dir, &format!("{name}.yaml"), machine);
    run_cli("validate", plan, &path, &[])
}

fn machine_with_bad_state(bad: &str, bad_transitions: &str) -> String {
    format!(
        r#"name: session-validation
version: 1
states:
  work:
    initial: true
    description: Assigned valid work
    target: fake:acme:model-a
  bad:
{bad}  completed:
    description: Done
    final: true
transitions:
  - from: work
    to: completed
{bad_transitions}"#
    )
}

/// The authored field is a checked contract even when its value is `cold`.
/// Every shape below is otherwise accepted by today's machine grammar, so an
/// unexpected success means `session` was ignored. §FS-rhei-states.1.3
#[test]
fn session_validation_rejects_invalid_values_and_illegal_state_shapes() {
    let (dir, plan) = validation_fixture("session-validation-illegal");
    let cases = [
        (
            "invalid-value",
            "    description: Bad enum\n    target: fake:acme:model-a\n    session: warm\n",
            "  - from: bad\n    to: bad\n  - from: bad\n    to: completed\n",
            "cold or continue",
        ),
        (
            "final",
            "    description: Final\n    final: true\n    session: cold\n",
            "  - from: bad\n    to: bad\n",
            "final",
        ),
        (
            "gating",
            "    description: Gate\n    gating: true\n    target: fake:acme:model-a\n    session: cold\n",
            "  - from: bad\n    to: bad\n  - from: bad\n    to: completed\n",
            "gating",
        ),
        (
            "program",
            "    description: Program\n    program: \"true\"\n    session: cold\n",
            "  - from: bad\n    to: bad\n  - from: bad\n    to: completed\n",
            "program",
        ),
        (
            "poll",
            "    description: Poll\n    target: fake:acme:model-a\n    session: cold\n    poll:\n      interval: 1s\n      max_attempts: 1\n",
            "  - from: bad\n    to: bad\n  - from: bad\n    to: completed\n    condition: pollAttempts >= pollMaxAttempts\n",
            "poll",
        ),
        (
            "no-self-loop",
            "    description: No loop\n    target: fake:acme:model-a\n    session: continue\n",
            "  - from: bad\n    to: completed\n",
            "self-loop",
        ),
        (
            "unresolved-target",
            "    description: No effective tuple\n    agent: fake\n    session: cold\n",
            "  - from: bad\n    to: bad\n  - from: bad\n    to: completed\n",
            "target",
        ),
        (
            "no-agent",
            "    description: No agent executor\n    session: cold\n",
            "  - from: bad\n    to: bad\n  - from: bad\n    to: completed\n",
            "agent",
        ),
    ];

    let mut unexpected = Vec::new();
    let mut wrong_diagnostic = Vec::new();
    for (name, bad, transitions, expected) in cases {
        let run = validate_machine(&dir, &plan, name, &machine_with_bad_state(bad, transitions));
        let output = format!("{}{}", run.stdout, run.stderr);
        if run.status.success() {
            unexpected.push(name);
        } else if !output.to_lowercase().contains(expected) {
            wrong_diagnostic.push(format!("{name}: expected {expected:?}, got:\n{output}"));
        }
    }
    assert!(
        unexpected.is_empty() && wrong_diagnostic.is_empty(),
        "session validation was not enforced; unexpected successes: {unexpected:?}\n{}",
        wrong_diagnostic.join("\n")
    );
}

/// One invocation cannot have both the implicit previous-visit source and an
/// authored named-inherit source. §FS-rhei-snapshots.4.7 §FS-rhei-snapshots.11
#[test]
fn continuation_rejects_named_inherit() {
    let (dir, plan) = validation_fixture("session-validation-inherit-conflict");
    let machine = r#"name: session-validation
version: 1
states:
  work:
    initial: true
    description: Assigned valid work
    target: fake:acme:model-a
  source:
    description: Named source
    target: fake:acme:model-a
    snapshot:
      emit: { name: context, on: always }
  bad:
    description: Two preload sources
    target: fake:acme:model-a
    session: continue
    snapshot:
      inherit:
        name: context
        select: { state: source }
  completed:
    description: Done
    final: true
transitions:
  - from: work
    to: completed
  - from: source
    to: completed
  - from: bad
    to: bad
  - from: bad
    to: completed
"#;
    let run = validate_machine(&dir, &plan, "inherit-conflict", machine);
    let output = format!("{}{}", run.stdout, run.stderr);
    assert!(
        !run.status.success() && output.contains("session") && output.contains("snapshot.inherit"),
        "the two preload sources must be rejected together:\n{output}"
    );
}

/// `cold` is the omitted default, while a named emission remains an allowed,
/// independently strict product beside continuation. §FS-rhei-snapshots.4.7
#[test]
fn cold_default_and_named_emit_coexistence_validate() {
    let (dir, plan) = validation_fixture("session-validation-controls");
    let machine = r#"name: session-validation
version: 1
states:
  work:
    initial: true
    description: Omitted session defaults cold
    target: fake:acme:model-a
  loop:
    description: Continue and publish an independently named snapshot
    target: fake:acme:model-a
    session: continue
    snapshot:
      emit: { name: context, on: always }
  explicit-cold:
    description: Explicit cold is legal on the same state shape
    target: fake:acme:model-a
    session: cold
  completed:
    description: Done
    final: true
transitions:
  - from: work
    to: completed
  - from: loop
    to: loop
  - from: loop
    to: completed
  - from: explicit-cold
    to: explicit-cold
  - from: explicit-cold
    to: completed
"#;
    let run = validate_machine(&dir, &plan, "controls", machine);
    assert_success(&run);
}

/// State-local continuation is not an authored override source;
/// `--from-snapshot` still requires named inheritance. §FS-rhei-snapshots.4.7
#[test]
fn continuation_does_not_authorize_from_snapshot() {
    let transitions =
        "  - from: loop\n    to: completed\n    condition: visitCount >= 2\n  - from: loop\n    to: loop\n";
    let fixture = super::session_continuation_support::continuation_fixture(
        "session-from-snapshot-contract",
        2,
        1,
        "supported",
        "",
        transitions,
    );
    let run = super::session_continuation_support::run_continuation(
        &fixture,
        &["--from-snapshot", "plan.1:_state:loop@1:fake-acme-model-a/g1"],
    );
    let output = format!("{}{}", run.stdout, run.stderr);
    assert!(
        !run.status.success()
            && output.contains("--from-snapshot")
            && output.contains("snapshot.inherit"),
        "continuation must not open a second override grammar:\n{output}"
    );
}
