fn write_run_agent_settings(dir: &Path, settings: &str) {
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    fs::write(settings_dir.join("settings.json"), settings).expect("write settings");
}

fn canonical_path_text(path: &Path) -> String {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf()).display().to_string()
}

/// A path spelled the way the CLI renders one, for asserting against output.
///
/// [`canonical_path_text`] cannot serve: it compares two canonicalizations
/// against each other, where Windows' verbatim `\\?\` prefix cancels. Printed
/// output has that prefix taken back off, so this takes it off too.
// §REQ-cross-platform.5
fn rendered_path_text(path: &Path) -> String {
    rhei_core::platform::canonical_path(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

fn recorded_value<'a>(recorded: &'a str, prefix: &str) -> &'a str {
    recorded
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("recorded output missing {prefix:?}: {recorded}"))
}

fn assert_recorded_path_eq(recorded: &str, expected: &Path) {
    assert_eq!(canonical_path_text(Path::new(recorded)), canonical_path_text(expected));
}

fn collect_run_agent_snapshot_manifests(dir: &Path) -> Vec<serde_json::Value> {
    fn visit(path: &Path, manifests: &mut Vec<serde_json::Value>) {
        for entry in fs::read_dir(path).unwrap_or_else(|_| panic!("read dir {}", path.display())) {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                visit(&path, manifests);
            } else if path.file_name().and_then(|name| name.to_str()) == Some("manifest.json") {
                let text = fs::read_to_string(&path).expect("manifest text");
                manifests.push(serde_json::from_str(&text).expect("manifest json"));
            }
        }
    }

    let cache = dir.join(".rhei/cache/snapshots");
    if !cache.exists() {
        return Vec::new();
    }
    let mut manifests = Vec::new();
    visit(&cache, &mut manifests);
    manifests
}

fn assert_run_agent_snapshot(
    manifests: &[serde_json::Value],
    snapshot_name: &str,
    completion: &str,
) {
    assert!(
        manifests.iter().any(|manifest| {
            manifest.get("snapshot_name").and_then(serde_json::Value::as_str)
                == Some(snapshot_name)
                && manifest.get("completion").and_then(serde_json::Value::as_str)
                    == Some(completion)
        }),
        "expected snapshot {snapshot_name:?} with completion {completion:?}; manifests: {manifests:#?}"
    );
}

const CHECKOUT_ROOT_MACHINE: &str = r#"name: checkout-root-agent
version: 1
states:
  review:
    initial: true
    agent: fake
  completed:
    final: true
transitions:
  - from: review
    to: completed
"#;

const CHECKOUT_ROOT_PLAN: &str = r#"# Rhei: Checkout Root

## Tasks

### Task 1: Record checkout root
**State:** review
"#;

/// A fixture that records the directory it was spawned in and the roots it was
/// shown in its prompt, one `name=value` line at a time.
fn write_checkout_recording_script(dir: &Path) -> PathBuf {
    let body = r#"write(
    pathlib.Path(env('RHEI_ROOT')) / 'runtime' / 'checkout-root.txt',
    '{}\nrhei={}\ncheckout={}\nworktree={}\n'.format(
        os.getcwd(),
        env('RHEI_ROOT'),
        env('RHEI_CHECKOUT_ROOT'),
        env('RHEI_WORKTREE_ROOT'),
    ),
)
# §FS-rhei-states.3.3: a state that can finish the ticket writes its result.
result('## Result\n\nAgent finished {}.\n'.format(env('RHEI_STATE')))
"#;
    write_python_agent(dir, "record-checkout.py", body)
}

fn write_fake_agent_settings(dir: &Path, script_path: &Path) {
    let command = fixture_command(script_path);
    let settings = format!(
        r#"{{
  "agents": {{
    "fake": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s"
    }}
  }}
}}"#
    );
    write_run_agent_settings(dir, &settings);
}

fn write_stdin_fake_agent_settings(dir: &Path, script_path: &Path) {
    let command = fixture_command(script_path);
    let settings = format!(
        r#"{{
  "agents": {{
    "fake": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s"
    }}
  }}
}}"#
    );
    write_run_agent_settings(dir, &settings);
}

