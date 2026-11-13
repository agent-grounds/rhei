//! What a qualified id among loose plans must not move: a project, a `panta/`
//! child, an existing path and the ancestors still resolve as before, and every
//! input that is not a ticket keeps today's refusal (#676). §FS-rhei-panta.6.7
//! §FS-rhei-panta.6 §FS-rhei-complete.2.1

use super::loose_rhei_ticket_target_support::{Loose, NO_SINGLE_PLAN};
use super::new_tests::flattened_output;
use super::*;

/// Today's refusal, word for word, flattened. Nothing pinned it before #676.
const REFUSAL: &str = "holds 2 rheis (italy.rhei.md, rome.rhei.md) but no `index.panta.md`, \
so there is no single plan to pick. Pass one explicitly, or run `rhei init` to make the \
directory a project (writes index.panta.md)";
const REFUSAL_HELP: &str =
    "help: run `rhei init --here` to make the directory a project, or pass one of the rheis above.";

fn assert_refused_as_today(loose: &Loose, args: &[&str]) {
    let result = loose.rhei(args);
    let said = flattened_output(&result);
    assert_eq!(result.status.code(), Some(1), "`rhei {}` should refuse:\n{said}", args.join(" "));
    assert!(said.contains(REFUSAL), "`rhei {}` changed its refusal:\n{said}", args.join(" "));
    assert!(said.contains(REFUSAL_HELP), "`rhei {}` changed its help:\n{said}", args.join(" "));
}

/// With no ticket to go by — and with id filters, which choose no plan — the
/// refusal keeps its text and its help. §FS-rhei-panta.6.7
#[test]
fn ticketless_commands_and_id_filters_keep_todays_refusal() {
    let loose = Loose::new("loose-guard-ticketless");
    for args in [
        &["list"][..],
        &["next"],
        &["list", "--parent", "italy.1"],
        &["list", "--has-prior", "italy.1"],
        &["note", "--restate", "italy.1"],
    ] {
        assert_refused_as_today(&loose, args);
        let said = flattened_output(&loose.rhei(args));
        assert!(
            !said.contains("by its rhei"),
            "a ticketless command gains no ticket help:\n{said}"
        );
    }
}

/// A rhei-local id names no rhei, and a rhei is not created by a ticket.
/// §FS-rhei-panta.6.7
#[test]
fn a_bare_number_and_a_new_rhei_still_refuse() {
    let loose = Loose::new("loose-guard-bare");
    for args in [&["release", "1"][..], &["show", "1"], &["new", "Venice"]] {
        let result = loose.rhei(args);
        assert_eq!(result.status.code(), Some(1), "`rhei {}` should refuse", args.join(" "));
        assert!(flattened_output(&result).contains(NO_SINGLE_PLAN));
    }
    let basin = loose.rhei(&["new", "Gelato", "--under", "basin"]);
    assert_eq!(basin.status.code(), Some(1), "no loose rhei is the basin");
    loose.assert_rome_untouched();
}

/// With `index.panta.md`, `italy.1` resolves through the project and `venice.1`
/// gets the project's not-found error. §FS-rhei-panta.6
#[test]
fn a_project_resolves_as_before() {
    let loose = Loose::new("loose-guard-project").with("index.panta.md", "# Panta: Trip\n");
    let italy = loose.rhei(&["show", "italy.1"]);
    assert_success(&italy);
    assert!(italy.stdout.contains("Task italy.1: Book the trains"), "{}", italy.stdout);
    let venice = flattened_output(&loose.rhei(&["show", "venice.1"]));
    assert!(venice.contains("task 'venice.1' not found in this project"), "{venice}");
}

/// A `panta/` child project still wins over two loose plans beside it.
/// §FS-rhei-panta.6
#[test]
fn a_panta_child_project_wins_over_loose_plans() {
    let loose =
        Loose::new("loose-guard-child").with("panta/index.panta.md", "# Panta: Trip\n").with(
            "panta/milan.rhei.md",
            "# Rhei: Milan\n\n## Tasks\n\n### Task 1: Opera\n**State:** draft\n",
        );
    let milan = loose.rhei(&["show", "milan.1"]);
    assert_success(&milan);
    let italy = flattened_output(&loose.rhei(&["show", "italy.1"]));
    assert!(italy.contains("task 'italy.1' not found in this project"), "{italy}");
}

/// A positional that names an existing path is the plan, however id-shaped.
/// §FS-rhei-complete.2.1
#[test]
fn an_existing_path_still_wins_over_the_id() {
    let loose = Loose::new("loose-guard-path");
    std::fs::create_dir_all(loose.dir.join("italy.1")).expect("a directory named like the id");
    let said = flattened_output(&loose.rhei(&["show", "italy.1"]));
    assert!(said.contains("'italy.1' is a plan path; name the ticket too"), "{said}");
}

/// A lone loose rhei is picked by rule 4 whatever the id says. §FS-rhei-panta.6
#[test]
fn a_lone_loose_rhei_is_still_picked_without_the_ticket() {
    let loose = Loose::new("loose-guard-lone");
    std::fs::remove_file(loose.dir.join("italy.rhei.md")).expect("leave rome alone");
    let said = flattened_output(&loose.rhei(&["show", "italy.1"]));
    assert!(said.contains("task 'italy.1' not found in this rhei"), "{said}");
}

/// An unmatched prefix below a project refuses there; it never climbs to the
/// ancestor's rhei of that id. §FS-rhei-panta.6.7
#[test]
fn an_unmatched_prefix_never_climbs_to_an_ancestor() {
    let loose = Loose::new("loose-guard-ancestor");
    let project = loose.dir.parent().expect("the loose directory has a parent").to_path_buf();
    let sub = project.join("sub");
    std::fs::rename(&loose.dir, &sub).expect("move the loose plans below the project");
    write_fixture_file(&project, "index.panta.md", "# Panta: Trip\n");
    let venice = "# Rhei: Venice\n\n## Tasks\n\n### Task 1: Gondola\n**State:** draft\n";
    write_fixture_file(&project, "venice.rhei.md", venice);

    let result = loose.rhei_in(&sub, &["show", "venice.1"], &[]);
    assert_eq!(result.status.code(), Some(1), "venice.1 must not climb");
    assert!(!result.stdout.contains("Gondola"), "the ancestor's venice was read");
}
