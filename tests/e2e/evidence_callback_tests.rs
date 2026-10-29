//! What a callback leaves as evidence, and when it leaves none: a callback the
//! run fires is recorded under its own `callback_id` with the invocation it
//! ran inside, its firing id is never written, and a transition outside any
//! live run — an operator's at a shell, or an agent's on another plan —
//! writes nothing. §FS-rhei-evidence.2.2 §FS-rhei-transitions.1.2

use std::fs;
use std::path::{Path, PathBuf};

use super::evidence_support::{
    assert_nowhere_under, assert_store_exists, assert_valid, end_for, every_file,
    evidence_workspace, one_start, records, EvidenceWorkspace, Profile,
};
use super::{fixture_command_line, rhei_binary, run_cli, write_fixture_file, write_python_agent};

/// A callback that saves the context it was handed on stdin beside itself, as
/// `<name>.stdin.json`, and accepts the move.
const SAVING_CALLBACK: &str = r#"data = sys.stdin.read()
write(str(pathlib.Path(__file__).with_suffix('.stdin.json')), data)
sys.exit(0)
"#;

/// `cli:<python> <script>` as a YAML scalar.
fn callback_yaml(script: &Path) -> String {
    serde_json::to_string(&format!("cli:{}", fixture_command_line(script)))
        .expect("callback should serialize")
}

/// Write the saving callback as `name` in `dir`; returns it and where it saves.
fn saving_callback(dir: &Path, name: &str) -> (PathBuf, PathBuf) {
    let script = write_python_agent(dir, name, SAVING_CALLBACK);
    let saved = script.with_extension("stdin.json");
    (script, saved)
}

/// Replace `old` in the workspace's machine, failing when it is not there.
fn edit_machine(ws: &EvidenceWorkspace, old: &str, new: &str) {
    let machine = fs::read_to_string(&ws.machine).expect("machine");
    assert!(machine.contains(old), "fixture: the machine has no {old:?}:\n{machine}");
    fs::write(&ws.machine, machine.replacen(old, new, 1)).expect("write machine");
}

/// The firing id the callback saw, from the context it saved.
fn firing_id(saved: &Path) -> String {
    let text = fs::read_to_string(saved).expect("fixture: the callback ran and saved its stdin");
    let context: serde_json::Value = serde_json::from_str(&text).expect("callback context JSON");
    context["transition"]["firingId"]
        .as_str()
        .unwrap_or_else(|| panic!("fixture: no transition.firingId in {context:#}"))
        .to_string()
}

/// The program's exit routes `measure -> work`, and that edge's `on_enter`
/// runs inside the run: its record names the program as its owner, carries
/// argv and cwd and an end record, and the firing id is nowhere in the store.
/// §FS-rhei-evidence.2.2 §FS-rhei-evidence.3.1 §FS-rhei-evidence.4
#[test]
fn a_callback_the_run_fires_records_its_owner_and_never_its_firing_id() {
    let ws = evidence_workspace("evidence-callback-run", Profile::default());
    let (script, saved) = saving_callback(&ws.root, "on-enter.py");
    edit_machine(
        &ws,
        "  - { from: measure, to: work, description: Measured, exit_code: 0 }\n",
        &format!(
            "  - from: measure\n    to: work\n    description: Measured\n    exit_code: 0\n    on_enter: {}\n",
            callback_yaml(&script)
        ),
    );
    let run = run_cli("run", &ws.plan, &ws.machine, &["--no-tui"]);
    assert!(run.status.success(), "fixture run:\n{}\n{}", run.stdout, run.stderr);
    let firing = firing_id(&saved);

    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);
    let program = one_start(&evidence, "program");
    let callback = one_start(&evidence, "callback");
    assert_valid("rhei.evidence.invocation.v1", &callback);
    assert!(callback["callback_id"].as_str().is_some_and(|id| !id.is_empty()), "{callback:#}");
    assert_eq!(callback["owner_invocation_ids"], serde_json::json!([program["invocation_id"]]));
    assert_eq!(callback["from"], "measure");
    assert_eq!(callback["to"], "work");
    assert_eq!(callback["callback_phase"], "on_enter");
    assert!(callback["argv"].as_array().is_some_and(|argv| !argv.is_empty()), "{callback:#}");
    let cwd = callback["cwd"].as_str().expect("cwd");
    assert!(Path::new(cwd).is_absolute(), "cwd is absolute: {cwd}");
    let end = end_for(&evidence, &callback);
    assert_eq!(end["exit_code"], 0);
    assert!(end.get("log").is_none_or(serde_json::Value::is_null), "a callback has no log");

    assert_nowhere_under(&evidence, &firing, "the firing id");
}

