// agent-grounds/rhei#176: which root `rhei snapshot continue` resolves each of
// its two session artifacts against. Every fixture here is a Panta project
// whose only rhei is a Directory Workspace, because a single-file plan makes
// the project root and the owning rhei's execution root the same directory and
// nothing in this file can be seen at all.

use std::fs;
use std::path::{Path, PathBuf};

use super::snapshot_tests::run_panta_snapshot_project;
use super::*;

/// `rhei snapshot <args...>` against a Panta project. The project's rheis
/// declare their own machines, so there is no single `--state-machine` to pass,
/// and the plan arrives through `--plan` rather than positionally.
fn run_panta_snapshot_cli(home_parent: &Path, args: &[&str]) -> CliRun {
    let mut cmd = rhei_command(home_parent.join(".home"));
    cmd.arg("snapshot");
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("rhei snapshot command should run");
    CliRun::from(&output)
}

/// The entries directly under `path`, sorted, and empty for a directory that
/// does not exist — which is one of the two answers these tests read.
fn entry_names(path: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(path) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A snapshot session is one ticket's live agent transcript, so it is that
/// ticket's own runtime artifact and a continuation writes it under the
/// execution root of the rhei that owns the ticket, where `rhei run` already
/// puts it. The project root the command was given is where the *cache* lives,
/// which is a different artifact with a different owner.
///
/// The claim is asserted positively, after the session directories the
/// preceding `rhei run` wrote have been cleared, so only the continuation can
/// satisfy it: asserting the absence of a stray at the project root would also
/// pass if the continuation stopped creating a session directory at all, which
/// is not the fix.
// §FS-rhei-snapshots.7 §FS-rhei-snapshot-operations.1.5
#[test]
fn snapshot_continue_writes_its_session_under_the_owning_rhei_execution_root() {
    let (dir, project, run) = run_panta_snapshot_project("snapshot-continue-panta-roots", &[]);
    assert_success(&run);

    let owned = project.join("work/runtime/snapshot-sessions");
    fs::remove_dir_all(&owned).expect("clear the session dirs the run wrote");
    assert!(
        !project.join("runtime/snapshot-sessions").exists(),
        "precondition: nothing holds a session dir at the project root before continue"
    );

    let continued = run_panta_snapshot_cli(
        &dir,
        &[
            "continue",
            "work.1:impl:source@1:fake-acme-model-a",
            "--plan",
            &project.display().to_string(),
            "--no-capture",
        ],
    );
    assert_success(&continued);

    let owned_names = entry_names(&owned);
    let stray_names = entry_names(&project.join("runtime/snapshot-sessions"));
    assert!(
        owned_names.iter().any(|name| name.starts_with("work.1-")),
        "the continued session for work.1 must sit under the owning rhei's execution root, \
         <project>/work/runtime/snapshot-sessions, which holds {owned_names:?}. \
         What the project root holds instead: {stray_names:?}\nstdout:\n{}",
        continued.stdout
    );
    assert!(
        stray_names.is_empty(),
        "the session directory must not follow the cache to the project root; \
         <project>/runtime/snapshot-sessions holds {stray_names:?}"
    );
}

/// An agent whose sessions live in a fixed, per-working-directory store, the
/// way Claude Code's do: one transcript per session under
/// `<parent>/<dashed cwd>`, with the working directory it ran in recorded in
/// the transcript's own first record. The agent derives both from its own
/// `getcwd`, so the directory it writes into is the one it actually ran in and
/// no other, whatever root the command was given.
const CWD_DASHED_AGENT_BODY: &str = r#"
import json
import uuid

cwd = os.path.realpath(os.getcwd())
dashed = re.sub(r'[^A-Za-z0-9-]', '-', cwd)
session_id = str(uuid.uuid4())
header = json.dumps({
    'type': 'session_meta',
    'payload': {
        'session_id': session_id,
        'cwd': cwd,
        'model_provider': env('RHEI_MODEL_PROVIDER', 'acme'),
    },
})
filler = json.dumps({'type': 'response_item', 'payload': {'role': 'assistant'}})
turn = json.dumps({
    'type': 'turn_context',
    'payload': {'cwd': cwd, 'model': env('RHEI_MODEL_NAME', 'model-a')},
})
write(
    pathlib.Path(SESSIONS_PARENT) / dashed / (session_id + '.jsonl'),
    '\n'.join([header] + [filler] * 10 + [turn]) + '\n',
)
append(pathlib.Path(CWD_LOG), json.dumps({'cwd': cwd, 'argv': sys.argv[1:]}) + '\n')
result('## Result\n\nFake agent finished.\n')
"#;

/// A Panta project whose only rhei is a Directory Workspace and whose agent has
/// no `session_dir_flag`, so every spawn's transcript is found through the
/// layout's fixed `dir_template` instead of a directory rhei redirected it
/// into. Returns the temp tree, which has to stay bound, and the project root.
fn run_cwd_dashed_panta_project(prefix: &str) -> (TestDir, PathBuf, CliRun) {
    let dir = unique_temp_dir(prefix);
    let sessions_parent = dir.join("agent-sessions");
    let preamble = format!(
        "SESSIONS_PARENT = {}\nCWD_LOG = {}\n",
        serde_json::to_string(&sessions_parent.display().to_string()).expect("path json"),
        serde_json::to_string(&dir.join("agent-cwd.jsonl").display().to_string())
            .expect("path json"),
    );
    let agent =
        write_python_agent(&dir, "cwdfixed.py", &format!("{preamble}{CWD_DASHED_AGENT_BODY}"));

    let project = dir.join("proj");
    let rhei_root = project.join("work");
    fs::create_dir_all(rhei_root.join("tasks")).expect("create rhei tasks dir");

    let settings_dir = project.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create .agent-grounds/rhei");
    let dir_template = format!("{}/{{cwd_dashed}}", sessions_parent.display());
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "cwdfixed": {{
      "command": {command},
      "prompt_flag": "-p",
      "model_flag": "--model",
      "timeout": "20s",
      "session": {{
        "resume": {{"flag": "--resume"}},
        "interactive": {{"args": ["--interactive"]}},
        "layout": {{
          "kind": "FlatById",
          "dir_template": {dir_template},
          "ext": "jsonl",
          "confirm_cwd_path": ["payload", "cwd"]
        }}
      }}
    }}
  }}
}}"#,
            command = fixture_command(&agent),
            dir_template = serde_json::to_string(&dir_template).expect("template json"),
        ),
    )
    .expect("write settings");

    write_fixture_file(&project, "index.panta.md", "# Panta: Continue Roots\n");
    write_fixture_file(&rhei_root, "index.rhei.md", "# Rhei: Continue Roots\n\n");
    write_fixture_file(
        &rhei_root,
        "states.yaml",
        r#"name: snapshot-cwd-dashed-roots
version: 1
states:
  source:
    initial: true
    description: Produce a reusable snapshot
    target: cwdfixed:acme:model-a
    snapshot:
      emit:
        name: impl
        on: always
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: completed
"#,
    );
    write_fixture_file(
        &rhei_root,
        "tasks/01-carry.md",
        "### Task 1: Carry context\n**State:** source\n",
    );

    let run = run_cli_without_machine("run", &project, &["--no-tui"]);
    (dir, project, run)
}

