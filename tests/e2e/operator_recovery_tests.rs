use std::fs;

use super::operator_force_support::*;
use super::*;

const RECOVERY_ID: &str = "018f0000-0000-7000-8000-000000000001";
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[cfg_attr(windows, allow(dead_code))]
struct RecoveryFixture {
    force: ForceFixture,
    before_plan: String,
    after_plan: String,
    pair: String,
}

fn recovery_fixture(prefix: &str) -> RecoveryFixture {
    let force = force_fixture(prefix, GATE_PLAN, FORCE_MACHINE);
    let before_plan = fs::read_to_string(&force.plan).expect("before plan");
    let after_plan = before_plan.replace("**State:** human-gate", "**State:** implement");
    let payload = serde_json::json!({
        "confirmation": "typed-hop-v1",
        "from": "human-gate",
        "os_user": "fixture-user",
        "reason": "repair route",
        "recovery_id": RECOVERY_ID,
        "schema_version": 1,
        "task_id": "plan.1",
        "timestamp": "2026-09-18T12:00:00Z",
        "to": "implement"
    });
    let payload = serde_json::to_string(&payload).expect("audit JSON");
    let metadata_line = format!("plan.1 !force-v1 {}\n", encode_base64url(payload.as_bytes()));
    let movement_line = "plan.1 human-gate@implement\n";
    let pair = format!("{metadata_line}{movement_line}");
    let marker = serde_json::json!({
        "files": [{
            "after": {"bytes": encode_base64url(after_plan.as_bytes()), "kind": "present"},
            "before": {"bytes": encode_base64url(before_plan.as_bytes()), "kind": "present"},
            "path": "plan.rhei.md",
            "roles": ["metadata", "task"]
        }],
        "hop": {"from": "human-gate", "task_id": "plan.1", "to": "implement"},
        "ledger": {
            "metadata_line": metadata_line,
            "movement_line": movement_line,
            "offset": 0,
            "path": "runtime/state-transitions.log",
            "prefix_sha256": EMPTY_SHA256
        },
        "recovery_id": RECOVERY_ID,
        "version": 1
    });
    fs::create_dir_all(force.dir.join(".rhei")).expect("marker directory");
    let mut marker = serde_json::to_string(&marker).expect("marker JSON");
    marker.push('\n');
    fs::write(force.dir.join(".rhei/forced-recovery.json"), marker).expect("recovery marker");
    RecoveryFixture { force, before_plan, after_plan, pair }
}

#[cfg_attr(windows, allow(dead_code))]
fn recovery_confirmation(decision: &str) -> String {
    format!("recover {RECOVERY_ID} plan.1 human-gate -> implement {decision}")
}

/// Absence is an idempotent success and never prompts. §FS-rhei-recover.1
#[test]
fn operator_recovery_with_no_marker_is_a_noop() {
    let fixture = force_fixture("recover-no-marker", GATE_PLAN, FORCE_MACHINE);
    let before = artifact_snapshot(&fixture.dir, &fixture.plan);
    let output = rhei_command(fixture.dir.join("home"))
        .arg("recover")
        .arg(fixture.dir.as_os_str())
        .output()
        .expect("recover command");
    let run = CliRun::from(&output);

    assert_success(&run);
    assert!(run.stdout.contains("no forced recovery pending"), "unexpected output: {}", run.stdout);
    assert_artifacts_unchanged(&before);
}

