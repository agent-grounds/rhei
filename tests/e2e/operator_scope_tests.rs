//! Scope evidence and safe fallback diagnostics for the shared ceremony.
//! §FS-rhei-transition-cmd.6 §FS-rhei-errors.1.2 §FS-rhei-recover.3

use super::operator_attended_tests::{attended, attended_command};
use super::operator_force_support::*;
use super::operator_run_support::*;
use super::*;

/// Journal writers and schemas stay unchanged: read their existing startup prefix.
/// Every row executes even on the old binary, so the red report lists the matrix.
/// §FS-rhei-transition-cmd.6 §FS-rhei-transition-cmd.3.4
#[test]
#[ignore = "red until #757: operator handoff must use verified journal scope; implement removes this"]
fn issue_757_scope_and_readonly_registry_matrix() {
    let mut missing = Vec::new();
    for mode in ["whole", "narrow", "repeated", "missing", "torn", "stale"] {
        let fixture = OperatorRunFixture::new();
        let scope = match mode {
            "whole" => vec![],
            "repeated" => vec!["constants", "profile", "constants"],
            _ => vec!["constants"],
        };
        let mut run = fixture.start(&scope);
        let id = fixture.id();
        let journal = fixture.force.dir.join("runtime/events.jsonl");
        let original = fs::read_to_string(&journal).unwrap();
        match mode {
            "missing" => fs::remove_file(&journal).unwrap(),
            "torn" => {
                let first = original.lines().next().unwrap();
                fs::write(&journal, format!("{first}\n{{\"seq\":2")).unwrap();
            }
            "stale" => fs::write(&journal, original.replace(&id, "stale-run")).unwrap(),
            _ => {}
        }
        let journal_before = fs::read(&journal).ok();
        let registry = fixture.home.join("state/rhei/runs");
        fs::create_dir_all(&registry).unwrap();
        // A read-only observer must not prune even unrelated malformed entries.
        fs::write(registry.join("unrelated.json"), "malformed unrelated entry").unwrap();
        let registry_snapshot = || -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
            fs::read_dir(&registry)
                .unwrap()
                .map(|entry| {
                    let path = entry.unwrap().path();
                    let bytes = fs::read(&path).unwrap();
                    (path, bytes)
                })
                .collect()
        };
        let registry_before = registry_snapshot();
        let before = fixture.snapshot();
        let out = fixture.command().args(fixture.force_args()).output().unwrap();
        let output = CliRun::from(&out);
        assert!(!output.status.success());
        assert_artifacts_unchanged(&before);
        assert_eq!(
            registry_snapshot(),
            registry_before,
            "{mode}: registry observation mutated entries"
        );
        assert_eq!(fs::read(&journal).ok(), journal_before, "{mode}: observation rewrote journal");
        assert!(run.child().try_wait().unwrap().is_none(), "{}", run.output());
        let text = &output.stderr;
        let expected = match mode {
            "whole" => text.contains("whole project") && text.contains("in scope"),
            "narrow" => text.contains("constants") && text.contains("outside"),
            "repeated" => {
                text.contains("constants") && text.contains("profile") && text.contains("in scope")
            }
            _ => text.contains("scope") && text.contains("unknown") && !text.contains("outside"),
        };
        if !expected || !text.contains(&id) {
            missing
                .push(format!("{mode}: missing verified scope/membership for run {id}:\n{text}"));
        }
        fixture.finish(&mut run);
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

/// Without a resolvable descriptor, an owner id alone cannot become a stop target.
/// §FS-rhei-errors.1.2 §FS-rhei-transition-cmd.6
#[test]
#[ignore = "red until #757: descriptor uncertainty needs safe launcher guidance; implement removes this"]
fn issue_757_missing_descriptor_uses_launcher_remedy() {
    let fixture = OperatorRunFixture::new();
    let mut run = fixture.start(&["constants"]);
    let id = fixture.id();
    fs::remove_file(fixture.force.dir.join("runtime/run.json")).unwrap();
    let registry = fixture.home.join("state/rhei/runs");
    for entry in fs::read_dir(&registry).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    let before = fixture.snapshot();
    let out = fixture.command().args(fixture.force_args()).output().unwrap();
    let output = CliRun::from(&out);
    assert_artifacts_unchanged(&before);
    assert!(run.child().try_wait().unwrap().is_none(), "{}", run.output());
    fixture.finish(&mut run);
    assert!(!output.status.success());
    assert!(!output.stderr.contains("rhei stop "), "unresolvable id {id}: {}", output.stderr);
    assert!(
        output.stderr.contains("original launcher") || output.stderr.contains("Ctrl-C"),
        "missing safe launcher remedy:\n{}",
        output.stderr
    );
}

/// A shared execution root still blocks a different plan under it.
/// §FS-rhei-transition-cmd.6 §FS-rhei-run.2.6
#[test]
#[ignore = "red until #757: shared-root blocker must be handed off with the hop; implement removes this"]
fn issue_757_shared_root_operator_handoff() {
    let fixture = force_fixture("issue-757-shared-root", GATE_PLAN, FORCE_MACHINE);
    write_fixture_file(&fixture.dir, "sibling.rhei.md", GATE_PLAN);
    let _held = hold_operator(&fixture.dir);
    let before = artifact_snapshot(&fixture.dir, &fixture.plan);
    let out =
        run_force_noninteractive(&fixture, "1", "human-gate", "implement", "repair route", &[]);
    assert!(!out.status.success());
    assert_artifacts_unchanged(&before);
    assert_stderr_contains(
        &out,
        "--force requires an interactive terminal and fresh typed confirmation",
    );
    assert_stderr_contains(&out, "other-operator");
    assert_stderr_contains(&out, "force plan.1 human-gate -> implement");
    assert!(!out.stderr.contains("rhei stop "));
}

/// These are control witnesses of existing eligibility, not new red regressions:
/// missing or unreadable lock evidence must still allow the prompt to appear.
/// §FS-rhei-transition-cmd.6
#[cfg(unix)]
#[test]
fn issue_757_existing_missing_and_uncertain_lock_evidence_allows_confirmation() {
    use std::os::unix::fs::PermissionsExt;
    for uncertain in [false, true] {
        let fixture = force_fixture("issue-757-uncertain-probe", GATE_PLAN, FORCE_MACHINE);
        let path = fixture.dir.join(".rhei/run.lock");
        if uncertain {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "owner bytes remain").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        }
        let (success, text) = attended(
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
            "\r\n",
        );
        if uncertain {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), "owner bytes remain");
        } else {
            assert!(!path.exists(), "observation must not create a run lock");
        }
        assert!(!success);
        assert!(text.contains(": type force plan.1 human-gate -> implement"), "{text}");
        assert!(text.contains("operator confirmation did not match"), "{text}");
    }
}

