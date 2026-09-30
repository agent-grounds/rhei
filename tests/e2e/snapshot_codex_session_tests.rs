// agent-grounds/rhei#146: the session contract as a codex-shaped agent needs it —
// a nested, `cwd`-confirmed transcript locator, a resume emitted before the
// stdin separator, and a capture-enabled `rhei snapshot continue` that does
// not require a `session_dir_flag`.

use std::fs;
use std::path::Path;

use super::*;

/// The body every fake agent in this file shares. It writes a transcript
/// wherever the profile under test expects one, then records the argument
/// vector it was actually spawned with — which is the thing under test, so it
/// is recorded verbatim rather than parsed.
///
/// `ARGV_LOG`, `NESTED_ROOT`, `FLAT_DIR` and `READ_STDIN` are baked in above
/// this body by [`write_argv_recording_agent`]; a `{` in here would be a
/// `format!` placeholder, so nothing in this string is formatted.
const ARGV_AGENT_BODY: &str = r#"
import json
import uuid

stdin_text = ''
if READ_STDIN:
    stdin_text = sys.stdin.read()

session_id = str(uuid.uuid4())
cwd = os.getcwd()
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
transcript = '\n'.join([header] + [filler] * 10 + [turn]) + '\n'

if NESTED_ROOT:
    day = pathlib.Path(NESTED_ROOT) / '2026' / '09' / '01'
    write(day / ('rollout-2026-09-01T00-19-57-' + session_id + '.jsonl'), transcript)
if FLAT_DIR:
    write(pathlib.Path(FLAT_DIR) / (session_id + '.jsonl'), transcript)
session_dir = env('RHEI_SNAPSHOT_SESSION_DIR')
if session_dir:
    write(pathlib.Path(session_dir) / (session_id + '.jsonl'), transcript)

append(pathlib.Path(ARGV_LOG), json.dumps({
    'state': env('RHEI_STATE'),
    'session': session_id,
    'stdin': stdin_text,
    'result_path': env('RHEI_RESULT_PATH'),
    'argv': sys.argv[1:],
}) + '\n')

result('## Result\n\nFake agent finished.\n')
"#;

fn write_argv_recording_agent(
    dir: &Path,
    name: &str,
    nested_root: Option<&Path>,
    flat_dir: Option<&Path>,
    read_stdin: bool,
) -> PathBuf {
    fn quoted(path: Option<&Path>) -> String {
        serde_json::to_string(&path.map(|p| p.display().to_string()).unwrap_or_default())
            .expect("path json")
    }
    let preamble = format!(
        "ARGV_LOG = {}\nNESTED_ROOT = {}\nFLAT_DIR = {}\nREAD_STDIN = {}\n",
        quoted(Some(&dir.join("agent-argv.jsonl"))),
        quoted(nested_root),
        quoted(flat_dir),
        u8::from(read_stdin),
    );
    write_python_agent(dir, name, &format!("{preamble}{ARGV_AGENT_BODY}"))
}

/// Every invocation the fake agent recorded, oldest first.
fn recorded_invocations(dir: &Path) -> Vec<serde_json::Value> {
    let log = fs::read_to_string(dir.join("agent-argv.jsonl")).expect("agent argv log");
    log.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("argv record json"))
        .collect()
}

fn argv_of(record: &serde_json::Value) -> Vec<String> {
    record["argv"]
        .as_array()
        .expect("argv array")
        .iter()
        .map(|value| value.as_str().expect("argv string").to_string())
        .collect()
}

fn index_of(argv: &[String], needle: &str) -> usize {
    argv.iter()
        .position(|arg| arg == needle)
        .unwrap_or_else(|| panic!("expected {needle} in the assembled command; got: {argv:?}"))
}

/// `rhei snapshot <args...>`. The subcommand takes its plan through `--plan`,
/// so it cannot go through [`run_cli`], which passes one positionally.
fn run_snapshot_cli(plan_path: &Path, machine_path: &Path, args: &[&str]) -> CliRun {
    let mut cmd = rhei_command(plan_path.parent().expect("plan parent").join(".home"));
    cmd.arg("--state-machine").arg(machine_path).arg("snapshot").args(args);
    let output = cmd.output().expect("rhei snapshot command should run");
    CliRun::from(&output)
}

