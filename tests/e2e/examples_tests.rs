use std::fs;
use std::path::{Path, PathBuf};

use super::*;

fn copy_example_workspace(prefix: &str, example_path: &str) -> (TestDir, PathBuf) {
    let dir = unique_scratchpad_dir(prefix);
    let src = repo_root().join(example_path);
    let leaf = Path::new(example_path).file_name().expect("example path has leaf");
    let workspace = dir.join(leaf);
    copy_dir_recursive(&src, &workspace);
    (dir, workspace)
}

fn write_mock_example_agent(dir: &Path) -> PathBuf {
    // Whether the `prepare-worktree` state may settle for a bare `git init`
    // is decided here rather than inside the fixture, so the allowance is one
    // platform's and not "whatever the fixture could manage": Windows refuses
    // a checkout path past MAX_PATH, and everywhere else a failed `git
    // worktree add` is a broken example rather than a portability fact.

    // §REQ-cross-platform.3
    let settings =
        format!("WORKTREE_FALLBACK_ALLOWED = {}\n\n", if cfg!(windows) { "True" } else { "False" });
    write_python_agent(dir, "mock-example-agent.py", &format!("{settings}{MOCK_EXAMPLE_AGENT}"))
}

/// The one mock agent every example test drives, dispatching on `RHEI_STATE`.
const MOCK_EXAMPLE_AGENT: &str = r#"import shutil
import subprocess

# The examples are driven from their workspace root: every relative path below
# is the one the example's own state machine names.
workspace = pathlib.Path(env('RHEI_PLAN_PATH', '.'))
if workspace.is_file():
    workspace = workspace.parent
os.chdir(workspace)

state = env('RHEI_STATE')
task = env('RHEI_TASK_ID', 'unknown')
target_slug = env('RHEI_TARGET_SLUG', env('RHEI_MODEL', 'mock'))
machine = env('RHEI_STATE_MACHINE_PATH')
runtime = pathlib.Path('runtime')

append(
    runtime / 'logs' / 'mock-agent.log',
    'task={} state={} model={} target={} agent={}\n'.format(
        task, state, env('RHEI_MODEL'), target_slug, env('RHEI_AGENT')
    ),
)

# A worker records why the ticket ends where it does. Rhei hands the path to
# every subprocess in RHEI_RESULT_PATH, and a `final: true` state is not entered
# until it has content. §FS-rhei-states.3.3 §FS-rhei-agents.4
result('## Result\n\nMock agent finished state {} for task {}.\n'.format(state, task))

if state == 'analyze':
    multi_model = bool(machine) and any(
        line.startswith('name: multi-model-analysis')
        for line in pathlib.Path(machine).read_text(encoding='utf-8').splitlines()
    )
    if multi_model:
        write(
            runtime / 'analyses' / (target_slug + '.md'),
            '# Mock analysis\n\nstate={}\ntarget={}\n'.format(state, target_slug),
        )
    else:
        write(
            runtime / 'analysis' / (task + '-findings.md'),
            '# Mock dispatch findings\n\n- id: mock-work\n  title: Mock work item\n',
        )
        tasks = pathlib.Path('tasks')
        tasks.mkdir(parents=True, exist_ok=True)
        if not (tasks / '02-mock-work.md').exists():
            write(
                tasks / '02-mock-work.md',
                '### Task mock-work: Mock dispatched work item\n'
                '**State:** address\n'
                '**Prior:** Task {}\n'
                '\n'
                'Write the mock work result.\n'.format(task),
            )
        if not (tasks / '03-report.md').exists():
            write(
                tasks / '03-report.md',
                '### Task report: Summarize the dispatched work\n'
                '**State:** report\n'
                '**Prior:** Task mock-work\n'
                '\n'
                'Summarize the mock work result.\n',
            )
elif state == 'address':
    write(runtime / 'work' / (task + '.md'), '# Mock work result\n\ntask={}\n'.format(task))
elif state == 'report':
    write(runtime / 'report.md', '# Mock dispatch report\n')
