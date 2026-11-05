//! `rhei retry`'s helps print the plan it was given, so a line it offers runs from
//! the directory the retry was typed in. §FS-rhei-errors.1.2.1 §FS-rhei-retry.2
#![cfg(unix)]

use std::fs;

use super::help_target_paste_tests::{assert_on_one_line, write, OPEN_PLAN};
use super::help_target_tests::{pasted, Case};
use super::*;

/// One program state that records its result, so a run of it finishes the ticket.
const RUNS_ONCE: &str = r#"name: runs-once
version: 1
states:
  open:
    initial: true
    description: Runs once and records that it did.
    program:
      command: ["sh", "-c", "echo ran > \"$RHEI_RESULT_PATH\""]
    program_timeout: 1m
  done:
    description: Finished.
    final: true
  failed:
    description: The program failed.
    final: true
transitions:
  - { from: open, to: done, exit_code: 0, description: It worked. }
  - { from: open, to: failed, exit_code: nonzero, description: It failed. }
"#;

/// A ticket that never ran has no visit to refill, and the line that runs it first
/// runs the plan retry was given, from the parent. §FS-rhei-errors.1.2.1 §FS-rhei-retry.2
#[test]
fn help_target_a_retry_with_nothing_to_refill_runs_the_plan_it_was_given() {
    let case = Case::new("help-target-retry");
    let project = case.root.join("project");
    write(&project, "plan.rhei.md", OPEN_PLAN);
    write(&project, "states.yaml", RUNS_ONCE);

    let out = case.rhei(&case.root, &["retry", "project/plan.rhei.md", "--task", "1"]);

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "nothing to refill:\n{rendered}");
    assert!(rendered.contains("has no spawn record"), "the refusal changed:\n{rendered}");
    let expected = "rhei run project/plan.rhei.md";
    assert_on_one_line(&rendered, &format!("run the task first with {expected};"));
    let again = pasted(expected, &case.root, &case.home);
    assert!(again.status.success(), "`{expected}` should run:\n{}", again.stderr);
    let plan = fs::read_to_string(project.join("plan.rhei.md")).expect("read the plan");
    assert!(plan.contains("**State:** done"), "the pasted run should finish the ticket:\n{plan}");
}