/// A fixture that writes the session file its `--session-dir` flag names, so a
/// snapshot of the invocation has something to capture. `tail` is appended
/// verbatim, which is how a scenario makes the fixture fail or hang.
fn write_session_recording_script(dir: &Path, tail: &str) -> PathBuf {
    // A spawn that never passed `--session-dir` is the bug these cases would
    // otherwise stop catching: with an empty `session_dir` the write lands on a
    // relative `session.jsonl` and the fixture exits 0, where the `sh` original
    // died on `mkdir -p ""` under `set -u`.
    let body = format!(
        r#"session_dir = ''
args = sys.argv[1:]
while args:
    if args.pop(0) == '--session-dir' and args:
        session_dir = args.pop(0)
if not session_dir:
    sys.exit('the agent was spawned without --session-dir')
write(pathlib.Path(session_dir) / 'session.jsonl', '{{"provider":"openai","model":"model"}}\n')
{tail}"#
    );
    write_python_agent(dir, "agent.py", &body)
}

fn write_session_fake_agent_settings(dir: &Path, script_path: &Path, timeout: &str) {
    let command = fixture_command(script_path);
    let settings = format!(
        r#"{{
  "agents": {{
    "fake": {{
      "command": {command},
      "timeout": "{timeout}",
      "session": {{
        "session_dir_flag": "--session-dir",
        "layout": {{ "kind": "FlatById", "ext": "jsonl" }}
      }}
    }}
  }}
}}"#
    );
    write_run_agent_settings(dir, &settings);
}

