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

#[derive(Debug, PartialEq, Eq)]
struct ResetDecisionView {
    scope: RheiScope,
    task_count: usize,
    descendant_count: usize,
    moves: Vec<(String, String, String)>,
    runtime_targets: Vec<PathBuf>,
}

fn reset_decision_view(decision: &ResetDecision) -> ResetDecisionView {
    ResetDecisionView {
        scope: decision.scope.clone(),
        task_count: decision.task_count,
        descendant_count: decision.descendant_count,
        moves: decision
            .authored
            .moves
            .iter()
            .map(|mv| (mv.task_id.clone(), mv.from.clone(), mv.to.clone()))
            .collect(),
        runtime_targets: decision.runtime_targets.clone(),
    }
}

/// Full reset retains every stable sidecar through runtime deletion. A ledger
/// waiter and a real transition that both started before cleanup can only run
/// afterwards, reopen the current pathname, and preserve both acknowledged
/// appends. §FS-rhei-reset.1.2 §FS-rhei-reset.3 §FS-rhei-reset.4
/// §AR-agent-orchestrator-workflow.3.3.1
#[test]
fn full_reset_excludes_waiters_through_runtime_deletion_and_recreation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let plan = dir.path().join("plan.rhei.md");
    let machine = dir.path().join("states.yaml");
    let runtime = dir.path().join("runtime");
    fs::write(&plan, reset_exclusion_plan("Full Reset", "draft", true)).expect("plan");
    fs::write(&machine, RESET_EXCLUSION_MACHINE).expect("machine");

    let (before_locks_tx, before_locks_rx) = mpsc::channel();
    let (acquire_tx, acquire_rx) = mpsc::channel();
    let (preview_tx, preview_rx) = mpsc::channel();
    let (clean_tx, clean_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel();
    let (unlock_tx, unlock_rx) = mpsc::channel();
    let reset_plan = plan.clone();
    let reset_machine = machine.clone();
    let resetter = std::thread::spawn(move || {
        set_reset_confirmation(true, || panic!("--yes reset must not prompt"));
        set_reset_before_locks_hook(move || {
            before_locks_tx.send(()).expect("reset reached lock acquisition");
            acquire_rx.recv_timeout(Duration::from_secs(2)).expect("acquire reset locks");
        });
        set_reset_after_preview_hook(move |decision| {
            preview_tx.send(reset_decision_view(decision)).expect("reset preview");
            continue_rx.recv_timeout(Duration::from_secs(2)).expect("continue reset");
        });
        set_reset_before_unlock_hook(move |decision| {
            clean_tx.send(reset_decision_view(decision)).expect("reset cleaned");
            unlock_rx.recv_timeout(Duration::from_secs(2)).expect("release reset");
        });
        reset_command(&reset_plan, Some(&reset_machine), &[], false, true)
            .map_err(|error| error.to_string())
    });

    // A writer that commits before acquisition belongs to the authoritative
    // preview rather than the stale plan reset first loaded.
    before_locks_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("reset before lock acquisition");
    transition_command(
        &plan,
        &[],
        Some(&machine),
        "1",
        "draft",
        "pending",
        None,
        None,
        true,
    )
    .expect("transition before reset locks");
    assert!(fs::read_to_string(&plan).expect("transitioned plan").contains("**State:** pending"));
    acquire_tx.send(()).expect("allow reset acquisition");
    let preview = preview_rx.recv_timeout(Duration::from_secs(2)).expect("reset preview");
    assert_eq!(preview.scope, None);
    assert_eq!((preview.task_count, preview.descendant_count), (1, 0));
    assert_eq!(preview.moves.len(), 1, "the pre-lock transition must be previewed");
    assert_eq!(preview.moves[0].1, "pending");
    assert_eq!(preview.moves[0].2, "draft");
    assert_eq!(preview.runtime_targets, vec![runtime.clone()]);

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
    let summary = clean_rx.recv_timeout(Duration::from_secs(2)).expect("reset cleanup boundary");
    assert_eq!(summary, preview, "preview and summary must use one locked decision");
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
/// §FS-rhei-reset.1.2 §FS-rhei-reset.3 §FS-rhei-reset.4 §FS-rhei-panta.6.4
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
    fs::create_dir_all(project.join("runtime/results")).expect("results");
    fs::write(project.join("runtime/results/auth.1.md"), "remove me\n").expect("auth result");
    fs::write(project.join("runtime/results/billing.1.md"), "keep me\n")
        .expect("billing result");

    let (preview_tx, preview_rx) = mpsc::channel();
    let (pruned_tx, pruned_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel();
    let (unlock_tx, unlock_rx) = mpsc::channel();
    let reset_project = project.to_path_buf();
    let resetter = std::thread::spawn(move || {
        set_reset_after_preview_hook(move |decision| {
            preview_tx.send(reset_decision_view(decision)).expect("reset preview");
            continue_rx.recv_timeout(Duration::from_secs(2)).expect("continue reset");
        });
        set_reset_before_unlock_hook(move |decision| {
            pruned_tx.send(reset_decision_view(decision)).expect("reset pruned");
            unlock_rx.recv_timeout(Duration::from_secs(2)).expect("release reset");
        });
        reset_command(&reset_project, None, &["auth".to_string()], false, true)
            .map_err(|error| error.to_string())
    });
    let preview = preview_rx.recv_timeout(Duration::from_secs(2)).expect("reset preview");
    assert_eq!(preview.scope, Some(BTreeSet::from(["auth".to_string()])));
    assert_eq!((preview.task_count, preview.descendant_count), (1, 0));
    assert_eq!(preview.moves.len(), 1);
    assert_eq!(preview.moves[0].0, "auth.1");
    assert!(preview.runtime_targets.is_empty(), "narrowed cleanup is ticket-scoped");

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
    let summary = pruned_rx.recv_timeout(Duration::from_secs(2)).expect("prune boundary");
    assert_eq!(summary, preview, "narrowed preview and summary must share one decision");
    assert!(
        matches!(done_rx.recv_timeout(Duration::from_millis(50)), Err(RecvTimeoutError::Timeout)),
        "the transition must remain outside the pruning boundary"
    );
    assert_eq!(
        fs::read_to_string(project.join("runtime/state-transitions.log")).expect("pruned ledger"),
        "billing.1 draft@pending\n",
        "narrowed reset must remove only targeted history"
    );
    assert!(!project.join("runtime/results/auth.1.md").exists());
    assert!(project.join("runtime/results/billing.1.md").is_file());

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

/// Declining the prompt performs no reset mutation and releases the complete
/// stack to a writer that began after the preview. §FS-rhei-reset.1.2
#[test]
fn declined_confirmation_releases_waiting_writer_without_reset_mutation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let plan = dir.path().join("plan.rhei.md");
    let machine = dir.path().join("states.yaml");
    let runtime = dir.path().join("runtime");
    fs::write(&plan, reset_exclusion_plan("Declined Reset", "pending", true)).expect("plan");
    fs::write(&machine, RESET_EXCLUSION_MACHINE).expect("machine");
    fs::create_dir_all(runtime.join("results")).expect("runtime");
    fs::write(runtime.join("state-transitions.log"), "plan.1 draft@pending\n")
        .expect("ledger");
    fs::write(runtime.join("results/plan.1.md"), "keep me\n").expect("result");

    let (preview_tx, preview_rx) = mpsc::channel();
    let (confirm_tx, confirm_rx) = mpsc::channel();
    let (decline_tx, decline_rx) = mpsc::channel();
    let reset_plan = plan.clone();
    let reset_machine = machine.clone();
    let resetter = std::thread::spawn(move || {
        set_reset_after_preview_hook(move |decision| {
            preview_tx.send(reset_decision_view(decision)).expect("reset preview");
        });
        set_reset_confirmation(true, move || {
            confirm_tx.send(()).expect("confirmation reached");
            decline_rx.recv_timeout(Duration::from_secs(2)).expect("decline reset");
            false
        });
        reset_command(&reset_plan, Some(&reset_machine), &[], false, false)
            .map_err(|error| error.to_string())
    });

    let preview = preview_rx.recv_timeout(Duration::from_secs(2)).expect("reset preview");
    assert_eq!(preview.moves.len(), 1);
    confirm_rx.recv_timeout(Duration::from_secs(2)).expect("confirmation boundary");

    let (plan_lock_tx, plan_lock_rx) = mpsc::channel();
    let transition_plan = plan.clone();
    let transition_machine = machine.clone();
    let transition = std::thread::spawn(move || {
        set_plan_lock_observer(plan_lock_tx);
        transition_command(
            &transition_plan,
            &[],
            Some(&transition_machine),
            "1",
            "pending",
            "review",
            None,
            None,
            true,
        )
        .map_err(|error| error.to_string())
    });
    assert_eq!(
        plan_lock_rx.recv_timeout(Duration::from_secs(2)).expect("transition contention"),
        PlanLockEvent::Contended
    );

    decline_tx.send(()).expect("decline confirmation");
    resetter.join().expect("reset thread").expect("declined reset succeeds");
    transition.join().expect("transition thread").expect("waiting transition succeeds");
    assert_eq!(
        plan_lock_rx.recv_timeout(Duration::from_secs(2)).expect("transition acquisition"),
        PlanLockEvent::Acquired
    );

    let final_plan = fs::read_to_string(&plan).expect("final plan");
    assert!(final_plan.contains("**State:** review"));
    assert!(final_plan.contains("**Assignee:** manual"), "decline must not clear ownership");
    assert!(final_plan.contains("pending: 1"), "decline must not clear prior visits");
    assert!(final_plan.contains("review: 1"), "waiting transition records its visit");
    assert!(runtime.join("results/plan.1.md").is_file(), "decline must retain runtime output");
    assert_eq!(
        fs::read_to_string(runtime.join("state-transitions.log")).expect("final ledger"),
        "plan.1 draft@pending\nplan.1 pending@review\n"
    );
}

/// Non-interactive refusal follows the same no-mutation release path after
/// printing the authoritative preview. §FS-rhei-reset.1.2
#[test]
fn noninteractive_refusal_releases_waiting_writer_without_reset_mutation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let plan = dir.path().join("plan.rhei.md");
    let machine = dir.path().join("states.yaml");
    let runtime = dir.path().join("runtime");
    fs::write(&plan, reset_exclusion_plan("Refused Reset", "pending", true)).expect("plan");
    fs::write(&machine, RESET_EXCLUSION_MACHINE).expect("machine");
    fs::create_dir_all(runtime.join("results")).expect("runtime");
    fs::write(runtime.join("state-transitions.log"), "plan.1 draft@pending\n")
        .expect("ledger");
    fs::write(runtime.join("results/plan.1.md"), "keep me\n").expect("result");

    let (preview_tx, preview_rx) = mpsc::channel();
    let (refuse_tx, refuse_rx) = mpsc::channel();
    let reset_plan = plan.clone();
    let reset_machine = machine.clone();
    let resetter = std::thread::spawn(move || {
        set_reset_after_preview_hook(move |decision| {
            preview_tx.send(reset_decision_view(decision)).expect("reset preview");
            refuse_rx.recv_timeout(Duration::from_secs(2)).expect("refuse reset");
        });
        set_reset_confirmation(false, || panic!("non-interactive reset must not prompt"));
        reset_command(&reset_plan, Some(&reset_machine), &[], false, false)
            .map_err(|error| error.to_string())
    });

    let preview = preview_rx.recv_timeout(Duration::from_secs(2)).expect("reset preview");
    assert_eq!(preview.moves.len(), 1);

    let (plan_lock_tx, plan_lock_rx) = mpsc::channel();
    let transition_plan = plan.clone();
    let transition_machine = machine.clone();
    let transition = std::thread::spawn(move || {
        set_plan_lock_observer(plan_lock_tx);
        transition_command(
            &transition_plan,
            &[],
            Some(&transition_machine),
            "1",
            "pending",
            "review",
            None,
            None,
            true,
        )
        .map_err(|error| error.to_string())
    });
    assert_eq!(
        plan_lock_rx.recv_timeout(Duration::from_secs(2)).expect("transition contention"),
        PlanLockEvent::Contended
    );

    refuse_tx.send(()).expect("continue to refusal");
    let error = resetter.join().expect("reset thread").expect_err("reset must refuse");
    assert!(error.contains("stdin is not a terminal"), "unexpected refusal: {error}");
    transition.join().expect("transition thread").expect("waiting transition succeeds");
    assert_eq!(
        plan_lock_rx.recv_timeout(Duration::from_secs(2)).expect("transition acquisition"),
        PlanLockEvent::Acquired
    );

    let final_plan = fs::read_to_string(&plan).expect("final plan");
    assert!(final_plan.contains("**State:** review"));
    assert!(final_plan.contains("**Assignee:** manual"), "refusal must not clear ownership");
    assert!(final_plan.contains("pending: 1"), "refusal must not clear prior visits");
    assert!(final_plan.contains("review: 1"), "waiting transition records its visit");
    assert!(runtime.join("results/plan.1.md").is_file(), "refusal must retain runtime output");
    assert_eq!(
        fs::read_to_string(runtime.join("state-transitions.log")).expect("final ledger"),
        "plan.1 draft@pending\nplan.1 pending@review\n"
    );
}
