//! The joint between per-invocation evidence and the seal: a strict run's
//! start records name the seal generation it checked, the manifest names that
//! generation's file, and `rhei verify` holds the file to its hash. A run that
//! is not strict names none. §FS-rhei-evidence.3.1 §FS-rhei-evidence.9.1
//! §FS-rhei-seal.8.2

use std::fs;
use std::path::Path;

use super::evidence_support::{
    assert_store_exists, assert_valid, evidence_workspace, only_manifest, records, sha256_hex,
    EvidenceWorkspace, Profile,
};
use super::run_cli;
use super::verify_findings_tests::finding;

/// A strict probe run to completion, with a callback on the routed edge so
/// every kind of start record is written.
fn strict_run(prefix: &str) -> EvidenceWorkspace {
    let ws = evidence_workspace(prefix, Profile::default());
    let callback =
        super::write_python_agent(&ws.root, "on-enter.py", "sys.stdin.read()\nsys.exit(0)\n");
    let machine = fs::read_to_string(&ws.machine).expect("machine");
    let routed = "  - { from: measure, to: work, description: Measured, exit_code: 0 }\n";
    assert!(machine.contains(routed), "fixture: the routed edge");
    let on_enter =
        serde_json::to_string(&format!("cli:{}", super::fixture_command_line(&callback)))
            .expect("callback YAML");
    fs::write(
        &ws.machine,
        machine.replace(
            routed,
            &format!(
                "  - from: measure\n    to: work\n    description: Measured\n    exit_code: 0\n    on_enter: {on_enter}\n"
            ),
        ),
    )
    .expect("write machine");
    let run = run_cli("run", &ws.plan, &ws.machine, &["--no-tui", "--strict-inputs"]);
    assert!(run.status.success(), "fixture strict run:\n{}\n{}", run.stdout, run.stderr);
    ws
}

/// Each start record's `seal`, by kind, sorted.
fn seals(evidence: &Path) -> Vec<(String, serde_json::Value)> {
    let mut found: Vec<(String, serde_json::Value)> = records(evidence, "start")
        .into_iter()
        .map(|record| (record["kind"].as_str().unwrap_or("").to_string(), record["seal"].clone()))
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// Under `--strict-inputs` every start record names generation 1 of the
/// rhei's seal, the one the run took and checked; the manifest names that
/// file with its hash; and verify passes. §FS-rhei-evidence.3.1
/// §FS-rhei-evidence.9.1 §FS-rhei-seal.8.1
#[test]
fn a_strict_runs_records_name_the_generation_it_checked() {
    let ws = strict_run("evidence-seal-strict");
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);
    let found = seals(&evidence);
    let kinds: Vec<&str> = found.iter().map(|(kind, _)| kind.as_str()).collect();
    assert_eq!(kinds, ["agent", "callback", "program"], "{found:#?}");
    let seal = found[0].1.clone();
    assert_eq!(seal["generation"], 1, "{seal:#}");
    let path = seal["path"].as_str().expect("the generation's path").to_string();
    let rhei = seal["rhei"].as_str().expect("rhei");
    assert_eq!(path, format!("runtime/seal/{rhei}/1.json"));
    for (kind, other) in &found {
        assert_eq!(other, &seal, "{kind} names the same checked generation");
    }
    for record in records(&evidence, "start") {
        assert_valid("rhei.evidence.invocation.v1", &record);
    }

    let (_, manifest) = only_manifest(&evidence);
    assert_valid("rhei.evidence.manifest.v1", &manifest);
    let generation = fs::read(ws.root.join(&path)).expect("the generation file");
    assert_eq!(
        manifest["seals"],
        serde_json::json!([{ "path": path, "sha256": sha256_hex(&generation) }])
    );
    let verified = ws.verify(&[]);
    assert!(verified.status.success(), "verify:\n{}\n{}", verified.stdout, verified.stderr);
}

/// A generation file is written once, so an edit to it is a mismatch.
/// §FS-rhei-evidence.9.1 §FS-rhei-verify.3
#[test]
fn an_edited_seal_generation_is_a_mismatch() {
    let ws = strict_run("evidence-seal-edited");
    let path = seals(&ws.evidence_dir())[0].1["path"].as_str().expect("path").to_string();
    let file = ws.root.join(&path);
    let recorded = sha256_hex(&fs::read(&file).expect("generation"));
    fs::write(&file, "{}\n").expect("edit the generation");

    let (result, json) = ws.verify_json(&[]);
    assert_eq!(result.status.code(), Some(1), "{json:#}");
    let mismatch = finding(&json, "mismatch").unwrap_or_else(|| panic!("a mismatch: {json:#}"));
    assert_eq!(mismatch["path"], path.as_str(), "{mismatch:#}");
    assert_eq!(mismatch["recorded_sha256"], recorded.as_str(), "{mismatch:#}");
    assert_eq!(mismatch["found_sha256"], sha256_hex(b"{}\n").as_str(), "{mismatch:#}");
}

/// A run that is not strict checks no seal, so every record's `seal` is
/// `null` and the manifest names none. §FS-rhei-evidence.3.1
#[test]
fn a_run_that_is_not_strict_names_no_seal() {
    let ws = evidence_workspace("evidence-seal-none", Profile::default());
    ws.run();
    let evidence = ws.evidence_dir();
    for (kind, seal) in seals(&evidence) {
        assert_eq!(seal, serde_json::Value::Null, "{kind}");
    }
    let (_, manifest) = only_manifest(&evidence);
    assert_eq!(manifest["seals"], serde_json::json!([]));
    assert!(!ws.root.join("runtime/seal").exists(), "fixture: nothing was sealed");
}
