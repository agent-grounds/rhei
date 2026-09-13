// The platform facts every rewriting command shares: what a refused lock
// means, which plan reading is authoritative, and the stable sidecar identity.

// §FS-rhei-run-headless.3 §FS-rhei-new.4 §AR-agent-orchestrator-workflow.3.3.1

mod file_lock_tests {
    use super::super::*;

    fn plan_file(dir: &tempfile::TempDir, contents: &str) -> PathBuf {
        let path = dir.path().join("plan.rhei.md");
        fs::write(&path, contents).expect("write plan");
        path
    }

    #[test]
    fn a_refused_lock_reads_as_held() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "held\n");

        // Two open file descriptions on one file, from this one process: on
        // Unix that is enough for `flock` to refuse the second, which is the
        // refusal the probe has to classify.
        let holder = fs::File::open(&path).expect("open holder");
        holder.lock_exclusive().expect("hold the lock");
        let contender = fs::File::open(&path).expect("open contender");

        let err = contender.try_lock_exclusive().expect_err("a held lock must refuse");
        assert!(lock_is_contended(&err), "a refused lock is a held lock: {err:?}");

        let _ = fs2::FileExt::unlock(&holder);
    }

    #[test]
    fn an_error_that_is_not_a_refusal_says_nothing_about_a_holder() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let err = fs::File::open(dir.path().join("absent.rhei.md"))
            .expect_err("opening a missing file must fail");

        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert!(!lock_is_contended(&err), "a missing file names no lock holder");
    }

    #[test]
    fn a_locked_plan_reads_back_while_the_lock_is_held() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "locked\n");
        let locked = LockedPlanFile::open(&path).expect("lock the plan");

        // The Windows case, asserted everywhere: a mandatory byte-range lock
        // refuses this process its own read by path, and the read falls back to
        // the handle it is holding rather than failing the command.
        assert_eq!(locked.read_to_string("failed to read plan file").expect("read"), "locked\n");
        locked.release();
    }

    #[test]
    fn a_waiter_reads_the_current_path_after_replacement() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "before\n");
        let locked = LockedPlanFile::open(&path).expect("lock the plan");
        let waiting_path = path.clone();
        let (lock_tx, lock_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            set_plan_lock_observer(lock_tx);
            let result = LockedPlanFile::open(&waiting_path).and_then(|waiting| {
                let raw = waiting.read_to_string("failed to read waiting plan")?;
                waiting.release();
                Ok(raw)
            });
            done_tx.send(result.map_err(|error| error.to_string())).expect("waiter result");
        });
        assert_eq!(
            lock_rx.recv_timeout(Duration::from_secs(2)).expect("waiter lock attempt"),
            PlanLockEvent::Contended
        );

        let mut replacement = tempfile::NamedTempFile::new_in(dir.path()).expect("temp file");
        replacement.write_all(b"after\n").expect("write replacement");
        persist_locked(replacement, &path, Some(&locked)).expect("replace the plan");
        assert!(
            matches!(done_rx.recv_timeout(Duration::from_millis(50)), Err(RecvTimeoutError::Timeout)),
            "the waiter completed before the replacing writer released its stable sidecar"
        );
        locked.release();

        assert_eq!(
            lock_rx.recv_timeout(Duration::from_secs(2)).expect("waiter lock acquisition"),
            PlanLockEvent::Acquired
        );
        assert_eq!(
            done_rx.recv_timeout(Duration::from_secs(2)).expect("waiter completion").unwrap(),
            "after\n"
        );
        waiter.join().expect("waiter thread");
    }

    #[test]
    fn releasing_a_plan_lock_twice_is_the_same_as_releasing_it_once() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "once\n");
        let locked = LockedPlanFile::open(&path).expect("lock the plan");

        locked.release();
        locked.release();

        // A released lock still reads: the path is the source of truth, and the
        // handle was only ever the fallback.
        assert_eq!(locked.read_to_string("failed to read plan file").expect("read"), "once\n");
    }

    /// A handle whose lock has been let go is never a source of content: after
    /// `persist_locked`'s rename it names an orphan, so the caller keeps its
    /// original refusal instead. §FS-rhei-new.4
    #[test]
    fn a_released_handle_serves_no_read() {
        let handle: PlanLockHandle = Arc::new(Mutex::new(None));
        assert!(
            read_through_handle(&handle).is_none(),
            "a released handle must not answer a read"
        );
    }

    /// And it is gone from the registry too, so nothing else in the process
    /// finds it either — including the loader's own path reader.
    // §FS-rhei-new.4
    #[test]
    fn a_released_lock_is_deregistered() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "registered\n");
        let locked = LockedPlanFile::open(&path).expect("lock the plan");
        assert!(
            read_through_held_lock(&path).is_some(),
            "a held lock must be reachable by path"
        );

        locked.release();

        assert!(
            read_through_held_lock(&path).is_none(),
            "a released lock must leave nothing behind to read through"
        );
    }

    #[test]
    fn a_rewrite_persists_over_the_file_it_locked() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "before\n");
        let locked = LockedPlanFile::open(&path).expect("lock the plan");

        let mut tmp = tempfile::NamedTempFile::new_in(dir.path()).expect("temp file");
        tmp.write_all(b"after\n").expect("write temp");
        persist_locked(tmp, &path, Some(&locked)).expect("persist over the locked plan");
        locked.release();

        assert_eq!(fs::read_to_string(&path).expect("read back"), "after\n");
        let mut leftovers: Vec<String> = fs::read_dir(dir.path())
            .expect("read dir")
            .map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|name| name != "plan.rhei.md")
            .collect();
        leftovers.sort();
        assert_eq!(
            leftovers,
            vec!["plan.rhei.md.lock"],
            "only the persistent writer sidecar remains"
        );
    }
}
