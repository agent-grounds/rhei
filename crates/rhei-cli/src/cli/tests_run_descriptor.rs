// The run descriptor: what a run publishes, what it stamps on the way out, and
// how liveness is decided.
// §FS-rhei-run-headless.2 §FS-rhei-run-headless.3

mod run_descriptor_tests {
    use super::super::*;

    /// The registry is machine-wide, so tests must not use the real one. Each
    /// test gets its own `XDG_STATE_HOME`; the env is process-global, so they
    /// are serialized through one mutex.
    pub(super) static REGISTRY_GUARD: Mutex<()> = Mutex::new(());

    pub(super) struct IsolatedRegistry {
        _dir: tempfile::TempDir,
        _guard: std::sync::MutexGuard<'static, ()>,
        previous: Option<std::ffi::OsString>,
    }

    impl IsolatedRegistry {
        pub(super) fn new() -> Self {
            let guard = REGISTRY_GUARD.lock().unwrap_or_else(|p| p.into_inner());
            let dir = tempfile::tempdir().expect("state dir");
            let previous = std::env::var_os("XDG_STATE_HOME");
            std::env::set_var("XDG_STATE_HOME", dir.path());
            Self { _dir: dir, _guard: guard, previous }
        }
    }

    impl Drop for IsolatedRegistry {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var("XDG_STATE_HOME", value),
                None => std::env::remove_var("XDG_STATE_HOME"),
            }
        }
    }

    /// A workspace whose path is already canonical, so what a test writes and
    /// what publication absolutizes to are the same string.
    pub(super) struct TestWorkspace {
        _dir: tempfile::TempDir,
        pub(super) path: PathBuf,
    }

    pub(super) fn workspace() -> TestWorkspace {
        let dir = tempfile::tempdir().expect("workspace");
        let path = dir.path().canonicalize().expect("canonical workspace");
        TestWorkspace { _dir: dir, path }
    }

    pub(super) fn descriptor(id: &str, workspace: &Path, started_at: &str) -> RunDescriptor {
        RunDescriptor {
            id: id.to_string(),
            pid: std::process::id(),
            status: RunStatus::Running,
            workspace: workspace.to_path_buf(),
            plan: workspace.join("plan.rhei.md"),
            state_machine: None,
            control_url: Some("http://127.0.0.1:54321".to_string()),
            started_at: started_at.to_string(),
            headless: true,
            parallel: 2,
            log: Some(workspace.join("runtime/run.log")),
            events: workspace.join("runtime/events.jsonl"),
            exit_code: None,
        }
    }

    /// Hold the current run lock as the supplied descriptor's exact owner.
    pub(super) fn held_run_lock_for(descriptor: &RunDescriptor) -> HeldRunLock {
        let mut held = try_acquire_run_lock(&descriptor.workspace)
            .expect("lock")
            .expect("available");
        write_run_lock_owner(&mut held, &descriptor.id, descriptor.pid)
            .expect("record lock owner");
        held
    }

    #[cfg(unix)]
    fn exited_process_pid() -> u32 {
        let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .arg("--help")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn short-lived process");
        let pid = child.id();
        assert!(child.wait().expect("wait for short-lived process").success());
        pid
    }

    /// A run that has recorded its own end, published into `workspace`.
    pub(super) fn publish_ended(id: &str, workspace: &Path, started_at: &str) {
        let mut ended = descriptor(id, workspace, started_at);
        ended.status = RunStatus::Finished;
        ended.exit_code = Some(0);
        publish_run_descriptor(&ended);
        if let Some(entry) = run_registry_path(id) {
            assert!(entry.is_file(), "the entry must exist for the test to mean anything");
        }
    }

    #[test]
    fn a_descriptor_round_trips_through_its_published_file() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let original = descriptor("aa11bb", &workspace.path, "2026-08-22T14:03:22Z");
        publish_run_descriptor(&original);

        let read = read_descriptor(&run_descriptor_path(&workspace.path)).expect("published");
        assert_eq!(read.id, "aa11bb");
        assert_eq!(read.pid, original.pid);
        assert_eq!(read.control_url.as_deref(), Some("http://127.0.0.1:54321"));
        assert!(read.headless);
        assert_eq!(read.exit_code, None);
    }

    #[test]
    fn publishing_also_writes_the_registry_pointer() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        publish_run_descriptor(&descriptor("cc22dd", &workspace.path, "2026-08-22T14:03:22Z"));
        let pointer = run_registry_dir().expect("registry dir").join("cc22dd.json");
        assert!(pointer.is_file(), "a bare id must be resolvable from anywhere");
    }

    /// A descriptor is read from someone else's working directory, so a
    /// relative plan path names nothing there. §FS-rhei-run-headless.2
    #[test]
    fn publishing_records_absolute_paths_whatever_it_was_given() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        fs::write(workspace.path.join("plan.rhei.md"), "# Rhei: X\n").expect("plan");
        let mut relative = descriptor("re11at", &workspace.path, "2026-08-22T14:03:22Z");
        relative.plan = PathBuf::from("./plan.rhei.md");
        relative.events = PathBuf::from("runtime/events.jsonl");
        publish_run_descriptor(&relative);

        let read = read_descriptor(&run_descriptor_path(&workspace.path)).expect("published");
        assert!(read.plan.is_absolute(), "plan was recorded as {}", read.plan.display());
        assert!(read.events.is_absolute(), "events was recorded as {}", read.events.display());
        assert!(read.workspace.is_absolute());
    }

    /// The registry entry outlives the run: `rhei attach <id>` after the run
    /// ends is the whole point of the CI shape of §FS-rhei-run-headless.5.3.
    #[test]
    fn finalizing_stamps_the_exit_code_and_keeps_the_registry_entry() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        publish_run_descriptor(&descriptor("ee33ff", &workspace.path, "2026-08-22T14:03:22Z"));

        finalize_run_descriptor(3);

        let read = read_descriptor(&run_descriptor_path(&workspace.path)).expect("published");
        assert_eq!(read.status, RunStatus::Failed);
        assert_eq!(read.exit_code, Some(3));
        // The control server is gone with the process; leaving its URL behind
        // would send a reader to a closed socket.
        assert_eq!(read.control_url, None);

        let entry = run_registry_dir().expect("registry dir").join("ee33ff.json");
        let kept = read_descriptor(&entry).expect("the entry outlives the run");
        assert_eq!(kept.status, RunStatus::Failed);
        assert_eq!(kept.exit_code, Some(3), "the entry carries the answer, not a dangling pointer");
    }

    #[test]
    fn a_zero_exit_finalizes_as_finished() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        publish_run_descriptor(&descriptor("aabbcc", &workspace.path, "2026-08-22T14:03:22Z"));
        finalize_run_descriptor(0);
        let read = read_descriptor(&run_descriptor_path(&workspace.path)).expect("published");
        assert_eq!(read.status, RunStatus::Finished);
        assert_eq!(read.exit_code, Some(0));
    }

    /// Six hex characters collide eventually. A finalizing run must not stamp
    /// a *live* run's entry with its own exit code. §FS-rhei-run-headless.2
    #[test]
    fn finalizing_leaves_an_entry_that_belongs_to_another_run_alone() {
        let _registry = IsolatedRegistry::new();
        let mine = workspace();
        let theirs = workspace();
        publish_run_descriptor(&descriptor("c01115", &mine.path, "2026-08-22T14:03:22Z"));
        // Same id, different workspace and pid: a collision, not this run.
        let mut other = descriptor("c01115", &theirs.path, "2026-08-22T15:00:00Z");
        other.pid = std::process::id() + 1;
        let entry = run_registry_path("c01115").expect("registry path");
        write_descriptor(&entry, &other).expect("their entry");

        finalize_run_descriptor(7);

        let kept = read_descriptor(&entry).expect("entry");
        assert_eq!(kept.workspace, theirs.path, "the other run's entry was rewritten");
        assert_eq!(kept.exit_code, None);
    }

    /// A `SIGKILL`ed run leaves its descriptor saying `running`. A free lock
    /// and a confirmed-absent recorded process still say it is gone.
    // §FS-rhei-run-headless.3
    #[test]
    fn a_dead_recorded_process_with_a_free_lock_has_ended() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let running = descriptor("dead01", &workspace.path, "2026-08-22T14:03:22Z");
        #[cfg(unix)]
        let running = {
            let mut running = running;
            running.pid = exited_process_pid();
            running
        };
        publish_run_descriptor(&running);
        // The lock file has to exist before its absence stops being ambiguous.
        drop(try_acquire_run_lock(&workspace.path).expect("lock"));
        assert_eq!(
            running.liveness(),
            Liveness::Ended,
            "nothing holds the lock, so the run is gone whatever its status says"
        );
    }

    /// Linux contention identifies a lock holder, not the recorded supervisor.
    /// A successful exact-owner check finds this dead pid and decides the
    /// descriptor has ended. §FS-rhei-run-headless.3
    #[cfg(target_os = "linux")]
    #[test]
    fn a_dead_supervisor_with_a_held_current_lock_has_ended() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let mut running = descriptor("dead02", &workspace.path, "2026-08-22T14:03:22Z");
        running.pid = exited_process_pid();
        publish_run_descriptor(&running);
        let _held = try_acquire_run_lock(&workspace.path).expect("lock").expect("available");

        assert_eq!(
            running.liveness(),
            Liveness::Ended,
            "another process retaining the current lock does not keep the recorded pid live"
        );
    }

    /// An exact process can still hold the wrong run's lock. A successful
    /// inspection of the structured owner record decides that mismatch as an
    /// end, not as liveness for this descriptor. §FS-rhei-run-headless.3
    #[cfg(target_os = "linux")]
    #[test]
    fn a_recorded_process_that_does_not_own_the_held_current_lock_has_ended() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let running = descriptor("stale1", &workspace.path, "2026-08-22T14:03:22Z");
        publish_run_descriptor(&running);
        let mut held = try_acquire_run_lock(&workspace.path).expect("lock").expect("available");
        write_run_lock_owner(&mut held, "other1", running.pid).expect("other run's owner record");

        assert_eq!(running.liveness(), Liveness::Ended);
    }

    /// A held pathname with an unreadable ownership record proves contention
    /// but cannot decide who owns it. §FS-rhei-run-headless.3
    #[cfg(target_os = "linux")]
    #[test]
    fn an_inconclusive_owner_check_for_the_held_current_lock_is_unknown() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let running = descriptor("blind1", &workspace.path, "2026-08-22T14:03:22Z");
        publish_run_descriptor(&running);
        let _held = try_acquire_run_lock(&workspace.path).expect("lock").expect("available");

        assert!(matches!(running.liveness(), Liveness::Unknown(_)));
    }

    /// A free replacement pathname does not decide liveness when the recorded
    /// process cannot be checked. §FS-rhei-run-headless.3
    #[cfg(target_os = "linux")]
    #[test]
    fn an_inconclusive_process_probe_with_a_free_lock_is_unknown() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let mut running = descriptor("badpid", &workspace.path, "2026-08-22T14:03:22Z");
        running.pid = 0;
        publish_run_descriptor(&running);
        drop(try_acquire_run_lock(&workspace.path).expect("lock"));

        assert!(matches!(running.liveness(), Liveness::Unknown(_)));
    }

    #[test]
    fn a_terminal_status_is_never_reported_live() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let _held = try_acquire_run_lock(&workspace.path).expect("lock").expect("available");
        let mut finished = descriptor("done01", &workspace.path, "2026-08-22T14:03:22Z");
        finished.status = RunStatus::Finished;
        publish_run_descriptor(&finished);
        // The lock is held here by the *test*, not by the run: a descriptor
        // that recorded its own end must not be resurrected by a later run's
        // lock on the same workspace.
        assert_eq!(finished.liveness(), Liveness::Ended);
    }

    /// The bug this check exists for: a stale entry from a dead run read as
    /// live the moment an unrelated run took the same workspace, because a held
    /// lock says only that *someone* is there. §FS-rhei-run-headless.3
    #[test]
    fn a_superseded_entry_is_gone_even_while_another_run_holds_the_workspace() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let ghost = descriptor("ghost1", &workspace.path, "2026-08-22T10:00:00Z");
        publish_run_descriptor(&ghost);

        let successor = descriptor("live22", &workspace.path, "2026-08-22T11:00:00Z");
        publish_run_descriptor(&successor);
        let _held = held_run_lock_for(&successor);

        assert_eq!(ghost.liveness(), Liveness::Gone, "the workspace no longer names the ghost");
        assert_eq!(
            successor.liveness(),
            Liveness::Live,
            "the run the workspace names is the live one"
        );
    }

    /// Matching short ids are not enough: the workspace copy must still name
    /// the registry record's pid before any held lock can make it live.
    // §FS-rhei-run-headless.3
    #[test]
    fn a_same_id_registry_entry_with_a_different_workspace_pid_is_gone() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let registry = descriptor("sameid", &workspace.path, "2026-08-22T10:00:00Z");
        let mut current = registry.clone();
        current.pid = registry.pid.wrapping_add(1).max(1);
        publish_run_descriptor(&current);
        let entry = run_registry_path(&registry.id).expect("registry path");
        write_descriptor(&entry, &registry).expect("stale registry entry");
        let _held = try_acquire_run_lock(&workspace.path).expect("lock").expect("available");

        assert_eq!(registry.liveness(), Liveness::Gone);
    }

    #[test]
    fn a_workspace_that_is_gone_makes_its_entry_prunable() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let run = descriptor("rmrf01", &workspace.path, "2026-08-22T10:00:00Z");
        publish_run_descriptor(&run);
        fs::remove_dir_all(&workspace.path).expect("delete the workspace");
        assert_eq!(run.liveness(), Liveness::Gone);
    }

    /// A `chmod 000` on `.rhei` is a transient accident, not a death
    /// certificate — and the sweep that reads this verdict deletes things.
    // §FS-rhei-run-headless.3
    #[cfg(unix)]
    #[test]
    fn an_unreadable_run_lock_is_unknown_rather_than_dead() {
        use std::os::unix::fs::PermissionsExt;
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let mut run = descriptor("locked1", &workspace.path, "2026-08-22T10:00:00Z");
        run.pid = exited_process_pid();
        publish_run_descriptor(&run);
        drop(try_acquire_run_lock(&workspace.path).expect("lock"));

        let rhei_dir = workspace.path.join(".rhei");
        fs::set_permissions(&rhei_dir, fs::Permissions::from_mode(0o000)).expect("chmod 000");
        let verdict = run.liveness();
        fs::set_permissions(&rhei_dir, fs::Permissions::from_mode(0o755)).expect("chmod 755");

        assert!(
            matches!(verdict, Liveness::Unknown(_)),
            "an unreadable lock says nothing about the run, got {verdict:?}"
        );
        assert_eq!(run.liveness(), Liveness::Ended, "and it is readable again afterwards");
    }

    /// `flock` survives unlinking. The exact recorded process and its stamped
    /// lock inode close the gap left by the missing pathname.
    // §FS-rhei-run-headless.3
    #[cfg(target_os = "linux")]
    #[test]
    fn a_process_that_owns_the_unlinked_recorded_lock_is_live() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let run = descriptor("nolock", &workspace.path, "2026-08-22T10:00:00Z");
        publish_run_descriptor(&run);
        let mut held = try_acquire_run_lock(&workspace.path).expect("lock").expect("available");
        write_run_lock_owner(&mut held, &run.id, run.pid).expect("record lock owner");
        fs::remove_file(workspace.path.join(".rhei/run.lock")).expect("unlink held lock");

        assert_eq!(run.liveness(), Liveness::Live);
    }

    /// A matching stale descriptor pair, an allocated pid, and even ownership
    /// of the stale inode do not identify a run after the pid's start identity
    /// changes.
    // §FS-rhei-run-headless.3
    #[cfg(target_os = "linux")]
    #[test]
    fn a_reused_live_pid_with_a_stale_lock_identity_has_ended() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let run = descriptor("reused", &workspace.path, "2026-08-22T10:00:00Z");
        publish_run_descriptor(&run);
        let mut held = try_acquire_run_lock(&workspace.path).expect("lock").expect("available");
        let stale_owner = RunLockOwner {
            version: 1,
            id: run.id.clone(),
            pid: run.pid,
            workspace: run.workspace.clone(),
            process_start_ticks: Some(0),
        };
        held.file.set_len(0).expect("clear owner");
        serde_json::to_writer(&held.file, &stale_owner).expect("stale owner");
        held.file.flush().expect("flush stale owner");
        let lock_path = workspace.path.join(".rhei/run.lock");
        fs::rename(&lock_path, workspace.path.join(".rhei/run.lock.stale")).expect("rename lock");
        fs::write(&lock_path, []).expect("free replacement");

        assert_eq!(run.liveness(), Liveness::Ended);
    }

    /// A listing is a read. `open_run_lock_file` creates the directory and the
    /// file, so probing every registered workspace used to write into all of
    /// them. §FS-rhei-run-headless.3
    #[test]
    fn probing_liveness_creates_nothing_in_the_workspace() {
        let _registry = IsolatedRegistry::new();
        let workspace = workspace();
        let run = descriptor("nowrite", &workspace.path, "2026-08-22T10:00:00Z");
        publish_run_descriptor(&run);
        let _ = run.liveness();
        assert!(!workspace.path.join(".rhei").exists(), "a probe must not write to disk");
    }
    /// `exit_code` is part of the documented shape whether or not it is known.
    /// Omitting it while it is `null` made a consumer tell "still running" from
    /// "this build dropped the field", which the object offers no way to do.
    // §FS-rhei-run-headless.2
    #[test]
    fn exit_code_is_serialized_even_while_it_is_unknown() {
        let workspace = workspace();
        let running = descriptor("nullex", &workspace.path, "2026-08-22T14:03:22Z");
        let rendered: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&running).expect("render"))
                .expect("valid JSON");
        assert!(rendered.get("exit_code").expect("present").is_null(), "got: {rendered}");
        // A genuinely optional field still goes missing, so the two cases stay
        // distinguishable.
        let mut foreground = running.clone();
        foreground.log = None;
        let rendered: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&foreground).expect("render"))
                .expect("valid JSON");
        assert!(rendered.get("log").is_none(), "an absent console is absent: {rendered}");
    }

    /// The shared budget every waiting consumer uses. Undecided must not mean
    /// "give up now" — the first probe only starts the clock — and must not mean
    /// "wait forever" either.
    // §FS-rhei-run-headless.3
    #[test]
    fn an_undecided_watch_reports_only_once_its_grace_has_run_out() {
        let mut watch = UndecidedWatch::default();
        assert!(!watch.exhausted("lock unreadable"), "the first probe starts the grace");
        assert!(!watch.exhausted("lock unreadable"), "and the next one is still inside it");

        // Reaching into the clock rather than sleeping out the real grace: what
        // is under test is the decision, not the duration.
        watch.since = Some(Instant::now() - UNDECIDED_GRACE - Duration::from_millis(1));
        assert!(watch.exhausted("still unreadable"));
        assert_eq!(watch.reason(), "still unreadable", "it reports the last reason it saw");
        assert!(!watch.exhausted("still unreadable"), "and rearms rather than firing every poll");

        watch.decided();
        assert!(watch.reason().is_empty(), "a decided probe retires the grace");
        assert!(!watch.exhausted("unreadable again"), "which starts a fresh one");
    }

    /// `rhei stop --wait` returned the moment a lock became unreadable, saying
    /// the run had ended while it was still tearing down the work it had just
    /// been asked to stop.
    // §FS-rhei-run-headless.7 §FS-rhei-run-headless.3
    #[cfg(unix)]
    #[test]
    fn stops_wait_keeps_waiting_while_liveness_is_undecided() {
        use std::os::unix::fs::PermissionsExt;
        let workspace = workspace();
        let run = descriptor("waitng", &workspace.path, "2026-08-22T10:00:00Z");
        write_test_descriptor(&run);
        // The lock file must exist and be unreadable: existence is what makes
        // its absence stop being the answer, unreadability is the outage.
        drop(try_acquire_run_lock(&workspace.path).expect("lock"));
        let lock = workspace.path.join(".rhei").join("run.lock");
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o000)).expect("chmod 000");
        assert!(matches!(run.liveness(), Liveness::Unknown(_)), "the case under test");

        let (done, waited) = std::sync::mpsc::channel();
        let waiting = run.clone();
        let waiter = std::thread::spawn(move || {
            let outcome = await_run_end(&waiting);
            let _ = done.send(());
            outcome
        });
        assert!(
            waited.recv_timeout(Duration::from_secs(1)).is_err(),
            "it returned on an undecided probe instead of waiting"
        );

        // The run records its own end, which is decidable even with the lock
        // still unreadable — and is what a real `stop` is waiting for.
        let mut ended = run.clone();
        ended.status = RunStatus::Finished;
        ended.exit_code = Some(130);
        write_test_descriptor(&ended);
        let outcome = waiter.join().expect("the waiter returns once the run records its end");
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o644)).expect("chmod 644");
        assert!(outcome.is_ok(), "it saw the recorded end: {outcome:?}");
    }

    /// Write a workspace descriptor without publishing a registry entry: these
    /// cases are about one workspace, and the registry is machine-wide.
    #[cfg(unix)]
    fn write_test_descriptor(descriptor: &RunDescriptor) {
        let path = run_descriptor_path(&descriptor.workspace);
        fs::create_dir_all(path.parent().expect("runtime directory")).expect("runtime directory");
        fs::write(&path, serde_json::to_string_pretty(descriptor).expect("render"))
            .expect("workspace descriptor");
    }

}
