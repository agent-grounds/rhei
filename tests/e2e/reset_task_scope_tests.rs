//! `rhei reset --task`: a reset narrower than a rhei, which takes the named
//! tickets, what depends on them and the finished ancestors over them back to
//! their authored states and leaves every other ticket as it stands
//! (agent-grounds/rhei#679).

// §FS-rhei-reset.2.5 §FS-rhei-reset.2.5.1 §FS-rhei-reset.2.5.2 §FS-rhei-reset.2.1 §FS-rhei-reset.4

use std::fs;

use super::reset_task_scope_support::*;

/// The issue's repro: a new version of the paper arrives, so `ingest` and the two
/// tickets that read it run again, and `venue` keeps everything it had.
#[test]
#[ignore = "red until #679: `rhei reset --task` is an unknown argument; implement removes this"]
fn the_issues_repro_resets_ingest_and_its_dependents_and_keeps_venue_byte_for_byte() {
    let world = ScopeWorld::paper("reset-task-repro");
    let plan_before = world.read("paper.rhei.md");
    let venue_before = task_block(&plan_before, "### Task 1: venue").to_string();
    let venue_result = fs::read(world.path("runtime/results/paper.1.md")).expect("venue result");
    let venue_rows = ledger_rows(&world, "paper.1");
    let account = world.snapshot(".agent-grounds/rhei/budgets");

    let preview = world.reset("paper.rhei.md", &["--task", "paper.2", "--dry-run"]);
    assert!(preview.status.success(), "the dry run goes through:\n{}", preview.stderr);
    assert_eq!(
        preview.stdout,
        "Would reset 3 task(s) to their authored states.\n\
         Would move 3 task(s) back:\n\
         \x20 Task paper.2: completed → pending\n\
         \x20 Task paper.3: completed → pending   (prior: paper.2)\n\
         \x20 Task paper.4: completed → pending   (prior: paper.2)\n\
         Kept 1 task(s) as they stand, with their results and logs: paper.1\n\
         Would remove per-ticket runtime artifacts (results, ledgers).\n\
         \n\
         Dry run — nothing was changed.\n"
    );
    assert_eq!(world.read("paper.rhei.md"), plan_before, "a dry run changes nothing");

    let reset = world.reset("paper.rhei.md", &["--task", "paper.2", "--yes"]);
    assert_line(&reset, "Kept 1 task(s) as they stand, with their results and logs: paper.1");

    let plan = world.read("paper.rhei.md");
    assert_eq!(task_block(&plan, "### Task 1: venue"), venue_before, "venue's state and link");
    assert_eq!(
        fs::read(world.path("runtime/results/paper.1.md")).expect("venue result kept"),
        venue_result,
        "venue's result, byte for byte"
    );
    assert_eq!(ledger_rows(&world, "paper.1"), venue_rows, "venue's ledger rows");
    for ticket in ["paper.2", "paper.3", "paper.4"] {
        assert_eq!(world.state_of("paper.rhei.md", ticket), "pending", "{ticket} goes back");
        assert!(!world.exists(&format!("runtime/results/{ticket}.md")), "{ticket}'s result");
        assert!(ledger_rows(&world, ticket).is_empty(), "{ticket}'s ledger rows");
    }
    // §FS-rhei-reset.2.1: a narrowed reset removes no budget receipts.
    assert_eq!(world.snapshot(".agent-grounds/rhei/budgets"), account, "the account");

    let validated = world.rhei(&["validate", "paper.rhei.md"]);
    let said = format!("{}{}", validated.stdout, validated.stderr);
    assert!(
        validated.status.success() && !said.contains("prerequisites are unsatisfied"),
        "the plan it leaves does not contradict itself:\n{said}"
    );
}

