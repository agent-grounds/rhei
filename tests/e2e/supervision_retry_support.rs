// The rhei.59 reproducer: one child finishes, the supervisor fails twice, and
// the next child waits on a brief only that held supervisor can write.
// §FS-rhei-supervision.3.5 §FS-rhei-retry.2

use super::*;

pub const PLAN: &str = r#"# Rhei: Exhausted supervising visit

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Supervise
**State:** supervising

#### Task 1.1: Work
**State:** work

#### Task 1.2: Review
**State:** review
"#;

pub const MACHINE: &str = r#"name: exhausted-supervising-visit
version: 1
states:
  supervising:
    initial: true
    description: Supervise the subtree
    execute_on: child-terminal
    agent: mock
    agent_timeout: 30s
    visits: 12
    attempts: 2
    instructions: You supervise Task {task_id}.
  work:
    description: Work that needs no brief
    agent: mock
    agent_timeout: 30s
    attempts: 2
    instructions: Do the work.
  review:
    description: Work only the supervisor can brief
    agent: mock
    agent_timeout: 30s
    inputs:
      - name: brief
        path: runtime/supervise/{task_id}.md
    instructions: Review as briefed.
  waiting:
    description: Poll
    agent: mock
    agent_timeout: 30s
    poll: { interval: 1s, max_attempts: 2 }
    instructions: Poll.
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervising, to: supervising, description: Released the subtree }
  - { from: work, to: completed, description: Worked }
  - { from: review, to: completed, description: Reviewed }
  - { from: waiting, to: waiting, description: Poll again }
  - { from: waiting, to: completed, description: Arrived }
  - { from: "*", to: cancelled, description: Dropped }
"#;

const AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
state = env('RHEI_STATE')
visit = env('RHEI_VISIT_COUNT', '1')
attempt = env('RHEI_ATTEMPT', '1')
append(root / 'spawns.log', '{} {} visit={} attempt={}\n'.format(task, state, visit, attempt))
if state == 'supervising':
    write(root / 'prompts' / ('{}-{}-{}.md'.format(task, visit, attempt)), agent_prompt())
    if visit == '1':
        sys.exit(0)
    if not (root / 'recover').exists():
        sys.exit(1)
    write(root / 'runtime' / 'supervise' / 'plan.1.2.md', 'Review the finished work.\n')
    if visit == '2':
        sys.exit(0)
result('Task {} finished {}.\n'.format(task, state))
"#;

pub fn fixture() -> (TestDir, PathBuf, PathBuf) {
    super::supervision_tests::setup_supervision_with_agent(
        "supervision-retry",
        PLAN,
        MACHINE,
        AGENT,
    )
}

pub fn cli(dir: &Path, verb: &str, task: &str, args: &[&str], worker: bool) -> CliRun {
    let mut cmd = rhei_command(dir.join("home"));
    // This fixture must model an operator, independent of its launching run.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("RHEI_") {
            cmd.env_remove(key);
        }
    }
    if worker {
        cmd.env("RHEI_TASK_ID", "plan.1");
    }
    cmd.current_dir(dir).args(["--state-machine", "states.yaml", verb]);
    if verb == "run" {
        cmd.arg("plan.rhei.md");
    } else {
        cmd.arg(task);
    }
    cmd.args(args);
    CliRun::from(&cmd.output().expect("fixture CLI"))
}

pub fn run(dir: &Path) -> CliRun {
    cli(dir, "run", "", &["--no-tui", "--no-callbacks", "--continue-on-error"], false)
}

pub fn text(out: &CliRun) -> String {
    format!("{}{}", out.stdout, out.stderr)
}

pub fn supervision(plan: &Path) -> serde_yaml::Value {
    let body = fs::read_to_string(plan).expect("plan");
    let frontmatter = body.split("---").nth(1).expect("frontmatter");
    let meta: serde_yaml::Value = serde_yaml::from_str(frontmatter).expect("metadata");
    let tasks = &meta["metadata"]["tasks"];
    let task = tasks
        .get("1")
        .or_else(|| tasks.get(serde_yaml::Value::Number(1.into())))
        .expect("supervisor metadata (numeric or string YAML key)");
    task["supervision"].clone()
}

pub fn record(dir: &Path) -> (PathBuf, serde_json::Value) {
    let path = fs::read_dir(dir.join("runtime/spawns"))
        .expect("spawn directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            let value: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(path).expect("record")).expect("JSON");
            value["task"] == "plan.1" && value["state"] == "supervising" && value["attempt"] == 2
        })
        .expect("exhausted supervisor record");
    let value = serde_json::from_str(&fs::read_to_string(&path).expect("record")).expect("JSON");
    (path, value)
}

pub fn exhaust(dir: &Path, plan: &Path) -> CliRun {
    let first = run(dir);
    assert!(!first.status.success(), "failed supervisor must halt: {}", text(&first));
    assert_eq!(
        fs::read_to_string(dir.join("spawns.log")).expect("spawns"),
        "plan.1 supervising visit=1 attempt=1\nplan.1.1 work visit=1 attempt=1\nplan.1 supervising visit=2 attempt=1\nplan.1 supervising visit=2 attempt=2\n"
    );
    let held = supervision(plan);
    assert_eq!(held["phase"].as_str(), Some("held"));
    assert_eq!(held["checkpoints"].as_sequence().expect("checkpoints").len(), 1);
    assert_eq!(held["checkpoints"][0]["task"].as_str(), Some("1.1"));
    let rerun = run(dir);
    assert!(!rerun.status.success());
    assert!(text(&rerun).contains("2 attempts spent"), "{}", text(&rerun));
    rerun
}

pub fn artifacts(dir: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn collect(root: &Path, dir: &Path, out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>) {
        if !dir.exists() {
            return;
        }
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(root, &path, out);
            } else {
                out.insert(path.strip_prefix(root).unwrap().to_path_buf(), fs::read(path).unwrap());
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    out.insert(PathBuf::from("plan.rhei.md"), fs::read(dir.join("plan.rhei.md")).unwrap());
    collect(dir, &dir.join("runtime"), &mut out);
    out
}
