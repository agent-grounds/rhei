// §FS-rhei-memory driven end to end: the four sections as a spawned agent
// actually receives them, and the same sections as `rhei next` hands them to a
// manual worker.

use std::fs;

use super::supervision_tests::{prompt_for, setup_supervision};
use super::*;

/// A plain two-state workflow: enough for a ticket to be worked, reviewed, and
/// finished, so a later ticket has a history to read.
pub(super) const MEMORY_MACHINE: &str = r#"name: memory-e2e
version: 1
states:
  pending:
    initial: true
    description: Ready for work
    agent: mock
    agent_timeout: 30s
    instructions: Do the work for Task {task_id}.
  review:
    description: Review
    agent: mock
    agent_timeout: 30s
    instructions: Review Task {task_id}.
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: pending, to: review, description: Work done }
  - { from: review, to: completed, description: Reviewed }
  - { from: "*", to: cancelled, description: Dropped }
"#;

/// Two root tickets, the second waiting on the first for both a prior and an
/// export, plus a plan-level standing note.
const MEMORY_PLAN: &str = r#"# Rhei: Memory

## House Rules

Write the one-line summary first.

## Tasks

### Task 1: Build the index
**State:** pending
**Provides:** findings

### Task 2: Query the index
**State:** pending
**Prior:** 1
**Consumes:** 1:findings
"#;

/// §FS-rhei-memory.3: every section a spawned agent is owed reaches it, at the
/// position the spec puts it, with the ids qualified throughout.
#[test]
fn a_spawned_agent_receives_its_position_history_and_map() {
    let (dir, plan_path, machine_path) =
        setup_supervision("memory-run", MEMORY_PLAN, MEMORY_MACHINE, "");
    let export_root = dir.join("runtime/exports/plan.1");
    fs::create_dir_all(&export_root).expect("create findings export directory");
    // §FS-rhei-plan-language.3.12.3: a declared producer export must be nonblank before terminal entry.
    write_fixture_file(&export_root, "findings.md", "Indexed findings.\n");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&result);

    // §FS-rhei-memory.3.1: orientation comes before the instructions.
    let first = prompt_for(&dir, "plan.1", "pending", 1);
    assert!(
        first.contains(
            "## Position\n\nPanta: Memory \u{203a} rhei `plan`: Memory\n\
             \u{203a} **Task plan.1: Build the index [pending]** \u{2190} this invocation \
             (visit 1)\n"
        ),
        "got:\n{first}"
    );
    assert!(
        first.find("## Position") < first.find("## Instructions"),
        "orientation precedes the instructions; got:\n{first}"
    );
    // §FS-rhei-memory.3.1: the plan writer's standing notes, which until now
    // only a worker that opened the file read.
    assert!(first.contains("### Rhei Context"), "got:\n{first}");
    assert!(first.contains("Write the one-line summary first."), "got:\n{first}");
    assert!(!first.contains("### Project Context"), "a bare rhei has none; got:\n{first}");
    // §FS-rhei-memory.3.2: who reads what this task writes.
    assert!(
        first.contains(
            "### Dependents\n\n- Task plan.2: Query the index [pending] \u{2014} prior, \
             consumes `findings`\n"
        ),
        "got:\n{first}"
    );
    // §FS-rhei-memory.3.3: nothing has happened to it yet.
    assert!(!first.contains("## Previous Visits"), "got:\n{first}");
    // §FS-rhei-memory.3.4: the map follows the transitions. This non-git
    // fixture has a different agent cwd, so prompt-root paths are absolute —
    // under the name the run was given, not another one. §FS-rhei-agents.4.1
    let root_path = dir.to_path_buf();
    let root = root_path.display().to_string();
    let plan = root_path.join("plan.rhei.md").display().to_string();
    let dash = "\u{2014}";
    assert!(
        first.contains(&format!(
            "- This rhei: `{root}` {dash} plan `{plan}`, this task's file `{plan}`\n"
        )),
        "got:\n{first}"
    );
    assert!(first.contains(&format!("  - `plan` {dash} `{root}`\n")), "got:\n{first}");
    assert!(
        first.find("Available transitions from") < first.find("### Reading the rhei"),
        "the map follows the transition list; got:\n{first}"
    );
    assert!(first.contains("### Leaving a trail"), "got:\n{first}");
    // §FS-rhei-memory.3.4: under `rhei run` the two sub-sections already have a
    // `##` parent — `## Rhei Commands` — and get no second one.
    assert!(!first.contains("## Rhei Navigation"), "got:\n{first}");

    // §FS-rhei-memory.3.3: the second state of the same ticket knows what
    // already happened to it.
    let reviewed = prompt_for(&dir, "plan.1", "review", 1);
    // §FS-rhei-memory.4.4: the ledger holds `pending@review`, so it already
    // ends in the state being entered — that state is annotated, not repeated.
    assert!(
        reviewed.contains("Trail for this task: pending \u{2192} review (this visit, visit 1).\n"),
        "got:\n{reviewed}"
    );
    assert!(
        !reviewed.contains("Task plan.1 finished pending."),
        "a non-terminal visit is not asked for a terminal result; got:\n{reviewed}"
    );

    // §FS-rhei-memory.3.2: the next ticket reads what finished before it, and
    // pays for the prior's result once.
    let second = prompt_for(&dir, "plan.2", "pending", 1);
    assert!(
        second.contains("- Task plan.1: Build the index \u{2014} completed \u{2014} see above\n"),
        "the prior is pasted in full above, so the history refers to it; got:\n{second}"
    );
    assert!(second.contains("## Prior Task Results"), "got:\n{second}");
    // §FS-rhei-memory.4.5: and that pasted body is fenced.
    assert!(second.contains("### Task plan.1\n\n```markdown\n## Result"), "got:\n{second}");
}