/// Marker images may contain invalid plan text. Confirmation still derives from
/// the validated marker, never from ordinary plan parsing. §FS-rhei-recover.3
#[test]
fn issue_757_existing_recovery_confirmation_does_not_parse_plan() {
    let fixture = recovery_fixture("issue-757-in-doubt-plan");
    let marker_path = fixture.force.dir.join(".rhei/forced-recovery.json");
    let mut marker: serde_json::Value =
        serde_json::from_slice(&fs::read(&marker_path).unwrap()).unwrap();
    let invalid = "This is not a Rhei plan.\n";
    marker["files"][0]["before"]["bytes"] = encode_base64url(invalid.as_bytes()).into();
    fs::write(&fixture.force.plan, invalid).unwrap();
    fs::write(&marker_path, format!("{}\n", serde_json::to_string(&marker).unwrap())).unwrap();
    let before = artifact_snapshot(&fixture.force.dir, &fixture.force.plan);
    let (success, text) = attended_command(
        rhei_command(fixture.force.dir.join("attended-home")),
        &["recover", fixture.force.dir.to_str().unwrap()],
        Some("\r\n"),
        || {},
    );
    assert!(!success);
    assert!(text.contains(&format!(": type {}", recovery_confirmation("rollback"))), "{text}");
    assert!(text.contains("operator confirmation did not match"), "{text}");
    assert_artifacts_unchanged(&before);
}
