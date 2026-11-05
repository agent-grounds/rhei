//! The other messages that offer `rhei reset`: a finished ticket's `rhei
//! complete`, and the stranded supervisor's halt row on each surface it renders
//! on. The visit-pairing note is held with its fixture in
//! `agent_target_edit_tests.rs` (agent-grounds/rhei#549).

// §FS-rhei-reset.5.1 §FS-rhei-complete.4 §FS-rhei-supervision.3.6 §FS-rhei-run-report.3.1

use std::fs;

use super::entry_log_support::settings;
use super::reset_remedy_support::*;
use super::*;

/// `rhei complete` on a finished ticket, named by its local id: the rhei in the
/// command cannot come from what was typed.
#[test]
#[ignore = "red until #549: `rhei reset <plan> <task>` takes no task and names placeholders; implement removes this"]
fn the_reset_remedy_of_a_finished_tickets_complete_runs_with_a_local_id() {
    let world = RemedyWorld::at("reset-remedy-complete", "lone").with_found_machine();
    world.write("plan.rhei.md", TWO_TICKETS);
    let world = RemedyWorld { target: world.root.join("plan.rhei.md"), ..world };
    let finish = ["--task", "1", "--from", "open", "--to", "cancelled"];
    world.step("transition", &[&finish[..], &["--result", "dropped", "--no-callbacks"]].concat());

    let refused = world.rhei("complete", &["--task", "1", "--result", "again", "--no-callbacks"]);

    assert!(!refused.status.success(), "a finished ticket is refused:\n{}", refused.stdout);
    assert!(
        refused.stderr.contains("already in terminal state"),
        "fixture: this is the refusal under test:\n{}",
        refused.stderr
    );
    assert_offers_runnable_reset(&refused.stderr, &world.home, "plan", &["plan.1"]);
}

/// The same message with what a lone plan cannot show: a member named relatively,
/// with a sibling to stop at and a machine named on the command.
#[test]
#[ignore = "red until #549: `rhei reset <plan> <task>` takes no task and names placeholders; implement removes this"]
fn the_reset_remedy_of_a_finished_tickets_complete_runs_for_a_member_under_a_named_machine() {
    let world = RemedyWorld::at("reset-remedy-complete-member", "project");
    world.write("index.panta.md", "# Panta: Remedy\n");
    world.write("alpha.rhei.md", TWO_TICKETS);
    world.write("beta.rhei.md", TWO_TICKETS);
    let machines = world.dir.join("machines");
    fs::create_dir_all(&machines).expect("create the machine's directory");
    world.write_machine_at(&machines.join("states.yaml"));
    let world = RemedyWorld {
        cwd: world.root.clone(),
        target: "alpha.rhei.md".into(),
        machine: Some("../machines/states.yaml".into()),
        ..world
    };
    let finish = ["--task", "1", "--from", "open", "--to", "cancelled"];
    world.step("transition", &[&finish[..], &["--result", "dropped", "--no-callbacks"]].concat());

    let refused = world.rhei("complete", &["--task", "1", "--result", "again", "--no-callbacks"]);

    assert!(
        refused.stderr.contains("already in terminal state"),
        "fixture: this is the refusal under test:\n{}{}",
        refused.stdout,
        refused.stderr
    );
    assert_offers_runnable_reset(&refused.stderr, &world.home, "alpha", &["alpha.1"]);
}

const STRANDED_MACHINE: &str = r#"name: reset-remedy-stranded
version: 1
states:
  supervising:
    initial: true
    description: Supervise the subtree
    execute_on: child-terminal
    agent: mock
    agent_timeout: 30s
    visits: 12
    instructions: You supervise Task {task_id}.
  review:
    description: Review
    agent: mock
    agent_timeout: 30s
    inputs:
      - name: brief
        path: runtime/supervise/{task_id}.md
    instructions: Review as briefed.
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
  - { from: supervising, to: human-review, description: Budget exhausted, condition: visitCount >= visits }
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervising, to: supervising, description: Released the subtree }
  - { from: review, to: completed, description: Findings written }
  - { from: "*", to: cancelled, description: Dropped }
"#;

/// A supervisor already `released` over a subtree nothing can move.
const STRANDED_PLAN: &str = r#"# Rhei: Already stranded

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

### Task 1: Supervise
**State:** supervising-2

#### Task 1.1: Review
**State:** review
"#;

const SIBLING_PLAN: &str = r#"# Rhei: Sibling

## Tasks

### Task 1: Waits for a person
**State:** human-review
"#;

fn stranded_project(name: &str) -> RemedyWorld {
    let world = RemedyWorld::at(name, "project");
    let agent = write_python_agent(&world.dir, "agent.py", "sys.stdout.write('AGENT\\n')\n");
    settings(&world.root, &agent);
    world.write("states.yaml", STRANDED_MACHINE);
    world.write("index.panta.md", "# Panta: Stranded\n");
    world.write("plan.rhei.md", STRANDED_PLAN);
    world.write("sibling.rhei.md", SIBLING_PLAN);
    world
}

/// The halt message, the dry run and the report's Attention table are one
/// classification, and the row names its command wherever it is read from.
#[test]
#[ignore = "red until #549: the row's `rhei reset` names no target; implement removes this"]
fn the_reset_remedy_of_a_stranded_supervisors_halt_row_runs_on_every_surface() {
    let world = stranded_project("reset-remedy-stranded");

    let halted = world.rhei("run", &["--no-tui", "--no-callbacks"]);
    let halt = format!("{}{}", halted.stdout, halted.stderr);
    assert!(
        halt.contains("released its subtree on a visit that changed nothing"),
        "fixture: this is the halt row under test:\n{halt}"
    );
    assert_offers_runnable_reset(&halt, &world.home, "plan", &[]);

    let report = world.report();
    assert!(
        report.contains("released its subtree on a visit that changed nothing"),
        "fixture: the report's Attention table carries the row:\n{report}"
    );
    assert_offers_runnable_reset(&report, &world.home, "plan", &[]);

    let dry = world.rhei("run", &["--dry-run", "--no-tui", "--no-callbacks"]);
    let dry = format!("{}{}", dry.stdout, dry.stderr);
    assert!(
        dry.contains("released its subtree on a visit that changed nothing"),
        "fixture: the dry run carries the row:\n{dry}"
    );
    assert_offers_runnable_reset(&dry, &world.home, "plan", &[]);
}

/// Started from elsewhere with a relative target, the row is read from a report
/// long after that directory stopped mattering.
#[test]
#[ignore = "red until #549: the row's `rhei reset` names no target; implement removes this"]
fn the_reset_remedy_of_a_stranded_supervisors_halt_row_runs_for_a_relative_target() {
    let world = stranded_project("reset-remedy-stranded-relative");
    let world = RemedyWorld { cwd: world.dir.to_path_buf(), target: "project".into(), ..world };

    let halted = world.rhei("run", &["--no-tui", "--no-callbacks"]);
    let halt = format!("{}{}", halted.stdout, halted.stderr);
    assert!(
        halt.contains("released its subtree on a visit that changed nothing"),
        "fixture: this is the halt row under test:\n{halt}"
    );

    assert_offers_runnable_reset(&world.report(), &world.home, "plan", &[]);
}
