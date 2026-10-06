//! `rhei remove`: which ticket an argument names, and every refusal that keeps
//! a ticket something depends on or has acted on.
//! §FS-rhei-remove.1.2 §FS-rhei-remove.3

use std::fs;

use super::new_tests::{
    assert_failure, empty_project, flattened_output, new_run, project_with_rhei,
};
use super::*;

/// Two rheis, `auth` and `billing`, each holding a ticket `1`.
fn two_rheis(prefix: &str) -> TestDir {
    let dir = empty_project(prefix);
    assert_success(&new_run(&["new", "Authentication", "--id", "auth"], &dir));
    assert_success(&new_run(&["new", "Billing", "--id", "billing"], &dir));
    assert_success(&new_run(&["new", "Login", "--under", "auth"], &dir));
    assert_success(&new_run(&["new", "Invoice", "--under", "billing"], &dir));
    dir
}

fn plan_text(dir: &Path, rhei: &str) -> String {
    fs::read_to_string(dir.join(format!("{rhei}.rhei.md"))).expect("plan")
}

/// §FS-rhei-remove.1.2: a local id that matches in two rheis is refused,
/// naming both and `--rhei`; `--rhei` and a qualified id each name one.
#[test]
fn a_local_id_in_two_rheis_is_ambiguous_until_narrowed() {
    let dir = two_rheis("remove-ambiguous");
    let result = new_run(&["remove", "1"], &dir);
    assert_failure(&result, "ambiguous");
    let said = flattened_output(&result);
    assert!(said.contains("auth.1") && said.contains("billing.1"), "names both; got:\n{said}");
    assert!(said.contains("--rhei"), "points at --rhei; got:\n{said}");
    assert!(
        plan_text(&dir, "auth").contains("Login") && plan_text(&dir, "billing").contains("Invoice")
    );

    let narrowed = new_run(&["remove", "1", "--rhei", "billing"], &dir);
    assert_success(&narrowed);
    assert!(narrowed.stdout.contains("removed billing.1; id retired"), "got:\n{}", narrowed.stdout);
    assert!(plan_text(&dir, "auth").contains("Login"), "the other rhei's ticket stays");

    let qualified = new_run(&["remove", "auth.1"], &dir);
    assert_success(&qualified);
    assert!(!plan_text(&dir, "auth").contains("Login"));
}

