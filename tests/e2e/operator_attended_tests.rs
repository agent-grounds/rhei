//! The same attended scenarios run on native Unix PTYs and Windows ConPTY.
//! §FS-rhei-recover.5 §FS-rhei-transition-cmd.6

use super::operator_force_support::*;
use super::*;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

/// Input is sent only after the child's prompt is observed, using a channel barrier.
/// The harness is test-only; production has no environment authorization. §FS-rhei-recover.1
pub(super) fn attended(fixture: &ForceFixture, arguments: &[&str], answer: &str) -> (bool, String) {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    let source = rhei_command(fixture.dir.join("attended-home"));
    let mut command = CommandBuilder::new(source.get_program());
    for (key, value) in source.get_envs() {
        if let Some(value) = value {
            command.env(key, value);
        } else {
            command.env_remove(key);
        }
    }
    command.args(arguments);
    let pair = native_pty_system()
        .openpty(PtySize { rows: 40, cols: 240, pixel_width: 0, pixel_height: 0 })
        .unwrap();
    let mut child = pair.slave.spawn_command(command).unwrap();
    let mut killer = child.clone_killer();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();
    let (prompt_tx, prompt_rx) = mpsc::channel();
    let (output_tx, output_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut all = Vec::new();
        let mut buffer = [0; 4096];
        let mut prompted = false;
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 {
                break;
            }
            all.extend_from_slice(&buffer[..count]);
            if !prompted && String::from_utf8_lossy(&all).contains(": type ") {
                prompted = true;
                let _ = prompt_tx.send(());
            }
        }
        let _ = output_tx.send(String::from_utf8_lossy(&all).into_owned());
    });
    if prompt_rx.recv_timeout(Duration::from_secs(20)).is_err() {
        let _ = killer.kill();
        panic!(
            "operator prompt missing: {}",
            output_rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default()
        );
    }
    writer.write_all(answer.as_bytes()).unwrap();
    writer.flush().unwrap();
    let (exit_tx, exit_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = exit_tx.send(child.wait());
    });
    let status = match exit_rx.recv_timeout(Duration::from_secs(20)) {
        Ok(status) => status.unwrap(),
        Err(err) => {
            let _ = killer.kill();
            panic!("attended child did not exit: {err}");
        }
    };
    drop(writer);
    drop(pair.master);
    let output =
        output_rx.recv_timeout(Duration::from_secs(10)).expect("terminal transcript drained");
    (status.success(), output)
}

