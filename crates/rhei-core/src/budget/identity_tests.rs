//! One identity, one live ticket — at the function that decides it.
//!
//! Every case here turns on one number: how many live tickets claim the uuid.
//! The document cannot say more than that, because a definition copied onto a
//! sibling and a definition renumbered in place are the same bytes at the same
//! path, so each case writes a document and asks `bind_ticket` what it made of
//! it rather than asserting on an edit.
//! §FS-rhei-budgets.5.2.1

use super::test_support::*;
use super::*;
use std::fs;
use std::path::Path;

const UUID: &str = "11111111-1111-4111-8111-111111111111";

/// A plan whose frontmatter gives each named task the identity beside it, and
/// whose body declares those tasks. `None` leaves a task with no key, which is
/// the document a lost identity write leaves behind.
fn plan(tasks: &[(&str, Option<&str>)]) -> String {
    // Header first, then the frontmatter block: the order rhei writes, and the
    // only one the parser accepts. §FS-rhei-plan-language.2
    let mut text = String::from("# Rhei: Case\n\n---\nmetadata:\n  tasks:\n");
    for (id, identity) in tasks {
        text.push_str(&format!("    {id}:\n"));
        match identity {
            Some(identity) => text.push_str(&format!("      budgetTicketId: {identity}\n")),
            None => text.push_str("      stateVisits:\n        work: 1\n"),
        }
    }
    text.push_str("---\n\n## Tasks\n");
    for (id, _) in tasks {
        text.push_str(&format!("\n### Task {id}: Work\n**State:** work\n"));
    }
    text
}

fn write(path: &Path, text: &str) {
    fs::write(path, text).expect("write the plan");
}

/// A binding this case starts from: one live ticket, `plan.1`, holding `UUID`.
fn bound(case: &Case) -> (Journal, String) {
    write(&case.source, &plan(&[("1", Some(UUID))]));
    let mut journal = case.account.open(true).expect("open the account");
    let ticket = case.ticket();
    let moved = journal
        .bind_ticket(&ticket, "plan.1", &case.source, &audit())
        .expect("the first binding of an identity is never a move");
    assert_eq!(moved, None, "a first binding has no display id to have moved from");
    (journal, ticket)
}

fn display_of(journal: &Journal, ticket: &str) -> String {
    journal.identities()[ticket]["display_id"].as_str().expect("a bound display id").to_string()
}

/// Two live tickets claiming one identity is one travel bound covering two, so
/// the second is refused rather than handed the first one's history.
/// §FS-rhei-budgets.5.2.1 §REQ-bounded-neural-work.4
#[test]
fn a_second_live_claimant_is_refused_and_moves_nothing() {
    let case = Case::new();
    let (mut journal, ticket) = bound(&case);

    write(&case.source, &plan(&[("1", Some(UUID)), ("2", Some(UUID))]));
    let refused = journal
        .bind_ticket(&ticket, "plan.2", &case.source, &audit())
        .expect_err("a copy claiming a live identity is refused");

    assert_eq!(refused.reason_code, "identity_claimed");
    assert!(!refused.is_exhaustion(), "no bound is what stopped this");
    for named in ["plan.1", "plan.2", UUID, "plan.rhei.md", "to fix it:"] {
        assert!(refused.message.contains(named), "the refusal names {named}: {}", refused.message);
    }
    assert_eq!(display_of(&journal, &ticket), "plan.1", "a refused admission moves no binding");
}

/// A renumbered task is the same ticket: one live claimant, so the key follows
/// it, the travel comes with it, and the move is reported.
/// §FS-rhei-budgets.5.2.1
#[test]
fn one_live_claimant_under_a_new_display_id_moves_and_says_so() {
    let case = Case::new();
    let (mut journal, ticket) = bound(&case);

    write(&case.source, &plan(&[("3", Some(UUID))]));
    let moved = journal
        .bind_ticket(&ticket, "plan.3", &case.source, &audit())
        .expect("a renumbered ticket is a lawful move")
        .expect("a move is reported rather than silent");

    assert_eq!(
        moved,
        IdentityMove { identity: UUID.into(), from: "plan.1".into(), to: "plan.3".into() }
    );
    // Both lines in full, indentation included: §FS-rhei-budgets.5.2.1 aligns
    // `counted` under `travel`, and this line is printed plain rather than
    // through a reporter, so what is built is what a person reads.
    assert_eq!(
        moved.warning(),
        format!(
            "warning: travel for {UUID} now counts against 'plan.3'; it was\n\
             {blank:9}counted against 'plan.1', which this plan no longer has",
            blank = ""
        )
    );
    assert_eq!(display_of(&journal, &ticket), "plan.3", "the binding followed the ticket");
}

