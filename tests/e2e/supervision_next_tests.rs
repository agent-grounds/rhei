// What `rhei next` says and renders about a supervised subtree: which tickets
// it refuses, whose turn each refusal names, and the two sections it composes
// for a ticket that supervises or is briefed. The other manual surfaces —
// `rhei validate`, `rhei reset`, `rhei list --ready`, the run report — are next
// door in `supervision_surfaces_tests.rs`.

// §AR-source-file-size.3 §FS-rhei-supervision.3.4

use std::fs;

use super::supervision_barrier_tests::TWO_CHILD_PLAN;
use super::supervision_tests::{setup_supervision, supervision_machine};
use super::*;

/// §FS-rhei-supervision.3.4: `rhei next` never claims a descendant of a held
/// supervisor, and names the supervisor rather than reporting a stall.
#[test]
fn rhei_next_reports_a_held_descendant_instead_of_claiming_it() {
    let plan = TWO_CHILD_PLAN
        .replace("**State:** supervising\n", "**State:** supervising\n**Assignee:** pi\n");
    let (_dir, plan_path, machine_path) = setup_supervision(
        "supervision-next-held",
        &plan,
        &supervision_machine("descendant-terminal", "completed"),
        "",
    );

    let targeted = run_cli("next", &plan_path, &machine_path, &["--task", "1.1", "--peek"]);
    assert!(!targeted.status.success(), "a held descendant is not claimable");
    assert_stderr_contains(&targeted, "held by supervisor Task plan.1 (supervising)");

    let auto = run_cli("next", &plan_path, &machine_path, &["--peek"]);
    assert!(!auto.status.success(), "nothing else is claimable either");
    assert_stderr_contains(&auto, "ticket(s) held by a supervisor");
    assert_stderr_contains(&auto, "Task plan.1.1 held by supervisor Task plan.1 (supervising)");
    // §FS-rhei-supervision.3.4: the visit is already claimed here, so the row
    // names who holds it and how to hand it back.
    assert_stderr_contains(&auto, "pi holds it");
    assert_stderr_contains(&auto, "rhei release");
}

/// A worker whose whole scope is held is told which ticket to work.
///
/// The supervisor is in no other category the diagnosis reports — its own
/// subtree is open, so the workable set excludes it — and a row that stopped
/// at "everything is held" left the worker with nowhere to go.
// §FS-rhei-supervision.3.4
#[test]
fn the_held_row_names_the_supervisor_as_the_ticket_to_work() {
    // `supervising` is not the profile's initial state, so nothing is
    // auto-claimable and the diagnosis is all the worker gets.
    let machine = r#"name: midflow
version: 1
states:
  plan: { initial: true, description: Plan, agent: mock, agent_timeout: 30s, instructions: plan }
  supervising:
    description: Supervise
    execute_on: descendant-terminal
    agent: mock
    agent_timeout: 30s
    visits: 12
    instructions: supervising
  fix: { description: Fix, agent: mock, agent_timeout: 30s, instructions: fix }
  completed: { description: Done, final: true }
  cancelled: { description: Dropped, final: true }
transitions:
  - { from: plan, to: supervising, description: Start supervising }
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervising, to: supervising, description: Released }
  - { from: fix, to: completed, description: Fixed }
  - { from: "*", to: cancelled, description: Dropped }
"#;
    let plan = r#"# Rhei: Mid

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Parent
**State:** supervising

#### Task 1.1: A
**State:** fix
"#;
    let (_dir, plan_path, machine_path) =
        setup_supervision("supervision-held-next-step", plan, machine, "");

    let diagnosed = run_cli("next", &plan_path, &machine_path, &["--peek"]);
    assert!(!diagnosed.status.success(), "nothing is auto-claimable");
    assert_stderr_contains(&diagnosed, "Task plan.1.1 held by supervisor Task plan.1");
    assert_stderr_contains(&diagnosed, "Work the supervisor instead");
    assert_stderr_contains(&diagnosed, "--task plan.1");

    // And the command it names is one that works.
    let worked = run_cli("next", &plan_path, &machine_path, &["--task", "plan.1", "--peek"]);
    assert_success(&worked);
}

