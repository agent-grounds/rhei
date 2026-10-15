// §FS-rhei-supervision driven end to end: a parent woken between its children,
// the checkpoints it reads, the briefs it writes, and the barrier that keeps it
// from ever running beside one of them.
use std::fs;
use std::path::{Path, PathBuf};

use super::*;

/// The mock agent every supervision scenario runs.
///
/// It logs one line per invocation, saves the prompt it was handed, writes the
/// state's declared output and its result, and — in a supervising state — leaves
/// a brief for each of its children. `extra` is spliced into the `supervising`
/// arm so a scenario can make the supervisor act on its subtree.
pub fn supervision_agent_script(extra: &str) -> String {
    // A marker line rather than a `format!` hole: the body is Python, and
    // `format!` would need every brace in it doubled.
    const BODY: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
state = env('RHEI_STATE')
visit = env('RHEI_VISIT_COUNT', '1')
for folder in ('logs', 'prompts', 'supervise', 'review'):
    (root / 'runtime' / folder).mkdir(parents=True, exist_ok=True)
append(root / 'runtime' / 'logs' / 'spawns.log', '{} {} {}\n'.format(task, state, visit))

prompt = ''
args = sys.argv[1:]
while args:
    if args.pop(0) == '--prompt' and args:
        prompt = args.pop(0)
write(root / 'runtime' / 'prompts' / '{}-{}-{}.md'.format(task, state, visit), prompt)

if state == 'supervising':
    for child in (1, 2):
        write(
            root / 'runtime' / 'supervise' / '{}.{}.md'.format(task, child),
            'Brief for child {} written on visit {}.\n'.format(child, visit),
        )
    # SUPERVISING-EXTRA
elif state == 'review':
    write(root / 'runtime' / 'review' / (task + '.md'), 'Findings from {}.\n'.format(task))

result('## Result\n\nTask {} finished {}.\n'.format(task, state))
"#;
    if extra.trim().is_empty() {
        // The marker is a comment, and the branch it sits in already has a
        // statement, so leaving it in place is the empty splice.
        return BODY.to_string();
    }
    BODY.replace("    # SUPERVISING-EXTRA", extra)
}

/// The canonical supervisor machine of §FS-rhei-supervision.7.
pub fn supervision_machine(execute_on: &str, review_to: &str) -> String {
    format!(
        r#"name: supervision-e2e
version: 1
states:
  supervising:
    initial: true
    description: Supervise the subtree
    execute_on: {execute_on}
    agent: mock
    agent_timeout: 30s
    visits: 12
    instructions: You supervise Task {{task_id}}.
  review:
    description: Review
    agent: mock
    agent_timeout: 30s
    outputs:
      - name: findings
        path: runtime/review/{{task_id}}.md
    instructions: Review as briefed.
  fix:
    description: Fix
    agent: mock
    agent_timeout: 30s
    instructions: Apply exactly the fixes the brief asks for.
  human-review:
    description: A human decides
    gating: true
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - {{ from: supervising, to: human-review, description: Budget exhausted, condition: visitCount >= visits }}
  - {{ from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }}
  - {{ from: supervising, to: supervising, description: Released the subtree }}
  - {{ from: review, to: {review_to}, description: Findings written }}
  - {{ from: fix, to: completed, description: Fixes applied }}
  - {{ from: "*", to: cancelled, description: Dropped }}
"#
    )
}

/// A supervision workspace: a single-file plan, the machine, and a `mock` agent
/// registered in project settings. Returns `(dir, plan_path, machine_path)`.
pub fn setup_supervision(
    prefix: &str,
    plan: &str,
    machine: &str,
    script_extra: &str,
) -> (TestDir, PathBuf, PathBuf) {
    setup_supervision_with_agent(prefix, plan, machine, &supervision_agent_script(script_extra))
}

/// [`setup_supervision`] with the agent body given whole, for a scenario whose
/// supervisor does something other than write a brief per child.
pub fn setup_supervision_with_agent(
    prefix: &str,
    plan: &str,
    machine: &str,
    script: &str,
) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script = write_python_agent(&dir, "mock-agent.py", script);
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let command = fixture_command(&script);
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "30s" }},
  "agents": {{ "mock": {{ "command": {command}, "prompt_flag": "--prompt", "timeout": "30s" }} }}
}}"#
        ),
    )
    .expect("write settings");
    (dir, plan_path, machine_path)
}

