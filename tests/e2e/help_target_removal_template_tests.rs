//! Refusals about a plan the command was pointed at, or one it laid: removal's two
//! alternatives, and the parse of an `--into` host or a kept template. Each line is
//! pasted from where the command was typed. §FS-rhei-errors.1.2.1
#![cfg(unix)]

use std::fs;
use std::path::PathBuf;

use super::help_target_paste_tests::{assert_on_one_line, write, OPEN_PLAN};
use super::help_target_tests::{assert_offers, discovered, pasted, Case};
use super::*;

/// Two open tickets under a machine that can cancel one.
const CANCELLABLE_MACHINE: &str = r#"name: cancellable
version: 1
states:
  open:
    initial: true
    description: Open.
  cancelled:
    description: Dropped.
    final: true
  done:
    description: Finished.
    final: true
transitions:
  - { from: open, to: done, description: Done. }
  - { from: open, to: cancelled, description: Dropped. }
"#;

const TWO_TICKETS: &str = "# Rhei: Project\n\n## Tasks\n\n### Task 1: One\n**State:** open\n\n\
                           ### Task 2: Two\n**State:** open\n";

/// A plan two levels down from where removal is typed.
fn removable(case: &Case) -> PathBuf {
    let project = case.root.join("project");
    write(&project, "plan.rhei.md", TWO_TICKETS);
    write(&project, "states.yaml", CANCELLABLE_MACHINE);
    project
}

/// A rhei id is refused as not a ticket, and the line that lists its tickets lists
/// the plan removal was given. §FS-rhei-errors.1.2.1 §FS-rhei-remove.3.2
#[test]
fn help_target_a_rhei_named_to_removal_lists_its_tickets() {
    let case = Case::new("help-target-remove-rhei");
    removable(&case);

    let out = case.rhei(&case.root, &["remove", "project/plan.rhei.md", "--task", "plan"]);

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "a rhei is not a ticket:\n{rendered}");
    let expected = "rhei list project/plan.rhei.md";
    assert_on_one_line(&rendered, &format!("`{expected}` shows them"));
    let again = pasted(expected, &case.root, &case.home);
    assert!(again.status.success(), "`{expected}` should run:\n{}", again.stderr);
    assert!(again.stdout.contains("plan.1") && again.stdout.contains("plan.2"), "{}", again.stdout);
}

/// A ticket with history is sent to its cancellation, in a form the parser takes:
/// the target, the ticket, and the state it is in. §FS-rhei-errors.1.2.1 §FS-rhei-remove.3.4
#[test]
fn help_target_a_ticket_with_history_cancels_from_where_it_was_typed() {
    let case = Case::new("help-target-remove-history");
    let project = removable(&case);
    write(&project, "runtime/results/plan.1.md", "## Result\n\nEarlier work.\n");

    let out = case.rhei(&case.root, &["remove", "project/plan.rhei.md", "--task", "1"]);

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "history refuses removal:\n{rendered}");
    let expected = "rhei transition project/plan.rhei.md --task plan.1 --from open --to <state>";
    assert_on_one_line(&rendered, &format!("cancellation state with `{expected}`"));
    let cancel = expected.replace("<state>", "cancelled");
    let again = pasted(&cancel, &case.root, &case.home);
    assert!(again.status.success(), "`{cancel}` should run:\n{}", again.stderr);
    let plan = fs::read_to_string(project.join("plan.rhei.md")).expect("read the plan");
    assert!(plan.contains("### Task 1: One\n**State:** cancelled"), "the ticket moves:\n{plan}");
}

/// A single-file template whose plan is `plan`.
fn template(case: &Case, plan: &str) {
    let part = case.root.join("part");
    write(&part, "template.yaml", "name: part\nversion: 1.0.0\ndescription: A part.\n");
    write(&part, "plan.rhei.md", plan);
}

const UNPARSEABLE: &str = "# NotARhei\n\n**State:** nope\n";

/// `--into` names a rhei whose own plan does not parse: the line validates that rhei,
/// not whatever the directory holds. §FS-rhei-errors.1.2.1 §FS-rhei-library.2.2
#[test]
fn help_target_an_into_host_that_does_not_parse_validates_the_host() {
    let case = Case::new("help-target-into-host");
    template(&case, OPEN_PLAN);
    let release = case.root.join("release");
    write(&release, "index.rhei.md", UNPARSEABLE);
    write(&release, "states.yaml", CANCELLABLE_MACHINE);

    let out = case.rhei(&case.root, &["instantiate", "./part", "--into", "release"]);

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "the host refuses:\n{rendered}");
    let expected =
        format!("rhei validate {}", shell_quote(&discovered(&release).display().to_string()));
    assert_offers(&rendered, &expected);
    let again = pasted(&expected, &case.root, &case.home);
    assert!(
        again.stderr.contains("PARSE ERROR") && again.stderr.contains("index.rhei.md"),
        "`{expected}` should report the host's parse:\n{}",
        again.stderr
    );
}

/// A template whose plan does not parse, kept with `--keep-on-error`: the line
/// validates what was laid, not the plan beside the template. §FS-rhei-errors.1.2.1
/// §FS-rhei-templates.6.1.2
#[test]
fn help_target_a_kept_template_that_does_not_parse_validates_what_it_laid() {
    let case = Case::new("help-target-kept");
    template(&case, UNPARSEABLE);
    write(&case.root, "other.rhei.md", &OPEN_PLAN.replace("Project", "Other"));

    let out =
        case.rhei(&case.root, &["instantiate", "./part", "--output", "out", "--keep-on-error"]);

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "the laid plan refuses:\n{rendered}");
    let expected = "rhei validate out/plan.rhei.md";
    assert_offers(&rendered, expected);
    let again = pasted(expected, &case.root, &case.home);
    assert!(
        again.stderr.contains("PARSE ERROR") && again.stderr.contains("out/plan.rhei.md"),
        "`{expected}` should report what was laid:\n{}",
        again.stderr
    );
}
