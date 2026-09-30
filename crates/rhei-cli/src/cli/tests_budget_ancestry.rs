// The once-per-run latch behind the cross-project note.
//
// The acceptance clause it serves — exactly one note per `rhei run` — is pinned
// end to end by `a_cross_project_descriptor_is_reported_once_per_run`. What is
// left to hold here is the part an end-to-end test cannot reach: that a *second*
// run in the same process is owed its own note, which is how every in-process
// caller of the runtime behaves.

// §FS-rhei-budgets.7.2

/// A descriptor the ledger declined, as `reserve` would report it.
fn declined() -> Ancestry {
    Ancestry::Elsewhere {
        reservation: "reservation:11111111-2222-3333-4444-555555555555".into(),
        account: "99999999-9999-4999-8999-999999999999".into(),
    }
}

/// Once per run, and again on the next run in the same process.
/// §FS-rhei-budgets.7.2
#[test]
fn the_cross_project_note_is_owed_once_per_run() {
    begin_budget_run();
    let first = cross_project_note(&declined(), "this-project")
        .expect("the first admission of a run owes the note");
    assert!(first.starts_with("note: "), "a note, not a warning: {first}");
    assert!(first.contains("another project"), "{first}");
    assert!(first.contains("reservation:11111111-2222-3333-4444-555555555555"), "{first}");
    assert!(first.contains("this-project"), "the account it charges instead: {first}");
    assert_eq!(first.lines().count(), 1, "one line, because a reader greps for it: {first}");

    assert_eq!(
        cross_project_note(&declined(), "this-project"),
        None,
        "a second admission of the same run repeats nothing"
    );

    begin_budget_run();
    assert!(
        cross_project_note(&declined(), "this-project").is_some(),
        "the next run in this process is owed its own note"
    );
}

/// An admission that was placed under an ancestor, or claimed none at all, owes
/// no note and does not spend the run's one. §FS-rhei-budgets.7.1
#[test]
fn an_ancestry_that_was_not_declined_owes_no_note() {
    begin_budget_run();

    assert_eq!(cross_project_note(&Ancestry::Unclaimed, "this-project"), None);
    let placed = Ancestry::Placed { reservation: "reservation:live".into(), envelope: 1 };
    assert_eq!(cross_project_note(&placed, "this-project"), None);

    assert!(
        cross_project_note(&declined(), "this-project").is_some(),
        "neither of those may have spent the note this run still owed"
    );
}

/// An account this machine's witness index has never seen is named by its uuid
/// rather than by a directory nobody could point at. §FS-rhei-budgets.7.2
#[test]
fn an_unwitnessed_account_is_named_by_its_uuid() {
    let stranger = "12121212-3434-4343-8565-787878787878";
    assert_eq!(minting_project_label(stranger), stranger);
}
