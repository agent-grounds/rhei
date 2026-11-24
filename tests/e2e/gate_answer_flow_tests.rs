//! What an answer leads to: an answer no route takes, an edge the transition
//! path refuses after the answer was written, and the states after a gate,
//! which read the answer as the implicit `gate-answer` input.
//! §FS-rhei-gate-brief.5 §FS-rhei-gate-brief.7.1 §FS-rhei-gate-brief.7.3

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::gate_answer_tests::{answer_json, gate_schema};
use super::gate_support::*;
use super::*;

/// `choose` routes `a` to an agent state, `b` to a state whose `on_enter`
/// refuses, and `c` nowhere. `record` is a program that saves what it was
/// given. §FS-rhei-gate-brief.1
const PICK_MACHINE: &str = r#"name: gate-answer-flow
version: 1
states:
  todo:
    initial: true
  choose:
    gating: true
    description: A person picks
    gate:
      questions:
        - id: pick
          kind: choice
          prompt: Which one?
          options:
            - { value: a, label: Option A }
            - { value: b }
            - { value: c }
      routes:
        - { to: apply, when: { pick: a } }
        - { to: guarded, when: { pick: b } }
  apply:
    description: Apply the ruling
    agent: fake
    instructions: |
      Apply the ruling in {gate_answer}.

      {if input.gate-answer.exists}
      The ruling is on file.
      {endif}
  record:
    description: Record what the program is given
    program:
      command: PROGRAM
    program_timeout: 30s
  guarded:
    description: Entered only past a callback that refuses
  done:
    final: true
    description: Done
transitions:
  - { from: todo, to: choose }
  - { from: choose, to: apply }
  - { from: choose, to: guarded, on_enter: CALLBACK }
  - { from: apply, to: record }
  - { from: record, to: done, exit_code: 0 }
  - { from: guarded, to: done }
"#;

const PICK_PLAN: &str = r#"# Rhei: Pick

## Tasks

### Task 1: Picked at a gate
**State:** choose

### Task 2: Never at a gate
**State:** apply
"#;

/// A Python string literal naming `path`, for a fixture script.
fn py_path(path: &Path) -> String {
    format!("pathlib.Path({:?})", path.display().to_string())
}

fn pick_fixture(prefix: &str) -> GateFixture {
    let dir = unique_temp_dir(prefix);
    let root: PathBuf = dir.to_path_buf();
    // On entering `guarded`, note whether the answer file is already there, then refuse.
    let callback = write_python_agent(
        &root,
        "refuse.py",
        &format!(
            "import json\nseen = {}.exists()\nwrite({}, 'present' if seen else 'absent')\n\
             sys.stdout.write(json.dumps({{'success': False, 'error': 'not today'}}))\n",
            py_path(&root.join("runtime/gates/pick.1/choose.1.answer.json")),
            py_path(&root.join("seen.txt")),
        ),
    );
    let program = write_python_agent(
        &root,
        "record.py",
        "import json\nkeys = ['RHEI_INPUT_GATE_ANSWER_EXISTS', 'RHEI_INPUT_GATE_ANSWER_PATH']\n\
         seen = {key: os.environ.get(key) for key in keys}\n\
         write(pathlib.Path(os.environ['RHEI_PLAN_PATH']).parent / 'runtime' / (os.environ['RHEI_TASK_ID'] + '-env.json'), json.dumps(seen))\n\
         result('## Result\\n\\nRecorded.\\n')\n",
    );
    let agent = write_python_agent(
        &root,
        "agent.py",
        "prompt = sys.stdin.read()\n\
         write(pathlib.Path(env('RHEI_ROOT')) / 'runtime' / (env('RHEI_TASK_ID') + '-prompt.txt'), prompt)\n\
         result('## Result\\n\\nApplied.\\n')\n",
    );
    let machine = PICK_MACHINE.replace("PROGRAM", &fixture_command(&program)).replace(
        "CALLBACK",
        &serde_json::to_string(&format!("cli:{}", fixture_command_line(&callback)))
            .expect("callback"),
    );
    let command: Value = serde_json::from_str(&fixture_command(&agent)).expect("command");
    let settings = json!({ "agents": { "fake": { "command": command, "stdin_prompt": true, "timeout": "30s" } } });
    let fx = GateFixture { dir, home: root.join(".home") };
    fx.write("pick.rhei.md", PICK_PLAN);
    fx.write("states.yaml", &machine);
    fx.write(".agent-grounds/rhei/settings.json", &settings.to_string());
    fx
}

fn pick(fx: &GateFixture, value: &str) -> (CliRun, Value) {
    let digest = fx.gate_json(&["show", "pick.1"])["digest"].clone();
    let sent = json!({ "version": 1, "request_digest": digest, "answers": { "pick": value } });
    answer_json(fx, "pick.1", "choose", &sent, &[])
}