/// Every spawn the fake agent recorded, oldest first.
fn recorded_spawns(dir: &Path) -> Vec<serde_json::Value> {
    let log = fs::read_to_string(dir.join("agent-cwd.jsonl")).expect("agent cwd log");
    log.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("cwd record json"))
        .collect()
}

/// The other half of the same function, and the half a one-argument correction
/// gets wrong. A continuation resolves two roots, not one: the session
/// directory belongs to the owning rhei's execution root, but the
/// fixed-location `dir_template` and the locator that reads it back belong to
/// the directory the continuation's own agent process runs in. Point those at
/// an execution root the agent never ran in and the capture hunts for a
/// transcript nothing wrote.
///
/// This is a regression guard rather than a reproduction: the single root the
/// continue path passes today is already the right one for this branch, so the
/// test passes before the fix and fails the moment the two roots are collapsed
/// back into one. `snapshot_continue_captures_without_a_session_dir_flag` does
/// not cover it — that fixture is single-file, with an absolute `dir_template`
/// and no cwd confirmation, so the two roots coincide there and either value
/// would satisfy it.
// §FS-rhei-snapshots.7 §FS-rhei-snapshots.9.1
#[test]
fn snapshot_continue_resolves_a_fixed_location_template_against_the_spawn_working_dir() {
    let (dir, project, run) = run_cwd_dashed_panta_project("snapshot-continue-cwd-dashed");
    assert_success(&run);

    let listed = run_panta_snapshot_cli(
        &dir,
        &["list", "--plan", &project.display().to_string(), "--format", "json"],
    );
    assert_success(&listed);
    let rows: serde_json::Value = serde_json::from_str(&listed.stdout).expect("snapshot list json");
    let row = rows
        .as_array()
        .expect("snapshot list should be an array")
        .iter()
        .find(|row| row["snapshot_name"] == "impl")
        .unwrap_or_else(|| panic!("the named snapshot the source state emitted; got:\n{rows:#}"));
    let reference = format!(
        "{}:impl:source@{}:{}/g{}",
        row["task_id"].as_str().expect("task id"),
        row["visit"].as_u64().unwrap_or(1),
        row["target_slug"].as_str().expect("target slug"),
        row["generation"].as_u64().expect("generation"),
    );

    let continued = run_panta_snapshot_cli(
        &dir,
        &["continue", &reference, "--plan", &project.display().to_string()],
    );
    assert_success(&continued);
    assert!(
        continued.stdout.contains("captured "),
        "the capture has to find the transcript in the directory the continuation's own agent \
         ran in, which is not the owning rhei's execution root; got:\nstdout:\n{}\nstderr:\n{}",
        continued.stdout,
        continued.stderr
    );

    // Without this the test could pass vacuously: if the continuation ever came
    // to run in the execution root, one root would serve both artifacts here
    // and the assertion above would no longer tell the two apart.
    let spawns = recorded_spawns(&dir);
    let interactive = spawns
        .iter()
        .find(|record| {
            record["argv"]
                .as_array()
                .is_some_and(|argv| argv.iter().any(|arg| arg == "--interactive"))
        })
        .unwrap_or_else(|| panic!("the interactive continuation spawn; got:\n{spawns:#?}"));
    let execution_root = fs::canonicalize(project.join("work")).expect("canonical execution root");
    assert_ne!(
        interactive["cwd"].as_str().expect("recorded cwd"),
        execution_root.display().to_string(),
        "this test only distinguishes the two roots while the continuation runs somewhere \
         other than the owning rhei's execution root"
    );
}