/// Priors of a named ticket are never pulled in: `paper.3` goes back alone.
#[test]
#[ignore = "red until #679: `rhei reset --task` is an unknown argument; implement removes this"]
fn a_named_tickets_priors_are_never_pulled_in() {
    let world = ScopeWorld::paper("reset-task-no-priors");
    let ingest_result = world.read("runtime/results/paper.2.md");

    let reset = world.reset("paper.rhei.md", &["--task", "paper.3", "--yes"]);
    assert_line(&reset, "  Task paper.3: completed → pending");
    assert_line(
        &reset,
        "Kept 3 task(s) as they stand, with their results and logs: paper.1, paper.2, paper.4",
    );

    assert_eq!(world.state_of("paper.rhei.md", "paper.2"), "completed");
    assert_eq!(world.read("runtime/results/paper.2.md"), ingest_result, "ingest's result");
    assert_eq!(world.state_of("paper.rhei.md", "paper.3"), "pending");
}

/// `a → b → c`: each ticket names the edge that reached it, and `d` is untouched.
#[test]
#[ignore = "red until #679: `rhei reset --task` is an unknown argument; implement removes this"]
fn a_transitive_chain_is_reset_with_the_edge_that_reached_each_ticket() {
    let world = ScopeWorld::new("reset-task-chain");
    world.write(
        "t.rhei.md",
        "# Rhei: Chain\n\n## Tasks\n\n### Task 1: a\n**State:** pending\n\n\
         ### Task 2: b\n**State:** pending\n**Prior:** Task 1\n\n\
         ### Task 3: c\n**State:** pending\n**Prior:** Task 2\n\n\
         ### Task 4: d\n**State:** pending\n",
    );
    world.finish("t.rhei.md", &["t.1", "t.2", "t.3", "t.4"]);

    let preview = world.reset("t.rhei.md", &["--task", "t.1", "--dry-run"]);
    assert_line(&preview, "Would reset 3 task(s) to their authored states.");
    assert_line(&preview, "  Task t.1: completed → pending");
    assert_line(&preview, "  Task t.2: completed → pending   (prior: t.1)");
    assert_line(&preview, "  Task t.3: completed → pending   (prior: t.2)");
    assert_line(&preview, "Kept 1 task(s) as they stand, with their results and logs: t.4");
}

/// A ticket with children under it, and a dependent with a child of its own.
const FAMILY: &str = "# Rhei: Family

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: parent
**State:** pending

#### Task 1.1: first child
**State:** pending

#### Task 1.2: second child
**State:** pending

### Task 2: after the parent
**State:** pending
**Prior:** Task 1

#### Task 2.1: its child
**State:** pending

### Task 3: unrelated
**State:** pending
";

fn finished_family(name: &str) -> ScopeWorld {
    let world = ScopeWorld::new(name);
    world.write("x.rhei.md", FAMILY);
    world.finish("x.rhei.md", &["x.1.1", "x.1.2", "x.1", "x.2.1", "x.2", "x.3"]);
    world
}

#[test]
#[ignore = "red until #679: `rhei reset --task` is an unknown argument; implement removes this"]
fn children_of_a_named_ticket_and_of_a_dependent_are_reset() {
    let world = finished_family("reset-task-children");

    let preview = world.reset("x.rhei.md", &["--task", "x.1", "--dry-run"]);
    assert_line(&preview, "  Task x.1: completed → pending");
    assert_line(&preview, "  Task x.1.1: completed → pending   (child of x.1)");
    assert_line(&preview, "  Task x.1.2: completed → pending   (child of x.1)");
    assert_line(&preview, "  Task x.2: completed → pending   (prior: x.1)");
    assert_line(&preview, "  Task x.2.1: completed → pending   (child of x.2)");
    assert_line(&preview, "Kept 1 task(s) as they stand, with their results and logs: x.3");
}

