//! Which frontend a selected run gets, which combinations are refused, and
//! what the preview and the stream carry.
//!
//! `--until-idle` overrides TTY auto-detection and conflicts with an explicit
//! `--tui` or `--headless`. A detected terminal is overridden and never
//! refused: nobody should be refused for a flag they never typed.
// §FS-rhei-run-tui.1.4 §FS-rhei-run-headless.1 §FS-rhei-run-json.2.1

use super::until_idle_support::*;
use super::*;

const GATE_PLAN: &str = "## Tasks\n\n### Task 1: Waiting on a reviewer\n**State:** gate\n";

/// (12) An explicit `--tui` is a usage refusal, not a silent override — and it
/// fires before anything is written, so a refused invocation leaves the
/// workspace exactly as it found it. The diagnostic names the alternative,
/// which is the part only this change can produce: an unknown flag refuses
/// with the same status and writes nothing either.
// §FS-rhei-run-tui.1.4
#[test]
fn an_explicit_tui_beside_until_idle_is_refused_before_anything_is_written() {
    let plan = IdlePlan::new("until-idle-tui-conflict", GATE_PLAN);

    let result = plan.run(&["--tui", "--until-idle", "--no-dashboard"]);

    assert_exit(&result, EXIT_USAGE, "a finished TUI does not return on its own");
    assert_conflict(&result, "--tui");
    assert!(
        result.stderr.contains("--no-tui"),
        "the diagnostic names the alternative; got:\n{}",
        result.stderr
    );
    plan.assert_nothing_written();
    assert_task_state(&plan.plan, &plan.machine, "1", "gate");
}

/// (13) `--headless` beside the option is refused on every platform: the
/// launcher's exit code answers "did the run start?", so a detached selected
/// run could never hand its caller the status the option exists to deliver.
/// The refusal is asserted everywhere; its combination-specific diagnostic
/// only where `--headless` is a flag at all. §FS-rhei-run-headless.1.3
// §FS-rhei-run-headless.1
#[test]
fn headless_beside_until_idle_is_refused() {
    let plan = IdlePlan::new("until-idle-headless-conflict", GATE_PLAN);

    let result = plan.run(&["--headless", "--until-idle", "--no-dashboard"]);

    assert!(
        !result.status.success(),
        "a detached run can never carry this option\nstderr:\n{}",
        result.stderr
    );
    plan.assert_nothing_written();
    assert_task_state(&plan.plan, &plan.machine, "1", "gate");
    #[cfg(unix)]
    {
        assert_exit(&result, EXIT_USAGE, "the conflict is a usage refusal");
        assert_conflict(&result, "--headless");
    }
}

