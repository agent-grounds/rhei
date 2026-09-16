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

    /// The stable sibling sidecar is the sole writer lock. Keeping the plan
    /// destination unlocked is what lets callbacks and atomic replacement open
    /// the current pathname on mandatory-lock platforms.
    // §AR-agent-orchestrator-workflow.3.3.1 §FS-rhei-transition-cmd.3
    #[test]
    fn issue_95_writer_sidecar_does_not_lock_the_plan_destination() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "available by pathname\n");
        let locked = LockedPlanFile::open(&path).expect("take writer sidecar");

        let destination = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open unlocked destination");
        destination
            .try_lock_exclusive()
            .expect("Rhei must leave the replaceable plan destination unlocked");
        fs2::FileExt::unlock(&destination).expect("release observation lock");
        locked.release();
    }

    /// Creation establishes the destination's permanent writer identity before
    /// first publication, so locking cannot require the plan to exist already.
    // §AR-agent-orchestrator-workflow.3.3.1 §FS-rhei-new.4
    #[test]
    fn issue_95_writer_sidecars_guard_three_absent_candidates_permanently() {
        let dir = tempfile::tempdir().expect("tmpdir");
        for candidate in ["first.rhei.md", "second.rhei.md", "third.rhei.md"] {
            let path = dir.path().join(candidate);
            let locked = LockedPlanFile::open(&path).expect("lock before publication");
            assert!(!path.exists(), "taking the sidecar must not publish the plan");
            locked.release();
        }
        for sidecar in ["first.rhei.md.lock", "second.rhei.md.lock", "third.rhei.md.lock"] {
            assert!(
                dir.path().join(sidecar).is_file(),
                "an abandoned candidate must keep its coordination identity"
            );
        }
    }

    /// A participant waiting across first publication opens the current path
    /// only after acquisition and therefore observes the published bytes.
    // §AR-agent-orchestrator-workflow.3.3.1 §FS-rhei-new.4
    #[test]
    fn issue_95_first_publication_excludes_a_waiter_until_success() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("future.rhei.md");
        let locked = LockedPlanFile::open(&path).expect("lock before publication");
        let waiting_path = path.clone();
        let (lock_tx, lock_rx) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            set_plan_lock_observer(lock_tx);
            let waiting = LockedPlanFile::open(&waiting_path).expect("wait for publication");
            let raw = waiting.read_to_string("read published plan");
            waiting.release();
            raw.map_err(|error| error.to_string())
        });
        assert_eq!(
            lock_rx.recv_timeout(Duration::from_secs(2)).expect("waiter lock attempt"),
            PlanLockEvent::Contended
        );

        fs::write(&path, "published\n").expect("publish plan");
        locked.release();

        assert_eq!(
            lock_rx.recv_timeout(Duration::from_secs(2)).expect("waiter acquisition"),
            PlanLockEvent::Acquired
        );
        assert_eq!(waiter.join().expect("waiter thread").expect("waiter read"), "published\n");
    }

    /// The same waiter observes authoritative absence after rollback; it never
    /// reads a provisional or stale destination handle.
    // §AR-agent-orchestrator-workflow.3.3.1 §FS-rhei-new.4
    #[test]
    fn issue_95_first_publication_waiter_observes_rollback_absence() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("future.rhei.md");
        let locked = LockedPlanFile::open(&path).expect("lock before publication");
        let waiting_path = path.clone();
        let observed_path = path.clone();
        let (lock_tx, lock_rx) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            set_plan_lock_observer(lock_tx);
            let waiting = LockedPlanFile::open(&waiting_path).expect("wait for rollback");
            let exists = observed_path.exists();
            waiting.release();
            exists
        });
        assert_eq!(
            lock_rx.recv_timeout(Duration::from_secs(2)).expect("waiter lock attempt"),
            PlanLockEvent::Contended
        );

        fs::write(&path, "provisional\n").expect("provisional publication");
        fs::remove_file(&path).expect("roll back plan data");
        locked.release();

        assert_eq!(
            lock_rx.recv_timeout(Duration::from_secs(2)).expect("waiter acquisition"),
            PlanLockEvent::Acquired
        );
        assert!(!waiter.join().expect("waiter thread"), "waiter must observe rolled-back absence");
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