/// An answer that no route takes is refused before anything is written.
/// §FS-rhei-gate-brief.5
#[test]
fn an_answer_no_route_takes_is_unrouted() {
    let fx = pick_fixture("gate-answer-unrouted");
    let (run, outcome) = pick(&fx, "c");
    assert_eq!(run.status.code(), Some(1));
    assert_eq!(
        outcome["errors"],
        json!([{ "kind": "unrouted", "question": null, "message": outcome["errors"][0]["message"] }])
    );
    assert_matches_schema(&gate_schema(&fx, "result"), &outcome, "an unrouted answer");
    // The answers held, so they are rendered; only the destination is unknown. §FS-rhei-gate-cmd.3.2
    assert_eq!((&outcome["to"], &outcome["route"]), (&Value::Null, &Value::Null));
    assert_eq!(
        outcome["result"],
        "Which one?: c\nAnswer: runtime/gates/pick.1/choose.1.answer.json"
    );
    assert!(!fx.root().join("runtime/gates").exists());
    assert_eq!(fx.state_of("pick.1"), "choose");
}

/// The answer is written before the state, so an `on_enter` callback reads it;
/// when that callback refuses and the state is rolled back, the answer goes
/// too. §FS-rhei-gate-brief.7.1 §FS-rhei-gate-brief.6.2
#[test]
fn a_refused_transition_leaves_no_answer_file() {
    let fx = pick_fixture("gate-answer-refused");
    let (run, outcome) = pick(&fx, "b");
    assert_eq!(run.status.code(), Some(1), "{}", run.stderr);
    assert_eq!(outcome["errors"][0]["kind"], "refused", "{outcome:#}");
    assert!(
        outcome["errors"][0]["message"].as_str().is_some_and(|text| text.contains("not today")),
        "{outcome:#}"
    );
    assert_eq!((&outcome["to"], &outcome["route"]["index"]), (&json!("guarded"), &json!(1)));
    assert_eq!(
        fs::read_to_string(fx.root().join("seen.txt")).expect("the callback ran"),
        "present"
    );
    assert!(!fx.root().join("runtime/gates").exists(), "no answer file and no directory for one");
    assert_eq!(fx.state_of("pick.1"), "choose");
}

/// The states after a gate read the answer without declaring it: the agent's
/// `{gate_answer}` and `{if input.gate-answer.exists}`, a handoff section
/// naming the answer, and the program's environment. A ticket that never met
/// a gate sees none of it. §FS-rhei-gate-brief.7.3 §FS-rhei-gate-brief.7.3.1
#[test]
fn the_states_after_a_gate_read_the_answer_as_an_implicit_input() {
    let fx = pick_fixture("gate-answer-successor");
    let (run, _) = pick(&fx, "a");
    assert_success(&run);
    assert_success(&fx.rhei(&["run", "pick.rhei.md", "--no-tui"]));
    assert_eq!(fx.state_of("pick.1"), "done");
    assert_eq!(fx.state_of("pick.2"), "done");

    let path = "runtime/gates/pick.1/choose.1.answer.json";
    let prompt =
        fs::read_to_string(fx.root().join("runtime/pick.1-prompt.txt")).expect("pick.1 prompt");
    assert!(prompt.contains(&format!("Apply the ruling in {path}.")), "{prompt}");
    assert!(prompt.contains("The ruling is on file."), "{prompt}");
    assert!(prompt.contains("## Gate Answer from choose"), "{prompt}");
    assert!(prompt.contains("A person answered the `choose` gate (visit 1)"), "{prompt}");
    assert!(prompt.contains("\"pick\": \"a\""), "the answers, as data:\n{prompt}");

    let untouched =
        fs::read_to_string(fx.root().join("runtime/pick.2-prompt.txt")).expect("pick.2 prompt");
    assert!(
        untouched.contains("Apply the ruling in {gate_answer}."),
        "left as written:\n{untouched}"
    );
    assert!(
        !untouched.contains("The ruling is on file.") && !untouched.contains("## Gate Answer"),
        "{untouched}"
    );

    let env = |ticket: &str| -> Value {
        let text = fs::read_to_string(fx.root().join(format!("runtime/{ticket}-env.json")))
            .expect("program env");
        serde_json::from_str(&text).expect("env JSON")
    };
    assert_eq!(
        env("pick.1"),
        json!({ "RHEI_INPUT_GATE_ANSWER_EXISTS": "true", "RHEI_INPUT_GATE_ANSWER_PATH": path })
    );
    assert_eq!(
        env("pick.2"),
        json!({ "RHEI_INPUT_GATE_ANSWER_EXISTS": "false", "RHEI_INPUT_GATE_ANSWER_PATH": null })
    );
}
