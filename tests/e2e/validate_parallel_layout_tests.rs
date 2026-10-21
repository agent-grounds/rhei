//! `rhei validate` names the file layout that makes `rhei run --parallel N`
//! serialize a concurrent state, before any run spends the round.
//!
//! The plan shapes are the ones agent-grounds/rhei#503 reported: a claim
//! program and a ticket whose two proposals sit in the concurrent `propose`
//! state, with both top-level tickets in `tasks/01-ticket.md`.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

/// Every state is a program, so validation resolves no agent binding.
const BLIND_ROUND_MACHINE: &str = r#"name: blind-round
version: 1

states:
  claim:
    initial: true
    description: Take the claim on the ticket.
    program:
      command: ["bash", "ok.sh"]
    program_timeout: 1m
  ticket:
    description: Holds the round.
    program:
      command: ["bash", "ok.sh"]
    program_timeout: 1m
  propose:
    description: Write one blind proposal.
    concurrent: true
    program:
      command: ["bash", "ok.sh"]
    program_timeout: 1m
  judge:
    description: Read both proposals.
    program:
      command: ["bash", "ok.sh"]
    program_timeout: 1m
  completed:
    description: Done.
    final: true
  cancelled:
    description: Dropped.
    final: true

transitions:
  - { from: claim, to: completed, exit_code: 0, description: Claimed. }
  - { from: propose, to: completed, exit_code: 0, description: Proposal written. }
  - { from: ticket, to: completed, exit_code: 0, description: Round finished. }
  - { from: judge, to: completed, exit_code: 0, description: Judged. }
  - { from: "*", to: cancelled, description: Dropped. }
"#;

const CLAIM: &str = "### Task claim: Claim the ticket\n**State:** claim\n\nTake the claim.\n";

/// The ticket and its round; `b_state` is where Proposal B currently is.
fn ticket(b_state: &str) -> String {
    format!(
        "### Task t: The ticket\n**State:** ticket\n\n\
         #### Task t.a: Proposal A\n**State:** propose\n**Provides:** proposal\n\n\
         #### Task t.b: Proposal B\n**State:** {b_state}\n**Provides:** proposal\n\n\
         #### Task t.judge: Judge the proposals\n**State:** judge\n\
         **Prior:** Task t.a, Task t.b\n**Consumes:** t.a:proposal, t.b:proposal\n"
    )
}

/// A directory plan with the blind-round machine. Returns (dir, plan, machine).
fn blind_round_plan(prefix: &str, task_files: &[(&str, &str)]) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let ws = dir.join("plan");
    fs::create_dir_all(ws.join("tasks")).expect("create plan dirs");
    fs::write(ws.join("index.rhei.md"), "# Rhei: Blind round\n").expect("write index");
    for (name, content) in task_files {
        fs::write(ws.join("tasks").join(name), content).expect("write task file");
    }
    let machine = write_fixture_file(&ws, "states.yaml", BLIND_ROUND_MACHINE);
    (dir, ws, machine)
}

fn output(result: &CliRun) -> String {
    format!("{}\n{}", result.stdout, result.stderr)
}

/// The words §FS-rhei-validate.4.5 prescribes for the warning.
fn has_layout_warning(out: &str) -> bool {
    out.contains("is concurrent, but every top-level ticket is in")
        && out.contains("rhei run drops --parallel to 1")
        && out.contains("give a top-level ticket a file of its own")
}

fn names_file(out: &str, relative: &str) -> bool {
    out.contains(relative) || out.contains(&relative.replace('/', "\\"))
}

fn assert_silent(result: &CliRun, why: &str) {
    assert_success(result);
    let out = output(result);
    assert!(!has_layout_warning(&out), "{why}:\n{out}");
    assert!(!out.contains("drops --parallel"), "{why}:\n{out}");
}