fn run_git(args: &[&str]) {
    let output = Command::new("git").args(args).output().expect("git should run");
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(args: &[&str]) -> String {
    let output = Command::new("git").args(args).output().expect("git should run");
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn init_git_repo(repo: &Path) {
    fs::create_dir_all(repo).expect("create repo");
    run_git(&["-C", repo.to_str().expect("repo path"), "init"]);
    run_git(&[
        "-C",
        repo.to_str().expect("repo path"),
        "config",
        "user.email",
        "rhei@example.test",
    ]);
    run_git(&["-C", repo.to_str().expect("repo path"), "config", "user.name", "Rhei Test"]);
    fs::write(repo.join("README.md"), "repo\n").expect("write readme");
    run_git(&["-C", repo.to_str().expect("repo path"), "add", "README.md"]);
    run_git(&["-C", repo.to_str().expect("repo path"), "commit", "-m", "initial"]);
}

#[test]
fn run_agent_uses_enclosing_git_root_as_checkout_root() {
    let root = unique_temp_dir("run-agent-git-checkout-root");
    let repo = root.join("repo");
    init_git_repo(&repo);
    fs::write(repo.join("AGENTS.md"), "root instructions\n").expect("write agents");

    let plan_dir = repo.join(".agents/scratchpad/review");
    fs::create_dir_all(&plan_dir).expect("create plan dir");
    let plan_path = write_fixture_file(&plan_dir, "plan.rhei.md", CHECKOUT_ROOT_PLAN);
    let machine_path = write_fixture_file(&plan_dir, "states.yaml", CHECKOUT_ROOT_MACHINE);
    let script_path = write_checkout_recording_script(&plan_dir);
    write_fake_agent_settings(&plan_dir, &script_path);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let recorded =
        fs::read_to_string(plan_dir.join("runtime/checkout-root.txt")).expect("read checkout log");
    // §FS-rhei-agents.4: repository-root checkout context lets agents discover root AGENTS.md.
    assert_recorded_path_eq(recorded.lines().next().expect("recorded cwd"), &repo);
    assert_recorded_path_eq(recorded_value(&recorded, "rhei="), &plan_dir);
    assert_recorded_path_eq(recorded_value(&recorded, "checkout="), &repo);
    assert!(plan_dir.join("runtime/logs/task-plan.1-review.log").exists());
}

#[test]
fn run_agent_fails_when_agent_commit_leaves_orchestrator_transition_uncommitted() {
    let root = unique_temp_dir("run-agent-commit-dirty-transition");
    let repo = root.join("repo");
    init_git_repo(&repo);

    let plan_dir = repo.join(".agents/scratchpad/review");
    fs::create_dir_all(&plan_dir).expect("create plan dir");
    let plan_path = write_fixture_file(&plan_dir, "plan.rhei.md", CHECKOUT_ROOT_PLAN);
    write_fixture_file(&plan_dir, "states.yaml", CHECKOUT_ROOT_MACHINE);
    let script = r#"import subprocess

write('agent-work.txt', 'agent work\n')
subprocess.check_call(['git', 'add', 'agent-work.txt'])
subprocess.check_call(['git', 'commit', '-m', 'agent work'])
# §FS-rhei-states.3.3: a state that can finish the ticket writes its result.
result('## Result\n\nAgent finished {}.\n'.format(env('RHEI_STATE')))
"#;
    let script_path = write_python_agent(&plan_dir, "commit-work.py", script);
    write_fake_agent_settings(&plan_dir, &script_path);
    run_git(&["-C", repo.to_str().expect("repo path"), "add", ".agents"]);
    run_git(&["-C", repo.to_str().expect("repo path"), "commit", "-m", "add rhei plan"]);

    let result = run_run_command_in_dir(
        &plan_dir,
        Path::new("plan.rhei.md"),
        Path::new("states.yaml"),
        &["--no-callbacks"],
    );

    // §FS-rhei-run.3.1: a subprocess commit followed by a run-owned transition must not be silent.
    assert!(
        !result.status.success(),
        "run should fail when HEAD moved and the plan transition is dirty\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("plan/result paths remain uncommitted"),
        "diagnostic should explain the durable-state mismatch; got:\n{}",
        result.stderr
    );
    assert!(
        result.stderr.contains("plan.rhei.md"),
        "diagnostic should name the dirty plan path; got:\n{}",
        result.stderr
    );

    let worktree_plan = fs::read_to_string(&plan_path).expect("read worktree plan");
    assert!(worktree_plan.contains("**State:** completed"), "{worktree_plan}");
    let head_plan = git_stdout(&[
        "-C",
        repo.to_str().expect("repo path"),
        "show",
        "HEAD:.agents/scratchpad/review/plan.rhei.md",
    ]);
    assert!(head_plan.contains("**State:** review"), "{head_plan}");
}

#[test]
fn run_agent_renders_artifact_paths_at_rhei_root_when_checkout_root_differs() {
    let root = unique_temp_dir("run-agent-artifact-path-checkout-root");
    let repo = root.join("repo");
    init_git_repo(&repo);

    let machine = r#"name: checkout-artifact-root
version: 1
states:
  review:
    initial: true
    agent: fake
    instructions: "ARTIFACT={output.report.path}"
    outputs:
      - name: report
        path: runtime/reports/{task_id}.md
  completed:
    final: true
transitions:
  - from: review
    to: completed
"#;
    let plan_dir = repo.join(".agents/scratchpad/review");
    fs::create_dir_all(&plan_dir).expect("create plan dir");
    let plan_path = write_fixture_file(&plan_dir, "plan.rhei.md", CHECKOUT_ROOT_PLAN);
    let machine_path = write_fixture_file(&plan_dir, "states.yaml", machine);
    let script = r#"path = ''
for line in sys.stdin.read().splitlines():
    if line.startswith('ARTIFACT='):
        path = line[len('ARTIFACT='):]
        break
write(path, 'done')
write(pathlib.Path(env('RHEI_ROOT')) / 'runtime' / 'rendered-artifact-path.txt', path + '\n')
# §FS-rhei-states.3.3: a state that can finish the ticket writes its result.
result('## Result\n\nAgent finished {}.\n'.format(env('RHEI_STATE')))
"#;
    let script_path = write_python_agent(&plan_dir, "write-rendered-artifact.py", script);
    write_stdin_fake_agent_settings(&plan_dir, &script_path);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        result.status.success(),
        "run should succeed when the agent writes the rendered artifact path\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let expected = plan_dir.join("runtime/reports/plan.1.md");
    assert!(expected.exists(), "required output should be written under the prompt root");
    let rendered = fs::read_to_string(plan_dir.join("runtime/rendered-artifact-path.txt"))
        .expect("read rendered path");
    // §FS-rhei-agents.4: artifact paths stay rooted at the prompt root when cwd is elsewhere.
    assert_recorded_path_eq(rendered.trim(), &expected);
    assert!(
        !repo.join("runtime/reports/plan.1.md").exists(),
        "agent should not write checkout-root runtime artifacts"
    );
}

#[test]
fn run_agent_falls_back_to_invocation_cwd_when_no_git_root_exists() {
    let root = unique_temp_dir("run-agent-cwd-checkout-root");
    let cwd = root.join("caller");
    let plan_dir = root.join("scratchpad");
    fs::create_dir_all(&cwd).expect("create cwd");
    fs::create_dir_all(&plan_dir).expect("create plan dir");
    let plan_path = write_fixture_file(&plan_dir, "plan.rhei.md", CHECKOUT_ROOT_PLAN);
    let machine_path = write_fixture_file(&plan_dir, "states.yaml", CHECKOUT_ROOT_MACHINE);
    let script_path = write_checkout_recording_script(&plan_dir);
    write_fake_agent_settings(&plan_dir, &script_path);

    let ceiling = root.parent().expect("fixture has a temporary parent").to_str().expect("ceiling path");
    let result = run_run_command_in_dir_with_env(
        &cwd,
        &plan_path,
        &machine_path,
        &["--no-callbacks"],
        &[("GIT_CEILING_DIRECTORIES", ceiling)],
    );

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let recorded =
        fs::read_to_string(plan_dir.join("runtime/checkout-root.txt")).expect("read checkout log");
    // §FS-rhei-agents.4: no-git runs use the operator's invocation cwd as checkout context.
    assert_recorded_path_eq(recorded.lines().next().expect("recorded cwd"), &cwd);
    assert_recorded_path_eq(recorded_value(&recorded, "rhei="), &plan_dir);
    assert_recorded_path_eq(recorded_value(&recorded, "checkout="), &cwd);
}

#[test]
fn run_agent_clears_inherited_worktree_env_without_task_worktree_ref() {
    let root = unique_temp_dir("run-agent-clear-stale-worktree-env");
    let plan_dir = root.join("scratchpad");
    fs::create_dir_all(&plan_dir).expect("create plan dir");
    let plan_path = write_fixture_file(&plan_dir, "plan.rhei.md", CHECKOUT_ROOT_PLAN);
    let machine_path = write_fixture_file(&plan_dir, "states.yaml", CHECKOUT_ROOT_MACHINE);
    let script_path = write_checkout_recording_script(&plan_dir);
    write_fake_agent_settings(&plan_dir, &script_path);

    let result = run_run_command_with_env(
        &plan_path,
        &machine_path,
        &["--no-callbacks"],
        &[("RHEI_WORKTREE_ROOT", "/stale/worktree")],
    );

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let recorded =
        fs::read_to_string(plan_dir.join("runtime/checkout-root.txt")).expect("read checkout log");
    // §FS-rhei-agents.4: RHEI_WORKTREE_ROOT is unset unless a task worktree ref applies.
    assert_eq!(recorded_value(&recorded, "worktree="), "", "{recorded}");
    assert!(!recorded.contains("/stale/worktree"), "{recorded}");
}

#[test]
fn run_agent_prefers_task_worktree_ref_over_repository_root() {
    let root = unique_temp_dir("run-agent-task-worktree-root");
    let repo = root.join("repo");
    init_git_repo(&repo);
    let worktree = root.join("worktrees/task-1");
    let worktree_parent = worktree.parent().expect("worktree parent");
    fs::create_dir_all(worktree_parent).expect("create worktree parent");
    run_git(&[
        "-C",
        repo.to_str().expect("repo path"),
        "worktree",
        "add",
        "-b",
        "rhei/task-1",
        worktree.to_str().expect("worktree path"),
    ]);

    let plan_dir = repo.join(".agents/scratchpad/review");
    fs::create_dir_all(plan_dir.join("runtime/worktree-refs")).expect("create worktree refs");
    let plan_path = write_fixture_file(&plan_dir, "plan.rhei.md", CHECKOUT_ROOT_PLAN);
    let machine_path = write_fixture_file(&plan_dir, "states.yaml", CHECKOUT_ROOT_MACHINE);
    fs::write(
        plan_dir.join("runtime/worktree-refs/plan.1.yaml"),
        format!("path: {}\nbranch: rhei/task-1\n", worktree.display()),
    )
    .expect("write worktree ref");
    let script_path = write_checkout_recording_script(&plan_dir);
    write_fake_agent_settings(&plan_dir, &script_path);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let recorded =
        fs::read_to_string(plan_dir.join("runtime/checkout-root.txt")).expect("read checkout log");
    // §FS-rhei-agents.4: per-task worktree refs override the enclosing repository root.
    assert_recorded_path_eq(recorded.lines().next().expect("recorded cwd"), &worktree);
    assert_recorded_path_eq(recorded_value(&recorded, "checkout="), &worktree);
    assert_recorded_path_eq(recorded_value(&recorded, "worktree="), &worktree);
}

#[test]
fn run_spawns_agent_state_without_outputs() {
    let machine = r#"name: no-output-agent
version: 1
states:
  review:
    initial: true
    agent: fake
  completed:
    final: true
transitions:
  - from: review
    to: completed
"#;
    let plan = r#"# Rhei: No Output Agent

## Tasks

### Task 1: Review without artifacts
**State:** review
"#;
    let script = r#"write(pathlib.Path('runtime') / 'agent-invoked.txt', 'invoked')
# §FS-rhei-states.3.3: a state that can finish the ticket writes its result.
result('## Result\n\nAgent finished {}.\n'.format(env('RHEI_STATE')))
"#;

    let dir = unique_temp_dir("run-agent-no-outputs");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_python_agent(&dir, "agent.py", script);
    write_fake_agent_settings(&dir, &script_path);

    let ceiling = dir.parent().expect("fixture has a temporary parent").to_str().expect("ceiling path");
    let result = run_run_command_with_env(
        &plan_path,
        &machine_path,
        &["--no-callbacks"],
        &[("GIT_CEILING_DIRECTORIES", ceiling)],
    );

    assert!(
        result.status.success(),
        "run should spawn the no-output agent and complete\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(dir.join("runtime/agent-invoked.txt").exists(), "agent command should run");
    assert!(
        result.stdout.contains("1 agent(s)"),
        "summary should count the spawned agent; got:\n{}",
        result.stdout
    );
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "completed");
}

#[test]
fn run_auto_advances_nested_agent_task_after_outputs_exist() {
    let machine = r#"name: nested-agent-output
version: 1
states:
  waiting:
    gating: true
  pending:
    initial: true
    agent: fake
    outputs:
      - name: report
        path: runtime/reports/{task_id}.md
  completed:
    final: true
transitions:
  - from: pending
    to: completed
  - from: waiting
    to: completed
"#;
    let plan = r#"# Rhei: Nested Agent Output

## Tasks

### Task 1: Parent
**State:** waiting

#### Task 1.1: Child agent work
**State:** pending
"#;
    let script = r#"write(pathlib.Path('runtime') / 'reports' / 'plan.1.1.md', 'done')
# §FS-rhei-states.3.3: a state that can finish the ticket writes its result.
result('## Result\n\nAgent finished {}.\n'.format(env('RHEI_STATE')))
"#;

    let dir = unique_temp_dir("run-nested-agent-output");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_python_agent(&dir, "agent.py", script);
    write_fake_agent_settings(&dir, &script_path);

    let ceiling = dir.parent().expect("fixture has a temporary parent").to_str().expect("ceiling path");
    let result = run_run_command_with_env(
        &plan_path,
        &machine_path,
        &["--no-callbacks"],
        &[("GIT_CEILING_DIRECTORIES", ceiling)],
    );

    assert!(
        result.status.success(),
        "run should spawn and auto-advance the nested agent task\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(dir.join("runtime/reports/plan.1.1.md").exists(), "agent should write required output");
    // §FS-rhei-run.3: Successful agent output on a child task still applies the selected transition.
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    assert_eq!(rhei.tasks[0].state.as_str(), "waiting");
    assert_eq!(rhei.tasks[0].children[0].state.as_str(), "completed");
}

#[test]
fn run_model_state_without_resolved_agent_fails_instead_of_callback_fallback() {
    let machine = r#"name: missing-model-agent
version: 1
models:
  - local
states:
  review:
    initial: true
    model: local
  completed:
    final: true
transitions:
  - from: review
    to: completed
"#;
    let plan = r#"# Rhei: Missing Model Agent

## Tasks

### Task 1: Review with model
**State:** review
"#;
    let settings = r#"{
  "models": {
    "local": {
      "provider": "test",
      "model": "local-model"
    }
  }
}"#;

    let dir = unique_temp_dir("run-model-missing-agent");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    write_run_agent_settings(&dir, settings);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        !result.status.success(),
        "run should fail when model work has no agent transport\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("no agent configured for model 'local'"),
        "missing-agent error should be reported; got:\n{}",
        result.stderr
    );
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "review");
}