/// (14) The detached control, and the promise the refusal preserves: a
/// `--headless` run *without* the option still waits at the gate, so the run
/// this option can never be is unchanged by it.
///
/// The launcher returns only once the child has published a descriptor, so the
/// descriptor is there to be read; but another process writes it, and a read
/// that lands mid-write sees a partial object. The loop below waits out that
/// window and never waits for a descriptor that is not coming.
// §FS-rhei-run-headless.1.2
#[cfg(unix)]
#[test]
fn headless_alone_still_waits_at_the_gate() {
    let plan = IdlePlan::new("until-idle-headless-control", GATE_PLAN);

    let launch = plan.run(&["--headless", "--no-dashboard"]);
    assert_exit(&launch, 0, "the launcher reports that the run started");

    // Reading until it parses observes the published state rather than the
    // moment of publication. §FS-rhei-run-headless.1.1
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let descriptor = loop {
        let parsed = fs::read_to_string(plan.root().join("runtime/run.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
        match parsed {
            Some(descriptor) => break descriptor,
            None if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            None => panic!("the launcher exited 0, so a run descriptor should be readable"),
        }
    };
    assert_eq!(
        descriptor["status"], "running",
        "the detached run is holding the gate open, not finished"
    );
    assert_task_state(&plan.plan, &plan.machine, "1", "gate");

    let stop = plan.rhei(&["stop", "--wait", &plan.root().display().to_string()]);
    assert!(stop.status.success(), "stop the detached run:\n{}", stop.stderr);
}

/// (15) A detected terminal is overridden, never refused: the run produces
/// plain lines on a real pty and returns idle. Driven through the suite's
/// existing `portable-pty` harness, which is native on Unix and ConPTY on
/// Windows, so this is one case on every supported platform.
// §FS-rhei-run-tui.1.4 §REQ-cross-platform.2
#[test]
fn a_real_terminal_is_overridden_rather_than_refused() {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    use std::io::Read;

    let plan = IdlePlan::new("until-idle-real-terminal", GATE_PLAN);
    let source = rhei_command(&plan.home);
    let mut command = CommandBuilder::new(source.get_program());
    for (key, value) in source.get_envs() {
        match value {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        }
    }
    command.arg("--state-machine");
    command.arg(&plan.machine);
    command.arg("run");
    command.arg(&plan.plan);
    command.arg("--until-idle");
    command.arg("--no-dashboard");

    let pair = native_pty_system()
        .openpty(PtySize { rows: 40, cols: 240, pixel_width: 0, pixel_height: 0 })
        .expect("open a pty");
    let mut child = pair.slave.spawn_command(command).expect("spawn the run on a pty");
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().expect("read the pty");
    let transcript = std::thread::spawn(move || {
        let mut text = String::new();
        let mut buffer = [0u8; 4096];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 {
                break;
            }
            text.push_str(&String::from_utf8_lossy(&buffer[..count]));
        }
        text
    });
    let status = child.wait().expect("the run returns on its own");
    drop(pair.master);
    let transcript = transcript.join().expect("terminal transcript drained");

    assert_eq!(
        status.exit_code(),
        EXIT_IDLE as u32,
        "a detected terminal is overridden, not refused; got:\n{transcript}"
    );
    assert!(
        transcript.contains("Run idle:") && !transcript.contains("\u{1b}[?1049h"),
        "plain line output, with no alternate screen; got:\n{transcript}"
    );
}

/// (18) A dry run predicts the real run, including its exit status, when its
/// own scan finds nothing schedulable — and stays side-effect-free while doing
/// it.
// §FS-rhei-run.4
#[test]
fn a_dry_run_predicts_the_idle_stop_and_writes_nothing() {
    let plan = IdlePlan::new("until-idle-dry-run", GATE_PLAN);

    let result = plan.run(&["--dry-run", "--until-idle", "--no-dashboard"]);

    assert_exit(&result, EXIT_IDLE, "the preview predicts the status the real run would reach");
    assert_idle_line(&result, "gate", "none");
    for relative in ["runtime/run.json", "runtime/events.jsonl", "runtime/run-report.md"] {
        assert!(
            !plan.root().join(relative).exists(),
            "a dry run creates no runtime artifacts, and {relative} is one"
        );
    }
}

/// (19) The stream: `run_finished` carries the stop payload, the preview
/// carries the same prediction through the same record rather than a new one,
/// and `schema` does not move for either.
// §FS-rhei-run-json.2.1 §FS-rhei-run-json.2.2 §FS-rhei-run-json.4
#[test]
fn the_json_stream_carries_the_stop_payload_without_moving_the_schema() {
    let live = IdlePlan::new("until-idle-json-live", GATE_PLAN);
    let preview = IdlePlan::new("until-idle-json-preview", GATE_PLAN);

    let ran = live.run(&["--until-idle", "--json", "--no-dashboard"]);
    let predicted = preview.run(&["--until-idle", "--json", "--dry-run", "--no-dashboard"]);

    for (label, result) in [("run", &ran), ("preview", &predicted)] {
        assert_exit(result, EXIT_IDLE, &format!("{label}: idle"));
        let stop = stop_payload(result);
        assert_eq!(stop["reason"], "idle", "{label}");
        assert_eq!(stop["idle_blocker"], "gate", "{label}");
        assert_eq!(stop["next_attempt_at"], serde_json::Value::Null, "{label}");
        assert_eq!(stop["exit_code"], 3, "{label}");
        let started = records_of(result, "run_started");
        assert_eq!(started[0]["schema"], 1, "{label}: an additive payload does not move `schema`");
    }
    let kinds: std::collections::BTreeSet<String> = predicted
        .stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|record| record["event"].as_str().map(str::to_string))
        .collect();
    let known: std::collections::BTreeSet<String> =
        ["run_started", "run_finished", "message", "pass_started", "pass_ended", "link"]
            .iter()
            .map(|kind| (*kind).to_string())
            .collect();
    assert!(kinds.is_subset(&known), "the preview introduces no new record kind; got {kinds:?}");
}