/// The `<task> <state> <visit>` lines the mock agent logged, in order.
pub fn spawn_log(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join("runtime/logs/spawns.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// Assert a task's state anywhere in the tree, verified through `rhei render`.
pub fn assert_state_anywhere(plan: &Path, machine: &Path, task_id: &str, expected: &str) {
    fn find(node: &serde_json::Value, task_id: &str) -> Option<String> {
        let path = node["id"]["path"].as_str().unwrap_or_default();
        if path == task_id || path.split_once('.').is_some_and(|(_, local)| local == task_id) {
            return node["state"].as_str().map(str::to_string);
        }
        node["children"].as_array().into_iter().flatten().find_map(|child| find(child, task_id))
    }
    let json = render_json(plan, machine);
    let state = json["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .find_map(|task| find(task, task_id))
        .unwrap_or_else(|| panic!("Task {task_id} not found in rendered JSON"));
    assert_eq!(state, expected, "Task {task_id} should be '{expected}', got '{state}'");
}

pub fn prompt_for(dir: &Path, task: &str, state: &str, visit: u32) -> String {
    let path = dir.join(format!("runtime/prompts/{task}-{state}-{visit}.md"));
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("read prompt {}: {err}", path.display()))
}

/// The §FS-rhei-supervision.7 chain: two pre-authored children, a supervisor
/// woken after each of them.
pub const REVIEW_FIX_PLAN: &str = r#"# Rhei: Harden

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Harden the parser
**State:** supervising

Goal and acceptance criteria for the whole change.

#### Task 1.1: Review parser
**State:** review

#### Task 1.2: Fix findings
**State:** fix
**Prior:** Task 1.1
"#;

/// §FS-rhei-supervision.7: the whole trace — hold, visit, release, child,
/// checkpoint, visit — with the supervisor never running beside a child.
#[test]
fn a_descendant_terminal_supervisor_is_woken_between_its_children_and_finishes_after_them() {
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervision-task-chain",
        REVIEW_FIX_PLAN,
        &supervision_machine("descendant-terminal", "completed"),
        "",
    );

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&result);

    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert_state_anywhere(&plan_path, &machine_path, "1.1", "completed");
    assert_state_anywhere(&plan_path, &machine_path, "1.2", "completed");

    // The supervisor is scheduled *between* its children, never beside one.
    // §FS-rhei-supervision.3.1
    assert_eq!(
        spawn_log(&dir),
        vec![
            "plan.1 supervising 1".to_string(),
            "plan.1.1 review 1".to_string(),
            "plan.1 supervising 2".to_string(),
            "plan.1.2 fix 1".to_string(),
            "plan.1 supervising 3".to_string(),
        ],
        "expected hold \u{2192} visit \u{2192} release \u{2192} child \u{2192} checkpoint \u{2192} visit"
    );

    // §FS-rhei-supervision.5.1: the first visit has nothing to judge; the
    // second reads the checkpoint the first child produced.
    let first = prompt_for(&dir, "plan.1", "supervising", 1);
    assert!(!first.contains("## Checkpoints"), "got:\n{first}");
    assert!(first.contains("## Child Tasks"), "got:\n{first}");
    assert!(
        first.contains("You are supervising this task's subtree."),
        "the supervisor's command permissions are stated; got:\n{first}"
    );
    // §FS-rhei-supervision.1.1 §FS-rhei-supervision.5.1: which moves bring it
    // back, in the words of the state's own `execute_on` — an agent that does
    // not know what wakes it cannot tell waiting from being done with a step.
    assert!(
        first.contains("You are woken after every finished descendant."),
        "the prompt names the trigger; got:\n{first}"
    );
    // §FS-rhei-supervision.6: a cancel waives the abandoned step's outputs but
    // not its result, and the permission text says so.
    assert!(first.contains("pass `--result \"<why>\"` on every cancel"), "got:\n{first}");
    assert!(first.contains("`--supervisor plan.1`"), "got:\n{first}");
    // §FS-rhei-supervision.3.1: the barrier, in the sentence that decides how
    // the agent behaves for the rest of the visit.
    assert!(
        first.contains(
            "While you run, nothing beneath you runs; when this invocation ends the subtree \
             is released."
        ),
        "got:\n{first}"
    );
    // §FS-rhei-supervision.5.1: `## Result` is qualified, so a cold agent does
    // not write one on visit 1.
    assert!(
        first.contains(
            "A transition from this state can finish this task. The finished task's result \
             is read from this file. Write the result only on the visit where every \
             descendant is terminal and you intend to finish"
        ),
        "got:\n{first}"
    );
    // §FS-rhei-supervision.5.1: the constructive lever is named before the
    // destructive ones, with the paths this run resolves.
    let supervise_dir = dir.join("runtime").join("supervise");
    assert!(
        first.contains(&format!(
            "Steer the next step by writing {} (read by every state of \
             that descendant) or {} (that state only).",
            supervise_dir.join("<task-id>.md").display(),
            supervise_dir.join("<task-id>").join("<state>.md").display()
        )),
        "got:\n{first}"
    );

    let second = prompt_for(&dir, "plan.1", "supervising", 2);
    assert!(second.contains("## Checkpoints"), "got:\n{second}");
    assert!(
        second.contains(
            "### Task plan.1.1: Review parser \u{2014} review \u{2192} completed (visit 1)"
        ),
        "got:\n{second}"
    );
    // §FS-rhei-supervision.5.1: the pasted body is fenced, so its own
    // `## Result` heading cannot outrank the `### Task …` it sits under.
    assert!(
        second.contains(
            "### Task plan.1.1: Review parser \u{2014} review \u{2192} completed (visit 1)\n\n\
             ```markdown\n## Result\n\nTask plan.1.1 finished review.\n```\n"
        ),
        "got:\n{second}"
    );

    // §FS-rhei-supervision.5.2: the brief the supervisor wrote reaches the child.
    let child = prompt_for(&dir, "plan.1.2", "fix", 1);
    assert!(child.contains("## Supervisor Brief"), "got:\n{child}");
    assert!(child.contains("directions from the supervising Task plan.1."), "got:\n{child}");
    assert!(child.contains("Brief for child 2 written on visit 2."), "got:\n{child}");

    // §FS-rhei-supervision.3.3: leaving the supervising state removes the block.
    let plan = fs::read_to_string(&plan_path).expect("read plan");
    assert!(!plan.contains("supervision:"), "the block is gone once the supervisor left:\n{plan}");
}