/// A renamed plan file: the display id the account counted against is gone from
/// every live source, because the source it was in is gone.
/// §FS-rhei-budgets.5.2.1
#[test]
fn a_renamed_plan_file_moves_the_binding_to_the_id_it_now_has() {
    let case = Case::new();
    let (mut journal, ticket) = bound(&case);

    let renamed = case.root().join("work.rhei.md");
    fs::rename(&case.source, &renamed).expect("rename the plan file");
    let moved = journal
        .bind_ticket(&ticket, "work.1", &renamed, &audit())
        .expect("a renamed plan file is a lawful move")
        .expect("a move is reported rather than silent");

    assert_eq!(moved.from, "plan.1");
    assert_eq!(moved.to, "work.1");
    assert_eq!(display_of(&journal, &ticket), "work.1");
}

/// The adoption rule is not protected by a heuristic: a ticket carrying no
/// identity resolves through the binding on its own display id, so the display
/// id it then binds is the one already recorded and the guard is never reached.
/// §FS-rhei-budgets.5.2 §FS-rhei-budgets.5.2.1
#[test]
fn a_ticket_that_lost_its_key_adopts_its_own_binding_untouched() {
    let case = Case::new();
    let (mut journal, ticket) = bound(&case);

    write(&case.source, &plan(&[("1", None)]));
    let adopted = journal.bound_ticket("plan.1", &case.source).expect("resolve the binding");
    assert_eq!(adopted.as_deref(), Some(UUID), "the ledger's own binding is what answers");

    let moved = journal
        .bind_ticket(&ticket, "plan.1", &case.source, &audit())
        .expect("re-binding the recorded display id is idempotent");

    assert_eq!(moved, None, "nothing moved, so nothing is reported");
    assert_eq!(display_of(&journal, &ticket), "plan.1");
}

/// A second live source under the display id the account already counts
/// against is a relocated root, which §FS-rhei-budgets.5.2 tolerates: the
/// display id did not move, so neither the refusal nor the warning may fire.
/// §FS-rhei-budgets.5.2 §FS-rhei-budgets.5.2.1
#[test]
fn a_second_live_source_under_the_same_display_id_is_neither_refused_nor_reported() {
    let case = Case::new();
    let (mut journal, ticket) = bound(&case);

    let elsewhere = case.root().join("copy");
    fs::create_dir_all(&elsewhere).expect("a second root");
    let second = elsewhere.join("plan.rhei.md");
    write(&second, &plan(&[("1", Some(UUID))]));
    let moved = journal
        .bind_ticket(&ticket, "plan.1", &second, &audit())
        .expect("a relocated root keeps its identity");

    assert_eq!(moved, None, "a display id that did not move has no move to report");
    assert_eq!(display_of(&journal, &ticket), "plan.1");
}

/// A live source that will not parse is passed over rather than counted as a
/// claim, exactly as a missing one is: an unparseable document must not be able
/// to refuse an admission. §FS-rhei-budgets.5.2.1
#[test]
fn an_unparseable_live_source_is_passed_over_rather_than_counted_as_a_claim() {
    let case = Case::new();
    let (mut journal, ticket) = bound(&case);

    // Two claimants on the face of it, inside frontmatter that will not parse:
    // what is counted is what a parse yields, and this yields nothing.
    let unparseable = plan(&[("1", Some(UUID)), ("2", Some(UUID))]).replacen(
        "---\n\n## Tasks",
        "    3:\n      budgetTicketId: [unclosed\n---\n\n## Tasks",
        1,
    );
    assert!(
        crate::metadata::parse_metadata_file(&case.source, &unparseable).is_err(),
        "the fixture's whole point is a document that will not parse"
    );
    write(&case.source, &unparseable);
    let moved = journal
        .bind_ticket(&ticket, "plan.2", &case.source, &audit())
        .expect("an unparseable document claims nothing, so it refuses nothing")
        .expect("the display id still moved, and a move is reported");

    assert_eq!(moved.to, "plan.2");
}
