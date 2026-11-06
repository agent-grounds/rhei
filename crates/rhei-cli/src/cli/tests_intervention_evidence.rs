// Real-child stdin delivery, exit capture and evidence serialization under
// controlled transcript contention (#561); the CLI scenario remains in evidence_lifecycle_tests.rs.
// §FS-rhei-evidence.5.4

mod intervention_evidence {
    use super::super::*;
    use super::{python_fixture_command, FIXTURE_MARGIN};
    use crate::rhei_tui::InterveneSink;
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc;
    use std::time::Instant;

    const MESSAGE: &str = "  reçu – exact operator bytes  ";

    struct Fixture {
        sink: Arc<RunInterveneSink>,
        log: Arc<Mutex<fs::File>>,
        child: Child,
        // Drop handles before removing their directory, including on Windows.
        dir: tempfile::TempDir,
    }

    impl Fixture {
        fn new(lines: usize) -> Self {
            let dir = tempfile::tempdir().expect("intervention fixture directory");
            let command = python_fixture_command(
                dir.path(),
                "intervention-reader",
                r#"import pathlib
lines = int(sys.argv[1])
received = b''.join(sys.stdin.buffer.readline() for _ in range(lines))
pathlib.Path(__file__).with_name('receipt').write_bytes(received)
"#,
            );
            let mut child = Command::new(&command[0])
                .args(&command[1..])
                .arg(lines.to_string())
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .expect("spawn real stdin reader");
            let log = Arc::new(Mutex::new(fs::File::create(dir.path().join("agent.log")).unwrap()));
            let sink = Arc::new(RunInterveneSink::new(dir.path().to_path_buf()));
            sink.register(
                "plan.1",
                0,
                "working",
                log.clone(),
                child.stdin.take().expect("piped stdin"),
                AgentStdinFormat::PlainLine,
            );
            Self { sink, log, child, dir }
        }

