// Production spawn/finalization must cancel a blocked intervention when the
// direct child exits, while a descendant still holds its inherited stdin.
// §FS-rhei-agents.3.2.1 §FS-rhei-evidence.5.4

mod intervention_shutdown {
    use super::super::*;
    use super::{python_fixture_command, RecordingSink, FIXTURE_MARGIN};
    use crate::rhei_tui::InterveneSink;
    use std::sync::mpsc;
    use std::time::Instant;

    fn until(label: &str, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + FIXTURE_MARGIN;
        while !condition() {
            assert!(Instant::now() < deadline, "waiting for {label}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    struct ReleaseHolder(Arc<tempfile::TempDir>);

    impl Drop for ReleaseHolder {
        fn drop(&mut self) {
            // Release both processes even on an assertion/setup failure.
            let _ = fs::write(self.0.path().join("exit-parent"), "");
            let _ = fs::write(self.0.path().join("release-holder"), "");
            if self.0.path().join("holder-ready").exists() {
                let deadline = Instant::now() + FIXTURE_MARGIN;
                while !self.0.path().join("holder-done").exists() && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }

    #[test]
    fn exited_agent_does_not_wait_for_descendant_stdin() {
        let dir = Arc::new(tempfile::tempdir().expect("stdin holder fixture"));
        let cleanup = ReleaseHolder(dir.clone());
        let command = python_fixture_command(
            dir.path(),
            "stdin-holder-parent",
            &format!(r#"import pathlib, subprocess, time
root = pathlib.Path(__file__).parent
holder = '''import pathlib, sys, time
root = pathlib.Path(sys.argv[1])
(root / 'holder-ready').touch()
deadline = time.monotonic() + {hold}
while not (root / 'release-holder').exists() and time.monotonic() < deadline:
    time.sleep(0.01)
sys.stdin.close()
(root / 'holder-done').mkdir()
'''
subprocess.Popen([sys.executable, '-c', holder, str(root)], stdin=sys.stdin,
                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                 cwd=pathlib.Path(sys.executable).parent)
deadline = time.monotonic() + {hold}
while not (root / 'exit-parent').exists() and time.monotonic() < deadline:
    time.sleep(0.01)
"#, hold = 3 * FIXTURE_MARGIN.as_secs()),
        );
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("codex"),
            profile: CustomAgentProfile {
                command,
                intervene_stdin: true,
                ..CustomAgentProfile::default()
            },
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(3 * FIXTURE_MARGIN.as_secs()),
            autonomous_args: Vec::new(),
        };
        let registry = Arc::new(RunInterveneSink::new(dir.path().to_path_buf()));
        let running = registry.clone();
        let root = dir.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let root = root.path();
            let log = root.join("agent.log");
            let result = spawn_and_wait_agent(
                &resolved, &builtin_price_book(), "prompt", root, root, None,
                root, None, "probe", "working", 1, &ResolvedTooling::default(),
                &log, root, None, 0, Arc::new(RecordingSink::default()),
                Some(&running), &spawn_plan_for_test(&log), None,
            );
            let _ = done_tx.send(result.map(|outcome| outcome.status.success()).map_err(|e| e.to_string()));
        });
        until("holder and registration", || {
            dir.path().join("holder-ready").exists() && registry.reachable("probe", Some(0))
        });
        let sending = registry.clone();
        let (send_tx, send_rx) = mpsc::channel();
        let delivery = std::thread::spawn(move || {
            let _ = send_tx.send(sending.deliver(Some("probe"), Some(0), &"x".repeat(1024 * 1024)));
        });
        // Observe admission before exit, rather than sleeping to let it happen.
        until("unacknowledged intervention", || {
            let targets = registry.targets.lock().unwrap();
            let target = targets.get(&InterveneKey { task_id: "probe".into(), slot: 0 }).unwrap();
            target.writer.lock().is_ok_and(|writer| writer.pending.is_some())
        });
        assert!(matches!(send_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        fs::write(dir.path().join("exit-parent"), "").unwrap();
        let early = done_rx.recv_timeout(FIXTURE_MARGIN).ok();
        let sent_before_release = if early.is_some() {
            send_rx.recv_timeout(FIXTURE_MARGIN).ok()
        } else {
            send_rx.try_recv().ok()
        };
        let completed_before_release = early.is_some();
        let cancelled_before_release = sent_before_release.is_some();
        let holder_still_open = !dir.path().join("holder-done").exists();
        // Always release before asserting, including against a broken repair.
        drop(cleanup);
        let result = match early {
            Some(result) => result,
            None => done_rx.recv_timeout(FIXTURE_MARGIN).expect("completion after cleanup"),
        };
        let sent = match sent_before_release {
            Some(result) => result,
            None => send_rx.recv_timeout(FIXTURE_MARGIN).expect("delivery after cleanup"),
        };
        worker.join().expect("spawn worker");
        delivery.join().expect("delivery worker");
        assert_eq!(result, Ok(true));
        assert!(completed_before_release, "exited invocation waited for descendant stdin");
        assert!(cancelled_before_release, "unacknowledged delivery was not cancelled at exit");
        assert!(holder_still_open, "completion waited until descendant stdin closed");
        assert!(sent.is_err(), "cancelled write must not report a successful delivery");
        assert!(!registry.reachable("probe", Some(0)));
        assert!(registry.unregister("probe", 0).is_empty());
        let journal = fs::read_to_string(dir.path().join("interventions.log")).unwrap();
        assert!(journal.contains("outcome=stdin-closed"));
        assert!(!journal.contains("outcome=delivered"));
        let log = fs::read_to_string(dir.path().join("agent.log")).unwrap();
        assert!(log.contains("=== exit ==="));
        assert!(!log.contains("[intervene "));
    }
}
