//! Plans a reset would leave invalid, built the ways agent-grounds/rhei#569 found:
//! a parent finished by hand, a machine that retired the state the parent started
//! in, and a run and a reset under different machines. Each starts from a plan
//! `rhei validate` accepts, with results on disk for a reset to lose.

// §FS-rhei-reset.2.3

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::reset_remedy_support::*;

/// The smallest machine with a way out of `open` and none out of `done`.
pub(super) const MACHINE: &str = r#"name: reset-invalid-plan
version: 1
states:
  open:
    initial: true
    description: Not finished
  done:
    final: true
    description: Finished
transitions:
  - { from: open, to: done, description: It finished }
"#;

/// [`MACHINE`] with one more state to start in, which it then retires.
pub(super) const WITH_REVIEW: &str = r#"name: reset-invalid-plan
version: 1
states:
  review:
    description: Where the parent starts
  open:
    initial: true
    description: Not finished
  done:
    final: true
    description: Finished
transitions:
  - { from: review, to: done, description: It finished }
  - { from: open, to: done, description: It finished }
"#;

pub(super) const OPEN_PARENT: &str = r#"# Rhei: Plan

## Tasks

### Task 1: Parent
**State:** open

#### Task 1.1: Child
**State:** open
"#;

pub(super) const REVIEW_PARENT: &str = r#"# Rhei: Plan

## Tasks

### Task 1: Parent
**State:** review

#### Task 1.1: Child
**State:** open
"#;

/// What every refused reset says first. §FS-rhei-reset.4
pub(super) const REFUSAL: &str =
    "Refusing to reset: the plan it would leave fails validation, so nothing was changed.";

/// The validator's sentence for a parent the reset leaves finished over its child.
pub(super) const PARENT_OVER_CHILD: &str = "Task plan.1 is in terminal state 'done' but \
     descendant Task plan.1.1 ('Child') is in non-terminal state 'open'";

/// `plan` as a lone rhei under `machine`, with the command pointed at the plan's file.
pub(super) fn plan_world(name: &str, machine: &str, plan: &str) -> RemedyWorld {
    let world = RemedyWorld::at(name, "plans");
    world.write("states.yaml", machine);
    world.write("plan.rhei.md", plan);
    RemedyWorld { target: world.root.join("plan.rhei.md"), ..world }
}

/// Finish `ticket` through `rhei complete`, so the ledger records where it started.
pub(super) fn finish(world: &RemedyWorld, ticket: &str, extra: &[&str]) {
    let args = ["--task", ticket, "--result", "finished", "--no-callbacks"];
    world.step("complete", &[&args[..], extra].concat());
}

/// Replace `from` with `to` in the file at `relative`, the way a person edits it.
#[track_caller]
pub(super) fn edit(world: &RemedyWorld, relative: &str, from: &str, to: &str) {
    let path = world.root.join(relative);
    let text = fs::read_to_string(&path).expect("read the file to edit");
    assert!(text.contains(from), "fixture: {relative} holds {from:?}:\n{text}");
    fs::write(&path, text.replacen(from, to, 1)).expect("write the edited file");
}

/// The parent's `**State:**` line, as written under its heading.
pub(super) fn parent_line(state: &str) -> String {
    format!("### Task 1: Parent\n**State:** {state}\n")
}

/// Route H: the child finished through `rhei complete`, and the parent's line edited
/// to `done` by hand, so the ledger has a row for the child and none for the parent.
pub(super) fn parent_finished_by_hand(name: &str) -> RemedyWorld {
    let world = plan_world(name, MACHINE, OPEN_PARENT);
    finish(&world, "1.1", &[]);
    edit(&world, "plan.rhei.md", &parent_line("open"), &parent_line("done"));
    assert_validates(&world);
    world
}

/// Route M: both finished through `rhei complete`, then the machine retires
/// `review`, the state the parent started in.
pub(super) fn parent_started_in_a_retired_state(name: &str) -> RemedyWorld {
    let world = plan_world(name, WITH_REVIEW, REVIEW_PARENT);
    finish(&world, "1.1", &[]);
    finish(&world, "1", &[]);
    world.write("states.yaml", MACHINE);
    assert_validates(&world);
    world
}

/// Route S: no file edited. Both finished under a machine named with
/// `--state-machine`, beside a `states.yaml` that has no `review`.
pub(super) fn finished_under_another_machine(name: &str) -> RemedyWorld {
    let world = plan_world(name, MACHINE, REVIEW_PARENT);
    let with_review = world.dir.join("with-review.yaml");
    fs::write(&with_review, WITH_REVIEW).expect("write the run's machine");
    let named = ["--state-machine", with_review.to_str().expect("a UTF-8 path")];
    finish(&world, "1.1", &named);
    finish(&world, "1", &named);
    assert_validates(&world);
    world
}

/// The state `rhei list` shows for `ticket`, a subtask's indented line included.
pub(super) fn listed_state(world: &RemedyWorld, ticket: &str) -> String {
    let listing = world.rhei("list", &[]);
    let prefix = format!("Task {ticket}: ");
    let line = listing.stdout.lines().map(str::trim_start).find(|line| line.starts_with(&prefix));
    let line = line.unwrap_or_else(|| panic!("{ticket} is not listed:\n{}", listing.stdout));
    let (_, state) = line.rsplit_once('[').expect("a state in brackets");
    state.trim_end_matches(']').to_string()
}

#[track_caller]
pub(super) fn assert_validates(world: &RemedyWorld) {
    let run = world.rhei("validate", &[]);
    assert!(
        run.status.success(),
        "`rhei validate` must accept the plan:\n{}{}",
        run.stdout,
        run.stderr
    );
}

/// Every file under the world's root with its bytes: the plans and everything in
/// `runtime/`. Lock sidecars are left out, because taking a lock is not a change.
pub(super) fn snapshot(world: &RemedyWorld) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).expect("read a directory of the world").flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if path.extension().is_none_or(|ext| ext != "lock") {
                let relative = path.strip_prefix(root).expect("under the root");
                out.insert(relative.display().to_string(), fs::read(&path).expect("read"));
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(&world.root, &world.root, &mut files);
    files
}

/// `rhei reset <args>` refuses, before any preview, with every byte where it was and
/// a plan `rhei validate` still accepts. Returns what it said on stderr.
/// §FS-rhei-reset.2.3 §FS-rhei-reset.4
#[track_caller]
pub(super) fn assert_reset_refused_intact(world: &RemedyWorld, args: &[&str]) -> String {
    let before = snapshot(world);
    assert!(before.keys().any(|path| path.starts_with("runtime")), "fixture: a run left output");

    let run = world.rhei("reset", args);

    assert!(
        !run.status.success(),
        "a reset that would leave a plan `rhei validate` refuses must be refused; \
         `rhei reset {args:?}` exited 0:\n{}{}",
        run.stdout,
        run.stderr
    );
    assert!(
        run.stderr.contains(REFUSAL),
        "it says it refused and changed nothing:\n{}",
        run.stderr
    );
    assert!(
        !run.stdout.contains("Would reset") && !run.stdout.contains("Would move"),
        "the refusal comes before the preview's promise of deletion:\n{}",
        run.stdout
    );
    assert_eq!(snapshot(world), before, "a refused reset changes nothing");
    assert_validates(world);
    run.stderr
}
