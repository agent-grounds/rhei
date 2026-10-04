// A guard taken beside a test that moves XDG_STATE_HOME still locks where its
// own test does. agent-grounds/rhei#455 §REQ-test-isolation.6

/// Stands in for `an_unwritable_registry_is_warned_about_with_its_reason_and_without_a_wait`:
/// holds `REGISTRY_GUARD`, points the process-wide `XDG_STATE_HOME` at a dir whose
/// `rhei` is a regular file, and keeps it there until `done` arrives or 5 s pass,
/// so a fix that serializes through the guard waits instead of deadlocking.
fn block_the_state_home(
    ready: std::sync::mpsc::Sender<()>,
    done: std::sync::mpsc::Receiver<()>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let _guard = crate::tests::run_descriptor_tests::REGISTRY_GUARD
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let state = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("XDG_STATE_HOME");
        std::env::set_var("XDG_STATE_HOME", state.path());
        fs::write(state.path().join("rhei"), "not a directory").unwrap();
        ready.send(()).unwrap();
        let _ = done.recv_timeout(std::time::Duration::from_secs(5));
        match previous {
            Some(value) => std::env::set_var("XDG_STATE_HOME", value),
            None => std::env::remove_var("XDG_STATE_HOME"),
        }
    })
}

/// The basin's forced replay and a whole forced transition commit while a sibling
/// test holds the state home blocked. The path is absolute on every platform, so
/// Windows honours it ahead of LOCALAPPDATA and the pin means the same there.
/// §REQ-test-isolation.6 §FS-rhei-recover.4
#[test]
#[ignore = "#455: a root guard locks under whatever XDG_STATE_HOME a sibling test set"]
fn operator_basin_commits_while_another_test_blocks_the_state_home() {
    let (_dir, project, root, machine) = operator_basin_fixture();
    let request = operator_basin_request(&project, &machine);
    let prepared = prepare_forced_transition(&request).unwrap();
    let images = prepared.files.clone();
    interrupt_force_at("image-1-after");
    assert!(commit_confirmed_force(&request, prepared, "test").is_err());
    clear_force_interrupt();
    let marker = recorded_marker(&root);
    let decision = forced_decision(&root, &marker).unwrap();

    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let blocker = block_the_state_home(ready_tx, done_rx);
    ready_rx.recv().unwrap();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        forced_replay(&root, &marker, decision, "test").unwrap();
        assert_basin_images(&root, &images, decision == ForcedDecision::Forward);
        assert!(!root.join(rhei_core::root_access::MARKER).exists());
        operator_basin_force_commits_all_three_images();
    }));
    let _ = done_tx.send(());
    blocker.join().unwrap();
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic)
    }
}