/// Strict and lenient readers and every mutator stall on the same marker; none
/// silently chooses rollback or forward. §FS-rhei-recover.4 §FS-rhei-panta.6.6
#[test]
fn operator_recovery_marker_blocks_readers_and_mutators() {
    let cases: [(&str, Vec<&str>); 6] = [
        ("list", vec![]),
        ("render", vec!["--format", "json"]),
        (
            "transition",
            vec!["--task", "1", "--from", "human-gate", "--to", "completed", "--result", "done"],
        ),
        ("next", vec!["--peek"]),
        ("reset", vec!["-y"]),
        ("run", vec!["--dry-run"]),
    ];
    let mut violations = Vec::new();

    for (command, args) in cases {
        let fixture = recovery_fixture(&format!("recover-interlock-{command}"));
        let before = artifact_snapshot(&fixture.force.dir, &fixture.force.plan);
        let canonical_root = rhei_core::platform::canonical_path(&fixture.force.dir)
            .expect("canonical recovery root");
        let recovery = format!(
            "rhei recover {}",
            rhei_core::platform::shell_quote(&canonical_root.display().to_string())
        );
        let run = run_cli(command, &fixture.force.plan, &fixture.force.machine, &args);
        let diagnostic = serde_json::from_str::<serde_json::Value>(&run.stderr)
            .ok()
            .and_then(|value| value["error"]["message"].as_str().map(str::to_owned))
            .unwrap_or_else(|| run.stderr.clone());
        if run.status.success()
            || !diagnostic.contains("plan.1 human-gate -> implement")
            || diagnostic.matches(&recovery).count() != 1
            || artifact_snapshot(&fixture.force.dir, &fixture.force.plan) != before
        {
            violations.push(format!(
                "{command}: status={}\nstdout:\n{}\nstderr:\n{}",
                run.status, run.stdout, run.stderr
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "marked-root interlock violations:\n{}",
        violations.join("\n\n")
    );
}

/// No durable pair means rollback, even when an after-image reached disk.
/// §FS-rhei-recover.3
#[cfg(unix)]
#[test]
fn operator_recovery_rolls_back_before_the_commit_pair() {
    let fixture = recovery_fixture("recover-rollback");
    fs::write(&fixture.force.plan, &fixture.after_plan).expect("interrupted after-image");

    let run = run_recover_in_terminal(&fixture.force.dir, &recovery_confirmation("rollback"));

    assert!(run.status.success(), "rollback failed:\n{}", run.transcript);
    assert_eq!(fs::read_to_string(&fixture.force.plan).unwrap(), fixture.before_plan);
    assert!(!fixture.force.dir.join("runtime/state-transitions.log").exists());
    assert!(!fixture.force.dir.join(".rhei/forced-recovery.json").exists());
}

/// The exact durable pair commits, so recovery installs after-images and does
/// not duplicate the pair on a retry. §FS-rhei-recover.3
#[cfg(unix)]
#[test]
fn operator_recovery_rolls_forward_once_after_the_commit_pair() {
    let fixture = recovery_fixture("recover-forward");
    fs::create_dir_all(fixture.force.dir.join("runtime")).expect("runtime directory");
    fs::write(fixture.force.dir.join("runtime/state-transitions.log"), &fixture.pair)
        .expect("committed audit pair");

    let run = run_recover_in_terminal(&fixture.force.dir, &recovery_confirmation("forward"));

    assert!(run.status.success(), "roll-forward failed:\n{}", run.transcript);
    assert_eq!(fs::read_to_string(&fixture.force.plan).unwrap(), fixture.after_plan);
    assert_eq!(
        fs::read_to_string(fixture.force.dir.join("runtime/state-transitions.log")).unwrap(),
        fixture.pair
    );
    assert!(!fixture.force.dir.join(".rhei/forced-recovery.json").exists());

    let retry = rhei_command(fixture.force.dir.join("home"))
        .arg("recover")
        .arg(fixture.force.dir.as_os_str())
        .output()
        .expect("idempotent retry");
    let retry = CliRun::from(&retry);
    assert_success(&retry);
    assert!(retry.stdout.contains("no forced recovery pending"));
    assert_eq!(
        fs::read_to_string(fixture.force.dir.join("runtime/state-transitions.log")).unwrap(),
        fixture.pair
    );
}

/// A torn pair is the only ledger tail recovery may truncate before restoring
/// before-images. §FS-rhei-recover.3
#[cfg(unix)]
#[test]
fn operator_recovery_truncates_only_an_exact_torn_pair() {
    let fixture = recovery_fixture("recover-torn-pair");
    fs::create_dir_all(fixture.force.dir.join("runtime")).expect("runtime directory");
    let torn = &fixture.pair.as_bytes()[..fixture.pair.len() - 5];
    fs::write(fixture.force.dir.join("runtime/state-transitions.log"), torn).expect("torn pair");
    fs::write(&fixture.force.plan, &fixture.after_plan).expect("interrupted after-image");

    let run = run_recover_in_terminal(&fixture.force.dir, &recovery_confirmation("rollback"));

    assert!(run.status.success(), "torn rollback failed:\n{}", run.transcript);
    assert_eq!(fs::read_to_string(&fixture.force.plan).unwrap(), fixture.before_plan);
    assert_eq!(fs::read(fixture.force.dir.join("runtime/state-transitions.log")).unwrap(), b"");
}

/// Corruption is evidence for a person to restore, never permission to guess.
/// §FS-rhei-recover.2 §FS-rhei-recover.3
#[test]
fn operator_recovery_refuses_corrupt_and_unknown_markers() {
    for (name, marker, expected) in [
        ("corrupt", "{not-json}\n", "forced-recovery marker is corrupt"),
        ("unknown", "{\"version\":99}\n", "unsupported forced-recovery marker version 99"),
    ] {
        let fixture = force_fixture(&format!("recover-{name}"), GATE_PLAN, FORCE_MACHINE);
        fs::create_dir_all(fixture.dir.join(".rhei")).expect("marker directory");
        fs::write(fixture.dir.join(".rhei/forced-recovery.json"), marker).expect("bad marker");
        let before = fs::read(&fixture.plan).expect("plan bytes");

        let output = rhei_command(fixture.dir.join("home"))
            .arg("recover")
            .arg(fixture.dir.as_os_str())
            .output()
            .expect("recover command");
        let run = CliRun::from(&output);
        assert!(!run.status.success(), "{name} marker was accepted");
        assert_stderr_contains(&run, expected);
        assert!(run.stderr.contains("rhei recover"));
        assert_eq!(fs::read(&fixture.plan).unwrap(), before);
        assert!(fixture.dir.join(".rhei/forced-recovery.json").exists());
    }
}
