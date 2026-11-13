//! A narrowed reset under a machine whose artifact path puts a placeholder
//! before `{task_id}`: it removes the in-scope tickets' files and nothing else,
//! under `--task` and under `--rhei` alike (agent-grounds/rhei#679).

// §FS-rhei-reset.2.1 §FS-rhei-reset.2.5

use super::reset_task_scope_support::*;

/// A machine whose output puts `{state}` before `{task_id}`, so the literal text up
/// to its first placeholder is `runtime/`, which no ticket id stands in.
const STATE_FIRST_MACHINE: &str = "name: state-first
version: 1
states:
  pending:
    initial: true
    description: Open
    outputs:
      - name: note
        path: runtime/{state}/{task_id}.md
  completed: { final: true, description: Done }
transitions:
  - { from: pending, to: completed, description: Done }
";

/// The two-ticket plan, each ticket's output written where the machine declares
/// it and each ticket finished.
fn state_first_world(
    name: &str,
    target: &str,
    plan: &[(&str, &str)],
    tickets: &[&str],
) -> ScopeWorld {
    let world = ScopeWorld::new(name);
    world.write("states.yaml", STATE_FIRST_MACHINE);
    for (file, text) in plan {
        world.write(file, text);
    }
    for ticket in tickets {
        world.write(&format!("runtime/pending/{ticket}.md"), &format!("output of {ticket}\n"));
    }
    world.finish(target, tickets);
    world
}

/// `--task paper.2` removes `runtime/pending/paper.2.md` and its result, and
/// `paper.1` keeps its result, its output and its ledger rows: the placeholder
/// before the id matches one path component, never the whole `runtime/` tree.
/// §FS-rhei-reset.2.1 §FS-rhei-reset.2.5
#[test]
fn a_placeholder_before_the_id_matches_one_component_under_a_ticket_scope() {
    let plan = "# Rhei: Paper review\n\n## Tasks\n\n### Task 1: venue\n**State:** pending\n\n\
                ### Task 2: ingest\n**State:** pending\n";
    let world = state_first_world(
        "reset-task-state-first",
        "paper.rhei.md",
        &[("paper.rhei.md", plan)],
        &["paper.1", "paper.2"],
    );
    let result = world.read("runtime/results/paper.1.md");
    let output = world.read("runtime/pending/paper.1.md");
    let rows = ledger_rows(&world, "paper.1");
    assert!(!rows.is_empty(), "paper.1 has ledger rows to keep");

    let reset = world.reset("paper.rhei.md", &["--task", "paper.2", "--yes"]);
    assert_line(&reset, "  Task paper.2: completed → pending");
    assert!(
        !reset.stdout.contains("Kept records keyed by bare local ids"),
        "no bare-id record is there to keep:\n{}",
        reset.stdout
    );

    assert_eq!(world.read("runtime/results/paper.1.md"), result, "paper.1's result is kept");
    assert_eq!(world.read("runtime/pending/paper.1.md"), output, "paper.1's output is kept");
    assert_eq!(ledger_rows(&world, "paper.1"), rows, "paper.1's ledger rows are kept");
    assert!(!world.exists("runtime/pending/paper.2.md"), "paper.2's output goes");
    assert!(!world.exists("runtime/results/paper.2.md"), "paper.2's result goes");
    assert_eq!(world.state_of("paper.rhei.md", "paper.1"), "completed");

    let validate = world.rhei(&["validate", "paper.rhei.md"]);
    assert!(validate.status.success(), "the plan still validates:\n{}", validate.stderr);
}

/// `--rhei a` in a panta of `a` and `b` that share one execution root leaves
/// `b.1`'s result and output where they are. §FS-rhei-reset.2.1
#[test]
fn a_placeholder_before_the_id_keeps_another_rheis_files_under_rhei_scope() {
    let world = state_first_world(
        "reset-rhei-state-first",
        ".",
        &[
            ("index.panta.md", "# Panta: Scope\n"),
            ("a.rhei.md", "# Rhei: A\n\n## Tasks\n\n### Task 1: one\n**State:** pending\n"),
            ("b.rhei.md", "# Rhei: B\n\n## Tasks\n\n### Task 1: other\n**State:** pending\n"),
        ],
        &["a.1", "b.1"],
    );
    let result = world.read("runtime/results/b.1.md");
    let output = world.read("runtime/pending/b.1.md");

    let reset = world.reset(".", &["--rhei", "a", "--yes"]);
    assert_line(&reset, "  Task a.1: completed → pending");

    assert_eq!(world.read("runtime/results/b.1.md"), result, "b.1's result is kept");
    assert_eq!(world.read("runtime/pending/b.1.md"), output, "b.1's output is kept");
    assert!(!world.exists("runtime/pending/a.1.md"), "a.1's output goes");
    assert!(!world.exists("runtime/results/a.1.md"), "a.1's result goes");
    assert_eq!(world.state_of(".", "b.1"), "completed");

    let validate = world.rhei(&["validate", "."]);
    assert!(validate.status.success(), "the project still validates:\n{}", validate.stderr);
}
