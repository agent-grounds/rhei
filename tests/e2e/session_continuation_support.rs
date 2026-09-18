//! Shared fixtures for state-local continuation, governed by §FS-rhei-snapshots.4.7.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

pub(super) struct ContinuationFixture {
    pub dir: TestDir,
    pub plan: PathBuf,
    pub machine: PathBuf,
}

pub(super) fn continuation_agent(dir: &Path) -> PathBuf {
    write_python_agent(
        dir,
        "session-continuation-agent.py",
        r#"session_dir = ''
resume = ''
prompt = ''
args = sys.argv[1:]
while args:
    flag = args.pop(0)
    if flag == '--session-dir' and args:
        session_dir = args.pop(0)
    elif flag == '--resume' and args:
        resume = args.pop(0)
    elif flag in ('--prompt', '--model') and args:
        value = args.pop(0)
        if flag == '--prompt':
            prompt = value

root = pathlib.Path(env('RHEI_ROOT'))
runtime = root / 'runtime'
visit = env('RHEI_VISIT_COUNT', '1')
target = env('RHEI_TARGET_SLUG', 'target')
counter = runtime / ('attempt-{}-{}.txt'.format(visit, target))
attempt = int(counter.read_text() if counter.exists() else '0') + 1
write(counter, str(attempt))
session_id = '{}-{}-v{}-a{}'.format(env('RHEI_TASK_ID'), target, visit, attempt)
append(
    runtime / 'continuation-agent.log',
    'visit={} attempt={} target={} resume={} parent={}\n'.format(
        visit, attempt, target, resume, env('RHEI_SNAPSHOT_PARENT_REF')
    ),
)
write(runtime / 'prompts' / ('visit-{}-{}-{}.md'.format(visit, target, attempt)), prompt)

mode_path = runtime / 'agent-mode.txt'
mode = mode_path.read_text().strip() if mode_path.exists() else ''
result('## Result\n\nContinuation fixture completed visit {}.\n'.format(visit))

if session_dir:
    write(
        pathlib.Path(session_dir) / (session_id + '.jsonl'),
        '{{"session":{{"provider":"acme","model":"{}"}}}}\n'
        '{{"role":"assistant","content":"visit {} attempt {}"}}\n'.format(
            env('RHEI_MODEL_NAME', 'model-a'), visit, attempt
        ),
    )

if mode == 'fail-first-attempt' and attempt == 1:
    raise SystemExit(1)
if mode == 'fail-first-visit' and visit == '1':
    raise SystemExit(1)
"#,
    )
}

pub(super) fn continuation_settings(root: &Path, agent: &Path, session: &str) {
    let settings_dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    let session_field = match session {
        "supported" => {
            r#",
      "session": {
        "resume": {"flag": "--resume"},
        "session_dir_flag": "--session-dir",
        "layout": {"kind": "FlatById", "ext": "jsonl"}
      }"#
        }
        "emit-only" => {
            r#",
      "session": {
        "resume": "none",
        "session_dir_flag": "--session-dir",
        "layout": {"kind": "FlatById", "ext": "jsonl"}
      }"#
        }
        "none" => "",
        other => panic!("unknown session fixture {other}"),
    };
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "fake": {{
      "command": {},
      "prompt_flag": "--prompt",
      "model_flag": "--model",
      "timeout": "5s"{}
    }}
  }}
}}"#,
            fixture_command(agent),
            session_field
        ),
    )
    .expect("write settings");
}

pub(super) fn continuation_fixture(
    prefix: &str,
    visits: u32,
    attempts: u32,
    session_profile: &str,
    state_extra: &str,
    transitions: &str,
) -> ContinuationFixture {
    let dir = unique_temp_dir(prefix);
    let agent = continuation_agent(&dir);
    continuation_settings(&dir, &agent, session_profile);
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Session Continuation\n\n## Tasks\n\n### Task 1: Repeat\n**State:** loop\n",
    );
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &format!(
            r#"name: session-continuation
version: 1
states:
  loop:
    initial: true
    description: Revisit the same agent state
    target: fake:acme:model-a
    visits: {visits}
    attempts: {attempts}
    session: continue
{state_extra}  completed:
    description: Done
    final: true
transitions:
{transitions}"#
        ),
    );
    ContinuationFixture { dir, plan, machine }
}

pub(super) fn simple_transitions(visits: u32) -> String {
    format!(
        "  - from: loop\n    to: completed\n    condition: visitCount >= {visits}\n  - from: loop\n    to: loop\n"
    )
}

pub(super) fn run_continuation(fixture: &ContinuationFixture, extra: &[&str]) -> CliRun {
    let mut args = vec!["--no-tui", "--no-callbacks"];
    args.extend_from_slice(extra);
    run_cli("run", &fixture.plan, &fixture.machine, &args)
}

pub(super) fn continuation_log(fixture: &ContinuationFixture) -> String {
    fs::read_to_string(fixture.dir.join("runtime/continuation-agent.log"))
        .expect("continuation agent log")
}

pub(super) fn manifest(
    fixture: &ContinuationFixture,
    visit: u32,
    target: &str,
    name: &str,
) -> serde_json::Value {
    let path = fixture.dir.join(format!(
        ".rhei/cache/snapshots/plan.1/{name}/loop/{visit}/{target}/g1/manifest.json"
    ));
    serde_json::from_str(
        &fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read snapshot manifest {}: {err}", path.display())),
    )
    .expect("parse snapshot manifest")
}
