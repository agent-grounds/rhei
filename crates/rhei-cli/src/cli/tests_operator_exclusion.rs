// Deterministic contention witnesses, without sleeps. §FS-rhei-recover.4 §FS-rhei-recover.5

/// Both a loaded reader and a direct plan writer exclude marker publication. §FS-rhei-panta.6.6
#[test]
fn operator_force_waits_for_existing_readers_and_writers() {
    for writer in [false, true] {
        let (dir, plan, machine) = operator_fixture();
        let read = (!writer).then(|| load_plan(&plan).unwrap());
        let write = writer.then(|| LockedPlanFile::open(&plan).unwrap());
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            FORCED_BOUNDARY.with(|hook| *hook.borrow_mut() = Some(Box::new(move |point| {
                if point == "root-contended" { tx.send(()).unwrap(); }
                Ok(())
            })));
            let request = operator_request(&plan, &machine);
            let result = commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test");
            clear_force_interrupt();
            result.map_err(|err| err.to_string())
        });
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(!dir.path().join(rhei_core::root_access::MARKER).exists());
        drop(read); drop(write);
        worker.join().unwrap().unwrap();
        assert_eq!(read_ledger(dir.path()).unwrap().len(), 1);
    }
}

/// A reset holding its ordinary writer stack finishes before forced revalidation. §FS-rhei-recover.4
#[test]
fn operator_force_waits_for_an_active_reset() {
    let (_dir, plan, machine) = operator_fixture();
    let loaded = load_plan(&plan).unwrap();
    let scope = resolve_rhei_scope(&loaded, &[]).unwrap();
    let locks = ResetWriterLocks::acquire(&loaded, &plan, &scope).unwrap();
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        FORCED_BOUNDARY.with(|hook| *hook.borrow_mut() = Some(Box::new(move |point| {
            if point == "root-contended" { tx.send(()).unwrap(); }
            Ok(())
        })));
        let request = operator_request(&plan, &machine);
        let result = commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test");
        clear_force_interrupt();
        result.map_err(|err| err.to_string())
    });
    rx.recv_timeout(Duration::from_secs(10)).unwrap();
    drop(locks); drop(loaded);
    worker.join().unwrap().unwrap();
}

