use std::fs;

use super::operator_force_support::*;
use super::*;

/// A real CLI hop proves the missing-edge exception, its adjacent audit pair,
/// and the unchanged slot-only run journal together. §FS-rhei-transition-cmd.6.1
#[cfg(unix)]
#[test]
fn operator_recovery_forces_a_stranded_gate_with_one_exceptional_audit_pair() {
    let fixture = force_fixture("force-stranded-gate", GATE_PLAN, FORCE_MACHINE);

    let run = run_force_in_terminal(
        &fixture,
        "1",
        "human-gate",
        "implement",
        "workflow was routed to the wrong gate",
        None,
    );

    assert!(
        run.status.success(),
        "forced recovery should succeed\ntranscript:\n{}",
        run.transcript
    );
    assert_task_state(&fixture.plan, &fixture.machine, "1", "implement");
    let ledger = fs::read_to_string(fixture.dir.join("runtime/state-transitions.log"))
        .expect("forced ledger");
    let lines: Vec<&str> = ledger.lines().collect();
    assert_eq!(lines.len(), 2, "one forced move is one adjacent pair: {ledger:?}");
    let payload = lines[0].strip_prefix("plan.1 !force-v1 ").expect("task-keyed force metadata");
    let metadata: serde_json::Value =
        serde_json::from_slice(&decode_base64url(payload)).expect("canonical JSON payload");
    assert_eq!(metadata["schema_version"], 1);
    assert_eq!(metadata["task_id"], "plan.1");
    assert_eq!(metadata["from"], "human-gate");
    assert_eq!(metadata["to"], "implement");
    assert_eq!(metadata["reason"], "workflow was routed to the wrong gate");
    assert_eq!(metadata["confirmation"], "typed-hop-v1");
    assert!(metadata["recovery_id"].as_str().is_some_and(|id| !id.is_empty()));
    assert!(metadata["os_user"].as_str().is_some_and(|user| !user.is_empty()));
    assert!(metadata["timestamp"].as_str().is_some_and(|time| time.ends_with('Z')));
    assert_eq!(lines[1], "plan.1 human-gate@implement");
    assert!(
        !fixture.dir.join("runtime/transitions.log").exists(),
        "a manual recovery owns no run slot"
    );
    assert!(!fixture.dir.join(".rhei/forced-recovery.json").exists());
}

/// An old result is history, not consent to finish this forced invocation.
/// §FS-rhei-transition-cmd.3.2 §FS-rhei-states.3.3
#[cfg(unix)]
#[test]
fn operator_recovery_forced_final_entry_requires_a_fresh_result() {
    let plan = GATE_PLAN.replace("human-gate", "implement");
    let fixture = force_fixture("force-fresh-final-result", &plan, FORCE_MACHINE);
    fs::create_dir_all(fixture.dir.join("runtime/results")).expect("result directory");
    fs::write(fixture.dir.join("runtime/results/plan.1.md"), "## Result\n\nold\n")
        .expect("old result");
    let before = artifact_snapshot(&fixture.dir, &fixture.plan);

    let run = run_force_in_terminal(
        &fixture,
        "1",
        "implement",
        "completed",
        "finish after correcting the route",
        None,
    );

    assert!(!run.status.success(), "missing fresh result must be refused");
    assert!(
        run.transcript.contains(
            "forced entry into terminal state 'completed' requires a fresh non-empty --result"
        ),
        "wrong refusal:\n{}",
        run.transcript
    );
    assert_artifacts_unchanged(&before);
}

