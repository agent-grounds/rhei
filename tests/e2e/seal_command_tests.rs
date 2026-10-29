//! `rhei seal` and the record it writes: generations that append and keep the
//! earlier bytes, a versioned header, a refusal while a run is live, and the
//! lifecycle across a full and a narrowed reset. §FS-rhei-seal

use std::fmt::Write as _;
use std::fs;
use std::process::Stdio;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::strict_inputs_support::*;
use super::*;

fn one_ticket(prefix: &str) -> Drift {
    lay_drift(prefix, &[&ticket_chain(1)], &MachineShape::default())
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

fn entry<'a>(record: &'a serde_json::Value, suffix: &str) -> &'a serde_json::Value {
    record["entries"]
        .as_array()
        .and_then(|entries| {
            entries.iter().find(|e| e["path"].as_str().is_some_and(|p| p.ends_with(suffix)))
        })
        .unwrap_or_else(|| panic!("no entry ending in {suffix}: {record:#}"))
}

// §FS-rhei-seal.3.1 §FS-rhei-seal.5
#[test]
#[ignore = "red until #521: there is no `rhei seal`; implement removes this"]
fn rhei_seal_appends_a_generation_and_keeps_the_earlier_one_byte_identical() {
    let drift = one_ticket("seal-append");
    let first = drift.seal();
    assert_success(&first);
    assert!(first.stdout.contains("generation 1"), "stdout:\n{}", first.stdout);
    let before = fs::read(drift.generation(1)).expect("generation 1 written");

    let script = drift.ws.join("scripts/step.py");
    let text = fs::read_to_string(&script).unwrap();
    fs::write(&script, text.replace("MARK = 'laid'", "MARK = 'edited'")).unwrap();
    let second = drift.seal();
    assert_success(&second);
    for expected in ["generation 2", "generation 1 kept", "modified", "scripts/step.py"] {
        assert!(second.stdout.contains(expected), "`{expected}` in:\n{}", second.stdout);
    }
    assert_eq!(fs::read(drift.generation(1)).unwrap(), before, "generation 1 is never rewritten");
    let latest = read_json(&drift.generation(2));
    assert_eq!(
        entry(&latest, "scripts/step.py")["sha256"],
        sha256_hex(&fs::read(&script).unwrap())
    );
}

/// The header names the generation, the rhei, what took it and its build, and
/// the record holds digests of the files a run reads — and none of the `laid`
/// fence's facts. §DA-run-input-seal.3
// §FS-rhei-seal.3.2 §FS-rhei-seal.3.3
#[test]
#[ignore = "red until #521: there is no seal record; implement removes this"]
fn a_generation_carries_its_header_and_validates_against_its_schema() {
    let drift = one_ticket("seal-header");
    assert_success(&drift.seal());
    let record = read_json(&drift.generation(1));

    let schema_path = repo_root().join("crates/rhei-cli/schemas/rhei.seal.v1.schema.json");
    let mut schema = read_json(&schema_path);
    schema.as_object_mut().unwrap().remove("$id");
    let compiled = jsonschema::JSONSchema::compile(&schema).expect("schema compiles");
    if let Err(errors) = compiled.validate(&record) {
        let messages: Vec<String> = errors.map(|error| error.to_string()).collect();
        panic!("generation 1 does not validate:\n{}\n{record:#}", messages.join("\n"));
    }
    assert_eq!(record["schema"], "rhei.seal.v1");
    assert_eq!(record["generation"], 1);
    assert_eq!(record["rhei"], "drift");
    assert_eq!(record["taken_by"], "rhei seal");
    assert_eq!(record["run_id"], serde_json::Value::Null);
    assert_eq!(record["machine"], "states.yaml");
    for file in ["states.yaml", "scripts/step.py"] {
        let bytes = fs::read(drift.ws.join(file)).unwrap();
        assert_eq!(entry(&record, file)["path"], file, "a path under the base is relative");
        assert_eq!(entry(&record, file)["sha256"], sha256_hex(&bytes));
    }
    assert_eq!(entry(&record, ".config/rhei/settings.json")["kind"], "absent");
    let text = fs::read_to_string(drift.generation(1)).unwrap();
    for fence in ["src:sha256", "inputs:sha256", "laid"] {
        assert!(!text.contains(fence), "the seal holds none of the fence's facts: {text}");
    }
}

