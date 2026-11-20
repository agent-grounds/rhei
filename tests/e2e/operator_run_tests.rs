//! Fresh CLI regressions for #757, held red until implementation.
//! §FS-rhei-transition-cmd.6 §FS-rhei-recover.3 §FS-rhei-errors.1.2

use super::operator_attended_tests::attended_command;
use super::operator_force_support::*;
use super::operator_run_support::*;
use super::*;

/// The live run is outside this ticket's scope but still excludes forced writes.
/// Project and member entry points must observe the same project lock.
/// §FS-rhei-transition-cmd.6 §FS-rhei-run.2.6
#[test]
fn issue_757_force_live_run_refuses_before_prompt() {
    for member_cwd in [false, true] {
        let fixture = OperatorRunFixture::new();
        let mut run = fixture.start(&["constants"]);
        let before = fixture.snapshot();
        let mut command = fixture.command();
        if member_cwd {
            command.current_dir(fixture.force.dir.join("profile"));
        }
        let (success, text) = attended_command(command, &fixture.force_args(), None, || {});
        assert_artifacts_unchanged(&before);
        assert!(run.child().try_wait().unwrap().is_none(), "{}", run.output());
        assert!(!success, "{text}");
        assert!(
            !text.contains(": type "),
            "held run must refuse before the typed-hop prompt:\n{text}"
        );
        assert!(!text.contains("[exit wait expired"), "refusal must exit without input:\n{text}");
        assert_run_help(&text, &fixture, &fixture.id());
    }
}

/// Separate from ordering: captures today's refusal without needing a terminal.
/// §FS-rhei-errors.1.2 §FS-rhei-run.2.6
#[test]
fn issue_757_force_live_run_help() {
    let fixture = OperatorRunFixture::new();
    let mut run = fixture.start(&["constants"]);
    let before = fixture.snapshot();
    let out = fixture.command().args(fixture.force_args()).output().unwrap();
    let output = CliRun::from(&out);
    assert!(!output.status.success());
    assert_artifacts_unchanged(&before);
    assert!(run.child().try_wait().unwrap().is_none(), "{}", run.output());
    assert_run_help(&output.stderr, &fixture, &fixture.id());
}

/// Keep the terminal-required headline and hand over the exact qualified hop.
/// §FS-rhei-errors.1.2 §FS-rhei-transition-cmd.6
#[test]
fn issue_757_force_nonterminal_handoff() {
    let fixture = OperatorRunFixture::new();
    let mut run = fixture.start(&["constants"]);
    let before = fixture.snapshot();
    let out = fixture.command().args(fixture.force_args()).output().unwrap();
    let output = CliRun::from(&out);
    assert!(!output.status.success());
    assert_artifacts_unchanged(&before);
    assert!(run.child().try_wait().unwrap().is_none(), "{}", run.output());
    assert_stderr_contains(
        &output,
        "--force requires an interactive terminal and fresh typed confirmation",
    );
    assert_stderr_contains(&output, "force profile.deliver human-gate -> implement");
    assert_run_help(&output.stderr, &fixture, &fixture.id());
}

/// The prompt is an event barrier, not a timed guess. Start the actual scheduler
/// while input is pending, then refuse safely and require a fresh confirmation.
/// §FS-rhei-transition-cmd.6 §FS-rhei-run.2.6
#[test]
fn issue_757_run_can_start_during_confirmation_then_fresh_retry_succeeds() {
    let fixture = OperatorRunFixture::new();
    let mut before = None;
    let mut run = None;
    let (success, text) = attended_command(
        fixture.command(),
        &fixture.force_args(),
        Some("force profile.deliver human-gate -> implement\r\n"),
        || {
            for owner in [
                fixture.force.dir.to_path_buf(),
                fixture.force.dir.join("constants"),
                fixture.force.dir.join("profile"),
            ] {
                assert!(
                    !owner.join(".rhei/run.lock").exists(),
                    "observation created {}",
                    owner.join(".rhei/run.lock").display()
                );
            }
            run = Some(fixture.start(&["constants"]));
            before = Some(fixture.snapshot());
        },
    );
    let mut run = run.expect("prompt hook started the scheduler");
    let id = fixture.id();
    assert!(!success, "{text}");
    assert_artifacts_unchanged(&before.unwrap());
    assert!(run.child().try_wait().unwrap().is_none(), "{}", run.output());
    fixture.finish(&mut run);
    let (retry_success, retry_text) = attended_command(
        fixture.command(),
        &fixture.force_args(),
        Some("force profile.deliver human-gate -> implement\r\n"),
        || {},
    );
    assert!(retry_success, "fresh confirmation after run ends must succeed:\n{retry_text}");
    assert!(
        retry_text.contains(": type force profile.deliver human-gate -> implement"),
        "{retry_text}"
    );
    assert_run_help(&text, &fixture, &id);
}