/// Reopening keeps the old outcome as history and unlinks it from a live task;
/// final re-entry appends a new outcome and relinks once. §FS-rhei-plan-language.3.8
#[cfg(unix)]
#[test]
fn operator_recovery_terminal_exit_and_reentry_preserve_result_history() {
    let fixture = force_fixture(
        "force-terminal-round-trip",
        &GATE_PLAN.replace("human-gate", "completed"),
        FORCE_MACHINE,
    );
    fs::create_dir_all(fixture.dir.join("runtime/results")).expect("result directory");
    fs::write(fixture.dir.join("runtime/results/plan.1.md"), "## Result\n\nfirst\n")
        .expect("first result");
    let with_link = fs::read_to_string(&fixture.plan).expect("plan").replace(
        "**State:** completed\n",
        "**State:** completed\n\n> **Result:** [plan.1](runtime/results/plan.1.md)\n",
    );
    fs::write(&fixture.plan, with_link).expect("seed result link");

    let exit = run_force_in_terminal(
        &fixture,
        "1",
        "completed",
        "implement",
        "completion was premature",
        None,
    );
    assert!(exit.status.success(), "terminal exit failed:\n{}", exit.transcript);
    let reopened = fs::read_to_string(&fixture.plan).expect("reopened plan");
    assert!(!reopened.contains("> **Result:**"), "live task retained a result link");
    assert_eq!(
        fs::read_to_string(fixture.dir.join("runtime/results/plan.1.md")).unwrap(),
        "## Result\n\nfirst\n"
    );

    let reenter = run_force_in_terminal(
        &fixture,
        "1",
        "implement",
        "completed",
        "corrected work is now complete",
        Some("second"),
    );
    assert!(reenter.status.success(), "final re-entry failed:\n{}", reenter.transcript);
    let final_plan = fs::read_to_string(&fixture.plan).expect("final plan");
    assert_eq!(final_plan.matches("> **Result:**").count(), 1);
    let results = fs::read_to_string(fixture.dir.join("runtime/results/plan.1.md")).unwrap();
    assert!(results.contains("first"));
    assert!(results.contains("second"));
    assert_eq!(results.matches("## Result").count(), 2);
}

/// The authority surface is absent from noninteractive and machine-governed
/// execution; a pipe never becomes consent. §FS-rhei-transition-cmd.6
#[test]
fn operator_recovery_refuses_noninteractive_force_without_effects() {
    let fixture = force_fixture("force-noninteractive", GATE_PLAN, FORCE_MACHINE);
    let before = artifact_snapshot(&fixture.dir, &fixture.plan);

    let run =
        run_force_noninteractive(&fixture, "1", "human-gate", "implement", "repair route", &[]);

    assert!(!run.status.success());
    assert_stderr_contains(
        &run,
        "--force requires an interactive terminal and fresh typed confirmation",
    );
    assert_artifacts_unchanged(&before);
}

/// Required and incompatible flags are stable preflight refusals, before a
/// prompt or any effect. §FS-rhei-transition-cmd.2 §FS-rhei-transition-cmd.6
#[test]
fn operator_recovery_refuses_missing_reason_and_no_callbacks() {
    let fixture = force_fixture("force-option-refusals", GATE_PLAN, FORCE_MACHINE);
    let before = artifact_snapshot(&fixture.dir, &fixture.plan);
    let missing = run_cli(
        "transition",
        &fixture.plan,
        &fixture.machine,
        &["--task", "1", "--from", "human-gate", "--to", "implement", "--force"],
    );
    assert!(!missing.status.success());
    assert_stderr_contains(&missing, "--force requires a fresh non-empty --reason");
    assert_artifacts_unchanged(&before);

    let whitespace = run_force_noninteractive(&fixture, "1", "human-gate", "implement", "   ", &[]);
    assert!(!whitespace.status.success());
    assert_stderr_contains(&whitespace, "--force requires a fresh non-empty --reason");
    assert_artifacts_unchanged(&before);

    let reason_without_force = run_cli(
        "transition",
        &fixture.plan,
        &fixture.machine,
        &[
            "--task",
            "1",
            "--from",
            "human-gate",
            "--to",
            "completed",
            "--reason",
            "ordinary move",
            "--result",
            "done",
        ],
    );
    assert!(!reason_without_force.status.success());
    assert_stderr_contains(&reason_without_force, "--reason is only valid with --force");
    assert_artifacts_unchanged(&before);

    let supervisor = run_force_noninteractive(
        &fixture,
        "1",
        "human-gate",
        "implement",
        "repair route",
        &["--supervisor", "1"],
    );
    assert!(!supervisor.status.success());
    assert_stderr_contains(&supervisor, "--force cannot be combined with --supervisor");
    assert_artifacts_unchanged(&before);

    let no_callbacks = run_force_noninteractive(
        &fixture,
        "1",
        "human-gate",
        "implement",
        "repair route",
        &["--no-callbacks"],
    );
    assert!(!no_callbacks.status.success());
    assert_stderr_contains(
        &no_callbacks,
        "a forced recovery runs no callbacks; drop --no-callbacks",
    );
    assert_artifacts_unchanged(&before);
}