elif state == 'prepare-worktree':
    # The one fixture that really does drive another program: this example is
    # about `git worktree`, so the mock has to leave a checkout behind. `git` is
    # exec'd directly, never handed to a shell.
    worktree = pathlib.Path.cwd() / 'runtime' / 'worktrees' / task
    worktree.parent.mkdir(parents=True, exist_ok=True)
    (runtime / 'worktree-refs').mkdir(parents=True, exist_ok=True)
    checkout = env('RHEI_CHECKOUT_ROOT', '.')

    def git(*args):
        return subprocess.run(
            ['git', '-C', checkout] + list(args),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

    added = False
    if git('rev-parse', '--is-inside-work-tree').returncode == 0:
        shutil.rmtree(worktree, ignore_errors=True)
        git('worktree', 'prune')
        # A checkout is not always possible where the example runs: Windows
        # refuses a path past MAX_PATH, and this one is a temp directory plus a
        # workspace plus a task id. The directory alone is what the example's
        # contract needs.
        added = git('worktree', 'add', '--detach', str(worktree), 'HEAD').returncode == 0
    if not added:
        if not WORKTREE_FALLBACK_ALLOWED:
            sys.exit(
                'git worktree add failed, and this platform has no license to '
                'fall back to a bare repository'
            )
        # The ref this state writes is checked against the git root of the path
        # it names, so a bare directory inside the repository is not enough.
        worktree.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            ['git', 'init', str(worktree)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    write(
        runtime / 'worktree-refs' / (task + '.yaml'),
        'task_id: {}\npath: {}\nbranch: docs-pass/{}\ntarget_path: mock\n'.format(
            task, worktree, task
        ),
    )
elif state == 'work':
    write(
        runtime / 'summaries' / (task + '-work.md'),
        '# Mock worktree change summary\n\ntask={}\n'.format(task),
    )
elif state == 'integrate':
    write(
        runtime / 'summaries' / (task + '-summary.md'),
        '# Mock worktree summary\n\ntask={}\nbranch=docs-pass/{}\n'.format(task, task),
    )
elif state == 'summarize':
    write(runtime / 'final-analysis.md', '# Mock final analysis\n')
elif state in ('review', 'fix'):
    folder = runtime / ('reviews' if state == 'review' else 'fixes')
    folder.mkdir(parents=True, exist_ok=True)
    pass_number = len(list(folder.glob('task-{}-{}-*.md'.format(task, state)))) + 1
    write(
        folder / 'task-{}-{}-{}.md'.format(task, state, pass_number),
        '# Mock {} pass {}\n'.format(state, pass_number),
    )
"#;

fn write_mock_agent_settings(workspace: &Path, agent_script: &Path) {
    let settings_dir = workspace.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create .agent-grounds/rhei");
    let profile = format!(
        r#"{{
      "command": {},
      "prompt_flag": "--prompt",
      "model_flag": "--model",
      "timeout": "5s",
      "modes": {{ "yolo": [] }}
    }}"#,
        fixture_command(agent_script)
    );
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{
    "agent": "mock",
    "agent_timeout": "5s"
  }},
  "agents": {{
    "mock": {profile},
    "claude-code": {profile},
    "codex": {profile},
    "gemini": {profile},
    "cursor": {profile}
  }},
  "models": {{
    "claude": {{ "provider": "mock", "model": "claude", "default_agent": "mock" }},
    "codex": {{ "provider": "mock", "model": "codex", "default_agent": "mock" }},
    "gemini": {{ "provider": "mock", "model": "gemini", "default_agent": "mock" }},
    "cursor": {{ "provider": "mock", "model": "cursor", "default_agent": "mock" }}
  }}
}}"#
        ),
    )
    .expect("write mock settings");
}

fn run_example_with_mock_agents(
    prefix: &str,
    example_path: &str,
    state_machine_name: &str,
    args: &[&str],
) -> (TestDir, PathBuf, PathBuf, CliRun) {
    let (dir, workspace) = copy_example_workspace(prefix, example_path);
    let agent = write_mock_example_agent(&dir);
    write_mock_agent_settings(&workspace, &agent);
    let machine_path = workspace.join(state_machine_name);
    let result = run_cli("run", &workspace, &machine_path, args);
    (dir, workspace, machine_path, result)
}

