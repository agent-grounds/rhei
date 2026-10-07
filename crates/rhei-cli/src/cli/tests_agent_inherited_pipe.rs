// When an agent's invocation is over: at the direct agent's exit, however long
// a grandchild keeps the stdout and stderr it inherited. agent-grounds/rhei#393:
// the first form of this test bounded two interpreter start-ups and the pipe
// wait together at 1 s, so a slow Windows runner failed it with a message that
// blamed a pipe wait that never happened.
//
// Its own part so the fixture and the bound it is tuned against stay together.

// §AR-source-file-size.3 §FS-rhei-agents.3.2.1

    /// An agent that exits at once but leaves a grandchild holding the stdout
    /// and stderr it inherited — the spawn must not wait for that pipe's EOF
    /// (§FS-rhei-agents.3.2.1).
    ///
    /// The grandchild is handed both streams explicitly, so it inherits them by
    /// `subprocess`'s documented contract on Windows too, and nothing else: its
    /// stdin is null and its working directory is outside `dir`, which it would
    /// otherwise keep from being removed on Windows. It holds the pipe for
    /// [`INHERITED_PIPE_HOLD`], far longer than any interpreter start-up, unless
    /// [`INHERITED_PIPE_HOLD_FILE`] in `dir` is removed first.
    fn write_inherited_pipe_fake_agent(dir: &Path) -> Vec<String> {
        python_fixture_command(
            dir,
            "inherited-pipe-agent",
            &format!(
                r#"import os
import subprocess
import sys

HOLDER = '''import os, sys, time
deadline = time.monotonic() + {hold}
while os.path.exists(sys.argv[1]) and time.monotonic() < deadline:
    time.sleep(0.05)
'''

print('stdout:before-background', flush=True)
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
        )
    }

    /// How long the inherited-pipe grandchild holds the pipe when nothing
    /// releases it: a spawn that waits for EOF returns no sooner than this.
    const INHERITED_PIPE_HOLD: Duration = Duration::from_secs(30);

    /// The file whose removal releases the inherited-pipe grandchild early.
    const INHERITED_PIPE_HOLD_FILE: &str = "inherited-pipe-hold";

    /// §FS-rhei-agents.3.2.1: the spawn returns at the direct agent's exit, not
    /// at EOF on a pipe its grandchild still holds. The bound sits far above two
    /// interpreter start-ups and far below the grandchild's hold, so a slow
    /// runner cannot cross it and a spawn that waits for EOF cannot stay under it.
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
            timeout_secs: Some(10),
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
        // Release the grandchild now; had the spawn panicked, dropping `dir` would.
        let _ = fs::remove_file(dir.path().join(INHERITED_PIPE_HOLD_FILE));

        assert!(
            status.status.success(),
            "agent failed after {elapsed:?} (timed out: {}): {:?}",
            status.timed_out,
            status.status
        );
        assert!(
            elapsed < Duration::from_secs(10),
            "spawn returned after {elapsed:?}; the grandchild holds the pipe \
             {INHERITED_PIPE_HOLD:?}, so a return near that is an EOF wait"
        );
        let log = fs::read_to_string(&log_path).expect("read log");
        assert!(log.contains("stdout:before-background"));
        assert!(log.contains("=== exit ==="));
    }
