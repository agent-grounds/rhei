use std::fs;
use std::path::Path;

use super::*;

pub(super) fn run_snapshot_command(plan_path: &Path, machine_path: &Path, args: &[&str]) -> CliRun {
    let mut cmd = rhei_command(isolated_home_for(plan_path));
    cmd.arg("--state-machine").arg(machine_path).arg("snapshot");
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("rhei snapshot command should run");
    CliRun::from(&output)
}

pub(super) fn write_fake_snapshot_agent(dir: &Path) -> PathBuf {
    write_python_agent(
        dir,
        "fake-snapshot-agent.py",
        r#"session_dir = ''
resume_value = ''
interactive = 0
args = sys.argv[1:]
while args:
    flag = args.pop(0)
    if flag == '--interactive':
        interactive = 1
    elif flag == '--session-dir':
        session_dir = args.pop(0) if args else ''
    elif flag == '--resume':
        resume_value = args.pop(0) if args else ''
    elif flag in ('--prompt', '--model'):
        if args:
            args.pop(0)

runtime_root = pathlib.Path(env('RHEI_ROOT', '.')) / 'runtime'
append(
    runtime_root / 'fake-agent.log',
    'task={} state={} target={} resume={} parent={}\n'.format(
        env('RHEI_TASK_ID'),
        env('RHEI_STATE'),
        env('RHEI_TARGET_SLUG'),
        resume_value,
        env('RHEI_SNAPSHOT_PARENT_REF'),
    ),
)

# §FS-rhei-states.3.3: a state that can finish the ticket writes its result.
result('## Result\n\nFake agent finished {}.\n'.format(env('RHEI_STATE')))

if session_dir:
    session_id = '{}-{}-{}'.format(
        env('RHEI_TASK_ID'), env('RHEI_STATE'), env('RHEI_TARGET_SLUG', 'target')
    )
    write(
        pathlib.Path(session_dir) / (session_id + '.jsonl'),
        '{{"session":{{"provider":"{}","model":"{}"}}}}\n'
        '{{"role":"assistant","content":"{}","interactive":{}}}\n'.format(
            env('RHEI_MODEL_PROVIDER', 'acme'),
            env('RHEI_MODEL_NAME', 'model-a'),
            env('RHEI_STATE'),
            interactive,
        ),
    )
"#,
    )
}

/// The snapshot agent's settings, written under `root`. Which root that is
/// carries meaning in a Panta project, so callers say it rather than inherit it.
pub(super) fn write_fake_snapshot_settings(root: &Path, fake_agent: &Path) {
    let settings_dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create .agent-grounds/rhei");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "fake": {{
      "command": {},
      "prompt_flag": "--prompt",
      "model_flag": "--model",
      "timeout": "5s",
      "session": {{
        "resume": {{"flag": "--resume"}},
        "interactive": {{"args": ["--interactive"]}},
        "session_dir_flag": "--session-dir",
        "layout": {{"kind": "FlatById", "ext": "jsonl"}}
      }}
    }}
  }}
}}"#,
            fixture_command(fake_agent)
        ),
    )
    .expect("write settings");
}

#[test]
fn snapshot_cli_lists_shows_and_run_preloads_from_snapshot() {
    let dir = unique_temp_dir("snapshot-cli-smoke");
    let fake_agent = write_fake_snapshot_agent(&dir);
    write_fake_snapshot_settings(&dir, &fake_agent);

    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        r#"# Rhei: Snapshot CLI Smoke

## Tasks

### Task 1: Carry context
**State:** source
"#,
    );
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: snapshot-cli-smoke
version: 1
states:
  source:
    initial: true
    description: Produce a reusable snapshot
    target: fake:acme:model-a
    snapshot:
      emit:
        name: impl
        on: always
  review:
    description: Consume the implementation snapshot
    target: fake:acme:model-a
    snapshot:
      inherit:
        name: impl
        required: true
        select:
          state: source
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: review
  - from: review
    to: completed