#[test]
fn example_agent_discussion_runs_with_mock_agents() {
    let (_dir, workspace, machine_path, result) = run_example_with_mock_agents(
        "example-agent-discussion",
        "examples/agent-discussion",
        "discussion-states.yaml",
        &["--no-tui"],
    );
    assert_success(&result);

    let json = render_json(&workspace, &machine_path);
    let states: Vec<&str> = json["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .map(|task| task["state"].as_str().expect("state field"))
        .collect();
    assert!(
        states.contains(&"converged") && states.contains(&"completed"),
        "expected discussion seed to converge and downstream task to complete; got:\n{}",
        result.stdout
    );
    assert!(workspace.join("runtime/discussion/decision.md").exists());
    assert!(workspace.join("runtime/discussion/applied.md").exists());
}

/// The supervision example runs the whole §7 chain with its own committed mock.
///
/// Unlike the other examples here it is *not* handed the shared mock agent: the
/// point of the fixture is that a reader can copy the directory and run it, so
/// the committed `workflow.py` and the committed settings are what the test
/// exercises.
// §FS-rhei-supervision.7
#[test]
fn example_subtree_supervision_runs_its_supervisor_between_its_children() {
    let (_dir, workspace) =
        copy_example_workspace("example-subtree-supervision", "examples/subtree-supervision");
    let machine_path = workspace.join("states.yaml");
    let result = run_cli("run", &workspace, &machine_path, &["--no-tui"]);
    assert_success(&result);
    assert_all_tasks_in_state(&workspace, &machine_path, "completed");

    // The supervisor is scheduled between its children, never beside one, and
    // one visit more than there are children. §FS-rhei-supervision.3.1
    let log = fs::read_to_string(workspace.join("runtime/logs/subtree-supervision.log"))
        .expect("the mock logs every invocation");
    let states: Vec<&str> = log
        .lines()
        .filter_map(|line| line.split("state=").nth(1))
        .filter_map(|rest| rest.split_whitespace().next())
        .collect();
    assert_eq!(
        states,
        vec![
            "supervising", "review", "supervising", "fix", "supervising", "review", "supervising", "fix",
            "supervising"
        ],
        "expected hold \u{2192} visit \u{2192} release \u{2192} child \u{2192} checkpoint; got:\n{log}"
    );

    // §FS-rhei-supervision.5.2: one brief per child, at the reserved path.
    for child in 1..=4 {
        let brief = workspace.join(format!("runtime/supervise/subtree-supervision.1.{child}.md"));
        assert!(
            brief.exists(),
            "the supervisor briefs every step it releases: {}",
            brief.display()
        );
    }
}

#[test]
fn example_analyze_and_dispatch_runs_with_mock_agents() {
    let (_dir, workspace, machine_path, result) = run_example_with_mock_agents(
        "example-analyze-dispatch",
        "examples/analyze-and-dispatch-example",
        "states.yaml",
        &["--no-tui", "--parallel", "3"],
    );
    assert_success(&result);
    assert_all_tasks_in_state(&workspace, &machine_path, "completed");
    assert!(workspace.join("tasks/02-mock-work.md").exists());
    assert!(workspace.join("runtime/report.md").exists());
}

#[test]
fn example_parallel_worktrees_runs_with_mock_agents() {
    let (_dir, workspace, machine_path, result) = run_example_with_mock_agents(
        "example-parallel-worktrees",
        "examples/parallel-worktrees-example",
        "states.yaml",
        &["--no-tui", "--parallel", "3"],
    );
    assert_success(&result);
    assert_all_tasks_in_state(&workspace, &machine_path, "completed");
    // The integrate state's artifact is `summary`, not `result`: a declared
    // artifact called `result` would collide in the prompt and in stall reports
    // with the ticket's own terminal result. §FS-rhei-states.3.3
    assert!(workspace.join("runtime/summaries/parallel-worktrees-example.cli-summary.md").exists());
    assert!(workspace
        .join("runtime/summaries/parallel-worktrees-example.core-summary.md")
        .exists());
    assert!(workspace
        .join("runtime/summaries/parallel-worktrees-example.validator-summary.md")
        .exists());
}

#[test]
fn example_multi_model_analysis_runs_with_mock_agents() {
    let (_dir, workspace, machine_path, result) = run_example_with_mock_agents(
        "example-multi-model-analysis",
        "examples/multi-model-analysis-example",
        "states.yaml",
        &["--no-tui"],
    );
    assert_success(&result);
    assert_all_tasks_in_state(&workspace, &machine_path, "completed");
    assert!(workspace.join("runtime/final-analysis.md").exists());
    assert!(workspace
        .join("runtime/analyses/claude-code-yolo-anthropic-claude-opus-4-7.md")
        .exists());
    assert!(workspace
        .join("runtime/analyses/gemini-yolo-google-gemini-3.1-pro-preview.md")
        .exists());
    assert!(workspace.join("runtime/analyses/codex-yolo-openai-gpt-5-codex.md").exists());
}

#[test]
fn example_spec_review_runs_with_mock_agents() {
    let (_dir, workspace, machine_path, result) = run_example_with_mock_agents(
        "example-spec-review",
        "examples/spec-review-example",
        "states.yaml",
        &["--no-tui"],
    );
    assert_success(&result);
    assert_all_tasks_in_state(&workspace, &machine_path, "completed");
    assert!(workspace.join("specs/template-review-fixture.spec.md").exists());
    assert!(workspace.join("runtime/reviews/task-spec-review-example.review-review-1.md").exists());
    assert!(workspace.join("runtime/reviews/task-spec-review-example.review-review-2.md").exists());
    assert!(workspace.join("runtime/fixes/task-spec-review-example.review-fix-1.md").exists());
    assert!(workspace.join("runtime/fixes/task-spec-review-example.review-fix-2.md").exists());
}

/// The bundled UI fixture, instantiated and run end to end with its own mock
/// agent and program.
///
/// This is the one bundled workspace whose workers are committed scripts rather
/// than a real agent, so it is the one that breaks silently when the engine
/// starts asking workers for something. It did: `script-check -> completed` is a
/// program state whose exit-0 edge is terminal, and neither mock wrote
/// `RHEI_RESULT_PATH`, so every terminal ticket stalled on a missing `result`.
/// Instantiating and running here keeps the fixture honest about the contract
/// it demonstrates.
///
/// It runs where the three directories are all different: the workspace is
/// `nested/ws` under a git repository, so the checkout root the workers are
/// started in is neither the artifact root nor the directory `rhei run` was
/// launched from. That is the arrangement in which a path composed against the
/// launcher resolves for nobody, and it is how the bundled scripts are held to
/// the one rule a wrapper is given (§FS-rhei-agents.4.1).
// §FS-rhei-states.3.3 §FS-rhei-agents.4.1 §FS-rhei-programs.2
#[test]
fn bundled_ui_fixture_instantiates_and_runs_to_its_human_gate() {
    let dir = unique_temp_dir("example-ui-test-canonical");
    if !git_init_or_absent(&dir) {
        eprintln!("skipping: git is not on this machine");
        return;
    }
    let template = dir.join(".agent-grounds/rhei/templates/ui-test-canonical");
    copy_dir_recursive(
        &repo_root().join(".agent-grounds/rhei/templates/ui-test-canonical"),
        &template,
    );
    let home = dir.join(".home");
    fs::create_dir_all(&home).expect("isolated home");
    let launch = dir.join("nested");
    fs::create_dir_all(&launch).expect("launch directory below the checkout root");

    let instantiate = rhei_command(&home)
        .current_dir(&launch)
        .args(["instantiate", "ui-test-canonical", "--output", "ws"])
        .output()
        .expect("rhei instantiate should run");
    assert!(
        instantiate.status.success(),
        "instantiate failed:\nstdout:\n{}\nstderr:\n{}",
        stdout(&instantiate),
        stderr(&instantiate)
    );

    let run = rhei_command(&home)
        .current_dir(&launch)
        .args(["run", "ws", "--no-tui", "--parallel", "4"])
        .output()
        .expect("rhei run should run");
    let stdout = stdout(&run);
    let stderr = stderr(&run);
    assert!(
        run.status.success(),
        "the fixture must run to its human gate:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("required outputs are missing"),
        "no worker may stall on a missing artifact:\nstdout:\n{stdout}"
    );

    let workspace = launch.join("ws");
    // A program-driven terminal edge and an agent-driven one, each with the
    // worker's own account on disk.
    for task in
        ["ws.scenario-dashboard-checkout-flow", "ws.full-pipeline.snapshot-inherit-ancestor"]
    {
        let result = workspace.join(format!("runtime/results/{task}.md"));
        let body = fs::read_to_string(&result)
            .unwrap_or_else(|err| panic!("read {}: {err}", result.display()));
        assert!(!body.trim().is_empty(), "{task}: terminal result must have content");
    }

    // Every invocation of a fanned-out state gets its own declared output,
    // keyed by identity, so no reviewer overwrites another. Each path is one
    // the prompt named. §FS-rhei-states.3.2
    for slug in ["mock-agent-yolo-mock-review-alpha", "mock-agent-slow-mock-review-beta"] {
        let review = workspace.join(format!("runtime/reviews/ws.full-pipeline-{slug}.md"));
        assert!(
            fs::read_to_string(&review).is_ok_and(|body| !body.trim().is_empty()),
            "each review target writes its own findings: {}",
            review.display()
        );
    }

    // No terminal edge leaves `parallel-review` by name, so its prompt names no
    // result and no invocation owes a fragment; rebuilding the conventional path
    // writes files nothing merges. §FS-rhei-agents.4.1
    let unowed = workspace.join("runtime/results/ws.full-pipeline");
    assert!(
        !unowed.exists(),
        "a worker writes only where the prompt named a file; found {}",
        unowed.display()
    );
}
