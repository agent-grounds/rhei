//! A plan `rhei validate` refuses before any reset is one `rhei reset` refuses too:
//! with `--dry-run` and with `--yes`, in validate's own report, before any preview,
//! and with every byte where it was (agent-grounds/rhei#572). The four shapes are
//! the ones the issue's reproducer tries: a parent finished over an open child, a
//! prior naming a task the plan does not have, two tasks that are each other's
//! prior, and a state the machine does not declare.
//!
//! The controls pass today and must keep passing: the check reads no link, and the
//! markers of an unfinished recovery or removal still come first.

// §FS-rhei-reset.2 §FS-rhei-reset.4

use std::fs;

use super::reset_invalid_plan_support::*;
use super::reset_remedy_support::*;
use super::*;

/// The result a run left, which nothing else holds a copy of.
const RESULT: &str = "runtime/results/plan.1.md";
const ONLY_COPY: &str = "the only copy\n";

/// What a semantic validation failure says first, as `rhei validate` reports it.
/// §FS-rhei-validate.6
const VALIDATE_REPORT: &str = "I validated this plan using";

const FINISHED_PARENT: &str = "# Rhei: Plan\n\n## Tasks\n\n### Task 1: Parent\n**State:** done\n\n\
     #### Task 1.1: Child\n**State:** open\n";

const MISSING_PRIOR: &str = "# Rhei: Plan\n\n## Tasks\n\n### Task 1: First\n**State:** open\n\
     **Prior:** plan.9\n\n### Task 2: Second\n**State:** open\n";

const CYCLE: &str = "# Rhei: Plan\n\n## Tasks\n\n### Task 1: First\n**State:** open\n\
     **Prior:** plan.2\n\n### Task 2: Second\n**State:** open\n**Prior:** plan.1\n";

const UNDECLARED_STATE: &str = "# Rhei: Plan\n\n## Tasks\n\n### Task 1: First\n**State:** bogus\n";

/// `plan` under [`MACHINE`], with a result on disk for a reset to lose, and
/// `rhei validate` already refusing it for `problem`: a shape validate accepts
/// would prove nothing here.
fn refused_plan(name: &str, plan: &str, problem: &str) -> RemedyWorld {
    let world = plan_world(name, MACHINE, plan);
    world.write(RESULT, ONLY_COPY);
    let validate = world.rhei("validate", &[]);
    assert!(
        !validate.status.success() && validate.stderr.contains(problem),
        "fixture: `rhei validate` refuses the plan for {problem:?}:\n{}{}",
        validate.stdout,
        validate.stderr
    );
    world
}

/// `rhei reset --dry-run` and `rhei reset --yes` each refuse with validate's report
/// naming `problem`, print no preview, and change no byte: the plan and the result
/// stay as they were. §FS-rhei-reset.2 §FS-rhei-reset.4
#[track_caller]
fn assert_reset_refuses_as_validate_does(world: &RemedyWorld, problem: &str) {
    let plan = fs::read(world.root.join("plan.rhei.md")).expect("read the plan");
    for args in [&["--dry-run"][..], &["--yes"][..]] {
        let before = snapshot(world);

        let run = world.rhei("reset", args);

        assert!(
            !run.status.success(),
            "a plan `rhei validate` refuses is refused by `rhei reset {args:?}` too, \
             which exited 0:\n{}{}",
            run.stdout,
            run.stderr
        );
        assert!(
            run.stderr.contains(VALIDATE_REPORT) && run.stderr.contains(problem),
            "`rhei reset {args:?}` says what validate says, {problem:?}:\n{}",
            run.stderr
        );
        assert!(
            !run.stderr.contains(REFUSAL),
            "an error the plan already carries is not one the reset would introduce:\n{}",
            run.stderr
        );
        assert!(
            !run.stdout.contains("Would reset") && !run.stdout.contains("Would delete"),
            "`rhei reset {args:?}` refuses before any preview:\n{}",
            run.stdout
        );
        assert_eq!(snapshot(world), before, "`rhei reset {args:?}` changes nothing");
    }
    let result = fs::read_to_string(world.root.join(RESULT)).expect("the result survives");
    assert_eq!(result, ONLY_COPY, "the result is byte-identical");
    assert_eq!(fs::read(world.root.join("plan.rhei.md")).expect("read"), plan, "the plan too");
}

/// The issue's own shape.
#[test]
fn a_parent_finished_over_an_open_child_is_refused_before_any_reset() {
    let world = refused_plan("reset-standing-parent", FINISHED_PARENT, PARENT_OVER_CHILD);
    assert_reset_refuses_as_validate_does(&world, PARENT_OVER_CHILD);
}

#[test]
fn a_prior_naming_a_missing_task_is_refused_before_any_reset() {
    let problem = "Task plan.1 depends on missing Task plan.9";
    let world = refused_plan("reset-standing-prior", MISSING_PRIOR, problem);
    assert_reset_refuses_as_validate_does(&world, problem);
}