/// `rhei next` renders the same two supervision sections `rhei run` composes:
/// the checkpoints a supervisor is owed, and the brief its descendant was
/// written.
// §FS-rhei-supervision.3.4
#[test]
fn rhei_next_renders_the_checkpoints_and_the_brief_the_run_prompt_would() {
    let plan = r#"# Rhei: Handover

---
structure:
  maxLevels: 3
metadata:
  tasks:
    1:
      stateVisits:
        supervising: 2
      supervision:
        phase: released
---

## Tasks

### Task 1: Parent
**State:** supervising-2

#### Task 1.1: A
**State:** fix
"#;
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervision-next-sections",
        plan,
        &supervision_machine("descendant-terminal", "completed"),
        "",
    );
    let brief = dir.join("runtime/supervise");
    fs::create_dir_all(&brief).expect("create brief dir");
    fs::write(brief.join("plan.1.1.md"), "Fix only the parser overflow.\n").expect("write brief");

    // §FS-rhei-supervision.5.2: the released descendant reads its brief.
    let child = run_cli("next", &plan_path, &machine_path, &["--task", "1.1", "--peek"]);
    assert_success(&child);
    assert!(child.stdout.contains("## Supervisor Brief"), "got:\n{}", child.stdout);
    assert!(child.stdout.contains("Fix only the parser overflow."), "got:\n{}", child.stdout);

    // §FS-rhei-supervision.5.1: and the supervisor reads its checkpoints, in
    // JSON as a field of its own.
    let done =
        run_cli("complete", &plan_path, &machine_path, &["--task", "1.1", "--result", "fixed"]);
    assert_success(&done);
    let supervisor =
        run_cli("next", &plan_path, &machine_path, &["--task", "1", "--peek", "--json"]);
    assert_success(&supervisor);
    let payload: serde_json::Value =
        serde_json::from_str(&supervisor.stdout).expect("next --json parses");
    assert!(
        payload["checkpoints"]
            .as_str()
            .expect("a checkpoint section")
            .contains("### Task plan.1.1: A \u{2014} fix \u{2192} completed"),
        "got: {payload}"
    );
    // §FS-rhei-supervision.3.4: and the notes `rhei run` carries in sections
    // `rhei next` does not render — starting with where a brief goes.
    let supervising = payload["supervising"].as_str().expect("a supervising section");
    assert!(
        supervising.contains(&format!(
            "Steer the next step by writing {}",
            dir.join("runtime").join("supervise").join("<task-id>.md").display()
        )),
        "got: {supervising}"
    );
    // §FS-rhei-supervision.1.1: a manual worker is told what brings the ticket
    // back, the same clause `rhei run` puts in `## Rhei Commands`.
    assert!(
        supervising.contains("You are woken after every finished descendant."),
        "got: {supervising}"
    );
    // §FS-rhei-supervision.3.4: the barrier, and the one command that ends the
    // visit — spelled with this invocation's own plan and machine.
    assert!(
        supervising.contains("The subtree below is held for as long as this ticket is claimed"),
        "got: {supervising}"
    );
    // Both paths are quoted the way the command that printed them quoted
    // them, so this is the whole line and not a bag of substrings.
    assert!(
        supervising.contains(&format!(
            "rhei transition {} --task plan.1 --from supervising --to supervising \
             --release-subtree --state-machine {}",
            shell_quote(&plan_path.display().to_string()),
            shell_quote(&machine_path.display().to_string()),
        )),
        "got: {supervising}"
    );
    // §FS-rhei-supervision.5.1: and the qualified result rule `rhei run` puts
    // in `## Result`, which `rhei next` does not render.
    assert!(
        supervising.contains("Write the result only on the visit where every descendant is"),
        "got: {supervising}"
    );
}

/// One `rhei next` screen spells a state one way, the child list included: a
/// child in a counted loop is named by the machine's own state, not by the
/// `-<n>` bookkeeping its `**State:**` line carries. And the authority this
/// visit has is printed before the map the prompt ends with, the way
/// `## Rhei Commands` carries both under `rhei run`.
// §FS-rhei-next.4.1 §FS-rhei-supervision.3.4 §FS-rhei-memory.5
#[test]
fn the_supervisor_screen_spells_its_children_and_orders_its_sections() {
    let plan = r#"# Rhei: Handover

---
structure:
  maxLevels: 3
metadata:
  tasks:
    1:
      stateVisits:
        supervising: 2
      supervision:
        phase: released
---

## Tasks

### Task 1: Parent
**State:** supervising-2

#### Task 1.1: A
**State:** fix-2
"#;
    // A counted `fix`, so the child's authored state carries a suffix.
    let machine = supervision_machine("descendant-terminal", "completed")
        .replace("  fix:\n    description: Fix\n", "  fix:\n    description: Fix\n    visits: 3\n");
    let (_dir, plan_path, machine_path) =
        setup_supervision("supervision-next-spelling", plan, &machine, "");

    let peek = run_cli("next", &plan_path, &machine_path, &["--task", "1", "--peek"]);
    assert_success(&peek);
    assert!(peek.stdout.contains("  - Task plan.1.1: A [fix]\n"), "got:\n{}", peek.stdout);
    for suffix in ["supervising-2", "fix-2"] {
        assert!(!peek.stdout.contains(suffix), "one spelling only; got:\n{}", peek.stdout);
    }
    // §FS-rhei-supervision.3.4: what this visit may do, then where to read the
    // rest of the project.
    let supervising =
        peek.stdout.find("## Supervising This Subtree").expect("the supervising section");
    let navigation = peek.stdout.find("## Rhei Navigation").expect("the map");
    assert!(supervising < navigation, "the authority precedes the map; got:\n{}", peek.stdout);
}

