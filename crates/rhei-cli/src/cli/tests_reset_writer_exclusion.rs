const RESET_EXCLUSION_MACHINE: &str = r#"name: reset-writer-exclusion
version: 1
states:
  draft:
    initial: true
    description: Setup
  pending:
    description: Ready
    visits: 2
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

fn reset_exclusion_plan(title: &str, state: &str, assigned: bool) -> String {
    format!(
        "# Rhei: {title}\n\n---\nmetadata:\n  tasks:\n    1:\n      stateVisits:\n        {state}: 1\n---\n\n## Tasks\n\n### Task 1: Work\n**State:** {state}\n{}",
        if assigned { "**Assignee:** manual\n" } else { "" }
    )
}

/// Full reset retains every stable sidecar through runtime deletion. A ledger
/// waiter and a real transition that both started before cleanup can only run
/// afterwards, reopen the current pathname, and preserve both acknowledged
/// appends. §FS-rhei-reset.3 §AR-agent-orchestrator-workflow.3.3.1
#[test]
fn full_reset_excludes_waiters_through_runtime_deletion_and_recreation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let plan = dir.path().join("plan.rhei.md");
    let machine = dir.path().join("states.yaml");
    let runtime = dir.path().join("runtime");
    fs::write(&plan, reset_exclusion_plan("Full Reset", "pending", true)).expect("plan");
    fs::write(&machine, RESET_EXCLUSION_MACHINE).expect("machine");
    fs::create_dir_all(&runtime).expect("runtime");
    fs::write(runtime.join("state-transitions.log"), "plan.1 draft@pending\n")
        .expect("ledger");

    let (locked_tx, locked_rx) = mpsc::channel();
    let (clean_tx, clean_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel();
    let (unlock_tx, unlock_rx) = mpsc::channel();
    let reset_plan = plan.clone();
    let reset_machine = machine.clone();
    let resetter = std::thread::spawn(move || {
        set_reset_after_locks_hook(move || {
            locked_tx.send(()).expect("reset locked");
            continue_rx.recv_timeout(Duration::from_secs(2)).expect("continue reset");
        });
        set_reset_before_unlock_hook(move || {
            clean_tx.send(()).expect("reset cleaned");
            unlock_rx.recv_timeout(Duration::from_secs(2)).expect("release reset");
        });
        reset_command(&reset_plan, Some(&reset_machine), &[], false, true)
            .map_err(|error| error.to_string())
    });
    locked_rx.recv_timeout(Duration::from_secs(2)).expect("reset lock boundary");

    let (plan_lock_tx, plan_lock_rx) = mpsc::channel();
    let (transition_done_tx, transition_done_rx) = mpsc::channel();
    let transition_plan = plan.clone();
    let transition_machine = machine.clone();
    let transition = std::thread::spawn(move || {
        set_plan_lock_observer(plan_lock_tx);
        let result = transition_command(
            &transition_plan,
            &[],
            Some(&transition_machine),
            "1",
            "draft",
            "pending",
            None,
            None,
            true,
        )
        .map_err(|error| error.to_string());
        transition_done_tx.send(()).expect("transition done");
        result
    });
    assert_eq!(
        plan_lock_rx.recv_timeout(Duration::from_secs(2)).expect("transition lock attempt"),
        PlanLockEvent::Contended
    );

    let (ledger_lock_tx, ledger_lock_rx) = mpsc::channel();
    let (ledger_done_tx, ledger_done_rx) = mpsc::channel();
    let ledger_root = dir.path().to_path_buf();
    let ledger_writer = std::thread::spawn(move || {
        set_ledger_lock_observer(ledger_lock_tx);
        let result = append_state_transition_log_entry(
            &ledger_root,
            "plan.9",
            "pending",
            "completed",
        )
        .map_err(|error| error.to_string());
        ledger_done_tx.send(()).expect("ledger writer done");
        result
    });
    assert_eq!(
        ledger_lock_rx.recv_timeout(Duration::from_secs(2)).expect("ledger lock attempt"),
        LedgerLockEvent::Contended
    );

    continue_tx.send(()).expect("continue reset cleanup");
    clean_rx.recv_timeout(Duration::from_secs(2)).expect("reset cleanup boundary");
    assert!(!runtime.exists(), "full reset must remove runtime while retaining its locks");
    assert!(
        dir.path().join("runtime.state-transitions.log.lock").is_file(),
        "the stable ledger sidecar must survive outside runtime"
    );
    assert!(
        matches!(ledger_done_rx.recv_timeout(Duration::from_millis(50)), Err(RecvTimeoutError::Timeout)),
        "the ledger waiter must not append through the removed runtime tree"
    );
    assert!(
        matches!(transition_done_rx.recv_timeout(Duration::from_millis(50)), Err(RecvTimeoutError::Timeout)),
        "the transition must not overlap the reset boundary"
    );

    unlock_tx.send(()).expect("release reset locks");
    resetter.join().expect("reset thread").expect("reset succeeds");
    ledger_writer.join().expect("ledger writer thread").expect("ledger append succeeds");
    transition.join().expect("transition thread").expect("transition succeeds");

    assert_eq!(
        ledger_lock_rx.recv_timeout(Duration::from_secs(2)).expect("ledger acquisition"),
        LedgerLockEvent::Acquired
    );
    assert_eq!(
        plan_lock_rx.recv_timeout(Duration::from_secs(2)).expect("plan acquisition"),
        PlanLockEvent::Acquired
    );
    let final_plan = fs::read_to_string(&plan).expect("final plan");
    assert!(final_plan.contains("**State:** pending"), "waiting transition runs after reset");
    assert!(!final_plan.contains("**Assignee:**"), "reset ownership clearing survives");
    assert!(final_plan.contains("pending: 1"), "the later transition records counted metadata");
    let ledger = fs::read_to_string(runtime.join("state-transitions.log")).expect("recreated ledger");
    assert!(ledger.contains("plan.9 pending@completed\n"), "direct production append survives");
    assert!(ledger.contains("plan.1 draft@pending\n"), "ordinary transition append survives");
    assert_eq!(ledger.lines().count(), 2, "deleted history must not reappear: {ledger}");
}