        fn received_and_exited(&mut self, expected: &[u8]) {
            let deadline = Instant::now() + FIXTURE_MARGIN;
            loop {
                if let Some(status) = self.child.try_wait().expect("wait for reader") {
                    assert!(status.success(), "reader exits successfully: {status}");
                    break;
                }
                assert!(Instant::now() < deadline, "reader did not exit within {FIXTURE_MARGIN:?}");
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(fs::read(self.dir.path().join("receipt")).unwrap(), expected);
        }

        fn evidence(&self) -> (EvidenceInvocation, PathBuf) {
            let run = Arc::new(EvidenceRun::new(
                self.dir.path(),
                "issue-561",
                "2026-11-06T00:00:00Z",
                &rhei_tui::RunProvenance::default(),
                RheiSettings::default(),
            ));
            let invocation = EvidenceInvocation::publish(
                run,
                "agent",
                "invocation_id",
                "issue-561-agent".to_string(),
                serde_json::json!({ "kind": "agent", "task_id": "plan.1" }),
            )
            .expect("publish start record");
            let end = invocation.start_path.with_file_name(
                invocation.start_path.file_name().unwrap().to_str().unwrap()
                    .replace(".start.json", ".end.json"),
            );
            (invocation, end)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn assert_retained(root: &Path, value: &serde_json::Value, message: &str) {
        assert_eq!(value["sha256"], sha256_hex(message.as_bytes()));
        assert_eq!(value["bytes"], message.len());
        assert_eq!(value["outcome"], "delivered");
        chrono::DateTime::parse_from_rfc3339(value["at"].as_str().expect("delivery timestamp"))
            .expect("valid delivery timestamp");
        assert_eq!(value["stored"], "original");
        let object = evidence_dir(root).join("objects/sha256")
            .join(value["sha256"].as_str().unwrap());
        assert_eq!(fs::read(object).expect("retained operator bytes"), message.as_bytes());
    }

    /// An acknowledged write is retained even if the reader exits while its
    /// transcript mirror is blocked. §FS-rhei-evidence.5.4
    #[test]
    #[ignore = "red until #561: immediate exit loses an acknowledged intervention; implement removes this"]
    fn a_received_intervention_survives_exit_during_transcript_contention() {
        let mut fixture = Fixture::new(1);
        let (evidence, end_path) = fixture.evidence();
        // The same mutex with_agent_log takes; stdout/stderr readers can hold
        // it while writing the transcript. No production path is replaced.
        let log = fixture.log.clone();
        let held_log = log.lock().unwrap();
        let sending = fixture.sink.clone();
        let (sent_tx, sent_rx) = mpsc::channel();
        let sender = std::thread::spawn(move || {
            let _ = sent_tx.send(sending.deliver(Some("plan.1"), Some(0), MESSAGE));
        });
        // Receipt + exit, rather than delivery's return, establishes that the
        // stdin write happened before exit capture while the mirror is blocked.
        fixture.received_and_exited(format!("{MESSAGE}\n").as_bytes());
        println!("child received exact operator bytes and exited 0 while transcript mutex held");

        let exiting = fixture.sink.clone();
        let log_path = fixture.dir.path().join("agent.log");
        let (started_tx, started_rx) = mpsc::channel();
        let (end_tx, end_rx) = mpsc::channel();
        let finisher = std::thread::spawn(move || {
            let _ = started_tx.send(());
            let captured = exiting.unregister("plan.1", 0);
            // Exercise the actual serializer using unregister's return value,
            // as the agent-spawn exit path does, without waiting for deliver.
            evidence.finish(Some(0), "exited", Some(&log_path), Some(captured));
            let _ = end_tx.send(());
        });
        started_rx.recv_timeout(FIXTURE_MARGIN).expect("exit capture started");
        // This bounded observation only chooses when to release contention.
        // A correct unregister may wait. Neither an early return nor a timeout
        // decides success; only the final evidence contents do.
        let early = end_rx.recv_timeout(Duration::from_secs(1)).ok();
        drop(held_log);
        assert_eq!(sent_rx.recv_timeout(FIXTURE_MARGIN).expect("delivery completed"), Ok(()));
        if early.is_none() {
            end_rx.recv_timeout(FIXTURE_MARGIN).expect("exit capture completed");
        }
        sender.join().expect("delivery thread");
        finisher.join().expect("exit thread");

        let transcript = fs::read(fixture.dir.path().join("agent.log")).unwrap();
        assert!(String::from_utf8_lossy(&transcript).contains(MESSAGE));
        let journal = fs::read_to_string(fixture.dir.path().join("interventions.log")).unwrap();
        assert!(journal.contains("outcome=delivered"));
        println!("delivery returned Ok(()); transcript mirrored; journal outcome=delivered");
        let end: serde_json::Value = serde_json::from_slice(&fs::read(end_path).unwrap()).unwrap();
        assert_eq!(end["phase"], "end");
        assert_eq!(end["ending"], "exited");
        assert_eq!(end["exit_code"], 0);
        let retained = end["interventions"].as_array().expect("end interventions");
        assert_eq!(retained.len(), 1, "received intervention missing from end record: {end:#}");
        assert_retained(fixture.dir.path(), &retained[0], MESSAGE);
        assert_eq!(end["log"]["sha256"], sha256_hex(&transcript), "final hash covers mirror");
        assert_eq!(end["log"]["bytes"], transcript.len());
    }

    /// Completed real deliveries keep their original bytes and delivery order.
    /// §FS-rhei-evidence.5.4
    #[test]
    fn sequential_interventions_are_retained_in_delivery_order() {
        let mut fixture = Fixture::new(2);
        let (evidence, end_path) = fixture.evidence();
        for message in ["first", MESSAGE] {
            fixture.sink.deliver(Some("plan.1"), Some(0), message).expect("delivered");
        }
        fixture.received_and_exited(format!("first\n{MESSAGE}\n").as_bytes());
        let captured = fixture.sink.unregister("plan.1", 0);
        evidence.finish(Some(0), "exited", Some(&fixture.dir.path().join("agent.log")), Some(captured));
        let end: serde_json::Value = serde_json::from_slice(&fs::read(end_path).unwrap()).unwrap();
        let retained = end["interventions"].as_array().expect("end interventions");
        assert_eq!(retained.len(), 2);
        for (value, message) in retained.iter().zip(["first", MESSAGE]) {
            assert_retained(fixture.dir.path(), value, message);
        }
    }

    /// A real broken pipe and a delivery after unregister retain no successful
    /// interventions. §FS-rhei-evidence.5.4
    #[test]
    fn closed_stdin_and_unreachable_targets_retain_no_interventions() {
        let mut fixture = Fixture::new(0);
        let (evidence, end_path) = fixture.evidence();
        fixture.received_and_exited(b"");
        let failed = fixture.sink.deliver(Some("plan.1"), Some(0), MESSAGE).unwrap_err();
        assert!(failed.contains("stdin is closed"), "{failed}");
        let captured = fixture.sink.unregister("plan.1", 0);
        assert!(captured.is_empty());
        let failed = fixture.sink.deliver(Some("plan.1"), Some(0), MESSAGE).unwrap_err();
        assert!(failed.contains("not interactively reachable"), "{failed}");
        assert!(fixture.sink.unregister("plan.1", 0).is_empty());
        evidence.finish(Some(0), "exited", None, Some(captured));
        let end: serde_json::Value = serde_json::from_slice(&fs::read(end_path).unwrap()).unwrap();
        assert_eq!(end["interventions"], serde_json::json!([]));
        let journal = fs::read_to_string(fixture.dir.path().join("interventions.log")).unwrap();
        assert!(journal.contains("outcome=stdin-closed"));
        assert!(journal.contains("outcome=unreachable"));
        assert_eq!(fs::read(fixture.dir.path().join("agent.log")).unwrap(), b"");
    }
}