/// The report's case: both top-level tickets in one file, two tasks in the
/// concurrent `propose` state. Validation still succeeds, but names the state
/// and the shared file, and keeps the Consumes advisory as it is.
// §FS-rhei-validate.4.5 §FS-rhei-run.2.5
#[test]
#[ignore = "red until #503: validate is silent about the one-file layout; implement removes this"]
fn validate_warns_when_one_file_layout_serializes_a_concurrent_state() {
    let ticket = ticket("propose");
    let (_dir, ws, machine) = blind_round_plan(
        "validate-layout-one-file",
        &[("01-ticket.md", &format!("{CLAIM}\n{ticket}"))],
    );

    let result = run_cli("validate", &ws, &machine, &[]);
    assert_success(&result);
    let out = output(&result);
    assert!(has_layout_warning(&out), "validate must warn about the layout:\n{out}");
    assert!(out.contains("state 'propose' is concurrent"), "the warning names the state:\n{out}");
    assert!(names_file(&out, "tasks/01-ticket.md"), "the warning names the shared file:\n{out}");
    assert_eq!(
        out.matches("is concurrent, but every top-level ticket is in").count(),
        1,
        "one warning per concurrent state:\n{out}"
    );
    assert!(
        out.contains(
            "For a blind round, schedule participants concurrently and brief them not to inspect sibling exports"
        ),
        "the Consumes advisory stays beside it:\n{out}"
    );
}

/// The workaround the report names: the claim in a file of its own.
// §FS-rhei-validate.4.5
#[test]
fn validate_is_silent_when_top_level_tickets_span_two_files() {
    let ticket = ticket("propose");
    let (_dir, ws, machine) = blind_round_plan(
        "validate-layout-split",
        &[("00-claim.md", CLAIM), ("01-ticket.md", &ticket)],
    );
    assert_silent(&run_cli("validate", &ws, &machine, &[]), "a split layout runs in parallel");
}

/// §FS-rhei-run.2.5 counts only top-level tickets: one ticket with its
/// concurrent subtasks in one file already runs concurrently.
// §FS-rhei-validate.4.5
#[test]
fn validate_is_silent_for_one_ticket_with_its_concurrent_subtasks() {
    let ticket = ticket("propose");
    let (_dir, ws, machine) =
        blind_round_plan("validate-layout-one-ticket", &[("01-ticket.md", &ticket)]);
    assert_silent(&run_cli("validate", &ws, &machine, &[]), "one top-level ticket is not shared");
}

/// The one-file layout with only one task in the concurrent state has nothing
/// to serialize.
// §FS-rhei-validate.4.5
#[test]
fn validate_is_silent_when_no_concurrent_state_holds_two_tasks() {
    let ticket = ticket("completed");
    let (_dir, ws, machine) = blind_round_plan(
        "validate-layout-one-proposal",
        &[("01-ticket.md", &format!("{CLAIM}\n{ticket}"))],
    );
    assert_silent(
        &run_cli("validate", &ws, &machine, &[]),
        "one concurrent task has no overlap to lose",
    );
}

fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create copy");
    for entry in fs::read_dir(src).expect("read example") {
        let entry = entry.expect("example entry");
        let to = dst.join(entry.file_name());
        if entry.file_type().expect("entry type").is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            fs::copy(entry.path(), &to).expect("copy example file");
        }
    }
}

/// rhei's own example of `--parallel` must not be the layout that disables it.
// §FS-rhei-run.2.5 §FS-rhei-validate.4.5
#[test]
fn parallel_worktrees_example_keeps_its_parallelism() {
    let dir = unique_temp_dir("parallel-worktrees-layout");
    let ws = dir.join("parallel-worktrees-example");
    copy_tree(&repo_root().join("examples/parallel-worktrees-example"), &ws);
    let machine = ws.join("states.yaml");

    let run = run_cli("run", &ws, &machine, &["--dry-run", "--no-tui", "--parallel", "3"]);
    assert_success(&run);
    let out = output(&run);
    assert!(
        !out.contains("Falling back to sequential execution"),
        "the example must run its targets in parallel:\n{out}"
    );
    assert_silent(
        &run_cli("validate", &ws, &machine, &[]),
        "the example's layout keeps --parallel",
    );
}