#[test]
fn run_program_exit_zero_missing_outputs_fails_run_and_leaves_task_in_place() {
    let dir = unique_temp_dir("run-program-missing-output");
    // The program that does nothing but succeed: `true` is a shell builtin, and
    // a state machine names a command, not a shell.
    let program = write_python_agent(&dir, "true-program.py", "sys.exit(0)\n");
    let command = fixture_command(&program);
    let machine = format!(
        r#"name: program-missing-output
version: 1
states:
  build:
    initial: true
    program:
      command: {command}
    outputs:
      - name: bundle
        path: runtime/bundle.txt
  completed:
    final: true
transitions:
  - from: build
    to: completed
    exit_code: 0
"#
    );
    let plan = r#"# Rhei: Program Missing Output

## Tasks

### Task 1: Build
**State:** build
"#;

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        !result.status.success(),
        "run should fail when a successful program misses outputs\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("program exited 0 but required outputs are missing"),
        "missing-output warning should be shown; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    // §FS-rhei-agents.3.2.1: the warning names the resolved path, not just the
    // artifact name, so a stale or mis-templated path is visible on the first line.
    assert!(
        result.stderr.contains("bundle (runtime/bundle.txt)"),
        "missing-output warning should name the resolved path; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("non-terminal tasks remaining"),
        "run should report stalled non-terminal work; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "build");
}

#[test]
fn run_agent_exit_zero_missing_outputs_emits_failure_snapshots_and_fails_run() {
    let machine = r#"name: agent-missing-output-snapshot
version: 1
states:
  build:
    initial: true
    target: fake:openai:model
    outputs:
      - name: artifact
        path: runtime/artifact.txt
    snapshot:
      emit:
        name: failure
        on: failure
  completed:
    final: true
transitions:
  - from: build
    to: completed
"#;
    let plan = r#"# Rhei: Agent Missing Output Snapshot

## Tasks

### Task 1: Build
**State:** build
"#;

    let dir = unique_temp_dir("run-agent-missing-output-snapshot");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_session_recording_script(&dir, "");
    write_session_fake_agent_settings(&dir, &script_path, "5s");

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        !result.status.success(),
        "run should fail on missing outputs\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("agent exited 0 but required outputs are missing"),
        "missing-output warning should be shown; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    // §FS-rhei-agents.3.2.1: the warning names the resolved path.
    assert!(
        result.stderr.contains("artifact (runtime/artifact.txt)"),
        "missing-output warning should name the resolved path; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("non-terminal tasks remaining"),
        "run should report stalled non-terminal work; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let manifests = collect_run_agent_snapshot_manifests(&dir);
    assert_run_agent_snapshot(&manifests, "_state", "failure");
    assert_run_agent_snapshot(&manifests, "failure", "failure");
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "build");
}

#[test]
fn run_agent_exit_zero_without_transition_emits_always_snapshots() {
    let machine = r#"name: agent-no-transition-snapshot
version: 1
states:
  review:
    initial: true
    target: fake:openai:model
    snapshot:
      emit:
        name: always
        on: always
  completed:
    final: true
transitions:
  # `review` has a way out, so the machine loads; the condition is false on the
  # first visit, so this run selects nothing — which is what the test is about.
  - from: review
    to: completed
    condition: visitCount > 1
"#;
    let plan = r#"# Rhei: Agent No Transition Snapshot

## Tasks

### Task 1: Review
**State:** review
"#;

    let dir = unique_temp_dir("run-agent-no-transition-snapshot");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_session_recording_script(&dir, "");
    write_session_fake_agent_settings(&dir, &script_path, "5s");

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        !result.status.success(),
        "run should halt because the task cannot advance\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("agent exited 0 but task plan.1 did not advance from 'review'"),
        "no-transition warning should be shown; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let manifests = collect_run_agent_snapshot_manifests(&dir);
    assert_run_agent_snapshot(&manifests, "_state", "success");
    assert_run_agent_snapshot(&manifests, "always", "success");
}

#[test]
fn run_agent_nonzero_exit_without_route_emits_failure_snapshot_before_abort() {
    let machine = r#"name: agent-error-snapshot
version: 1
states:
  work:
    initial: true
    target: fake:openai:model
    snapshot:
      emit:
        name: failure
        on: failure
  failed:
    final: true
transitions:
  - from: work
    to: failed
"#;
    let plan = r#"# Rhei: Agent Error Snapshot

## Tasks

### Task 1: Work
**State:** work
"#;

    let dir = unique_temp_dir("run-agent-error-snapshot");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_session_recording_script(&dir, "sys.exit(7)\n");
    write_session_fake_agent_settings(&dir, &script_path, "5s");

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        !result.status.success(),
        "run should abort after the nonzero exit\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let manifests = collect_run_agent_snapshot_manifests(&dir);
    assert_run_agent_snapshot(&manifests, "_state", "failure");
    assert_run_agent_snapshot(&manifests, "failure", "failure");
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "work");
}