/// §FS-rhei-supervision.2.1: `execute_on: descendant-transition` hears every hop, so a child
/// with its own two-step machine hands control back twice.
#[test]
fn a_descendant_transition_supervisor_is_woken_after_every_hop_of_a_descendant() {
    let plan = r#"# Rhei: Every hop

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Harden the parser
**State:** supervising

#### Task 1.1: Review then fix
**State:** review
"#;
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervision-state-chain",
        plan,
        // `review` lands in `fix` rather than `completed`, so the child takes
        // two hops of its own.
        &supervision_machine("descendant-transition", "fix"),
        "",
    );

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&result);

    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert_state_anywhere(&plan_path, &machine_path, "1.1", "completed");
    assert_eq!(
        spawn_log(&dir),
        vec![
            "plan.1 supervising 1".to_string(),
            "plan.1.1 review 1".to_string(),
            "plan.1 supervising 2".to_string(),
            "plan.1.1 fix 1".to_string(),
            "plan.1 supervising 3".to_string(),
        ],
        "a non-terminal hop is a checkpoint under `execute_on: descendant-transition`"
    );

    let second = prompt_for(&dir, "plan.1", "supervising", 2);
    assert!(
        second.contains("\u{2014} review \u{2192} fix (visit 1)"),
        "the non-terminal hop is rendered with the source state's outputs; got:\n{second}"
    );
    assert!(
        second.contains("Findings from plan.1.1."),
        "the `review` state's declared output rides along; got:\n{second}"
    );
}

/// The plan and the machine printed in the spec's example validate as written.
///
/// They did not: the machine had no `name`, no state marked `initial`, and a
/// `snapshot:` block on an `agent:` that resolves no provider or model — three
/// hard errors before a reader gets to the first pass. An example a reader
/// cannot paste is worse than no example, so the spec's own text is the fixture.
// §FS-rhei-supervision.7
#[test]
fn the_specs_own_example_validates_as_printed() {
    let spec =
        fs::read_to_string(repo_root().join("docs/functional-spec/rhei-supervision.spec.md"))
            .expect("read the supervision spec");
    let section = spec
        .split("## 7. Example")
        .nth(1)
        .expect("the spec has a §FS-rhei-supervision.7")
        .split("## 8. Related Specifications")
        .next()
        .expect("§FS-rhei-supervision.7 ends before Related Specifications");

    let block = |fence: &str| -> String {
        let opened = section
            .split(&format!("```{fence}\n"))
            .nth(1)
            .unwrap_or_else(|| panic!("§FS-rhei-supervision.7 has a ```{fence} block"));
        opened.split("```").next().expect("the block is closed").to_string()
    };

    let dir = unique_temp_dir("supervision-spec-example");
    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        &format!("# Rhei: Harden the parser\n\n## Tasks\n\n{}", block("markdown")),
    );
    let machine_path = write_fixture_file(&dir, "states.yaml", &block("yaml"));

    let result = run_cli("validate", &plan_path, &machine_path, &[]);
    assert_success(&result);
    assert!(
        result.stdout.contains("Validation succeeded"),
        "the spec's example must validate clean; got:\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}