/// §FS-rhei-remove.1.2: a retired id is no longer a ticket, so once
/// `billing.1` is gone the bare `1` names the one live `auth.1`.
#[test]
fn a_retired_id_does_not_make_a_local_id_ambiguous() {
    let dir = two_rheis("remove-ambiguous-retired");
    assert_success(&new_run(&["remove", "billing.1"], &dir));
    let result = new_run(&["remove", "1"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("removed auth.1; id retired"), "got:\n{}", result.stdout);
}

/// §FS-rhei-remove.1.2: the path form — a plan path plus `--task` — names the
/// ticket in that plan, and a unique local id resolves without a qualifier.
#[test]
fn resolves_a_local_id_and_the_path_with_task_form() {
    let dir = project_with_rhei("remove-local");
    assert_success(&new_run(&["new", "One", "--under", "auth"], &dir));
    assert_success(&new_run(&["new", "Two", "--under", "auth"], &dir));

    let local = new_run(&["remove", "2"], &dir);
    assert_success(&local);
    assert!(local.stdout.contains("removed auth.2; id retired"), "got:\n{}", local.stdout);

    let by_path = new_run(&["remove", "auth.rhei.md", "--task", "1"], &dir);
    assert_success(&by_path);
    assert!(by_path.stdout.contains("removed auth.1; id retired"), "got:\n{}", by_path.stdout);
}

/// §FS-rhei-remove.3.2: a rhei is not a ticket.
#[test]
fn refuses_a_rhei_id() {
    let dir = project_with_rhei("remove-rhei-node");
    let before = fs::read_to_string(dir.join("auth.rhei.md")).expect("plan");
    let result = new_run(&["remove", "auth"], &dir);
    assert_failure(&result, "it is a rhei, not a ticket");
    assert_eq!(fs::read_to_string(dir.join("auth.rhei.md")).expect("plan"), before);
}

/// §FS-rhei-remove.3.1: a `**Consumes:**` reader is a dependent too, and every
/// field that names the ticket is reported, not only the first.
#[test]
fn refuses_a_producer_another_ticket_consumes() {
    let dir = project_with_rhei("remove-consumes");
    assert_success(&new_run(
        &["new", "Produce", "--under", "auth", "--provides", "findings"],
        &dir,
    ));
    assert_success(&new_run(
        &["new", "Read", "--under", "auth", "--prior", "auth.1", "--consumes", "auth.1:findings"],
        &dir,
    ));
    let before = plan_text(&dir, "auth");

    let result = new_run(&["remove", "auth.1"], &dir);
    assert!(!result.status.success(), "a consumer must refuse removal\n{}", result.stdout);
    let said = flattened_output(&result);
    assert!(said.contains("auth.2 names it in **Prior:**"), "got:\n{said}");
    assert!(said.contains("auth.2 names it in **Consumes:**"), "got:\n{said}");
    assert_eq!(plan_text(&dir, "auth"), before, "a refusal changes nothing");
}

/// §FS-rhei-remove.2: a terminal state is a recorded outcome even with no
/// ledger row, result file or log behind it.
#[test]
fn refuses_a_terminal_ticket_with_no_files_behind_it() {
    let dir = project_with_rhei("remove-terminal");
    assert_success(&new_run(&["new", "Hand-closed", "--under", "auth"], &dir));
    let plan = dir.join("auth.rhei.md");
    let text = fs::read_to_string(&plan)
        .expect("plan")
        .replace("**State:** pending", "**State:** completed");
    fs::write(&plan, &text).expect("hand-edit the state");
    assert!(!dir.join("runtime").exists(), "nothing but the plan records it");

    let result = new_run(&["remove", "auth.1"], &dir);
    assert_failure(&result, "auth.1 cannot be removed");
    let said = flattened_output(&result);
    assert!(said.contains("completed is terminal"), "names the evidence; got:\n{said}");
    assert!(said.contains("rhei transition auth.1"), "points at cancellation; got:\n{said}");
    assert_eq!(fs::read_to_string(&plan).expect("plan"), text);
}

/// Hold `path` exclusively the way another rhei process would.
fn hold(path: &Path) -> fs::File {
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .expect("open lock");
    fs2::FileExt::try_lock_exclusive(&file).expect("hold lock");
    file
}

/// §FS-rhei-remove.3.3, §FS-rhei-remove.6.1: a live run, or a writer holding
/// the plan's sidecar, refuses removal without waiting, and nothing changes.
#[test]
fn refuses_while_a_run_or_a_writer_holds_the_project() {
    let dir = project_with_rhei("remove-locked");
    assert_success(&new_run(&["new", "Mistake", "--under", "auth"], &dir));
    let before = plan_text(&dir, "auth");

    let run = super::summary_repricing_support::hold_run_lock(&dir, "another-run");
    let result = new_run(&["remove", "auth.1"], &dir);
    assert_failure(&result, "auth.1 cannot be removed");
    assert!(flattened_output(&result).contains("run.lock"), "names the run lock");
    drop(run);

    let writer = hold(&dir.join("auth.rhei.md.lock"));
    let result = new_run(&["remove", "auth.1"], &dir);
    assert_failure(&result, "a writer holds");
    drop(writer);

    assert_eq!(plan_text(&dir, "auth"), before, "a refusal changes nothing");
    assert!(!dir.join(".rhei/pending-removal.json").exists(), "no marker is left");
    assert_success(&new_run(&["remove", "auth.1"], &dir));
}