/// Recovery observes marker owners and does not require an ordinary plan load.
/// §FS-rhei-recover.3
#[test]
fn issue_757_recovery_refuses_before_prompt() {
    let fixture = recovery_fixture("issue-757-recovery-order");
    let _held = hold_operator(&fixture.force.dir);
    let before = artifact_snapshot(&fixture.force.dir, &fixture.force.plan);
    let command = rhei_command(fixture.force.dir.join("attended-home"));
    let (success, text) =
        attended_command(command, &["recover", fixture.force.dir.to_str().unwrap()], None, || {});
    assert_artifacts_unchanged(&before);
    assert!(!success, "{text}");
    assert!(
        !text.contains(": type "),
        "recovery must observe contention before prompting:\n{text}"
    );
    assert!(!text.contains("[exit wait expired"), "recovery must exit without input:\n{text}");
    assert!(text.contains("other-operator"), "identify the operator holder:\n{text}");
}

/// Recovery's handoff includes its id, qualified hop and evidence-derived decision.
/// §FS-rhei-recover.3 §FS-rhei-errors.1.2
#[test]
fn issue_757_recovery_nonterminal_handoff() {
    let fixture = recovery_fixture("issue-757-recovery-handoff");
    let _held = hold_operator(&fixture.force.dir);
    let before = artifact_snapshot(&fixture.force.dir, &fixture.force.plan);
    let output = rhei_command(fixture.force.dir.join("attended-home"))
        .args(["recover", fixture.force.dir.to_str().unwrap()])
        .output()
        .unwrap();
    let output = CliRun::from(&output);
    assert!(!output.status.success());
    assert_artifacts_unchanged(&before);
    assert_stderr_contains(
        &output,
        "recover requires an interactive terminal and fresh typed confirmation",
    );
    assert_stderr_contains(&output, &recovery_confirmation("rollback"));
    assert_stderr_contains(&output, "other-operator");
    assert!(!output.stderr.contains("rhei stop "), "an operator holder is not a stop target");
}

/// Retry rendering must retain supplied/discovered targets and task selectors,
/// special characters, explicit machine, result and invocation directory.
/// §FS-rhei-errors.1.2
#[test]
fn issue_757_force_retry_preserves_invocation_context() {
    for supplied in [false, true] {
        let fixture = OperatorRunFixture::new();
        let mut args = vec!["--state-machine", "profile/states.yaml", "transition"];
        if supplied {
            args.push("./index.panta.md");
        }
        args.extend([
            "--task",
            "profile.deliver",
            "--from",
            "human-gate",
            "--to",
            "implement",
            "--force",
            "--reason",
            "owner's $(literal) route",
            "--result",
            "outcome with spaces",
        ]);
        let before = fixture.snapshot();
        let out = fixture.command().args(&args).output().unwrap();
        let output = CliRun::from(&out);
        assert!(!output.status.success());
        assert_artifacts_unchanged(&before);
        assert_stderr_contains(
            &output,
            "--force requires an interactive terminal and fresh typed confirmation",
        );
        let text = &output.stderr;
        for part in [
            "rhei ",
            "--state-machine=profile/states.yaml",
            "--task=profile.deliver",
            "--from=human-gate",
            "--to=implement",
            "--force",
            "--reason",
            "--result",
        ] {
            assert!(text.contains(part), "retry lost {part:?}:\n{text}");
        }
        for value in ["owner's $(literal) route", "outcome with spaces"] {
            assert!(text.contains(&shell_quote(value)), "retry lost quoted {value:?}:\n{text}");
        }
        let invocation_dir = fixture.invocation_dir();
        let target = if supplied {
            "./index.panta.md".to_owned()
        } else {
            invocation_dir.display().to_string()
        };
        assert!(text.contains(&shell_quote(&target)), "retry lost target {target}:\n{text}");
        assert!(
            text.contains(&invocation_dir.display().to_string()),
            "missing invocation directory:\n{text}"
        );
        let raw = raw_stderr(&out);
        assert!(
            raw.lines().any(|line| line.contains("rhei ")
                && line.contains("--state-machine")
                && line.contains("--result")
                && line.contains(&shell_quote("owner's $(literal) route"))),
            "the full retry command must remain on one physical line:\n{raw}"
        );
    }
}