fn write_settings(root: &Path, body: &str) {
    let settings_dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create .agent-grounds/rhei");
    fs::write(settings_dir.join("settings.json"), body).expect("write settings");
}

const TWO_STATE_PLAN: &str = r#"# Rhei: Codex Session

## Tasks

### Task 1: Carry context
**State:** source
"#;

/// The reproduction the ticket opens with, inverted: a state machine whose
/// states target codex and declare both halves of the snapshot contract. Today
/// `rhei validate` rejects it with the two `unsupported-snapshot-session`
/// errors the built-in profile's empty `session` block produces.
// §FS-rhei-snapshots.9.2 §FS-rhei-snapshots.9.3.4
#[test]
fn codex_target_validates_snapshot_emit_and_required_inherit() {
    let dir = unique_temp_dir("snapshot-codex-validate");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", TWO_STATE_PLAN);
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: codex-snapshot
version: 1
states:
  source:
    initial: true
    description: Produce a reusable snapshot
    target: codex[yolo]:openai:gpt-5.6-luna
    agent_timeout: 30m
    snapshot:
      emit:
        name: impl
        on: always
  review:
    description: Consume the implementation snapshot
    target: codex[yolo]:openai:gpt-5.6-luna
    agent_timeout: 30m
    snapshot:
      emit:
        name: reviewed
        on: always
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

    let validated = run_cli("validate", &plan_path, &machine_path, &[]);
    assert!(
        !format!("{}{}", validated.stdout, validated.stderr)
            .contains("unsupported-snapshot-session"),
        "a codex state machine must validate with snapshots declared; got:\n{}\n{}",
        validated.stdout,
        validated.stderr
    );
    assert_success(&validated);
}

/// A profile shaped like codex: the prompt on stdin behind a `--`, a resume
/// spelled as a positional subcommand, and one transcript per session under a
/// date-partitioned root the agent derives for itself.
fn write_codexish_settings(root: &Path, agent: &Path, sessions_root: &Path) {
    write_settings(
        root,
        &format!(
            r#"{{
  "agents": {{
    "codexish": {{
      "command": {},
      "model_flag": "--model",
      "stdin_prompt": true,
      "timeout": "20s",
      "modes": {{"yolo": ["--sandbox", "danger-full-access"]}},
      "session": {{
        "resume": {{"flag": "resume"}},
        "layout": {{
          "kind": "FlatById",
          "dir_template": {},
          "ext": "jsonl",
          "nested": true,
          "id_from_stem": "trailing_uuid",
          "confirm_cwd_path": ["payload", "cwd"]
        }}
      }}
    }}
  }}
}}"#,
            fixture_command(agent),
            serde_json::to_string(&sessions_root.display().to_string()).expect("json"),
        ),
    );
}