/// Equivalent success on Linux/macOS/Windows; no Unix-only test gate. §FS-rhei-recover.5
#[test]
fn operator_attended_native_terminal_success_and_explicit_recovery() {
    for decision in ["rollback", "forward"] {
        let fixture = force_fixture(&format!("attended-{decision}"), GATE_PLAN, FORCE_MACHINE);
        let before = fs::read(&fixture.plan).unwrap();
        let machine = fixture.machine.to_str().unwrap();
        let plan = fixture.plan.to_str().unwrap();
        let (success, transcript) = attended(
            &fixture,
            &[
                "--state-machine",
                machine,
                "transition",
                plan,
                "--task",
                "1",
                "--from",
                "human-gate",
                "--to",
                "implement",
                "--force",
                "--reason",
                "repair route",
            ],
            "force plan.1 human-gate -> implement\r\n",
        );
        assert!(success, "{transcript}");
        let after = fs::read(&fixture.plan).unwrap();
        let ledger_path = fixture.dir.join("runtime/state-transitions.log");
        let ledger = fs::read_to_string(&ledger_path).unwrap();
        let moves = rhei_core::transition_history::parse(&ledger).unwrap();
        assert_eq!(moves.len(), 1);
        let audit = moves[0].audit.as_ref().unwrap();
        let (metadata_line, movement_line) = audit.pair().unwrap();
        // Writer fault ordering is covered by native unit tests. This fixture
        // exercises attended recovery with actual command-produced images/pair. §FS-rhei-recover.3
        let marker = serde_json::json!({"version":1,"recovery_id":audit.recovery_id,
            "hop":{"task_id":"plan.1","from":"human-gate","to":"implement"},
            "files":[{"path":"plan.rhei.md","roles":["metadata","task"],
                "before":{"kind":"present","bytes":encode_base64url(&before)},
                "after":{"kind":"present","bytes":encode_base64url(&after)}}],
            "ledger":{"path":"runtime/state-transitions.log","offset":0,
                "prefix_sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                "metadata_line":metadata_line,"movement_line":movement_line}});
        fs::write(
            fixture.dir.join(".rhei/forced-recovery.json"),
            format!("{}\n", serde_json::to_string(&marker).unwrap()),
        )
        .unwrap();
        if decision == "rollback" {
            fs::write(&ledger_path, "").unwrap();
        } else {
            fs::write(&fixture.plan, &before).unwrap();
        }
        let confirmation =
            format!("recover {} plan.1 human-gate -> implement {decision}\r\n", audit.recovery_id);
        let (success, transcript) =
            attended(&fixture, &["recover", fixture.dir.to_str().unwrap()], &confirmation);
        assert!(success, "{transcript}");
        assert_eq!(
            fs::read(&fixture.plan).unwrap(),
            if decision == "forward" { after } else { before }
        );
        assert_eq!(
            rhei_core::transition_history::parse(&fs::read_to_string(&ledger_path).unwrap())
                .unwrap()
                .len(),
            usize::from(decision == "forward")
        );
        assert!(!fixture.dir.join(".rhei/forced-recovery.json").exists());
        assert!(!fixture.dir.join("runtime/transitions.log").exists());
    }
}

/// A terminal is necessary but a wrong/blank response still refuses without effects. §FS-rhei-transition-cmd.6
#[test]
fn operator_attended_wrong_confirmation_changes_nothing() {
    for response in ["\r\n", "force plan.1 human-gate -> completed\r\n"] {
        let fixture = force_fixture("attended-refusal", GATE_PLAN, FORCE_MACHINE);
        let before = artifact_snapshot(&fixture.dir, &fixture.plan);
        let (success, transcript) = attended(
            &fixture,
            &[
                "--state-machine",
                fixture.machine.to_str().unwrap(),
                "transition",
                fixture.plan.to_str().unwrap(),
                "--task",
                "1",
                "--from",
                "human-gate",
                "--to",
                "implement",
                "--force",
                "--reason",
                "repair route",
            ],
            response,
        );
        assert!(!success);
        assert!(transcript.contains("operator confirmation did not match"), "{transcript}");
        assert_artifacts_unchanged(&before);
    }
}

/// Canonical-terminal EOF is also a refusal, not an empty authorization. §FS-rhei-transition-cmd.6
#[cfg(unix)]
#[test]
fn operator_attended_eof_changes_nothing() {
    let fixture = force_fixture("attended-eof", GATE_PLAN, FORCE_MACHINE);
    let before = artifact_snapshot(&fixture.dir, &fixture.plan);
    let (success, transcript) = attended(
        &fixture,
        &[
            "--state-machine",
            fixture.machine.to_str().unwrap(),
            "transition",
            fixture.plan.to_str().unwrap(),
            "--task",
            "1",
            "--from",
            "human-gate",
            "--to",
            "implement",
            "--force",
            "--reason",
            "repair route",
        ],
        "\x04",
    );
    assert!(!success);
    assert!(transcript.contains("operator confirmation did not match"), "{transcript}");
    assert_artifacts_unchanged(&before);
}

/// Windows console EOF must refuse after the prompt without authorizing a hop. §FS-rhei-transition-cmd.6
#[cfg(windows)]
#[test]
fn operator_attended_windows_console_eof_changes_nothing() {
    let fixture = force_fixture("attended-windows-eof", GATE_PLAN, FORCE_MACHINE);
    // Single-file plan snapshots include metadata/checkpoints as well as task bytes;
    // the other snapshots cover result, both ledgers and recovery marker. §FS-rhei-recover.2
    let before = artifact_snapshot(&fixture.dir, &fixture.plan);
    let (success, transcript) = attended(
        &fixture,
        &[
            "--state-machine",
            fixture.machine.to_str().unwrap(),
            "transition",
            fixture.plan.to_str().unwrap(),
            "--task",
            "1",
            "--from",
            "human-gate",
            "--to",
            "implement",
            "--force",
            "--reason",
            "repair route",
        ],
        // After the prompt barrier, ConPTY delivers Ctrl-Z to ReadConsoleW. Rust's
        // console reader wakes on SUB and removes it, yielding a zero-byte read.
        // No newline/answer, pipe EOF or timeout kill stands in for native EOF.
        "\x1a",
    );
    assert!(!success, "{transcript}");
    assert!(transcript.contains("operator confirmation did not match"), "{transcript}");
    assert_artifacts_unchanged(&before);
}

/// Terminal result/link semantics are exercised through ConPTY as well as Unix. §FS-rhei-transition-cmd.6
#[test]
fn operator_attended_native_terminal_result_round_trip() {
    let fixture = force_fixture(
        "attended-terminal",
        &GATE_PLAN.replace("human-gate", "implement"),
        FORCE_MACHINE,
    );
    for (from, to, message) in [
        ("implement", "completed", "first outcome"),
        ("completed", "implement", "reopen context"),
        ("implement", "completed", "fresh outcome"),
    ] {
        let confirmation = format!("force plan.1 {from} -> {to}\r\n");
        let (success, transcript) = attended(
            &fixture,
            &[
                "--state-machine",
                fixture.machine.to_str().unwrap(),
                "transition",
                fixture.plan.to_str().unwrap(),
                "--task",
                "1",
                "--from",
                from,
                "--to",
                to,
                "--force",
                "--reason",
                "repair route",
                "--result",
                message,
            ],
            &confirmation,
        );
        assert!(success, "{transcript}");
        let body = fs::read_to_string(&fixture.plan).unwrap();
        assert_eq!(body.matches("> **Result:**").count(), usize::from(to == "completed"));
    }
    let results = fs::read_to_string(fixture.dir.join("runtime/results/plan.1.md")).unwrap();
    assert_eq!(results.matches("## Result").count(), 3);
    let ledger = fs::read_to_string(fixture.dir.join("runtime/state-transitions.log")).unwrap();
    assert_eq!(rhei_core::transition_history::parse(&ledger).unwrap().len(), 3);
    assert!(!fixture.dir.join("runtime/transitions.log").exists());
}