/// A plan with no supervising state carries no supervision section and no
/// supervision field — not an empty one, not a blank line. What it does carry
/// is the mid-term memory `rhei run` composes for the same ticket, which every
/// plan gets.
// §FS-rhei-supervision.3.4 §FS-rhei-memory.5
#[test]
fn rhei_next_carries_no_supervision_sections_without_a_supervisor() {
    let machine = r#"name: plain
version: 1
states:
  fix:
    initial: true
    description: Fix
    agent: mock
    agent_timeout: 30s
    instructions: Apply the fixes.
  completed:
    description: Done
    final: true
transitions:
  - { from: fix, to: completed, description: Fixed }
"#;
    let plan = r#"# Rhei: Plain

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Fix findings
**State:** fix

Body text.

#### Task 1.1: A
**State:** fix
"#;
    let (_dir, plan_path, machine_path) =
        setup_supervision("supervision-next-untouched", plan, machine, "");

    let peek = run_cli("next", &plan_path, &machine_path, &["--task", "1.1", "--peek"]);
    assert_success(&peek);
    let head = [
        "Task plan.1.1 \u{2014} current state: 'fix' (read-only peek; not advanced)",
        "Agent: mock  |  Model: default",
        "",
        "## Task plan.1.1: A",
        "",
        "--- Instructions (fix) ---",
        "Apply the fixes.",
        "",
    ]
    .join("\n");
    assert!(peek.stdout.starts_with(&head), "got:\n{}", peek.stdout);
    for absent in ["## Checkpoints", "## Supervisor Brief", "## Supervising This Subtree"] {
        assert!(!peek.stdout.contains(absent), "{absent} in:\n{}", peek.stdout);
    }
    // §FS-rhei-memory.5: the memory sections print after the instructions, in
    // the run prompt's order.
    assert!(peek.stdout.contains("\n## Position\n"), "got:\n{}", peek.stdout);
    assert!(peek.stdout.contains("\n### Reading the rhei\n"), "got:\n{}", peek.stdout);

    let json = run_cli("next", &plan_path, &machine_path, &["--task", "1.1", "--peek", "--json"]);
    assert_success(&json);
    let payload: serde_json::Value =
        serde_json::from_str(&json.stdout).expect("next --json parses");
    assert!(payload.get("checkpoints").is_none(), "got: {payload}");
    assert!(payload.get("supervisor_brief").is_none(), "got: {payload}");
    // §FS-rhei-memory.5: one string field per section, present exactly when the
    // section is. This plan has nothing finished and nobody waiting.
    assert!(
        payload["position"].as_str().expect("position field").starts_with("## Position"),
        "got: {payload}"
    );
    assert!(
        payload["navigation"].as_str().expect("navigation field").contains("### Reading the rhei"),
        "got: {payload}"
    );
    assert!(payload.get("plan_history").is_none(), "got: {payload}");
    assert!(payload.get("previous_visits").is_none(), "got: {payload}");
}

/// `rhei next --task <descendant>` answers with the same three facts the bare
/// listing does, and never with a command that will be refused.
///
/// Before: it always said "Work the supervisor instead: rhei next … --task
/// plan.1" — which fails "already assigned" when a worker holds that visit, and
/// whose help then told the worker to hand-edit `**Assignee:**`, a field every
/// other surface calls CLI-owned.
// §FS-rhei-supervision.3.4 §FS-rhei-release
#[test]
fn the_targeted_held_refusal_names_the_holder_and_rhei_release() {
    let plan = TWO_CHILD_PLAN
        .replace("**State:** supervising\n", "**State:** supervising\n**Assignee:** pi\n");
    let (_dir, plan_path, machine_path) = setup_supervision(
        "supervision-targeted-held",
        &plan,
        &supervision_machine("descendant-terminal", "completed"),
        "",
    );

    let held = run_cli("next", &plan_path, &machine_path, &["--task", "1.1", "--peek"]);
    assert!(!held.status.success(), "a held descendant is not claimable");
    assert!(
        held.stderr.contains("Task plan.1 is the ticket to work and pi holds it")
            && held.stderr.contains("rhei release"),
        "got:\n{}",
        held.stderr
    );
    assert!(
        !held.stderr.contains("Work the supervisor instead"),
        "the claim it would suggest is the one that fails:\n{}",
        held.stderr
    );

    // And the refusal that claim would have produced now names `rhei release`
    // rather than telling the worker to edit the plan by hand.
    let claimed = run_cli("next", &plan_path, &machine_path, &["--task", "1", "--peek"]);
    assert!(!claimed.status.success(), "a claimed ticket is not re-claimable");
    assert!(
        claimed.stderr.contains("rhei release") && claimed.stderr.contains("--task plan.1"),
        "got:\n{}",
        claimed.stderr
    );
    assert!(
        !claimed.stderr.contains("deleting the **Assignee:** line"),
        "`**Assignee:**` is CLI-owned:\n{}",
        claimed.stderr
    );
}
