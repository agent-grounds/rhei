// `rhei note` from where its writers actually are: inside an agent `rhei run`
// spawned, with the environment that agent really has and the command its own
// prompt tells it to run.
//
// Its own part beside `note_command_tests.rs`: those call the verb directly with
// `RHEI_TASK_ID` set by hand, the environment a program state gives it. No agent
// has that variable, so none of them can say whether an agent's note lands.
//
// Both cases are committed red and `#[ignore]`d, so the spec commit passes with
// them failing under `--ignored`. The fix for #480 removes both `#[ignore]`s.

// §FS-rhei-note.1

use std::fs;

use super::note_store_support::isolated_command;
use super::*;

/// The fact the agent leaves, in place of the trail line's `<fact>`.
const FACT: &str = "The fixture needs cargo on PATH; build under ~/ag/tmp.";

/// One task, in an agent state, in a single-file plan named `n` — so its
/// qualified id is `n.1`, the id the record must carry.
const PLAN: &str = "# Rhei: Note probe\n\n## Tasks\n\n### Task 1: Probe the note default\n\
                    **State:** review\n";

const MACHINE: &str = r#"name: note-agent-probe
version: 1
states:
  review:
    initial: true
    agent: mock
    instructions: |
      Leave one note for later tickets, the way Leaving a trail says to.
  human-review:
    gating: true
  completed:
    final: true
transitions:
  - from: review
    to: human-review
  - from: human-review
    to: completed
"#;

/// The stub agent: `agent.py <rhei-binary>`, prompt on stdin.
///
/// It records which task-id variables it can see, then runs the bare
/// `rhei note "<fact>"` and, after it, the exact command its prompt's
/// `Leaving a trail` line prints, with `<fact>` filled in and `rhei` taken to
/// be the binary under test. Every outcome lands in `evidence.json`; the stub
/// itself exits 0 so the run reaches the gate either way.
const AGENT: &str = r#"import json
import shlex
import subprocess

rhei = sys.argv[1]
fact = sys.argv[2]
prompt = agent_prompt()
cwd = pathlib.Path.cwd()


def note(args):
    done = subprocess.run(
        [rhei, 'note', *args],
        cwd=cwd,
        text=True,
        capture_output=True,
        stdin=subprocess.DEVNULL,
    )
    return {'args': args, 'code': done.returncode, 'stdout': done.stdout, 'stderr': done.stderr}


evidence = {
    'env': {name: os.environ.get(name) for name in ('RHEI_TASK_ID', 'RHEI_TASK_ID_LOCAL')},
    'bare': note(['A bare note.']),
}
trail = next(
    (line for line in prompt.splitlines() if line.startswith('- You may leave **one** note')),
    None,
)
evidence['trail'] = trail
command = re.search(r'`(rhei note [^`]*)`', trail or '')
evidence['command'] = command.group(1) if command else None
if command:
    words = shlex.split(command.group(1))
    evidence['printed'] = note([fact if word == '<fact>' else word for word in words[2:]])
write(cwd / 'evidence.json', json.dumps(evidence, indent=2, sort_keys=True) + '\n')
"#;

/// Lay the plan, the machine and the stub, run `rhei run` over them once, and
/// return what the agent recorded.
fn run_agent_fixture(prefix: &str) -> (TestDir, serde_json::Value) {
    let dir = unique_temp_dir(prefix);
    let root = dir.to_path_buf();
    write_fixture_file(&root, "n.rhei.md", PLAN);
    write_fixture_file(&root, "states.yaml", MACHINE);
    let agent = write_python_agent(&root, "agent.py", AGENT);
    let command = serde_json::to_string(&vec![
        python_command().to_string(),
        agent.display().to_string(),
        rhei_binary().display().to_string(),
        FACT.to_string(),
    ])
    .expect("serialize the stub agent's command");
    let settings_dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "60s" }},
  "agents": {{ "mock": {{ "command": {command}, "stdin_prompt": true, "timeout": "60s" }} }}
}}"#
        ),
    )
    .expect("write settings");

    let output = isolated_command(&root)
        .current_dir(&root)
        .env("GIT_CEILING_DIRECTORIES", root.parent().expect("fixture has a temporary parent"))
        .args(["--state-machine", "states.yaml", "run", "n.rhei.md", "--no-tui", "--no-callbacks"])
        .output()
        .expect("rhei run should run");
    let run = CliRun::from(&output);
    assert_success(&run);

    let path = root.join("evidence.json");
    let evidence = fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "the stub agent never ran ({}: {err}):\nstdout:\n{}\nstderr:\n{}",
            path.display(),
            run.stdout,
            run.stderr
        )
    });
    (dir, serde_json::from_str(&evidence).expect("evidence is JSON"))
}