"#,
    );

    let plan_arg = plan_path.to_string_lossy().to_string();
    let empty_list =
        run_snapshot_command(&plan_path, &machine_path, &["list", "--plan", &plan_arg]);
    assert_success(&empty_list);
    assert!(
        empty_list.stdout.contains("No snapshots found")
            && empty_list.stdout.contains("--produced-by all"),
        "expected empty snapshot list to guide the operator; got:\n{}",
        empty_list.stdout
    );
    let empty_json = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["list", "--plan", &plan_arg, "--format", "json"],
    );
    assert_success(&empty_json);
    assert_eq!(empty_json.stdout.trim(), "[]");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    let list = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["list", "--plan", &plan_arg, "--format", "json", "--produced-by", "all"],
    );
    assert_success(&list);
    let rows: serde_json::Value =
        serde_json::from_str(&list.stdout).expect("snapshot list json should parse");
    let rows = rows.as_array().expect("snapshot list should be an array");
    assert!(
        rows.iter().any(|row| {
            row["snapshot_name"] == "impl"
                && row["emitting_state"] == "source"
                && row["target_slug"] == "fake-acme-model-a"
                && row["current"] == true
        }),
        "expected current named source snapshot in list; got:\n{}",
        list.stdout
    );
    assert!(
        rows.iter()
            .any(|row| row["snapshot_name"] == "_state" && row["emitting_state"] == "review"),
        "expected auto-emitted review snapshot in list; got:\n{}",
        list.stdout
    );

    let snapshot_ref = "plan.1:impl:source@1:fake-acme-model-a/g1";
    let show = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["show", snapshot_ref, "--plan", &plan_arg],
    );
    assert_success(&show);
    assert!(
        show.stdout.contains("\"snapshot_name\": \"impl\"")
            && show.stdout.contains("\"emitting_state\": \"source\"")
            && show.stdout.contains("\"session_id\": \"plan.1-source-fake-acme-model-a\"")
            && show.stdout.contains("transcript preview:"),
        "expected snapshot show to print manifest and transcript preview; got:\n{}",
        show.stdout
    );

    fs::write(
        &plan_path,
        r#"# Rhei: Snapshot CLI Smoke

## Tasks

### Task 1: Carry context
**State:** review
"#,
    )
    .expect("rewind task to inherited state");
    let from_snapshot =
        run_cli("run", &plan_path, &machine_path, &["--no-tui", "--from-snapshot", snapshot_ref]);
    assert_success(&from_snapshot);
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    let agent_log = fs::read_to_string(dir.join("runtime/fake-agent.log")).expect("agent log");
    assert!(
        agent_log.contains("state=review")
            && agent_log.contains("resume=plan.1-source-fake-acme-model-a")
            && agent_log.contains("\"snapshot_name\":\"impl\""),
        "expected inherited run to preload the selected snapshot; got:\n{}",
        agent_log
    );

    let continued = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["continue", snapshot_ref, "--plan", &plan_arg],
    );
    assert_success(&continued);
    assert!(
        continued.stdout.contains("captured plan.1:impl:source@1:fake-acme-model-a/g2")
            && continued.stdout.contains("--produced-by operator"),
        "expected snapshot continue to capture an operator generation; got:\n{}",
        continued.stdout
    );
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    let operator = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["show", "plan.1:impl:source@1:fake-acme-model-a/g2", "--plan", &plan_arg],
    );
    assert_success(&operator);
    assert!(
        operator.stdout.contains("\"produced_by\": \"operator\"")
            && operator.stdout.contains("\"completion\": \"success\"")
            && operator.stdout.contains("\"parent_ref\"")
            && operator.stdout.contains("\"generation\": 1"),
        "expected operator generation to record success and source parent_ref; got:\n{}",
        operator.stdout
    );

    let after_continue = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["list", "--plan", &plan_arg, "--format", "json", "--produced-by", "all"],
    );
    assert_success(&after_continue);
    let rows: serde_json::Value =
        serde_json::from_str(&after_continue.stdout).expect("snapshot list json should parse");
    let rows = rows.as_array().expect("snapshot list should be an array");
    assert!(
        rows.iter().any(|row| {
            row["snapshot_name"] == "impl"
                && row["emitting_state"] == "source"
                && row["generation"] == 1
                && row["produced_by"] == "orchestrator"
                && row["current"] == true
        }) && rows.iter().any(|row| {
            row["snapshot_name"] == "impl"
                && row["emitting_state"] == "source"
                && row["generation"] == 2
                && row["produced_by"] == "operator"
                && row["current"] == false
        }),
        "expected operator generation to leave current on orchestrator generation; got:\n{}",
        after_continue.stdout
    );

    let no_capture = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["continue", snapshot_ref, "--plan", &plan_arg, "--no-capture"],
    );
    assert_success(&no_capture);
    assert!(
        no_capture.stdout.contains("without capture; no snapshot written"),
        "expected --no-capture to confirm no generation was written; got:\n{}",
        no_capture.stdout
    );
    let after_no_capture = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["list", "--plan", &plan_arg, "--format", "json", "--produced-by", "all"],
    );
    assert_success(&after_no_capture);
    let rows: serde_json::Value =
        serde_json::from_str(&after_no_capture.stdout).expect("snapshot list json should parse");
    let rows = rows.as_array().expect("snapshot list should be an array");
    assert!(
        !rows.iter().any(|row| {
            row["snapshot_name"] == "impl"
                && row["emitting_state"] == "source"
                && row["generation"] == 3
        }),
        "--no-capture must not write another generation; got:\n{}",
        after_no_capture.stdout
    );
}