/// The ordering claim, asserted on the argument vector the child was spawned
/// with rather than on a description of it: past the `--` a subcommand is
/// prompt text, so `resume <session-id>` has to arrive before it — and the
/// prompt still has to reach stdin. The session id it carries is the bare
/// UUID out of the `rollout-<stamp>-<uuid>` stem, which is the only value
/// `codex exec resume` takes back.
// §FS-rhei-snapshots.9.1.1 §FS-rhei-snapshots.10.1
#[test]
fn stdin_prompt_agent_resumes_before_the_prompt_separator() {
    let dir = unique_temp_dir("snapshot-codex-resume-order");
    let sessions_root = dir.join("agent-sessions");
    let agent = write_argv_recording_agent(&dir, "codexish.py", Some(&sessions_root), None, true);
    write_codexish_settings(&dir, &agent, &sessions_root);

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", TWO_STATE_PLAN);
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: codexish-snapshot
version: 1
states:
  source:
    initial: true
    description: Produce a reusable snapshot
    target: codexish[yolo]:acme:model-a
    snapshot:
      emit:
        name: impl
        on: always
  review:
    description: Consume the implementation snapshot
    target: codexish[yolo]:acme:model-a
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

    let run = run_cli("run", &plan_path, &machine_path, &["--no-tui"]);
    assert_success(&run);

    let records = recorded_invocations(&dir);
    let source = records.iter().find(|r| r["state"] == "source").expect("source invocation");
    let review = records.iter().find(|r| r["state"] == "review").expect("review invocation");
    let session_id = source["session"].as_str().expect("source session id").to_string();

    let argv = argv_of(review);
    let resume = index_of(&argv, "resume");
    assert_eq!(
        argv.get(resume + 1).map(String::as_str),
        Some(session_id.as_str()),
        "the resume subcommand must carry the source session's bare uuid; got: {argv:?}"
    );
    assert!(
        !session_id.starts_with("rollout-"),
        "the session id must be the uuid, not the rollout stem: {session_id}"
    );
    assert!(
        resume < index_of(&argv, "--"),
        "resume must precede the stdin separator, or it is prompt text: {argv:?}"
    );
    assert!(
        resume > index_of(&argv, "--model"),
        "resume must follow the mode and model flags: {argv:?}"
    );
    // The rendered heading is project-qualified, `Task plan.1`, so the plan
    // file's own `Task 1` never appears in the delivered prompt.
    assert!(
        review["stdin"]
            .as_str()
            .is_some_and(|prompt| prompt.contains("Task plan.1: Carry context")),
        "the prompt must still arrive on stdin under the subcommand"
    );
}

/// The same insertion point seen from the other side. A profile with no
/// `stdin_prompt` has no `--`, so moving the snapshot flags to that point puts
/// them ahead of the skill flags instead of behind them. Pi is the only
/// built-in with a session block, and nothing asserted its tolerance of the
/// move before this.
// §FS-rhei-snapshots.10.1
#[test]
fn skill_flag_agent_takes_its_session_flags_before_its_skills() {
    let dir = unique_temp_dir("snapshot-session-flags-order");
    let agent = write_argv_recording_agent(&dir, "piish.py", None, None, false);
    let skill_dir = dir.join("skills/notes");
    fs::create_dir_all(&skill_dir).expect("skill bundle dir");
    fs::write(skill_dir.join("SKILL.md"), "# notes\n").expect("skill file");
    write_settings(
        &dir,
        &format!(
            r#"{{
  "agents": {{
    "piish": {{
      "command": {},
      "prompt_flag": "-p",
      "model_flag": "--model",
      "skill_flag": "--skill",
      "timeout": "20s",
      "session": {{
        "resume": {{"flag": "--continue"}},
        "fork": {{"flag": "--fork"}},
        "session_dir_flag": "--session-dir",
        "layout": {{"kind": "FlatById", "ext": "jsonl"}}
      }}
    }}
  }},
  "skills": {{"notes": {{"path": {}}}}}
}}"#,
            fixture_command(&agent),
            serde_json::to_string(&skill_dir.display().to_string()).expect("json"),
        ),
    );

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", TWO_STATE_PLAN);
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: piish-snapshot
version: 1
states:
  source:
    initial: true
    description: Produce a reusable snapshot
    target: piish:acme:model-a
    skills:
      - notes
    snapshot:
      emit:
        name: impl
        on: always
  review:
    description: Consume the implementation snapshot
    target: piish:acme:model-a
    skills:
      - notes
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

    let run = run_cli("run", &plan_path, &machine_path, &["--no-tui"]);
    assert_success(&run);

    let records = recorded_invocations(&dir);
    let review = records.iter().find(|r| r["state"] == "review").expect("review invocation");
    let argv = argv_of(review);
    let skill = index_of(&argv, "--skill");
    assert!(
        index_of(&argv, "--session-dir") < skill,
        "the session dir flag must precede the skill flags: {argv:?}"
    );
    assert!(
        index_of(&argv, "--fork") < skill,
        "the fork flag must precede the skill flags: {argv:?}"
    );
}

