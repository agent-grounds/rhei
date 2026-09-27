//! Black-box output contract for `rhei show`: one heading, one blank line, the
//! body as stored, and nothing else — for any ticket, in any state.
//! §FS-rhei-show.3 §FS-rhei-show.5

use super::show_support::*;
use super::*;

/// The whole promise, as a golden output: the heading, a blank line, the body to
/// the byte, and nothing after it. The ticket is `completed`, which is the case
/// `rhei next --peek` refuses outright and the read the project had no verb for.
/// §FS-rhei-show.3
#[test]
fn show_prints_one_heading_the_body_and_nothing_else() {
    let (dir, plan) = show_fixture("show-golden");
    let home = dir.join(".home");

    let result =
        run_show(&home, &dir, &[plan.to_str().expect("utf-8 plan path"), "--task", "probe.2"]);

    assert_success(&result);
    assert_eq!(result.stdout, expected_ticket_2_output());
    assert_eq!(result.stderr, "", "a successful read says nothing on stderr");
}

/// The contrast the report is about: the ready-set gate still refuses a terminal
/// ticket, and `show` reads it anyway, because reading is not claiming.
/// §FS-rhei-show.2
#[test]
fn show_reads_the_terminal_ticket_the_ready_set_gate_refuses() {
    let (dir, plan) = show_fixture("show-terminal");
    let home = dir.join(".home");
    let machine = dir.join("states.yaml");

    let peek = run_cli("next", &plan, &machine, &["--task", "probe.2", "--peek"]);
    assert!(!peek.status.success(), "peek should still refuse a terminal ticket:\n{}", peek.stdout);

    let shown = run_show(&home, &dir, &["probe.2"]);
    assert_success(&shown);
    assert_eq!(shown.stdout, expected_ticket_2_output());
}

/// A ticket with nothing written under it is an answer, not a failure: the
/// heading alone, no blank line after it, exit 0. §FS-rhei-show.3
#[test]
fn show_prints_the_heading_alone_for_an_empty_body() {
    let (dir, _plan) = show_fixture("show-empty");
    let home = dir.join(".home");

    let result = run_show(&home, &dir, &["probe.3"]);

    assert_success(&result);
    assert_eq!(result.stdout, "## Task probe.3: A ticket with nothing written under it\n");
}

/// A bare rhei-local id resolves when exactly one in-scope rhei holds it, and
/// the heading prints the qualified id however the target was written.
/// §FS-rhei-show.2
#[test]
fn show_resolves_a_bare_local_id_in_a_one_rhei_project() {
    let (dir, _plan) = show_fixture("show-bare-local");
    let home = dir.join(".home");

    let result = run_show(&home, &dir, &["2"]);

    assert_success(&result);
    assert_eq!(result.stdout, expected_ticket_2_output());
}

/// A shorthand two rheis hold is refused with both qualified candidates named,
/// so the next command is a copy away. §FS-rhei-show.5
#[test]
fn show_names_the_qualified_candidates_for_an_ambiguous_bare_id() {
    let (dir, project) = ambiguous_fixture("show-ambiguous");
    let home = dir.join(".home");

    let result = run_show(&home, &project, &["7"]);

    assert!(!result.status.success(), "an ambiguous id should be refused:\n{}", result.stdout);
    let said = &result.stderr;
    assert!(said.contains("alpha.7"), "the error should name alpha.7; got:\n{said}");
    assert!(said.contains("beta.7"), "the error should name beta.7; got:\n{said}");
}

/// An id no rhei holds gets the near-miss error and the help line that names the
/// command revealing what does exist. §FS-rhei-show.5
#[test]
fn show_offers_the_closest_ids_for_an_id_no_rhei_holds() {
    let (dir, _plan) = show_fixture("show-near-miss");
    let home = dir.join(".home");

    let result = run_show(&home, &dir, &["probe.9"]);

    assert!(!result.status.success(), "an unknown id should be refused:\n{}", result.stdout);
    let said = &result.stderr;
    assert!(said.contains("not found"), "the error should say the id is not found; got:\n{said}");
    assert!(
        said.contains("rhei list"),
        "the help should name the command that lists the ids; got:\n{said}"
    );
}

/// The plan positional plus `--task`, the explicit form every ticket-taking
/// command accepts. §FS-rhei-show.1
#[test]
fn show_takes_the_plan_positional_with_task() {
    let (dir, plan) = show_fixture("show-plan-and-task");
    let home = dir.join(".home");
    let elsewhere = dir.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("a directory outside the project");

    let result = run_show(
        &home,
        &elsewhere,
        &[plan.to_str().expect("utf-8 plan path"), "--task", "probe.2"],
    );

    assert_success(&result);
    assert_eq!(result.stdout, expected_ticket_2_output());
}

/// A positional that is a real path and names no ticket asks for the ticket, in
/// the form that works when pasted. §FS-rhei-show.5
#[test]
fn show_asks_for_the_ticket_when_the_positional_is_only_a_plan() {
    let (dir, plan) = show_fixture("show-plan-only");
    let home = dir.join(".home");

    let result = run_show(&home, &dir, &[plan.to_str().expect("utf-8 plan path")]);

    assert!(
        !result.status.success(),
        "a plan with no ticket should be refused:\n{}",
        result.stdout
    );
    let said = &result.stderr;
    assert!(said.contains("rhei show"), "the error should show the form that works; got:\n{said}");
}

/// The trap in the shared positional. `list`, `render` and `validate` answer an
/// id-shaped argument with "this argument takes a plan or project, not a ticket
/// id" — the right sentence there and the opposite of the truth here, because
/// `rhei show probe.2` is the invocation this verb exists for. §FS-rhei-show.5
#[test]
fn show_never_calls_an_id_shaped_positional_a_plan_path() {
    let (dir, _plan) = show_fixture("show-id-positional");
    let home = dir.join(".home");

    let result = run_show(&home, &dir, &["probe.2"]);

    let said = &result.stderr;
    assert!(
        !said.contains("takes a plan or project"),
        "an id-shaped positional to `rhei show` is the ticket, not a bad path; got:\n{said}"
    );
    assert_success(&result);
}