/// After a run parks the ticket at a gate, an operator moves it on from a
/// shell: the edge's callback fires with no live run, and the store gains
/// nothing. §FS-rhei-evidence.2.2 §FS-rhei-evidence.1.3
#[test]
fn a_callback_an_operator_fires_outside_a_run_writes_nothing() {
    let ws = evidence_workspace("evidence-callback-shell", Profile::default());
    let (script, saved) = saving_callback(&ws.root, "on-release.py");
    edit_machine(
        &ws,
        "  done:\n",
        "  parked:\n    description: Waiting on a person\n    gating: true\n  done:\n",
    );
    edit_machine(
        &ws,
        "  - { from: work, to: done, description: Worked }\n",
        &format!(
            "  - {{ from: work, to: parked, description: Worked }}\n  - from: parked\n    to: done\n    description: Released\n    on_enter: {}\n",
            callback_yaml(&script)
        ),
    );
    let run = run_cli("run", &ws.plan, &ws.machine, &["--no-tui"]);
    let plan = fs::read_to_string(&ws.plan).expect("plan");
    assert!(
        plan.contains("**State:** parked"),
        "fixture: the run parks the ticket:\n{}",
        run.stderr
    );

    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);
    let before = every_file(&evidence);

    let moved = run_cli(
        "transition",
        &ws.plan,
        &ws.machine,
        &["--task", "1", "--from", "parked", "--to", "done", "--result", "Released by hand."],
    );
    assert!(moved.status.success(), "fixture transition:\n{}\n{}", moved.stdout, moved.stderr);
    assert!(saved.is_file(), "fixture: the operator's callback ran");
    assert_eq!(every_file(&evidence), before, "no live run, so no evidence");
}

/// A second, independent plan with a gate whose `on_enter` saves its stdin.
fn second_plan(dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    fs::create_dir_all(dir).expect("second plan dir");
    let (script, saved) = saving_callback(dir, "second-on-enter.py");
    let plan = write_fixture_file(
        dir,
        "plan.rhei.md",
        "# Rhei: Second\n\n## Tasks\n\n### Task 1: Elsewhere\n**State:** gate\n",
    );
    let machine = write_fixture_file(
        dir,
        "states.yaml",
        &format!(
            "name: second\nversion: 1\nstates:\n  gate:\n    initial: true\n    description: Gate\n    gating: true\n  opening:\n    description: Opened\n    gating: true\n  done:\n    final: true\n    description: Done\ntransitions:\n  - from: gate\n    to: opening\n    description: Open\n    on_enter: {}\n  - from: opening\n    to: done\n    description: Finish\n",
            callback_yaml(&script)
        ),
    );
    (plan, machine, saved)
}

/// The stub agent that, before it finishes, runs `rhei transition` on the
/// second plan through the argv in `nested.json`, recording its exit.
const NESTED_AGENT: &str = r#"import json
import subprocess
here = pathlib.Path(__file__).resolve().parent
argv = json.loads((here / 'nested.json').read_text(encoding='utf-8'))
nested = subprocess.run(argv, stdin=subprocess.DEVNULL, capture_output=True, text=True)
write(str(here / 'nested-exit.txt'), str(nested.returncode) + '\n' + nested.stdout + nested.stderr)
print('stub agent ran', flush=True)
result('## Result\n\nStub agent finished.\n')
"#;

/// An agent's `rhei transition` on another plan finds no live run there, so
/// neither its own run's store nor the other plan gains a callback record.
/// §FS-rhei-evidence.2.2 §FS-rhei-agents.4
#[test]
fn an_agents_transition_on_a_second_plan_writes_into_neither_store() {
    let ws = evidence_workspace("evidence-callback-nested", Profile::default());
    let (second, second_machine, saved) = second_plan(&ws.root.join("second"));
    write_python_agent(&ws.root, "stub-agent.py", NESTED_AGENT);
    let argv = serde_json::json!([
        rhei_binary().display().to_string(),
        "--state-machine",
        second_machine.display().to_string(),
        "transition",
        second.display().to_string(),
        "--task",
        "1",
        "--from",
        "gate",
        "--to",
        "opening"
    ]);
    fs::write(ws.root.join("nested.json"), argv.to_string()).expect("nested argv");
    ws.run();
    let nested = fs::read_to_string(ws.root.join("nested-exit.txt")).expect("fixture: agent ran");
    assert!(nested.starts_with("0\n"), "fixture: the nested transition succeeded:\n{nested}");
    assert!(saved.is_file(), "fixture: the second plan's callback ran");

    assert!(
        !ws.root.join("second/runtime/evidence").exists(),
        "the second plan has no live run, so no evidence store"
    );
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);
    let kinds: Vec<String> = records(&evidence, "start")
        .iter()
        .map(|record| record["kind"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(kinds.iter().filter(|kind| *kind == "callback").count(), 0, "kinds: {kinds:?}");
    let mut sorted = kinds.clone();
    sorted.sort();
    assert_eq!(sorted, ["agent", "program"], "only the outer run's own processes");
}