/// Force classifies declaration before prompting: available declared edges are
/// ordinary, and blocked declared edges keep their safeguard. §FS-rhei-transition-cmd.6
#[test]
fn operator_recovery_does_not_relabel_declared_edges() {
    let fixture = force_fixture("force-declared-edge", GATE_PLAN, FORCE_MACHINE);
    let before = artifact_snapshot(&fixture.dir, &fixture.plan);
    let available = run_force_noninteractive(
        &fixture,
        "1",
        "human-gate",
        "completed",
        "not exceptional",
        &["--result", "done"],
    );
    assert!(!available.status.success());
    assert_stderr_contains(
        &available,
        "the state machine already declares this edge; drop --force",
    );
    assert_artifacts_unchanged(&before);

    let machine = FORCE_MACHINE.replace(
        "  - from: human-gate\n    to: completed",
        "  - from: human-gate\n    to: completed\n    condition: visitCount > 1",
    );
    fs::write(&fixture.machine, machine).expect("condition-blocked machine");
    let blocked = run_force_noninteractive(
        &fixture,
        "1",
        "human-gate",
        "completed",
        "not exceptional",
        &["--result", "done"],
    );
    assert!(!blocked.status.success());
    assert_stderr_contains(
        &blocked,
        "transition from 'human-gate' to 'completed' is not currently applicable",
    );
    assert_stderr_contains(&blocked, "--force does not bypass safeguards on declared edges");
    assert_artifacts_unchanged(&before);
}

/// This compatibility change is independent of any force invocation.
/// §FS-rhei-states.1.3 §FS-rhei-transitions.4.6
#[test]
fn operator_recovery_rejects_exact_transitions_from_final_states_at_load() {
    let machine = r#"name: invalid-final-source
version: 1
states:
  pending:
    initial: true
    description: Work
  done:
    final: true
    description: Done
transitions:
  - from: pending
    to: done
  - from: done
    to: pending
"#;
    let dir = unique_temp_dir("force-final-source-validation");
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        GATE_PLAN.replace("human-gate", "pending").as_str(),
    );
    let states = write_fixture_file(&dir, "states.yaml", machine);

    let validation = run_cli("validate", &plan, &states, &[]);

    assert!(!validation.status.success(), "an exact final-source edge must fail loading");
    assert_stderr_contains(&validation, "transition from final state 'done' is forbidden");
    assert_stderr_contains(&validation, "§FS-rhei-transitions.4.6");
}

/// Help is the first discovery surface and must name both attended commands.
/// §FS-rhei-usage.2.2 §FS-rhei-completions.7
#[test]
fn operator_recovery_command_surface_is_discoverable() {
    let root = unique_temp_dir("force-command-help");
    let top = rhei_command(root.join("home")).arg("--help").output().expect("top-level help");
    let top = stdout(&top);
    assert!(top.contains("recover"), "top-level help omits recover:\n{top}");

    let transition = rhei_command(root.join("home"))
        .args(["transition", "--help"])
        .output()
        .expect("transition help");
    let transition = stdout(&transition);
    assert!(transition.contains("--force"), "transition help omits --force:\n{transition}");
    assert!(transition.contains("--reason"), "transition help omits --reason:\n{transition}");

    let completion = rhei_command(root.join("home"))
        .args(["completions", "bash"])
        .output()
        .expect("bash completions");
    let completion = stdout(&completion);
    assert!(completion.contains("recover"), "completion omits recover");
    assert!(completion.contains("--force"), "completion omits --force");
    assert!(completion.contains("--reason"), "completion omits --reason");
}