/// One emitting state and one inheriting state, for a rhei that is a Directory
/// Workspace inside a Panta project. Declared by name so the rhei's own
/// `states.yaml` resolves it, because a project run has no single machine to
/// pass on the command line.
const PANTA_SNAPSHOT_MACHINE: &str = r#"name: snapshot-panta-roots
version: 1
states:
  source:
    initial: true
    description: Produce a reusable snapshot
    target: fake:acme:model-a
    snapshot:
      emit:
        name: impl
        on: always
  review:
    description: Consume the implementation snapshot
    target: fake:acme:model-a
    snapshot:
      inherit:
        name: impl
        select:
          state: source
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: review
  - from: review
    to: completed
"#;

/// Stand up a Panta project whose only rhei is a Directory Workspace, and run
/// it. The project root and the rhei execution root are different directories,
/// which is the whole point: everything a single-file layout conflates is split
/// here. Returns the temp tree, which has to stay bound, and the project root.
pub(super) fn run_panta_snapshot_project(
    prefix: &str,
    extra_args: &[&str],
) -> (TestDir, PathBuf, CliRun) {
    let dir = unique_temp_dir(prefix);
    let fake_agent = write_fake_snapshot_agent(&dir);
    let project = dir.join("proj");
    let rhei_root = project.join("work");
    fs::create_dir_all(rhei_root.join("tasks")).expect("create rhei tasks dir");
    write_fake_snapshot_settings(&project, &fake_agent);
    write_fixture_file(&project, "index.panta.md", "# Panta: Snapshot Roots\n");
    write_fixture_file(
        &rhei_root,
        "index.rhei.md",
        "# Rhei: Snapshot Roots\n\n**States:** snapshot-panta-roots\n",
    );
    write_fixture_file(&rhei_root, "states.yaml", PANTA_SNAPSHOT_MACHINE);
    // Two tickets, because `--parallel 2` only reaches the worker pool when a
    // pass has more than one ticket to schedule; with one it runs sequentially
    // and the parallel spawn's own root handling is never exercised.
    write_fixture_file(
        &rhei_root,
        "tasks/01-carry.md",
        "### Task 1: Carry context\n**State:** source\n",
    );
    write_fixture_file(
        &rhei_root,
        "tasks/02-carry.md",
        "### Task 2: Carry context again\n**State:** source\n",
    );

    let mut args = vec!["--no-tui"];
    args.extend_from_slice(extra_args);
    let run = run_cli_without_machine("run", &project, &args);
    (dir, project, run)
}

/// The inheriting spawn ran warm: no cold-start warning, the named snapshot
/// under the project cache, and the resume flag carrying the source session id.
fn assert_panta_snapshot_preload_was_warm(project: &Path, run: &CliRun) {
    assert_success(run);
    let output = format!("{}{}", run.stdout, run.stderr);
    assert!(
        !output.contains("no snapshot found for inherit"),
        "inherit must read the cache emit wrote; got:\n{output}"
    );
    assert!(
        project.join(".rhei/cache/snapshots/work.1/impl/source").is_dir(),
        "the named snapshot belongs to the project cache, not the rhei root"
    );
    let agent_log =
        fs::read_to_string(project.join("work/runtime/fake-agent.log")).expect("agent log");
    assert!(
        agent_log.contains(
            "task=work.1 state=review target=fake-acme-model-a \
             resume=work.1-source-fake-acme-model-a"
        ),
        "the inheriting spawn must resume the source session; got:\n{agent_log}"
    );
    assert!(
        project.join("work/runtime/snapshot-sessions").is_dir(),
        "the session dir stays under the owning rhei's execution root"
    );
    assert!(
        !project.join("runtime/snapshot-sessions").exists(),
        "the session dir must not follow the cache to the project root"
    );
}

/// In a Panta project the snapshot cache is the project's, so a state that
/// pairs `emit:` with `inherit:` preloads warm even though the rhei that owns
/// the ticket has its own execution root. Resolving the cache against that
/// execution root instead reads a directory nothing ever wrote to and every
/// visit runs cold.
// §FS-rhei-snapshots.7 §FS-rhei-snapshots.10.1
#[test]
fn snapshot_inherit_preloads_warm_across_project_and_rhei_roots() {
    let (_dir, project, run) = run_panta_snapshot_project("snapshot-panta-roots", &[]);
    assert_panta_snapshot_preload_was_warm(&project, &run);
}

/// The parallel spawn path resolves the same two roots as the sequential one.
/// It rebinds a local `workspace_root` to the task's execution root, so the two
/// call sites read identically and differ only in a Panta project.
// §FS-rhei-snapshots.7 §FS-rhei-snapshots.10.1
#[test]
fn parallel_snapshot_inherit_preloads_warm_across_project_and_rhei_roots() {
    let (_dir, project, run) =
        run_panta_snapshot_project("snapshot-panta-roots-parallel", &["--parallel", "2"]);
    assert_panta_snapshot_preload_was_warm(&project, &run);
}
