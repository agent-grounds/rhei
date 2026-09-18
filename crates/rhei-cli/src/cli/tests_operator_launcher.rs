// Parent-side launch exclusion precedes all launch effects. §FS-rhei-recover.4

fn operator_headless_options() -> RunOptions {
    let mut options = default_run_options();
    options.standalone.headless = true;
    options
}

/// A marked project, manifest, basin or member cannot truncate evidence or create launcher files. §FS-rhei-recover.4
#[test]
fn operator_headless_parent_preserves_marked_basin_runtime_bytes() {
    let (_dir, project, root, machine) = operator_basin_fixture();
    let request = operator_basin_request(&project, &machine);
    interrupt_force_at("marker-after");
    assert!(commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test").is_err());
    clear_force_interrupt();
    for owner in [&project, &root] {
        fs::create_dir_all(owner.join("runtime")).unwrap();
        fs::write(owner.join("runtime/run.log"), "Existing console evidence must survive.\n").unwrap();
        fs::write(owner.join("runtime/run.json"), "Existing descriptor evidence.\n").unwrap();
    }
    let before = operator_snapshot(&project);
    for input in [&project, &project.join("index.panta.md"), &root, &project.join("other.rhei.md")] {
        let error = run_command(input, Some(&machine), operator_headless_options()).unwrap_err().to_string();
        assert!(error.contains("basin.1.1 work -> done"), "{error}");
        assert!(!error.contains("the run exited before it started"));
        assert_eq!(operator_snapshot(&project), before);
    }
    for owner in [&project, &root] { assert!(!owner.join(".rhei/headless-launch.lock").exists()); }
}

/// A force published after launch begins but before access acquisition still refuses the parent. §FS-rhei-recover.4
#[test]
fn operator_headless_parent_rechecks_force_publication_before_launch() {
    let (dir, plan, machine) = operator_fixture();
    fs::create_dir_all(dir.path().join("runtime")).unwrap();
    fs::write(dir.path().join("runtime/run.log"), "prior evidence").unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let launch_plan = plan.clone();
    let launch_machine = machine.clone();
    let launcher = std::thread::spawn(move || {
        FORCED_BOUNDARY.with(|hook| *hook.borrow_mut() = Some(Box::new(move |point| {
            if point == "launcher-before-guards" {
                entered_tx.send(()).unwrap();
                resume_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            }
            Ok(())
        })));
        let result = run_command(&launch_plan, Some(&launch_machine), operator_headless_options());
        clear_force_interrupt();
        result.err().map(|err| err.to_string())
    });
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let request = operator_request(&plan, &machine);
    interrupt_force_at("marker-after");
    assert!(commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test").is_err());
    clear_force_interrupt();
    let before = operator_snapshot(dir.path());
    resume_tx.send(()).unwrap();
    assert!(launcher.join().unwrap().unwrap().contains("plan.1 gate -> work"));
    assert_eq!(operator_snapshot(dir.path()), before);
    assert!(!dir.path().join(".rhei/headless-launch.lock").exists());
}

/// Once inside the parent access boundary, force waits until parent operations release it. §FS-rhei-recover.4
#[test]
fn operator_force_waits_for_guarded_headless_parent() {
    let (_dir, project, root, machine) = operator_basin_fixture();
    let prepared = prepare_forced_transition(&operator_basin_request(&project, &machine)).unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let launch_project = project.clone();
    let launcher = std::thread::spawn(move || {
        FORCED_BOUNDARY.with(|hook| *hook.borrow_mut() = Some(Box::new(move |point| {
            if point == "launcher-guarded" {
                entered_tx.send(()).unwrap();
                resume_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                return Err(miette!("fixture ends guarded launch before spawning"));
            }
            Ok(())
        })));
        let result = launch_headless_run(&launch_project, &operator_headless_options());
        clear_force_interrupt();
        result.err().map(|err| err.to_string())
    });
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let (waiting_tx, waiting_rx) = mpsc::channel();
    let force = std::thread::spawn(move || {
        FORCED_BOUNDARY.with(|hook| *hook.borrow_mut() = Some(Box::new(move |point| {
            if point == "root-contended" { waiting_tx.send(()).unwrap(); }
            Ok(())
        })));
        let result = commit_confirmed_force(&operator_basin_request(&project, &machine), prepared, "test");
        clear_force_interrupt();
        result.map_err(|err| err.to_string())
    });
    waiting_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(!root.join(rhei_core::root_access::MARKER).exists());
    resume_tx.send(()).unwrap();
    assert!(launcher.join().unwrap().unwrap().contains("fixture ends guarded launch"));
    force.join().unwrap().unwrap();
    assert_eq!(read_ledger(&root).unwrap().len(), 1);
}

/// The child refuses the operator's run lock before entering a shared queue its parent holds open. §FS-rhei-recover.4
#[test]
fn operator_headless_child_takes_run_locks_before_shared_access() {
    let (_dir, project, _root, _machine) = operator_basin_fixture();
    let parent_guards = rhei_core::root_access::for_input(&project).unwrap();
    let roots = rhei_core::root_access::input_roots(&project).unwrap();
    let (locked_tx, locked_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let operator = std::thread::spawn(move || {
        let _run_locks = operator_run_locks(&roots, "attended-operator").unwrap();
        assert!(rhei_core::root_access::RootAccessGuard::try_exclusive(&roots[0]).unwrap().is_none());
        locked_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    });
    locked_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let (child_tx, child_rx) = mpsc::channel();
    let child = std::thread::spawn(move || {
        let result = headless_startup_run_locks(&project).err().unwrap().to_string();
        child_tx.send(result).unwrap();
    });
    let error = child_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(error.contains("attended-operator"), "{error}");
    assert!(error.contains("already live"), "{error}");
    child.join().unwrap();
    drop(parent_guards);
    release_tx.send(()).unwrap();
    operator.join().unwrap();
}