/// A pending transaction blocks both initial run loading and watch refresh. §FS-rhei-recover.4
#[test]
fn operator_marker_blocks_run_startup_dashboard_and_accounting() {
    let (dir, plan, machine) = operator_fixture();
    let request = operator_request(&plan, &machine);
    interrupt_force_at("marker-after");
    assert!(commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test").is_err());
    clear_force_interrupt();
    let error = load_plan_for_run(&plan, &default_run_options(), Some(&machine)).err().unwrap();
    assert!(error.to_string().contains("plan.1 gate -> work"));
    let machine = rhei_validator::StateMachine::from_yaml_file(&machine).unwrap();
    let set = rhei_validator::MachineSet { default: machine, per_rhei: BTreeMap::new() };
    assert!(load_plan_for_dashboard(&plan, &set).is_none());
    let inspection = read_cost_inspection(&dir.path().join("runtime/accounting"));
    assert!(inspection.invocations.is_empty());
    assert!(inspection.errors.iter().any(|error| error.contains("plan.1 gate -> work")));
}

/// Waiting startup owns no shared root guard and rereads the marker after the run lock. §FS-rhei-recover.4
#[test]
fn operator_run_startup_wait_does_not_deadlock_recovery() {
    let (dir, plan, machine) = operator_fixture();
    let _run = operator_run_locks(&[dir.path().to_path_buf()], "operator").unwrap();
    let (waiting_tx, waiting_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        FORCED_BOUNDARY.with(|hook| *hook.borrow_mut() = Some(Box::new(move |point| {
            if point == "run-lock-contended" {
                waiting_tx.send(()).unwrap();
                resume_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            }
            Ok(())
        })));
        let result = run_command(&plan, Some(&machine), default_run_options());
        clear_force_interrupt();
        result.err().map(|err| err.to_string())
    });
    waiting_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let exclusive = rhei_core::root_access::RootAccessGuard::try_exclusive(dir.path()).unwrap()
        .expect("queued run released its initial shared guards");
    fs::write(dir.path().join(rhei_core::root_access::MARKER),
        r#"{"hop":{"task_id":"plan.1","from":"gate","to":"work"}}"#).unwrap();
    drop(exclusive); drop(_run);
    resume_tx.send(()).unwrap();
    assert!(worker.join().unwrap().unwrap().contains("plan.1 gate -> work"));
}

/// Runtime discovery refuses a marker before reading or pruning run records. §FS-rhei-recover.4
#[test]
fn operator_marker_blocks_run_discovery_and_intervention() {
    let (dir, plan, machine) = operator_fixture();
    let request = operator_request(&plan, &machine);
    interrupt_force_at("marker-after");
    assert!(commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test").is_err());
    clear_force_interrupt();
    let error = descriptor_for_path(&plan).unwrap_err().to_string();
    assert!(error.contains("plan.1 gate -> work"), "{error}");
    let error = intervene_command(&plan, "1", None, "resume").unwrap_err().to_string();
    assert!(error.contains("plan.1 gate -> work"), "{error}");
    let descriptor = RunDescriptor {
        id: "marked".into(), pid: std::process::id(), status: RunStatus::Running,
        workspace: dir.path().to_path_buf(), plan, state_machine: Some(machine),
        control_url: None, started_at: "2026-09-18T00:00:00Z".into(), headless: false,
        parallel: 1, log: None, events: "runtime/events.jsonl".into(), exit_code: None,
    };
    let entry = dir.path().join("isolated-registry-entry.json");
    fs::write(&entry, serde_json::to_vec(&descriptor).unwrap()).unwrap();
    let before = fs::read(&entry).unwrap();
    for pruning in [Pruning::Keep, Pruning::Prune] {
        let sweep = classify_registry_roots(vec![(entry.clone(), descriptor.clone())], RegistrySweep::default(), pruning);
        let error = sweep.ensure_access().unwrap_err().to_string();
        assert!(error.contains("plan.1 gate -> work"), "{error}");
        assert!(sweep.live.is_empty() && sweep.ended.is_empty() && sweep.undecided.is_empty());
        assert_eq!(fs::read(&entry).unwrap(), before);
    }
}

/// Real next/reset commands finish before force revalidates their state and claim effects. §FS-rhei-recover.4
fn operator_force_revalidates_after_command(reset: bool) {
        let (dir, plan, machine) = operator_fixture();
        let request = operator_request(&plan, &machine);
        if reset {
            commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test").unwrap();
        } else {
            // Explicit claiming uses an initial, non-gating task; R1-10 stays adjacent.
            fs::write(&machine, OPERATOR_MACHINE.replace("    gating: true\n", "")).unwrap();
        }
        let (from, to) = if reset { ("work", "gate") } else { ("gate", "work") };
        let mut request = operator_request(&plan, &machine);
        request.from = from; request.to = to;
        let preview = prepare_forced_transition(&request).unwrap();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let command_plan = plan.clone(); let command_machine = machine.clone();
        let command = std::thread::spawn(move || {
            let pause = move || {
                entered_tx.send(()).unwrap();
                resume_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            };
            let result = if reset {
                set_reset_after_preview_hook(move |_| pause());
                reset_command(&command_plan, Some(&command_machine), &[], false, true)
            } else {
                set_claim_before_lock_hook(pause);
                next_command(&command_plan, Some(&command_machine), Some("1"), true, true, false, &[])
            };
            result.map_err(|err| err.to_string())
        });
        entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let (waiting_tx, waiting_rx) = mpsc::channel();
        let force = std::thread::spawn(move || {
            FORCED_BOUNDARY.with(|hook| *hook.borrow_mut() = Some(Box::new(move |point| {
                if point == "root-contended" { waiting_tx.send(()).unwrap(); }
                Ok(())
            })));
            let mut request = operator_request(&plan, &machine);
            request.from = from; request.to = to;
            let result = commit_confirmed_force(&request, preview, "test");
            clear_force_interrupt();
            result.err().map(|err| err.to_string())
        });
        waiting_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(!dir.path().join(rhei_core::root_access::MARKER).exists());
        resume_tx.send(()).unwrap();
        command.join().unwrap().unwrap();
        let error = force.join().unwrap().unwrap();
        assert!(error.contains(if reset { "conflict:" } else { "assigned to" }), "{error}");
        assert!(!dir.path().join(rhei_core::root_access::MARKER).exists());
        assert!(read_ledger(dir.path()).unwrap().is_empty());
}

#[test]
fn operator_force_revalidates_after_concurrent_claim_command() {
    operator_force_revalidates_after_command(false);
}

#[test]
fn operator_force_revalidates_after_concurrent_reset_command() {
    operator_force_revalidates_after_command(true);
}
