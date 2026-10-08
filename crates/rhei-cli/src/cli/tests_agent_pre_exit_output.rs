// What a spawn reads of the output the agent wrote before it exited: all of it,
// however far behind its reader is when the exit arrives. agent-grounds/rhei#484:
// the drain grace detached a reader still working through that output, so
// provider-limit recognition counted a prefix of it, and a result event plus a
// plain limit line read as one refusal on a loaded Windows runner.
//
// Its own part beside the inherited-pipe test: the two are the two halves of
// one rule, and the second half's fixture borrows the first's hold.

// §AR-source-file-size.3 §FS-rhei-agent-output-drain §FS-rhei-agents.2.3

    /// A recognized Claude refusal line (§FS-rhei-agents.2.3).
    const LAGGING_LIMIT_SIGNAL: &str =
        "You've hit your session limit · resets 10:20pm (Europe/Zurich)";

    /// Far above any drain grace, the shipped 100 ms included, so a reader held
    /// this long cannot catch up inside the grace however fast the machine is.
    const READER_LAG: Duration = Duration::from_secs(3);

    /// A Claude stream-json `result` event whose decoded text is
    /// [`LAGGING_LIMIT_SIGNAL`], carrying usage so the capture has something to
    /// measure.
    fn lagging_limit_result_event() -> String {
        serde_json::json!({
            "type": "result",
            "subtype": "success",
            "is_error": true,
            "result": LAGGING_LIMIT_SIGNAL,
            "usage": {
                "input_tokens": 3,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
                "output_tokens": 5
            }
        })
        .to_string()
    }

    /// Holds the reader of `stream` inside the live event of that stream's
    /// first output line, until released or [`READER_LAG`] passes. That is the
    /// reader of a loaded runner: still busy with one line's log write, usage
    /// capture and events when the agent has already exited.
    struct LaggingReaderSink {
        stream: rhei_tui::AgentStream,
        held: std::sync::atomic::AtomicBool,
        released: Mutex<bool>,
        release: std::sync::Condvar,
    }

    impl LaggingReaderSink {
        fn new(stream: rhei_tui::AgentStream) -> Arc<Self> {
            Arc::new(Self {
                stream,
                held: std::sync::atomic::AtomicBool::new(false),
                released: Mutex::new(false),
                release: std::sync::Condvar::new(),
            })
        }

        fn release(&self) {
            *self.released.lock().expect("release lock") = true;
            self.release.notify_all();
        }
    }

    impl rhei_tui::EventSink for LaggingReaderSink {
        fn emit(&self, event: rhei_tui::RunEvent) {
            let rhei_tui::RunEvent::AgentOutput { stream, .. } = event else { return };
            if stream != self.stream || self.held.swap(true, std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            let released = self.released.lock().expect("release lock");
            let _ = self.release.wait_timeout_while(released, READER_LAG, |released| !*released);
        }
    }

    /// What a lagging-reader spawn left behind, read before the reader is let go.
    struct LaggingSpawn {
        outcome: AgentSpawnOutcome,
        /// The spawn record of §FS-rhei-agents.8.4: `ending` and `attempt_charged`.
        record: serde_json::Value,
        /// The log up to its `=== exit ===` footer.
        log_before_footer: String,
        /// The usage capture file, as text, and whether it measured usage.
        usage: String,
        usage_measured: bool,
    }

    /// Run a fake `claude-code` agent on provider `anthropic` that writes
    /// `stdout` and `stderr` whole, flushes both and exits 1, while `sink` holds
    /// one reader. Every byte is written before the exit.
    fn spawn_with_lagging_reader(
        stdout: &str,
        stderr: &str,
        sink: Arc<LaggingReaderSink>,
    ) -> LaggingSpawn {
        let dir = tempfile::tempdir().expect("tmpdir");
        fs::write(dir.path().join("stdout.bin"), stdout).expect("stdout payload");
        fs::write(dir.path().join("stderr.bin"), stderr).expect("stderr payload");
        let command = python_fixture_command(
            dir.path(),
            "limit-agent",
            r#"import os
here = os.path.dirname(os.path.abspath(__file__))
for name, stream in (('stdout.bin', sys.stdout.buffer), ('stderr.bin', sys.stderr.buffer)):
    with open(os.path.join(here, name), 'rb') as handle:
        stream.write(handle.read())
    stream.flush()
raise SystemExit(1)
"#,
        );
        let mut profile = built_in_agents().remove("claude-code").expect("claude-code");
        profile.command = command;
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
            profile,
            mode: None,
            target: None,
            model: None,
            model_provider: Some("anthropic".to_string()),
            model_name: None,
            timeout_secs: Some(30),
            autonomous_args: Vec::new(),
        };
        let log_path = dir.path().join("agent.log");
        let plan = spawn_plan_for_test(&log_path);
        let outcome = spawn_and_wait_agent(
            &resolved,
            &builtin_price_book(),
            "prompt",
            dir.path(),
            dir.path(),
            None,
            &dir.path().join("plan.rhei.md"),
            None,
            "1",
            "working",
            1,
            &ResolvedTooling::default(),
            &log_path,
            dir.path(),
            None,
            0,
            sink.clone(),
            None,
            &plan,
            None,
        )
        .expect("the fake Claude agent runs");
        let record = fs::read_to_string(&plan.record).expect("spawn record");
        let record = serde_json::from_str(&record).expect("spawn record is JSON");
        let log = fs::read_to_string(&log_path).expect("agent log");
        let log_before_footer = log.split("=== exit ===").next().unwrap_or_default().to_string();
        let capture = outcome.usage_capture_path.as_deref();
        let usage = capture.and_then(|path| fs::read_to_string(path).ok()).unwrap_or_default();
        let usage_measured =
            matches!(extract_usage_from_capture(capture), ExtractedUsageStatus::Measured(_));
        sink.release();
        LaggingSpawn { outcome, record, log_before_footer, usage, usage_measured }
    }

    /// The reported case, `result-and-stdout`: a result event and a plain
    /// signal line in one stdout write. Two signals are an ordinary result
    /// (§FS-rhei-agents.2.3), and both were written before the exit, so the
    /// drain grace may not drop the second (§FS-rhei-agent-output-drain.1).
    #[test]
    fn result_and_plain_signal_on_stdout_stay_ordinary_when_the_reader_lags() {
        let sink = LaggingReaderSink::new(rhei_tui::AgentStream::Stdout);
        let stdout = format!("{}\n{LAGGING_LIMIT_SIGNAL}\n", lagging_limit_result_event());
        let run = spawn_with_lagging_reader(&stdout, "", sink);

        assert!(!run.outcome.status.success(), "the fixture exits 1: {:?}", run.outcome.status);
        assert_eq!(
            run.record["ending"], "exited",
            "two signals written before the exit were classified as one: {:#}",
            run.record
        );
        assert_eq!(run.record["attempt_charged"], true, "{:#}", run.record);
        assert!(run.outcome.provider_limit.is_none(), "{:?}", run.outcome.provider_limit);
        assert_eq!(
            run.log_before_footer.matches(LAGGING_LIMIT_SIGNAL).count(),
            2,
            "both signal lines precede the exit footer:\n{}",
            run.log_before_footer
        );
    }

    /// The `result-and-stderr` shape: the plain signal is on stderr, behind an
    /// earlier stderr line its reader is still busy with at the exit.
    #[test]
    fn result_on_stdout_and_plain_signal_on_stderr_stay_ordinary_when_the_reader_lags() {
        let sink = LaggingReaderSink::new(rhei_tui::AgentStream::Stderr);
        let stdout = format!("{}\n", lagging_limit_result_event());
        let stderr = format!("warning: retrying\n{LAGGING_LIMIT_SIGNAL}\n");
        let run = spawn_with_lagging_reader(&stdout, &stderr, sink);

        assert_eq!(
            run.record["ending"], "exited",
            "two signals written before the exit were classified as one: {:#}",
            run.record
        );
        assert_eq!(run.record["attempt_charged"], true, "{:#}", run.record);
        assert!(run.outcome.provider_limit.is_none(), "{:?}", run.outcome.provider_limit);
        assert_eq!(
            run.log_before_footer.matches(LAGGING_LIMIT_SIGNAL).count(),
            2,
            "both signal lines precede the exit footer:\n{}",
            run.log_before_footer
        );
    }

    /// The other direction: one real refusal behind an earlier stdout line, the
    /// stream's `system` init event. It must park, uncharged, and its usage must
    /// be in the capture the completion path reads (§FS-rhei-agent-output-drain.1).
    #[test]
    fn single_refusal_behind_an_earlier_line_parks_when_the_reader_lags() {
        let sink = LaggingReaderSink::new(rhei_tui::AgentStream::Stdout);
        let init = serde_json::json!({
            "type": "system",
            "subtype": "init",
            "session_id": "session-before-the-refusal"
        });
        let stdout = format!("{init}\n{}\n", lagging_limit_result_event());
        let run = spawn_with_lagging_reader(&stdout, "", sink);

        assert_eq!(
            run.record["ending"], "provider_limited",
            "one signal written before the exit was not recognized: {:#}",
            run.record
        );
        assert_eq!(run.record["attempt_charged"], false, "{:#}", run.record);
        assert!(run.outcome.provider_limit.is_some(), "the refusal parks the task");
        assert!(
            run.usage_measured,
            "the refusal's usage is missing from the capture: {:?}",
            run.usage
        );
        assert!(
            run.log_before_footer.contains(LAGGING_LIMIT_SIGNAL),
            "the refusal precedes the exit footer:\n{}",
            run.log_before_footer
        );
    }

    /// An agent that writes a terminated line and then an unterminated one,
    /// hands both streams to a grandchild that holds them for
    /// [`INHERITED_PIPE_HOLD`], and exits 0. Returns how long the spawn took
    /// and its log as the spawn left it, and releases the grandchild.
    fn spawn_with_unterminated_last_line() -> (AgentSpawnOutcome, Duration, String) {
        let dir = tempfile::tempdir().expect("tmpdir");
        let command = python_fixture_command(
            dir.path(),
            "unterminated-agent",
            &format!(
                r#"import os
import subprocess

HOLDER = '''import os, sys, time
deadline = time.monotonic() + {hold}
while os.path.exists(sys.argv[1]) and time.monotonic() < deadline:
    time.sleep(0.05)
'''

sys.stdout.write('stdout:terminated\nstdout:unterminated-tail')
sys.stdout.flush()
hold = os.path.join(os.path.dirname(os.path.abspath(__file__)), '{hold_file}')
open(hold, 'w').close()
subprocess.Popen(
    [sys.executable, '-c', HOLDER, hold],
    stdin=subprocess.DEVNULL,
    stdout=sys.stdout,
    stderr=sys.stderr,
    cwd=os.path.dirname(sys.executable),
)
"#,
                hold = INHERITED_PIPE_HOLD.as_secs(),
                hold_file = INHERITED_PIPE_HOLD_FILE,
            ),
        );
        let log_path = dir.path().join("agent.log");
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("codex"),
            profile: CustomAgentProfile { command, ..CustomAgentProfile::default() },
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(10),
            autonomous_args: Vec::new(),
        };
        let start = Instant::now();
        let outcome = spawn_and_wait_agent(
            &resolved,
            &builtin_price_book(),
            "prompt",
            dir.path(),
            dir.path(),
            None,
            dir.path(),
            None,
            "task-tail",
            "pending",
            1,
            &ResolvedTooling::default(),
            &log_path,
            dir.path(),
            None,
            0,
            Arc::new(RecordingSink::default()),
            None,
            &spawn_plan_for_test(&log_path),
            None,
        );
        let elapsed = start.elapsed();
        let log = fs::read_to_string(&log_path).unwrap_or_default();
        // Release the grandchild now; had the spawn failed, dropping `dir` would.
        let _ = fs::remove_file(dir.path().join(INHERITED_PIPE_HOLD_FILE));
        let outcome = outcome.expect("the agent completes without its grandchild's EOF");
        (outcome, elapsed, log)
    }

    /// Guard: waiting for the whole of the pre-exit output must not become
    /// waiting for a newline. The last line has none and a grandchild still
    /// holds the pipe, so a reader that waits for the line's end waits for the
    /// grandchild; completion must still arrive at the direct agent's exit
    /// (§FS-rhei-agent-output-drain.2). The bound is the inherited-pipe test's.
    #[test]
    fn unterminated_last_line_does_not_hold_completion_for_a_descendant() {
        let (outcome, elapsed, _) = spawn_with_unterminated_last_line();
        assert!(outcome.status.success(), "agent failed after {elapsed:?}: {:?}", outcome.status);
        assert!(
            elapsed < Duration::from_secs(10),
            "spawn returned after {elapsed:?}; the grandchild holds the pipe \
             {INHERITED_PIPE_HOLD:?}, so a return near that is a wait for the line's end"
        );
    }

    /// An unterminated last line written before the exit is captured whole,
    /// as a line, ahead of the exit footer - even while a grandchild holds the
    /// pipe and could still extend it (§FS-rhei-agent-output-drain.1).
    #[test]
    fn unterminated_last_line_is_logged_ahead_of_the_exit_footer() {
        let (_, _, log) = spawn_with_unterminated_last_line();
        let before_footer = log.split("=== exit ===").next().unwrap_or_default();
        assert!(before_footer.contains("stdout:terminated"), "{log}");
        assert!(
            before_footer.contains("stdout:unterminated-tail"),
            "the unterminated last line written before the exit is missing ahead of the \
             exit footer:\n{log}"
        );
    }
