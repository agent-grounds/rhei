// Initialization observes the same pending-root boundary as ordinary writers. §FS-rhei-init.2

fn operator_init_pending(root: &Path) {
    fs::create_dir_all(root.join("runtime")).unwrap();
    let plan = root.join("plan.rhei.md");
    let machine = root.join("states.yaml");
    fs::write(&plan, "# Rhei: Recovery\n\n## Tasks\n\n### Task 1: Fix\n**State:** gate\n").unwrap();
    fs::write(&machine, OPERATOR_MACHINE).unwrap();
    fs::write(root.join(".gitignore"), "# keep host ignores\n").unwrap();
    fs::write(root.join("AGENTS.md"), "Keep host instructions.\n").unwrap();
    fs::write(root.join("runtime/keep.txt"), "Keep runtime bytes.\n").unwrap();
    let request = operator_request(&plan, &machine);
    let prepared = prepare_forced_transition(&request).unwrap();
    interrupt_force_at("marker-after");
    let error = commit_confirmed_force(&request, prepared, "test").unwrap_err();
    clear_force_interrupt();
    assert!(error.to_string().contains("marker-after"), "{error}");
    assert_eq!(recorded_marker(root).hop.task_id, "plan.1");
}

/// A canonical marker refuses before host updates or new directory creation. §FS-rhei-recover.4
fn operator_init_refuses_marked_host(here: bool) {
    let dir = tempfile::tempdir().unwrap();
    operator_init_pending(dir.path());
    let before = operator_snapshot(dir.path());
    let error = init_command(Some(dir.path()), None, true, true, here).unwrap_err().to_string();
    assert!(error.contains("plan.1 gate -> work"), "{error}");
    assert_eq!(error.matches("rhei recover ").count(), 1);
    assert_eq!(operator_snapshot(dir.path()), before);
    assert!(!dir.path().join("panta").exists());
    assert!(!dir.path().join("index.panta.md").exists());
}

#[test]
fn operator_init_default_refuses_marked_host_before_creating_destination() {
    operator_init_refuses_marked_host(false);
}

#[test]
fn operator_init_here_refuses_marked_host() {
    operator_init_refuses_marked_host(true);
}

/// Destination and containing execution owners participate in discovery. §FS-rhei-panta.6.6
#[test]
fn operator_init_refuses_marked_destination_and_nested_owner() {
    for (destination, here) in [(true, false), (false, false), (false, true)] {
        let dir = tempfile::tempdir().unwrap();
        let marked = if destination { dir.path().join("panta") } else { dir.path().to_path_buf() };
        operator_init_pending(&marked);
        let host =
            if destination { dir.path().to_path_buf() } else { dir.path().join("new/nested") };
        let before = operator_snapshot(dir.path());
        let error = init_command(Some(&host), None, true, true, here).unwrap_err().to_string();
        assert!(error.contains("plan.1 gate -> work"), "{destination}/{here}: {error}");
        assert_eq!(operator_snapshot(dir.path()), before);
    }
}

/// Missing hosts/destinations remain creatable, and normal companion writes still occur. §FS-rhei-init.2
#[test]
fn operator_init_unmarked_creation_preserves_both_modes() {
    for here in [false, true] {
        for missing_host in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let host =
                if missing_host { dir.path().join("new/nested") } else { dir.path().to_path_buf() };
            init_command(Some(&host), Some("New project"), false, false, here).unwrap();
            let project = if here { host.clone() } else { host.join("panta") };
            assert_eq!(
                fs::read_to_string(project.join("index.panta.md")).unwrap(),
                "# Panta: New project\n"
            );
            assert!(fs::read_to_string(host.join("AGENTS.md"))
                .unwrap()
                .contains(AGENTS_NOTE_BEGIN));
            assert!(fs::read_to_string(host.join(".gitignore")).unwrap().contains(if here {
                "runtime/"
            } else {
                "panta/"
            }));
        }
    }
}

/// Exclusive publication cannot enter during the remaining host/destination writes. §FS-rhei-recover.4
#[test]
fn operator_init_retains_existing_owner_guards_through_companion_writes() {
    use rhei_core::root_access::RootAccessGuard;
    for existing_destination in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path().to_path_buf();
        let destination = host.join("panta");
        if existing_destination {
            fs::create_dir(&destination).unwrap();
        }
        let (paused_tx, paused_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let worker_host = host.clone();
        let worker = std::thread::spawn(move || {
            INIT_AFTER_MANIFEST.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(move || {
                    paused_tx.send(()).unwrap();
                    resume_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                }))
            });
            init_command(Some(&worker_host), None, false, false, false)
                .map_err(|err| err.to_string())
        });
        paused_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(destination.join("index.panta.md").exists());
        assert!(!host.join(".gitignore").exists());
        assert!(RootAccessGuard::try_exclusive(&host).unwrap().is_none());
        if existing_destination {
            assert!(RootAccessGuard::try_exclusive(&destination).unwrap().is_none());
        }
        resume_tx.send(()).unwrap();
        worker.join().unwrap().unwrap();
        assert!(host.join("AGENTS.md").exists());
        assert!(destination.join(".gitignore").exists());
        assert!(RootAccessGuard::try_exclusive(&host).unwrap().is_some());
        assert!(RootAccessGuard::try_exclusive(&destination).unwrap().is_some());
    }
}