#[test]
fn run_agent_timeout_route_emits_timeout_snapshot_before_transition() {
    let machine = r#"name: agent-timeout-snapshot
version: 1
states:
  work:
    initial: true
    target: fake:openai:model
    snapshot:
      emit:
        name: failure
        on: failure
  timed_out:
    final: true
transitions:
  - from: work
    to: timed_out
    timeout: 1s
"#;
    let plan = r#"# Rhei: Agent Timeout Snapshot

## Tasks

### Task 1: Work
**State:** work
"#;

    let dir = unique_temp_dir("run-agent-timeout-snapshot");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    // Outlives the agent's 1s timeout, so the run has to end the invocation.
    let script_path = write_session_recording_script(&dir, "time.sleep(30)\n");
    write_session_fake_agent_settings(&dir, &script_path, "1s");

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        result.status.success(),
        "run should apply the timeout route\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let manifests = collect_run_agent_snapshot_manifests(&dir);
    assert_run_agent_snapshot(&manifests, "_state", "timeout");
    assert_run_agent_snapshot(&manifests, "failure", "timeout");
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "timed_out");
}

#[test]
fn run_spawns_all_fanout_invocations_for_selected_task_despite_parallel_one() {
    let machine = r#"name: fanout-parallel-task-limit
version: 1
models:
  - alpha
  - beta
states:
  review:
    initial: true
    agent: fake
    all_models: [alpha, beta]
    outputs:
      - name: model-output
        path: runtime/{model}.txt
  completed:
    final: true
transitions:
  - from: review
    to: completed
"#;
    let plan = r#"# Rhei: Fanout Parallel Limit

## Tasks

### Task 1: Review across models
**State:** review
"#;
    let script = r#"model = env('RHEI_MODEL')
if not model:
    sys.exit('RHEI_MODEL must be set')
append(pathlib.Path('runtime') / 'models.txt', model + '\n')
write(pathlib.Path('runtime') / (model + '.txt'), 'done')
# §FS-rhei-states.3.3: a state that can finish the ticket writes its result.
result('## Result\n\nAgent finished {}.\n'.format(env('RHEI_STATE')))
"#;

    let dir = unique_temp_dir("run-fanout-parallel-one");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_python_agent(&dir, "agent.py", script);
    let command = fixture_command(&script_path);
    write_run_agent_settings(
        &dir,
        &format!(
            r#"{{
  "agents": {{
    "fake": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s"
    }}
  }},
  "models": {{
    "alpha": {{
      "provider": "test",
      "model": "alpha-model"
    }},
    "beta": {{
      "provider": "test",
      "model": "beta-model"
    }}
  }}
}}"#
        ),
    );

    let ceiling = dir.parent().expect("fixture has a temporary parent").to_str().expect("ceiling path");
    let result = run_run_command_with_env(
        &plan_path,
        &machine_path,
        &["--no-callbacks", "--parallel", "1"],
        &[("GIT_CEILING_DIRECTORIES", ceiling)],
    );

    assert!(
        result.status.success(),
        "run should spawn every fanout invocation for the selected task\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("2 agent(s)"),
        "summary should count both fanout invocations; got:\n{}",
        result.stdout
    );
    assert!(dir.join("runtime/alpha.txt").exists(), "alpha output should exist");
    assert!(dir.join("runtime/beta.txt").exists(), "beta output should exist");
    let models = fs::read_to_string(dir.join("runtime/models.txt")).expect("read model log");
    assert!(models.lines().any(|line| line == "alpha"), "alpha invocation should run");
    assert!(models.lines().any(|line| line == "beta"), "beta invocation should run");

    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "completed");
}

