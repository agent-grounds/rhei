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
    fn a_sidecar_locked_plan_reads_back_by_its_current_path() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "locked\n");
        let locked = LockedPlanFile::open(&path).expect("lock the plan");

        // The sidecar never denies this process a current-path read, including
        // on mandatory-lock platforms. §FS-rhei-transition-cmd.3
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
    /// reads provisional or stale destination bytes.
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
        persist_locked(replacement, &path).expect("replace the plan");
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

        // A released lock still reads because the current pathname remains the
        // sole source of truth.
        assert_eq!(locked.read_to_string("failed to read plan file").expect("read"), "once\n");
    }

    #[test]
    fn a_rewrite_persists_over_the_file_it_locked() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "before\n");
        let locked = LockedPlanFile::open(&path).expect("lock the plan");

        let mut tmp = tempfile::NamedTempFile::new_in(dir.path()).expect("temp file");
        tmp.write_all(b"after\n").expect("write temp");
        persist_locked(tmp, &path).expect("persist over the unlocked plan");
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

    fn staged(dir: &tempfile::TempDir, contents: &[u8]) -> tempfile::NamedTempFile {
        let mut tmp = tempfile::NamedTempFile::new_in(dir.path()).expect("temp file");
        tmp.write_all(contents).expect("write temp");
        tmp
    }

    fn permission_denied() -> std::io::Error {
        std::io::Error::from(std::io::ErrorKind::PermissionDenied)
    }

    /// A replacement refused while another handle holds the destination - what
    /// Windows does to every reader's handle - is waited out under the held
    /// sidecar instead of failing the write. The refusal is arranged rather than
    /// provoked, so this pins the wait on every platform.
    // §AR-agent-orchestrator-workflow.3.3.1.1 §FS-rhei-transition-cmd.3
    #[test]
    fn issue_390_a_refused_replacement_is_waited_out_under_the_sidecar() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "before\n");
        let locked = LockedPlanFile::open(&path).expect("lock the plan");

        refuse_replacements(Some(3), permission_denied);
        let persisted = persist_locked(staged(&dir, b"after\n"), &path);
        let attempts = replace_attempts();
        locked.release();

        persisted.expect("a refusal that stops must be waited out, not reported");
        assert_eq!(attempts, 4, "three refused attempts, then the one that lands");
        assert_eq!(fs::read_to_string(&path).expect("read back"), "after\n");
        let mut leftovers: Vec<String> = fs::read_dir(dir.path())
            .expect("read dir")
            .map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|name| name != "plan.rhei.md")
            .collect();
        leftovers.sort();
        assert_eq!(leftovers, vec!["plan.rhei.md.lock"], "the staged file is what was renamed");
    }

    /// The wait is bounded: a refusal that never stops is retried, then reported
    /// unchanged, and the destination keeps its previous bytes. The bound is the
    /// code's to choose; this holds only that there is one.
    // §AR-agent-orchestrator-workflow.3.3.1.1 §FS-rhei-transition-cmd.3
    #[test]
    fn issue_390_a_refusal_that_outlasts_the_bound_is_reported_with_the_plan_intact() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "before\n");
        let locked = LockedPlanFile::open(&path).expect("lock the plan");

        refuse_replacements(None, permission_denied);
        let started = Instant::now();
        let persisted = persist_locked(staged(&dir, b"after\n"), &path);
        let waited = started.elapsed();
        let attempts = replace_attempts();
        locked.release();

        let refused = persisted.expect_err("a refusal that never stops must be reported");
        assert_eq!(refused.error.kind(), std::io::ErrorKind::PermissionDenied, "{refused:?}");
        assert!(attempts > 1, "the refusal was reported after {attempts} attempt(s), unretried");
        assert!(waited < Duration::from_secs(15), "the wait is bounded, but took {waited:?}");
        assert!(refused.file.path().exists(), "the error hands the staged file back");
        assert_eq!(fs::read_to_string(&path).expect("read back"), "before\n");
    }

    /// Only a refusal is waited out. A failure that says nothing about another
    /// handle is reported after the one attempt that met it.
    // §AR-agent-orchestrator-workflow.3.3.1.1
    #[test]
    fn issue_390_a_failure_that_is_not_a_refusal_is_reported_at_once() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "before\n");
        let locked = LockedPlanFile::open(&path).expect("lock the plan");

        refuse_replacements(None, || std::io::Error::from(std::io::ErrorKind::NotFound));
        let persisted = persist_locked(staged(&dir, b"after\n"), &path);
        let attempts = replace_attempts();
        locked.release();

        let failed = persisted.expect_err("the arranged failure must be reported");
        assert_eq!(failed.error.kind(), std::io::ErrorKind::NotFound, "{failed:?}");
        assert_eq!(attempts, 1, "a failure that is not a refusal is not retried");
        assert_eq!(fs::read_to_string(&path).expect("read back"), "before\n");
    }

    /// Windows also refuses a replacement as a sharing violation (os error 32)
    /// or a lock violation (os error 33), which std reads as no `ErrorKind` of
    /// its own. Windows-only because those numbers are Windows's: on Unix they
    /// are `EPIPE` and `EDOM`, which no rename refusal means, so there is no
    /// portable form of the raw code.
    // §AR-agent-orchestrator-workflow.3.3.1.1
    #[cfg(windows)]
    #[test]
    fn issue_390_a_windows_sharing_or_lock_violation_is_waited_out() {
        let violations: [fn() -> std::io::Error; 2] =
            [|| std::io::Error::from_raw_os_error(32), || std::io::Error::from_raw_os_error(33)];
        for (violation, code) in violations.into_iter().zip([32, 33]) {
            let dir = tempfile::tempdir().expect("tmpdir");
            let path = plan_file(&dir, "before\n");
            let locked = LockedPlanFile::open(&path).expect("lock the plan");

            refuse_replacements(Some(2), violation);
            let persisted = persist_locked(staged(&dir, b"after\n"), &path);
            let attempts = replace_attempts();
            locked.release();

            persisted.unwrap_or_else(|err| panic!("os error {code} must be waited out: {err:?}"));
            assert_eq!(attempts, 3, "os error {code}: two refused attempts, then the landing");
            assert_eq!(fs::read_to_string(&path).expect("read back"), "after\n");
        }
    }

    /// The platform rule itself: another thread holds a plain read handle on the
    /// destination while the replacement starts, and lets go shortly after.
    /// Linux and macOS replace under the handle; Windows refuses until it closes,
    /// so this is the test that proves the waiting out against a real handle.
    // §AR-agent-orchestrator-workflow.3.3.1.1 §FS-rhei-transition-cmd.3
    #[test]
    fn issue_390_a_reader_holding_the_plan_open_does_not_fail_its_replacement() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "before\n");
        let locked = LockedPlanFile::open(&path).expect("lock the plan");
        let reader_path = path.clone();
        let (held_tx, held_rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut handle = fs::File::open(&reader_path).expect("open the plan for reading");
            held_tx.send(()).expect("say the handle is held");
            std::thread::sleep(Duration::from_millis(250));
            let mut seen = String::new();
            handle.read_to_string(&mut seen).expect("read through the held handle");
            seen
        });
        held_rx.recv_timeout(Duration::from_secs(5)).expect("the reader holds the plan open");

        let persisted = persist_locked(staged(&dir, b"after\n"), &path);
        locked.release();
        let seen = reader.join().expect("reader thread");

        persisted.expect("a reader holding the plan open must not fail its replacement");
        assert_eq!(fs::read_to_string(&path).expect("read back"), "after\n");
        assert_eq!(seen, "before\n", "the reader reads the bytes it opened");
    }

    /// Step 15 replaces the task file a second time, to link the result, while
    /// the transition still holds its sidecar. A refusal there is waited out as
    /// at step 13: otherwise a reader fails a transition whose state write has
    /// already landed, and leaves the ticket terminal without its result link.
    // §FS-rhei-transition-cmd.3 §AR-agent-orchestrator-workflow.3.3.1.1
    #[test]
    fn issue_390_a_refused_result_link_replacement_is_waited_out() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(
            &dir,
            "# Rhei: Test\n\n## Tasks\n\n### Task 1: Alpha\n**State:** completed\n**Assignee:** agent-1\n",
        );
        let machine = rhei_validator::StateMachine::from_yaml_str(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: p\n  completed:\n    description: c\n    final: true\ntransitions:\n  - from: pending\n    to: completed\n",
        )
        .expect("machine");
        let locked = LockedPlanFile::open(&path).expect("lock the task file");

        refuse_replacements(Some(3), permission_denied);
        let recorded = record_transition_result(
            dir.path(),
            &path,
            "1",
            &machine,
            "1",
            "pending",
            "completed",
            Some("Done."),
        );
        let attempts = replace_attempts();
        locked.release();

        recorded.expect("a refused result-link replacement must be waited out, not reported");
        assert_eq!(
            attempts, 4,
            "the result link must be written by the replacement that waits out a refusal: \
             three refused attempts, then the one that lands"
        );
        let content = fs::read_to_string(&path).expect("read back");
        assert!(content.contains("> **Result:** [1](runtime/results/1.md)"), "{content}");
        assert!(!content.contains("**Assignee:**"), "{content}");
    }
}