/// §FS-rhei-memory.5: `rhei next` renders the same sections from the same
/// renderers — after the instructions in text, one string field each in JSON.
#[test]
fn rhei_next_mirrors_the_memory_sections_in_text_and_json() {
    let (dir, plan_path, machine_path) =
        setup_supervision("memory-next", MEMORY_PLAN, MEMORY_MACHINE, "");
    // A finished first ticket and a ledger to place it in time.
    fs::create_dir_all(dir.join("runtime/results")).expect("runtime tree");
    write_fixture_file(&dir, "runtime/state-transitions.log", "plan.1 review@completed\n");
    write_fixture_file(
        &dir,
        "runtime/results/plan.1.md",
        "## Result\n\nIndex built over 12k documents.\n",
    );
    let plan = fs::read_to_string(&plan_path).expect("read plan");
    fs::write(
        &plan_path,
        plan.replacen(
            "**State:** pending\n**Provides:**",
            "**State:** completed\n**Provides:**",
            1,
        ),
    )
    .expect("finish task 1");

    let peek = run_cli("next", &plan_path, &machine_path, &["--task", "2", "--peek"]);
    assert_success(&peek);
    let instructions = peek.stdout.find("--- Instructions").expect("instructions");
    for section in ["\n## Position\n", "\n## Plan History\n", "\n### Reading the rhei\n"] {
        let at =
            peek.stdout.find(section).unwrap_or_else(|| panic!("{section} in:\n{}", peek.stdout));
        assert!(at > instructions, "{section} follows the instructions; got:\n{}", peek.stdout);
    }
    // §FS-rhei-memory.5: `rhei next` prints no `## Rhei Commands`, so the two
    // sub-sections get a `##` parent of their own on this surface.
    assert!(
        peek.stdout.contains("\n## Rhei Navigation\n\n### Reading the rhei\n"),
        "got:\n{}",
        peek.stdout
    );
    assert!(
        peek.stdout.contains(
            "- Task plan.1: Build the index \u{2014} completed \u{2014} Index built over 12k \
             documents.\n"
        ),
        "got:\n{}",
        peek.stdout
    );

    let json = run_cli("next", &plan_path, &machine_path, &["--task", "2", "--peek", "--json"]);
    assert_success(&json);
    let payload: serde_json::Value =
        serde_json::from_str(&json.stdout).expect("next --json parses");
    assert!(
        payload["position"].as_str().expect("position").contains("**Task plan.2:"),
        "got: {payload}"
    );
    assert!(
        payload["plan_history"].as_str().expect("plan_history").contains("Index built over"),
        "got: {payload}"
    );
    assert!(
        payload["navigation"].as_str().expect("navigation").contains("### Leaving a trail"),
        "got: {payload}"
    );
    assert!(
        payload["navigation"].as_str().expect("navigation").starts_with("## Rhei Navigation"),
        "the field carries the section as rendered; got: {payload}"
    );
    // Task 2 has never been visited, so the section — and its field — is absent.
    assert!(payload.get("previous_visits").is_none(), "got: {payload}");
}

/// A counted `review` loop, so a ticket's `**State:**` carries a `-<visit>`
/// suffix that the machine itself never names.
const COUNTED_MACHINE: &str = r#"name: counted-e2e
version: 1
states:
  pending:
    initial: true
    description: Ready for work
    instructions: Do the work for Task {task_id}.
  review:
    description: Review
    visits: 3
    instructions: Review Task {task_id}.
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: pending, to: review, description: Work done }
  - { from: review, to: review, description: Another round }
  - { from: review, to: completed, description: Reviewed }
  - { from: "*", to: cancelled, description: Dropped }
"#;

/// One `rhei next` screen spells a state one way: the machine's own name, which
/// is the only form `rhei transition --from` accepts. The visit belongs to
/// `## Position`, and `--json` still reports the authored value.
// §FS-rhei-next.4.1 §FS-rhei-memory.3.1
#[test]
fn rhei_next_prints_one_spelling_of_the_state() {
    let dir = unique_temp_dir("memory-one-spelling");
    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Looper\n\n## Tasks\n\n### Task 1: Looper\n**State:** review-3\n",
    );
    let machine_path = write_fixture_file(&dir, "states.yaml", COUNTED_MACHINE);

    let peek = run_cli("next", &plan_path, &machine_path, &["--task", "1", "--peek"]);
    assert_success(&peek);
    assert!(
        peek.stdout.starts_with(
            "Task plan.1 \u{2014} current state: 'review' (read-only peek; not advanced)\n"
        ),
        "got:\n{}",
        peek.stdout
    );
    assert!(peek.stdout.contains("--- Instructions (review) ---\n"), "got:\n{}", peek.stdout);
    assert!(
        peek.stdout.contains("**Task plan.1: Looper [review]** \u{2190} this invocation (visit 3)"),
        "got:\n{}",
        peek.stdout
    );
    assert!(!peek.stdout.contains("review-3"), "one spelling only; got:\n{}", peek.stdout);

    // The authored form is data, not display: `--json` keeps it.
    let json = run_cli("next", &plan_path, &machine_path, &["--task", "1", "--peek", "--json"]);
    assert_success(&json);
    let payload: serde_json::Value =
        serde_json::from_str(&json.stdout).expect("next --json parses");
    assert_eq!(payload["state"], "review-3", "got: {payload}");
    assert_eq!(payload["from_state"], "review-3", "got: {payload}");
}