/// A Panta member whose initial state both declares autonomous agent work and
/// requires a brief. The agent makes `rhei run` take the orchestrated loop —
/// which is the loop issue #99 was filed against, and whose ready-set scan and
/// halt report are call sites of their own, untouched by the callback-mode
/// sibling in `workspace_validation_panta.rs`.
// §AR-rhei-panta.5
const PANTA_AGENT_INPUT_STATE_MACHINE: &str = r#"name: panta-agent-input-machine
version: 1
states:
  pending:
    description: Needs a brief at the owning rhei's execution root
    initial: true
    agent: fake
    inputs:
      - name: brief
        path: runtime/{task_id}.md
  completed:
    description: Task finished successfully
    final: true
transitions:
  - from: pending
    to: completed
"#;

/// The orchestrated loop resolves a member's required inputs at the member's own
/// execution root, and says so when it cannot find them.
///
/// The reported shape — a supervisor writing a brief for the child it holds —
/// takes this path, not the callback loop's, because the supervising state
/// carries a `target:`. Both loops call `find_runnable_tasks` and
/// `halted_task_report` for themselves, so covering one covers neither.
// §AR-rhei-panta.5 §FS-rhei-run-report.3.1
#[test]
fn run_agent_mode_resolves_a_panta_member_input_at_its_own_execution_root() {
    let project = create_panta_project(
        "panta-run-agent-input-root",
        "# Panta: Agent Run Inputs\n**States:** panta-agent-input-machine\n",
        &[
            ("auth/index.rhei.md", "# Rhei: Auth\n\n"),
            ("auth/tasks/login.md", "### Task 1: Login\n**State:** pending\n"),
        ],
        PANTA_AGENT_INPUT_STATE_MACHINE,
    );
    let script = write_python_agent(
        &project,
        "fake-agent.py",
        // §FS-rhei-states.3.3: `pending -> completed` is terminal, so the agent
        // writes the ticket's result before it exits.
        r#"append(
    pathlib.Path(env('RHEI_ROOT')) / 'agent-invocations.txt',
    env('RHEI_TASK_ID') + '\n',
)
result('## Result\n\nDone.\n')
"#,
    );
    write_run_agent_settings(
        &project,
        &format!(
            r#"{{ "agents": {{ "fake": {{ "command": {}, "stdin_prompt": true, "timeout": "60s" }} }} }}"#,
            fixture_command(&script)
        ),
    );

    let member_root = project.join("auth");
    let member_input = member_root.join("runtime").join("auth.1.md");
    let project_input = project.join("runtime").join("auth.1.md");
    let invocations = member_root.join("agent-invocations.txt");
    fs::create_dir_all(project_input.parent().expect("project runtime")).expect("mkdir project");
    fs::write(&project_input, "brief").expect("write the brief at the project root");

    let run = || {
        let output = rhei_command()
            .arg("run")
            .arg(&project)
            .arg("--no-callbacks")
            .output()
            .expect("run should run");
        (output.status.success(), String::from_utf8_lossy(&output.stdout).into_owned())
    };

    // The project root is not this member's execution root, so the scan refuses
    // the ticket — and the halt line names the file and the root it wanted it
    // under, absolutely, which is the whole question. §FS-rhei-run-report.3.1
    let (advanced, stdout) = run();
    let wanted = Path::new(&rendered_path_text(&member_root))
        .join("runtime")
        .join("auth.1.md")
        .display()
        .to_string();
    assert!(!advanced, "a brief at the project root must not schedule the member: {stdout}");
    assert!(
        stdout.contains("required input(s) not found: brief") && stdout.contains(&wanted),
        "the orchestrated loop's halt line should name {wanted}: {stdout}"
    );
    assert!(!invocations.exists(), "no agent should have been spawned: {stdout}");

    // The same file under the root the member's own prompt names schedules it.
    fs::create_dir_all(member_input.parent().expect("member runtime")).expect("mkdir member");
    fs::rename(&project_input, &member_input).expect("move the brief to the member root");
    let (advanced, stdout) = run();
    assert!(advanced, "a brief at the member's own root should schedule it: {stdout}");
    assert_eq!(
        fs::read_to_string(&invocations).expect("the orchestrated loop should spawn the agent"),
        "auth.1\n",
        "agent mode, not the callback loop, ran this ticket"
    );
}
