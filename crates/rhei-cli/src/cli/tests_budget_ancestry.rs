// The once-per-run latch behind the cross-project note.
//
// The acceptance clause it serves — exactly one note per `rhei run` — is pinned
// end to end by `a_cross_project_descriptor_is_reported_once_per_run`. What is
// left to hold here is the part a subprocess end-to-end test cannot reach:
// sequential and concurrent runs in one process each own their note. Channels
// force the interfering calls without sleeps or a lock around the latch.

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
    let run = begin_budget_run();
    let first = cross_project_note(&run, &declined(), "this-project")
        .expect("the first admission of a run owes the note");
    assert!(first.starts_with("note: "), "a note, not a warning: {first}");
    assert!(first.contains("another project"), "{first}");
    assert!(first.contains("reservation:11111111-2222-3333-4444-555555555555"), "{first}");
    assert!(first.contains("this-project"), "the account it charges instead: {first}");
    assert_eq!(first.lines().count(), 1, "one line, because a reader greps for it: {first}");

    assert_eq!(
        cross_project_note(&run, &declined(), "this-project"),
        None,
        "a second admission of the same run repeats nothing"
    );

    let run = begin_budget_run();
    assert!(
        cross_project_note(&run, &declined(), "this-project").is_some(),
        "the next run in this process is owed its own note"
    );
}

/// An admission that was placed under an ancestor, or claimed none at all, owes
/// no note and does not spend the run's one. §FS-rhei-budgets.7.1
#[test]
fn an_ancestry_that_was_not_declined_owes_no_note() {
    let run = begin_budget_run();

    assert_eq!(cross_project_note(&run, &Ancestry::Unclaimed, "this-project"), None);
    let placed = Ancestry::Placed { reservation: "reservation:live".into(), envelope: 1 };
    assert_eq!(cross_project_note(&run, &placed, "this-project"), None);

    assert!(
        cross_project_note(&run, &declined(), "this-project").is_some(),
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

mod cross_project_note_concurrency {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn wait_for(rx: &mpsc::Receiver<()>) {
        rx.recv_timeout(Duration::from_secs(10)).expect("the peer reaches the coordinated boundary");
    }

    /// Observe A's first and second admissions with B starting and ending between them.
    fn another_run_starts_between_admissions() -> (Option<String>, Option<String>) {
        let (first_tx, first_rx) = mpsc::channel();
        let (ended_tx, ended_rx) = mpsc::channel();
        let a = std::thread::spawn(move || {
            let run = begin_budget_run();
            let first = cross_project_note(&run, &declined(), "run-A");
            first_tx.send(()).unwrap();
            wait_for(&ended_rx);
            let second = cross_project_note(&run, &declined(), "run-A");
            (first, second)
        });
        let b = std::thread::spawn(move || {
            wait_for(&first_rx);
            // The same owner construction run_command uses through RunIdentity.
            let _run = begin_budget_run();
        });
        b.join().unwrap();
        ended_tx.send(()).unwrap();
        a.join().unwrap()
    }

    /// Observe A's first admission after the existing peer ancestry test finishes.
    fn another_caller_admits_first() -> Option<String> {
        let (begun_tx, begun_rx) = mpsc::channel();
        let (peer_tx, peer_rx) = mpsc::channel();
        let a = std::thread::spawn(move || {
            let run = begin_budget_run();
            begun_tx.send(()).unwrap();
            wait_for(&peer_rx);
            cross_project_note(&run, &declined(), "run-A")
        });
        let b = std::thread::spawn(move || {
            wait_for(&begun_rx);
            an_ancestry_that_was_not_declined_owes_no_note();
            peer_tx.send(()).unwrap();
        });
        let first = a.join().unwrap();
        b.join().unwrap();
        first
    }

    /// Observe A begins → A first → B begins → A second → B first.
    fn interleaved_admissions() -> (Option<String>, Option<String>, Option<String>) {
        let (first_tx, first_rx) = mpsc::channel();
        let (begun_tx, begun_rx) = mpsc::channel();
        let (second_tx, second_rx) = mpsc::channel();
        let a = std::thread::spawn(move || {
            let run = begin_budget_run();
            let first = cross_project_note(&run, &declined(), "run-A");
            first_tx.send(()).unwrap();
            wait_for(&begun_rx);
            let second = cross_project_note(&run, &declined(), "run-A");
            second_tx.send(()).unwrap();
            (first, second)
        });
        let b = std::thread::spawn(move || {
            wait_for(&first_rx);
            let run = begin_budget_run();
            begun_tx.send(()).unwrap();
            wait_for(&second_rx);
            cross_project_note(&run, &declined(), "run-B")
        });
        let (a_first, a_second) = a.join().unwrap();
        let b_first = b.join().unwrap();
        (a_first, a_second, b_first)
    }

    /// Independent runs cannot re-arm or consume each other's note. Keep all
    /// three controlled interleavings in one test so their baseline resets do
    /// not accidentally mask each other's failure under the parallel runner.
    /// The suite remains parallel; the callers inside each scenario overlap.
    /// §FS-rhei-budgets.7.2
    #[test]
    fn independent_runs_keep_their_cross_project_notes() {
        // Retain triage's passing controls before exercising interference.
        the_cross_project_note_is_owed_once_per_run();
        an_ancestry_that_was_not_declined_owes_no_note();
        let (reset_first, reset_second) = another_run_starts_between_admissions();
        let peer_first = another_caller_admits_first();
        let (a_first, a_second, b_first) = interleaved_admissions();
        // Check every first note and every second-note suppression together,
        // so neither B's lost note nor the peer's interference is hidden by
        // the assertion about A's repetition failing first.
        assert_eq!(
            (
                reset_first.is_some(), reset_second.is_some(), peer_first.is_some(),
                a_first.is_some(), a_second.is_some(), b_first.is_some(),
            ),
            (true, false, true, true, false, true),
            "first admissions owe notes and second admissions repeat nothing; \
             reset: A first={reset_first:?}, A second={reset_second:?}; \
             peer: A first={peer_first:?}; \
             interleaved: A first={a_first:?}, A second={a_second:?}, B first={b_first:?}"
        );
    }
}
