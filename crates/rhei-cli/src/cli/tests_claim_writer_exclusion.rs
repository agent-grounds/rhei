const CLAIM_WRITER_PLAN: &str = r#"# Rhei: Claim Writer Exclusion

---
metadata:
  tasks:
    1:
      priority: 7
---

## Tasks

### Task 1: Work
**State:** draft
"#;

const CLAIM_WRITER_MACHINE: &str = r#"name: claim-writer-exclusion
version: 1
states:
  draft:
    initial: true
    description: Setup only
  pending:
    description: Ready
    visits: 2
    instructions: Do the task.
  review:
    description: Review
    visits: 2
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: pending
  - from: pending
    to: review
  - from: review
    to: completed
"#;

// Positive claim-boundary observations have finite, portable test patience;
// this is not a command-latency promise. §FS-rhei-next.3.1
const TEST_PATIENCE: Duration = Duration::from_secs(30);

fn claim_writer_fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let plan = dir.path().join("plan.rhei.md");
    let machine = dir.path().join("states.yaml");
    fs::write(&plan, CLAIM_WRITER_PLAN).expect("plan");
    fs::write(&machine, CLAIM_WRITER_MACHINE).expect("state machine");
    (dir, plan, machine)
}

fn spawn_waiting_transition(
    plan: PathBuf,
    machine: PathBuf,
    lock_events: mpsc::Sender<PlanLockEvent>,
    done: mpsc::Sender<()>,
) -> std::thread::JoinHandle<Result<(), String>> {
    std::thread::spawn(move || {
        set_plan_lock_observer(lock_events);
        let result = transition_command(
            &plan,
            &[],
            Some(&machine),
            "1",
            "pending",
            "review",
            None,
            None,
            true,
        )
        .map_err(|error| error.to_string());
        done.send(()).expect("transition completion");
        result
    })
}

/// The claim sidecar excludes an ordinary transition from the provisional
/// state until ownership and the claim ledger entry commit. The waiter then
/// re-reads that committed state and applies its move without either command's
/// acknowledged transition being lost. §FS-rhei-next.3.1
#[test]
fn claim_commit_excludes_and_orders_an_ordinary_transition() {
    let (dir, plan, machine) = claim_writer_fixture();
    let claimant_plan = plan.clone();
    let claimant_machine = machine.clone();
    let (provisional_tx, provisional_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (claimant_done_tx, claimant_done_rx) = mpsc::channel();
    let claimant = std::thread::spawn(move || {
        set_claim_before_lock_hook(|| std::thread::sleep(Duration::from_secs(3)));
        set_claim_after_state_write_hook(move || {
            provisional_tx.send(()).expect("provisional state signal");
            release_rx.recv_timeout(TEST_PATIENCE).expect("release claim");
        });
        let result = next_command(
            &claimant_plan,
            Some(&claimant_machine),
            None,
            false,
            true,
            false,
            &[],
        )
        .map_err(|error| error.to_string());
        claimant_done_tx.send(()).expect("claimant completion");
        result
    });

    provisional_rx.recv_timeout(TEST_PATIENCE).expect("provisional claim state");
    let provisional = fs::read_to_string(&plan).expect("provisional plan");
    assert!(provisional.contains("**State:** pending"));
    assert!(provisional.contains("stateVisits:\n        pending: 1"));
    assert!(!provisional.contains("**Assignee:**"));

    let (lock_tx, lock_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let transition =
        spawn_waiting_transition(plan.clone(), machine.clone(), lock_tx, done_tx);
    assert_eq!(
        lock_rx.recv_timeout(TEST_PATIENCE).expect("transition lock attempt"),
        PlanLockEvent::Contended,
        "the ordinary transition must encounter the claim's stable writer lock"
    );
    assert!(
        matches!(done_rx.recv_timeout(Duration::from_millis(50)), Err(RecvTimeoutError::Timeout)),
        "the ordinary transition completed inside the provisional claim window"
    );

    release_tx.send(()).expect("release claimant");
    assert_eq!(
        lock_rx.recv_timeout(TEST_PATIENCE).expect("transition lock acquisition"),
        PlanLockEvent::Acquired
    );
    claimant_done_rx.recv_timeout(TEST_PATIENCE).expect("claimant completion");
    done_rx.recv_timeout(TEST_PATIENCE).expect("transition completion");
    claimant.join().expect("claimant thread").expect("claim succeeds");
    transition.join().expect("transition thread").expect("transition succeeds");

    let final_plan = fs::read_to_string(&plan).expect("final plan");
    assert!(final_plan.contains("**State:** review\n**Assignee:** manual"));
    assert!(final_plan.contains("pending: 1"), "claim visit metadata survives");
    assert!(final_plan.contains("review: 1"), "transition visit metadata survives");
    assert_eq!(
        fs::read_to_string(dir.path().join("runtime/state-transitions.log")).unwrap(),
        "plan.1 draft@pending\nplan.1 pending@review\n"
    );
}

/// A returned pre-commit failure restores while the same sidecar remains held.
/// The waiting ordinary writer proceeds only afterwards, re-reads `draft`, and
/// refuses its stale `pending` compare-and-swap without leaving or losing data.
/// §FS-rhei-next.3.1
#[test]
fn claim_rollback_excludes_and_revalidates_an_ordinary_transition() {
    let (dir, plan, machine) = claim_writer_fixture();
    let claimant_plan = plan.clone();
    let claimant_machine = machine.clone();
    let (provisional_tx, provisional_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let claimant = std::thread::spawn(move || {
        set_claim_faults(vec![(ClaimFaultPoint::AssigneeWrite, "ownership disk full")]);
        set_claim_after_state_write_hook(move || {
            provisional_tx.send(()).expect("provisional state signal");
            release_rx.recv_timeout(Duration::from_secs(2)).expect("release claim");
        });
        next_command(
            &claimant_plan,
            Some(&claimant_machine),
            None,
            false,
            true,
            false,
            &[],
        )
        .map_err(|error| error.to_string())
    });

    provisional_rx.recv_timeout(Duration::from_secs(2)).expect("provisional claim state");
    let (lock_tx, lock_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let transition =
        spawn_waiting_transition(plan.clone(), machine.clone(), lock_tx, done_tx);
    assert_eq!(
        lock_rx.recv_timeout(Duration::from_secs(2)).expect("transition lock attempt"),
        PlanLockEvent::Contended,
        "the ordinary transition must wait through claim restoration"
    );
    assert!(
        matches!(done_rx.recv_timeout(Duration::from_millis(50)), Err(RecvTimeoutError::Timeout)),
        "the ordinary transition completed before restoration"
    );

    release_tx.send(()).expect("release claimant");
    assert_eq!(
        lock_rx.recv_timeout(Duration::from_secs(2)).expect("transition lock acquisition"),
        PlanLockEvent::Acquired
    );
    let claim_error = claimant.join().expect("claimant thread").expect_err("claim fails");
    assert!(claim_error.contains("assignee persistence injection"));
    done_rx.recv_timeout(Duration::from_secs(2)).expect("transition completion");
    let transition_error =
        transition.join().expect("transition thread").expect_err("stale move is refused");
    assert!(transition_error.contains("is in state 'draft', expected 'pending'"));

    assert_eq!(fs::read_to_string(&plan).unwrap(), CLAIM_WRITER_PLAN);
    assert_eq!(
        fs::read_to_string(dir.path().join("runtime/state-transitions.log")).unwrap_or_default(),
        ""
    );
}