// §FS-rhei-seal.5
#[test]
#[ignore = "red until #521: there is no `rhei seal` to refuse a live run; implement removes this"]
fn rhei_seal_refuses_while_a_run_is_live_and_names_it() {
    let drift = one_ticket("seal-live");
    on_ticket(&drift.ws, 1, "write(ws / 'started', 'x')\nwait_for('release', 60.0)\n");
    let mut run = rhei_command(&drift.home)
        .current_dir(&drift.ws)
        .args(["run", &drift.ws.display().to_string(), "--no-tui"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run starts");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !drift.ws.join("started").exists() {
        if run.try_wait().unwrap().is_some() || Instant::now() > deadline {
            let _ = run.kill();
            panic!("the run never started its ticket: {:?}", run.wait_with_output());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let run_id = read_json(&drift.ws.join("runtime/run.json"))["id"].as_str().unwrap().to_string();

    let refused = drift.seal();
    write_fixture_file(&drift.ws, "release", "x");
    let finished = run.wait_with_output().expect("run finishes");
    assert_success(&CliRun::from(&finished));
    assert_eq!(refused.status.code(), Some(1), "stderr:\n{}", refused.stderr);
    assert!(
        refused.stderr.contains(&run_id),
        "the refusal names run {run_id}:\n{}",
        refused.stderr
    );
    assert!(!drift.ws.join("runtime/seal").exists(), "a refused seal writes nothing");
}

// §FS-rhei-seal.6 §FS-rhei-reset.2
#[test]
#[ignore = "red until #521: there is no seal for a reset to remove; implement removes this"]
fn a_full_reset_removes_the_seal() {
    let drift = one_ticket("seal-full-reset");
    assert_success(&drift.seal());
    assert!(drift.generation(1).is_file());
    assert_success(&drift.rhei(&["reset", &drift.ws.display().to_string(), "--yes"]));
    assert!(!drift.ws.join("runtime/seal").exists(), "a full reset removes the seal with runtime/");
}

/// A seal is keyed by rhei, not by ticket, so a narrowed reset keeps it, says
/// so, and the next generation is 2: numbers never restart. §FS-rhei-reset.2.1
// §FS-rhei-seal.6
#[test]
#[ignore = "red until #521: there is no seal for a narrowed reset to keep; implement removes this"]
fn a_narrowed_reset_keeps_the_seal_and_says_so() {
    let drift = one_ticket("seal-narrowed-reset");
    assert_success(&drift.seal());
    let before = fs::read(drift.generation(1)).expect("generation 1 written");
    let ws = drift.ws.display().to_string();
    let reset = drift.rhei(&["reset", &ws, "--rhei", "drift", "--yes"]);
    assert_success(&reset);
    assert!(
        reset.stdout.contains("runtime/seal"),
        "a narrowed reset says it kept the seal:\n{}",
        reset.stdout
    );
    assert_eq!(fs::read(drift.generation(1)).unwrap(), before);
    let next = drift.seal();
    assert_success(&next);
    assert!(next.stdout.contains("generation 2"), "{}", next.stdout);
}

/// Under the built-in machine there is no machine file and nothing beside one:
/// the seal records only the settings files, each candidate present or absent.
// §FS-rhei-seal.2.3 §FS-rhei-seal.2.2
#[test]
#[ignore = "red until #521: there is no seal under the built-in machine; implement removes this"]
fn a_rhei_under_the_built_in_machine_seals_only_settings() {
    let dir = unique_temp_dir("seal-built-in");
    let home = dir.join("home");
    write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Built in\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n",
    );
    for (sub, name) in [
        ("scripts", "helper.py"),
        ("prompt_templates", "p.md"),
        (".agent-grounds/rhei", "settings.json"),
    ] {
        fs::create_dir_all(dir.join(sub)).unwrap();
        write_fixture_file(&dir.join(sub), name, "{}\n");
    }
    fs::create_dir_all(home.join(".config/rhei")).unwrap();
    write_fixture_file(&home.join(".config/rhei"), "settings.json", "{}\n");

    assert_success(&rhei_at(&home, &dir, &["seal", "plan.rhei.md"]));
    let record = read_json(&generation(&dir, "plan", 1));
    assert_eq!(record["machine"], serde_json::Value::Null, "{record:#}");
    assert_eq!(record["directories"], serde_json::json!([]), "{record:#}");
    for path in record["entries"].as_array().unwrap().iter().filter_map(|e| e["path"].as_str()) {
        for unsealed in ["scripts/", "prompt_templates/", "states.yaml"] {
            assert!(!path.contains(unsealed), "{path} is not sealed under the built-in machine");
        }
    }
    assert_eq!(entry(&record, ".agent-grounds/rhei/settings.json")["kind"], "file");
    assert_eq!(entry(&record, ".agents/rhei/settings.json")["kind"], "absent");
    assert_eq!(entry(&record, ".config/rhei/settings.json")["kind"], "file");
}