/// §FS-rhei-note.1 §FS-rhei-memory.3.4: an agent that does what its prompt says
/// leaves its note, recorded as its own qualified task — and it does so with no
/// task-id variable in its environment, because §FS-rhei-agents.4 still removes
/// both. Exporting `RHEI_TASK_ID` to the agent again is not a way to pass this.
#[test]
#[ignore = "red until #480 prints the writing task in the trail line; implement removes this"]
fn the_note_command_an_agents_prompt_prints_records_its_note() {
    let (dir, evidence) = run_agent_fixture("note-agent-trail");

    assert_eq!(
        evidence["env"],
        serde_json::json!({ "RHEI_TASK_ID": null, "RHEI_TASK_ID_LOCAL": null }),
        "an agent must not see either task-id variable (§FS-rhei-agents.4)"
    );

    let command = evidence["command"].as_str().unwrap_or_else(|| {
        panic!("the prompt printed no `rhei note` command; trail line: {}", evidence["trail"])
    });
    let printed = &evidence["printed"];
    assert!(
        printed["code"] == 0
            && printed["stdout"].as_str().is_some_and(|out| out.contains("noted as n.1")),
        "inside the agent, the trail line's `{command}` must record the note as n.1; it exited {} \
         with\nstdout: {}\nstderr: {}",
        printed["code"],
        printed["stdout"],
        printed["stderr"]
    );
    let store = fs::read_to_string(dir.join("runtime/notes.md")).unwrap_or_default();
    assert_eq!(store, format!("- [n.1] {FACT}\n"), "one record, under the qualified id");
}

/// §FS-rhei-note.1 §FS-rhei-note.5: the bare form still has no writing task
/// inside an agent and is still usage, exit 2 — but nothing the agent is shown
/// says `rhei run` exports the variable, since the agent is under `rhei run`
/// already. The message and the `--task` help send it to `--task` instead.
#[test]
#[ignore = "red until #480 stops promising RHEI_TASK_ID to agents; implement removes this"]
fn a_bare_note_inside_an_agent_is_refused_without_promising_the_variable() {
    let (dir, evidence) = run_agent_fixture("note-agent-bare");

    let bare = &evidence["bare"];
    let stderr = bare["stderr"].as_str().unwrap_or_default();
    assert_eq!(bare["code"], 2, "usage, not a refusal; stderr:\n{stderr}");
    assert!(stderr.contains("--task"), "the refusal must name `--task`; got:\n{stderr}");
    assert!(
        !stderr.contains("exports") && !stderr.contains("under `rhei run`"),
        "the refusal still tells an agent of `rhei run` that `rhei run` exports \
         RHEI_TASK_ID; got:\n{stderr}"
    );

    let help = isolated_command(&dir).args(["note", "--help"]).output().expect("rhei note --help");
    let help = CliRun::from(&help);
    assert_success(&help);
    assert!(help.stdout.contains("--task"), "got:\n{}", help.stdout);
    assert!(
        !help.stdout.contains("exports"),
        "`rhei note --help` still says `rhei run` exports RHEI_TASK_ID; got:\n{}",
        help.stdout
    );
}