/// The finished parent goes back alone: its other child keeps its result, and
/// the parent's own dependent comes along.
#[test]
#[ignore = "red until #679: `rhei reset --task` is an unknown argument; implement removes this"]
fn a_finished_ancestor_is_taken_alone_and_its_other_children_are_kept() {
    let world = finished_family("reset-task-ancestor");
    let sibling = fs::read(world.path("runtime/results/x.1.1.md")).expect("sibling result");

    let preview = world.reset("x.rhei.md", &["--task", "x.1.2", "--dry-run"]);
    assert_line(&preview, "  Task x.1: completed → pending   (finished parent of x.1.2)");
    assert_line(&preview, "  Task x.1.2: completed → pending");
    assert_line(&preview, "  Task x.2: completed → pending   (prior: x.1)");
    assert_line(&preview, "  Task x.2.1: completed → pending   (child of x.2)");
    assert!(!preview.stdout.contains("Task x.1.1:"), "the sibling is no move:\n{}", preview.stdout);

    let reset = world.reset("x.rhei.md", &["--task", "x.1.2", "--yes"]);
    assert!(reset.status.success(), "the reset goes through:\n{}", reset.stderr);
    assert_eq!(world.state_of("x.rhei.md", "x.1.1"), "completed");
    assert_eq!(fs::read(world.path("runtime/results/x.1.1.md")).expect("kept"), sibling);
    assert_eq!(world.state_of("x.rhei.md", "x.1"), "pending");
    assert!(!world.exists("runtime/results/x.1.md"), "the parent loses its result");
}

/// A finished parent with no ledger row has no state to go back to: the
/// refusal of §FS-rhei-reset.2.3 stands, with nothing changed.
#[test]
#[ignore = "red until #679: `rhei reset --task` is an unknown argument; implement removes this"]
fn a_finished_ancestor_with_no_ledger_rows_is_still_refused() {
    let world = ScopeWorld::new("reset-task-ledgerless-ancestor");
    world.write("x.rhei.md", FAMILY);
    world.finish("x.rhei.md", &["x.1.1", "x.1.2"]);
    let plan = world.read("x.rhei.md").replacen("**State:** pending", "**State:** completed", 1);
    world.write("x.rhei.md", &plan);

    for args in [&["--task", "x.1.2", "--dry-run"][..], &["--task", "x.1.2", "--yes"][..]] {
        let refused = world.reset("x.rhei.md", args);
        assert_refused(&refused, "Refusing to reset: the plan it would leave fails validation");
    }
    assert_eq!(world.read("x.rhei.md"), plan, "nothing changed");
    assert!(world.exists("runtime/results/x.1.2.md"), "nothing deleted");
}

/// A machine whose `{task_id}-{state}` output leaves `x.build-` as the prefix.
const PREFIX_MACHINE: &str = "name: prefix
version: 1
states:
  pending:
    initial: true
    description: Open
    outputs:
      - name: note
        path: runtime/out/{task_id}-{state}.md
  completed: { final: true, description: Done }
transitions:
  - { from: pending, to: completed, description: Done }
";

/// `x.build`'s prefix matches `x.build-docs`'s files too; they are the longer
/// id's, so `--task x.build` leaves them. §FS-rhei-reset.2.1
#[test]
#[ignore = "red until #679: `rhei reset --task` is an unknown argument; implement removes this"]
fn a_prefix_matched_file_belongs_to_the_longest_ticket_id() {
    let world = ScopeWorld::new("reset-task-prefix");
    world.write("states.yaml", PREFIX_MACHINE);
    world.write(
        "x.rhei.md",
        "# Rhei: X\n\n## Tasks\n\n### Task build: build\n**State:** pending\n\n\
         ### Task build-docs: docs\n**State:** pending\n",
    );
    for ticket in ["build", "build-docs"] {
        world.write(&format!("runtime/out/x.{ticket}-pending.md"), "output\n");
        world.write(&format!("runtime/logs/task-x.{ticket}-pending.log"), "log\n");
    }
    world.finish("x.rhei.md", &["x.build", "x.build-docs"]);

    let reset = world.reset("x.rhei.md", &["--task", "x.build", "--yes"]);
    assert_line(&reset, "  Task x.build: completed → pending");

    assert!(!world.exists("runtime/out/x.build-pending.md"), "x.build's output goes");
    assert!(!world.exists("runtime/logs/task-x.build-pending.log"), "x.build's log goes");
    assert!(world.exists("runtime/out/x.build-docs-pending.md"), "x.build-docs's output stays");
    assert!(world.exists("runtime/logs/task-x.build-docs-pending.log"), "its log stays");
    assert_eq!(world.state_of("x.rhei.md", "x.build-docs"), "completed");
}