/// Gap 3. A profile that has no `session_dir_flag` still has a fixed location
/// emit can read, so a capture-enabled `rhei snapshot continue` has no reason
/// to refuse: whatever `snapshot.emit:` can capture for an agent, continue can
/// capture too. The interactive spawn also drops the profile's autonomous mode
/// flags, because `interactive.command` is a different command and the
/// operator is at the terminal to answer for themselves.
// §FS-rhei-snapshots.9.1 §FS-rhei-snapshots.9.3.4
#[test]
fn snapshot_continue_captures_without_a_session_dir_flag() {
    let dir = unique_temp_dir("snapshot-continue-fixed-location");
    let fixed_dir = dir.join("agent-sessions");
    let agent = write_argv_recording_agent(&dir, "flatfixed.py", None, Some(&fixed_dir), false);
    write_settings(
        &dir,
        &format!(
            r#"{{
  "agents": {{
    "flatfixed": {{
      "command": {command},
      "prompt_flag": "-p",
      "model_flag": "--model",
      "timeout": "20s",
      "modes": {{"yolo": ["--autonomous"]}},
      "session": {{
        "resume": {{"flag": "--resume"}},
        "interactive": {{"command": {command}}},
        "layout": {{
          "kind": "FlatById",
          "dir_template": {fixed},
          "ext": "jsonl"
        }}
      }}
    }}
  }}
}}"#,
            command = fixture_command(&agent),
            fixed = serde_json::to_string(&fixed_dir.display().to_string()).expect("json"),
        ),
    );

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", TWO_STATE_PLAN);
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: flatfixed-snapshot
version: 1
states:
  source:
    initial: true
    description: Produce a reusable snapshot
    target: flatfixed[yolo]:acme:model-a
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

    let run = run_cli("run", &plan_path, &machine_path, &["--no-tui"]);
    assert_success(&run);

    let plan_arg = plan_path.to_string_lossy().to_string();
    let listed = run_snapshot_cli(
        &plan_path,
        &machine_path,
        &["list", "--plan", &plan_arg, "--format", "json"],
    );
    assert_success(&listed);
    let rows: serde_json::Value = serde_json::from_str(&listed.stdout).expect("snapshot list json");
    let row = rows
        .as_array()
        .expect("array")
        .iter()
        .find(|row| row["snapshot_name"] == "impl")
        .expect("the named snapshot the source state emitted");
    let reference = format!(
        "{}:impl:source@{}:{}/g{}",
        row["task_id"].as_str().expect("task id"),
        row["visit"].as_u64().unwrap_or(1),
        row["target_slug"].as_str().expect("target slug"),
        row["generation"].as_u64().expect("generation"),
    );

    let continued =
        run_snapshot_cli(&plan_path, &machine_path, &["continue", &reference, "--plan", &plan_arg]);
    assert!(
        !continued.stderr.contains("without session_dir_flag"),
        "a fixed-location profile must not be refused capture; got:\n{}",
        continued.stderr
    );
    assert_success(&continued);
    assert!(
        continued.stdout.contains("captured "),
        "continue must write an operator generation; got:\n{}",
        continued.stdout
    );

    // Find the continuation by what this command emits — the resume flag —
    // rather than by an inherited variable the test does not control.
    let records = recorded_invocations(&dir);
    let interactive = records
        .iter()
        .find(|record| argv_of(record).windows(2).any(|pair| pair[0] == "--resume"))
        .expect("the interactive continuation invocation");
    let argv = argv_of(interactive);
    assert!(
        argv.windows(2).any(|pair| pair[0] == "--resume"),
        "the continuation must resume the source session: {argv:?}"
    );
    // `continue` clears the per-attempt output paths of whatever run launched
    // it, so it cannot write into that run's result file.
    // §FS-rhei-snapshot-operations.1.5
    assert_eq!(
        interactive["result_path"].as_str().unwrap_or_default(),
        "",
        "the continuation must not inherit a run's RHEI_RESULT_PATH: {:?}",
        interactive["result_path"]
    );
    assert!(
        !argv.iter().any(|arg| arg == "--autonomous"),
        "an interactive.command spawn does not inherit the autonomous mode flags: {argv:?}"
    );
}
