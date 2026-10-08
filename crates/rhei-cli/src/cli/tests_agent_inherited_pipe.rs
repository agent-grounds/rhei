// When an agent's invocation is over: at the direct agent's exit, however long
// a grandchild keeps the stdout and stderr it inherited. agent-grounds/rhei#393:
// the first form of this test bounded two interpreter start-ups and the pipe
// wait together at 1 s, so a slow Windows runner failed it with a message that
// blamed a pipe wait that never happened. agent-grounds/rhei#483: the second
// bounded them at 10 s, and a stalled runner crossed that too, so the test now
// observes the absence instead of timing it.
//
// Its own part so the fixture and the way it is observed stay together.

// §AR-source-file-size.3 §FS-rhei-agents.3.2.1 §REQ-cross-platform.8.2

    /// An agent that exits at once but leaves a grandchild holding the stdout
    /// and stderr it inherited — the spawn must not wait for that pipe's EOF
    /// (§FS-rhei-agents.3.2.1).
    fn write_inherited_pipe_fake_agent(dir: &Path) -> Vec<String> {
        python_fixture_command(
            dir,
            "inherited-pipe-agent",
            &format!("print('stdout:before-background', flush=True)\n{}", inherited_pipe_handoff()),
        )
    }

    /// The Python that ends a fixture by handing its stdout and stderr to a
    /// grandchild. The grandchild holds them while [`INHERITED_PIPE_HOLD_FILE`]
    /// in the fixture's directory exists, for at most [`INHERITED_PIPE_HOLD`],
    /// and makes [`INHERITED_PIPE_LET_GO`] there as its last act.
    ///
    /// The grandchild is handed both streams explicitly, so it inherits them by
    /// `subprocess`'s documented contract on Windows too, and nothing else: its
    /// stdin is null and its working directory is outside the fixture's
    /// directory, which it would otherwise keep from being removed on Windows.
    fn inherited_pipe_handoff() -> String {
        format!(
            r#"import os
import subprocess

HOLDER = '''import os, sys, time
deadline = time.monotonic() + {hold}
while os.path.exists(sys.argv[1]) and time.monotonic() < deadline:
    time.sleep(0.05)
os.mkdir(sys.argv[2])
'''

here = os.path.dirname(os.path.abspath(__file__))
hold = os.path.join(here, '{hold_file}')
open(hold, 'w').close()
subprocess.Popen(
    [sys.executable, '-c', HOLDER, hold, os.path.join(here, '{let_go}')],
    stdin=subprocess.DEVNULL,
    stdout=sys.stdout,
    stderr=sys.stderr,
    cwd=os.path.dirname(sys.executable),
)
"#,
            hold = INHERITED_PIPE_HOLD.as_secs(),
            hold_file = INHERITED_PIPE_HOLD_FILE,
            let_go = INHERITED_PIPE_LET_GO,
        )
    }

    /// How long the inherited-pipe grandchild holds the pipe when nothing
    /// releases it. Outside the margin (§REQ-cross-platform.8.2): a spawn that
    /// returns inside its timeout, however stalled the runner, returns before the
    /// hold runs out, so only one that waited for EOF finds the grandchild let go.
    const INHERITED_PIPE_HOLD: Duration = Duration::from_secs(2 * FIXTURE_MARGIN.as_secs());

    /// The file whose removal releases the inherited-pipe grandchild early.
    const INHERITED_PIPE_HOLD_FILE: &str = "inherited-pipe-hold";

    /// What the inherited-pipe grandchild makes as it lets go of the pipe,
    /// released or out of hold. A directory rather than a file, so that once it
    /// exists the grandchild has nothing open in the fixture's directory.
    const INHERITED_PIPE_LET_GO: &str = "inherited-pipe-let-go";

    /// Whether the inherited-pipe grandchild in `dir` has let go of the pipe.
    ///
    /// Read as the spawn returns, before [`release_inherited_pipe`]: a spawn that
    /// waits for the pipe's EOF cannot return before the grandchild lets go, so
    /// finding it let go is that wait, observed rather than timed
    /// (§REQ-cross-platform.8.2).
    fn inherited_pipe_let_go(dir: &Path) -> bool {
        dir.join(INHERITED_PIPE_LET_GO).exists()
    }

    /// Release the inherited-pipe grandchild in `dir` and wait until it has let
    /// go, so that `dir` is not removed while it is still making its mark there.
    /// A holding grandchild lets go within one of its polls; the wait is bounded
    /// by [`FIXTURE_MARGIN`] for one whose interpreter has not started yet.
    fn release_inherited_pipe(dir: &Path) {
        let _ = fs::remove_file(dir.join(INHERITED_PIPE_HOLD_FILE));
        let deadline = Instant::now() + FIXTURE_MARGIN;
        while !inherited_pipe_let_go(dir) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// §FS-rhei-agents.3.2.1: the spawn returns at the direct agent's exit, not
    /// at EOF on a pipe its grandchild still holds. The grandchild holds it until
    /// the test releases it after the spawn has returned, so a spawn that waited
    /// for EOF finds it let go and one that did not finds it holding, however
    /// slow the runner (§REQ-cross-platform.8.2).
    #[test]
    fn inherited_output_pipe_does_not_block_agent_completion() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let command = write_inherited_pipe_fake_agent(dir.path());
        let log_path = dir.path().join("agent.log");
        let recorder = Arc::new(RecordingSink::default());
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("codex"),
            profile: CustomAgentProfile { command, ..CustomAgentProfile::default() },
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(FIXTURE_MARGIN.as_secs()),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling::default();

        let start = Instant::now();
        let status = spawn_and_wait_agent(
            &resolved,
            &builtin_price_book(),
            "prompt",
            dir.path(),
            dir.path(),
            None,
            dir.path(),
            None,
            "task-pipe",
            "pending",
            1,
            &tooling,
            &log_path,
            dir.path(),
            None,
            0,
            recorder,
            None,
            &spawn_plan_for_test(&log_path),
            None,
        )
        .expect("agent should complete without waiting for inherited pipe EOF");
        let elapsed = start.elapsed();
        let let_go = inherited_pipe_let_go(dir.path());
        // Release the grandchild now; had the spawn panicked, dropping `dir` would.
        release_inherited_pipe(dir.path());

        assert!(
            status.status.success(),
            "agent failed after {elapsed:?} (timed out: {}): {:?}",
            status.timed_out,
            status.status
        );
        assert!(
            !let_go,
            "spawn returned after {elapsed:?}, once the grandchild had let go of the pipe it \
             holds for up to {INHERITED_PIPE_HOLD:?}, so the return was an EOF wait"
        );
        let log = fs::read_to_string(&log_path).expect("read log");
        assert!(log.contains("stdout:before-background"));
        assert!(log.contains("=== exit ==="));
    }