#[test]
fn two_tasks_that_are_each_others_prior_are_refused_before_any_reset() {
    let problem = r#"Circular dependency detected among tasks: ["plan.1", "plan.2"]"#;
    let world = refused_plan("reset-standing-cycle", CYCLE, problem);
    assert_reset_refuses_as_validate_does(&world, problem);
}

#[test]
fn a_state_the_machine_does_not_declare_is_refused_before_any_reset() {
    let problem = "Task plan.1 has invalid state 'bogus'. Allowed: [open, done]";
    let world = refused_plan("reset-standing-undeclared", UNDECLARED_STATE, problem);
    assert_reset_refuses_as_validate_does(&world, problem);
}

/// Step 1 checks the whole project graph, as `rhei validate` does, so a narrowed
/// reset of a sound rhei is refused while another rhei is invalid; that is what
/// `rhei complete` does to a ticket of `a` too. §FS-rhei-panta.6.4
#[test]
fn a_narrowed_reset_is_refused_while_another_rhei_is_invalid() {
    let world = RemedyWorld::at("reset-standing-project", "project");
    world.write("index.panta.md", "# Panta: Two\n");
    world.write("states.yaml", MACHINE);
    world.write("a.rhei.md", "# Rhei: A\n\n## Tasks\n\n### Task 1: Alone\n**State:** open\n");
    world.write("b.rhei.md", "# Rhei: B\n\n## Tasks\n\n### Task 1: First\n**State:** open\n");
    finish(&world, "a.1", &[]);
    edit(&world, "b.rhei.md", "**State:** open", "**State:** bogus");
    let problem = "Task b.1 has invalid state 'bogus'. Allowed: [open, done]";

    for args in [&["--rhei", "a", "--dry-run"][..], &["--rhei", "a", "--yes"][..]] {
        let before = snapshot(&world);
        let run = world.rhei("reset", args);
        assert!(
            !run.status.success() && run.stderr.contains(problem),
            "`rhei reset {args:?}` is refused for b's state:\n{}{}",
            run.stdout,
            run.stderr
        );
        assert_eq!(snapshot(&world), before, "`rhei reset {args:?}` changes nothing");
    }
    assert!(world.root.join("runtime/results/a.1.md").exists(), "a keeps its result");
}

/// The control for links: a `> **Result:**` link into a `runtime/` that is gone is
/// what `rhei validate` refuses and what a reset clears, so the reset goes through.
/// Passes today; it must keep passing. §FS-rhei-reset.2.3
#[test]
fn a_result_link_into_a_runtime_that_is_gone_does_not_refuse_the_reset() {
    let linked = "# Rhei: Plan\n\n## Tasks\n\n### Task 1: First\n**State:** done\n\n\
         > **Result:** [plan.1](runtime/results/plan.1.md)\n\n### Task 2: Second\n**State:** open\n";
    let world = plan_world("reset-standing-link", MACHINE, linked);
    let validate = world.rhei("validate", &[]);
    assert!(
        !validate.status.success() && validate.stderr.contains("does not exist"),
        "fixture: `rhei validate` refuses the dangling link:\n{}",
        validate.stderr
    );

    assert_success(&world.rhei("reset", &["--dry-run"]));
    assert_success(&world.rhei("reset", &["--yes"]));

    let plan = fs::read_to_string(world.root.join("plan.rhei.md")).expect("read the plan");
    assert!(!plan.contains("> **Result:**"), "the reset strips the link:\n{plan}");
    assert_validates(&world);
}

/// The control for order: a pending removal or forced recovery is refused before
/// the plan is judged, with the one command that resumes it, whatever else is
/// wrong with the plan. Passes today; it must keep passing.
/// §FS-rhei-remove.6.2 §FS-rhei-recover.4
#[test]
fn an_unfinished_removal_or_recovery_is_refused_before_the_invalid_plan() {
    let markers = [
        (".rhei/pending-removal.json", "{\"ticket\":\"plan.1.1\"}", "rhei remove plan.1.1"),
        (".rhei/forced-recovery.json", "{}", "rhei recover "),
    ];
    for (marker, contents, resume) in markers {
        let world = refused_plan("reset-standing-marker", FINISHED_PARENT, PARENT_OVER_CHILD);
        world.write(marker, contents);
        for args in [&["--dry-run"][..], &["--yes"][..]] {
            let before = snapshot(&world);
            let run = world.rhei("reset", args);
            assert!(
                !run.status.success()
                    && run.stderr.contains(resume)
                    && !run.stderr.contains(VALIDATE_REPORT),
                "`rhei reset {args:?}` names `{resume}` first, under {marker}:\n{}{}",
                run.stdout,
                run.stderr
            );
            assert_eq!(snapshot(&world), before, "`rhei reset {args:?}` changes nothing");
        }
    }
}