/// Narrowed reset prunes under the same ledger identity. A transition on an
/// untargeted sibling may hold its own plan lock while waiting, but it cannot
/// append until pruning commits and cannot be lost to stale replacement.
/// §FS-rhei-reset.3 §FS-rhei-panta.6.4
#[test]
fn narrowed_reset_serializes_pruning_with_an_untargeted_transition() {
    let dir = tempfile::tempdir().expect("tempdir");
    let project = dir.path();
    let auth = project.join("auth.rhei.md");
    let billing = project.join("billing.rhei.md");
    let machine = project.join("states.yaml");
    fs::write(
        project.join("index.panta.md"),
        "# Panta: Reset Exclusion\n**States:** reset-writer-exclusion\n",
    )
    .expect("manifest");
    fs::write(&auth, reset_exclusion_plan("Auth", "pending", true)).expect("auth plan");
    fs::write(&billing, reset_exclusion_plan("Billing", "pending", false))
        .expect("billing plan");
    fs::write(&machine, RESET_EXCLUSION_MACHINE).expect("machine");
    fs::create_dir_all(project.join("runtime")).expect("runtime");
    fs::write(
        project.join("runtime/state-transitions.log"),
        "auth.1 draft@pending\nbilling.1 draft@pending\n",
    )
    .expect("ledger");

    let (locked_tx, locked_rx) = mpsc::channel();
    let (pruned_tx, pruned_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel();
    let (unlock_tx, unlock_rx) = mpsc::channel();
    let reset_project = project.to_path_buf();
    let resetter = std::thread::spawn(move || {
        set_reset_after_locks_hook(move || {
            locked_tx.send(()).expect("reset locked");
            continue_rx.recv_timeout(Duration::from_secs(2)).expect("continue reset");
        });
        set_reset_before_unlock_hook(move || {
            pruned_tx.send(()).expect("reset pruned");
            unlock_rx.recv_timeout(Duration::from_secs(2)).expect("release reset");
        });
        reset_command(&reset_project, None, &["auth".to_string()], false, true)
            .map_err(|error| error.to_string())
    });
    locked_rx.recv_timeout(Duration::from_secs(2)).expect("reset lock boundary");

    let (ledger_lock_tx, ledger_lock_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let transition_project = project.to_path_buf();
    let transition = std::thread::spawn(move || {
        set_ledger_lock_observer(ledger_lock_tx);
        let result = transition_command(
            &transition_project,
            &[],
            None,
            "billing.1",
            "pending",
            "review",
            None,
            None,
            true,
        )
        .map_err(|error| error.to_string());
        done_tx.send(()).expect("transition done");
        result
    });
    assert_eq!(
        ledger_lock_rx.recv_timeout(Duration::from_secs(2)).expect("ledger contention"),
        LedgerLockEvent::Contended,
        "the untargeted transition must reach the production ledger lock"
    );

    continue_tx.send(()).expect("continue narrowed reset");
    pruned_rx.recv_timeout(Duration::from_secs(2)).expect("prune boundary");
    assert!(
        matches!(done_rx.recv_timeout(Duration::from_millis(50)), Err(RecvTimeoutError::Timeout)),
        "the transition must remain outside the pruning boundary"
    );
    assert_eq!(
        fs::read_to_string(project.join("runtime/state-transitions.log")).expect("pruned ledger"),
        "billing.1 draft@pending\n",
        "narrowed reset must remove only targeted history"
    );

    unlock_tx.send(()).expect("release narrowed reset");
    resetter.join().expect("reset thread").expect("narrowed reset succeeds");
    transition.join().expect("transition thread").expect("transition succeeds");
    assert_eq!(
        ledger_lock_rx.recv_timeout(Duration::from_secs(2)).expect("ledger acquisition"),
        LedgerLockEvent::Acquired
    );

    let auth = fs::read_to_string(auth).expect("auth plan");
    assert!(auth.contains("**State:** draft"), "targeted task returns to authored state");
    assert!(!auth.contains("**Assignee:**"), "targeted ownership is cleared");
    assert!(!auth.contains("stateVisits:"), "targeted counted metadata is cleared");
    let billing = fs::read_to_string(billing).expect("billing plan");
    assert!(billing.contains("**State:** review"), "untargeted transition commits afterwards");
    assert!(billing.contains("review: 1"), "untargeted counted metadata is recorded");
    assert_eq!(
        fs::read_to_string(project.join("runtime/state-transitions.log")).expect("final ledger"),
        "billing.1 draft@pending\nbilling.1 pending@review\n"
    );
}
