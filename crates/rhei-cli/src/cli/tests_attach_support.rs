// The pieces `--headless` and `rhei attach` are built out of: what the child is
// re-executed with, and how a live agent log is tailed.
// §FS-rhei-run-headless.1 §FS-rhei-run-headless.5

mod attach_support_tests {
    use super::super::*;

    fn args(given: &[&str]) -> Vec<String> {
        child_arguments_from(given.iter().map(std::ffi::OsString::from))
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_child_is_re_executed_without_the_launcher_only_flags() {
        assert_eq!(
            args(&["run", "--headless", "--json", "--parallel", "2", "plan.rhei.md"]),
            vec!["run", "--parallel", "2", "plan.rhei.md"]
        );
    }

    /// Everything after `--` is an operand. A plan named `--json` is a plan,
    /// and dropping it there silently ran a different plan than the one that
    /// was typed. §FS-rhei-run-headless.1
    #[test]
    fn filtering_stops_at_the_argument_separator() {
        assert_eq!(
            args(&["run", "--headless", "--", "--json"]),
            vec!["run", "--", "--json"],
            "the operand after `--` survives, and the separator with it"
        );
    }

    fn tail(workspace: &Path, path: &Path) -> Vec<String> {
        let mut tailer = AgentLogTailer::default();
        tailer.follow(workspace, "plan.1", 0, path);
        tailer
            .poll()
            .into_iter()
            .filter_map(|event| match event {
                rhei_tui::RunEvent::AgentOutput { line, .. } => Some(line),
                _ => None,
            })
            .collect()
    }

    /// A backfill offset lands wherever `len - BACKFILL_BYTES` falls, which is
    /// mid-line. Emitting that remainder as a whole line presents a fragment as
    /// something the agent wrote. §FS-rhei-run-tui.1.2
    #[test]
    fn a_backfilled_tail_discards_the_line_it_landed_inside() {
        let dir = tempfile::tempdir().expect("workspace");
        let path = dir.path().join("agent.log");
        // Comfortably past the backfill window, so the first read starts inside
        // a line rather than at byte zero.
        let filler = "x".repeat(120);
        let mut body = String::new();
        for index in 0..400 {
            body.push_str(&format!("line {index} {filler}\n"));
        }
        body.push_str("the whole last line\n");
        fs::write(&path, &body).expect("log");

        let lines = tail(dir.path(), &path);
        assert!(!lines.is_empty(), "the recent tail is still delivered");
        assert_eq!(lines.last().map(String::as_str), Some("the whole last line"));
        for line in &lines {
            assert!(
                line.starts_with("line ") || line == "the whole last line",
                "a mid-line fragment was presented as a whole line: {line}"
            );
        }
    }

    #[test]
    fn a_short_log_is_tailed_whole_from_the_start() {
        let dir = tempfile::tempdir().expect("workspace");
        let path = dir.path().join("agent.log");
        fs::write(&path, "first\nsecond\n").expect("log");
        assert_eq!(tail(dir.path(), &path), vec!["first".to_string(), "second".to_string()]);
    }
    /// The event log records a workspace-relative `log_path`, so the follower
    /// that reads it back has to resolve it against the run's workspace rather
    /// than against its own working directory.
    // §FS-rhei-run-json.2.1 §FS-rhei-run-headless.5
    #[test]
    fn a_workspace_relative_log_path_is_tailed_from_the_workspace() {
        let dir = tempfile::tempdir().expect("workspace");
        let logs = dir.path().join("runtime/logs");
        fs::create_dir_all(&logs).expect("log directory");
        fs::write(logs.join("task-plan.1-pending.log"), "agent said this\n").expect("log");

        let relative = Path::new("runtime/logs/task-plan.1-pending.log");
        assert_eq!(tail(dir.path(), relative), vec!["agent said this".to_string()]);
    }

    /// A follower's stopping rule, over the three verdicts. The undecided one is
    /// the whole point: reading it as an end truncated a live run's stream, and
    /// reading it as "keep going" forever would hang on an outage that never
    /// clears.
    // §FS-rhei-run-headless.3 §FS-rhei-run-json.2.1

    // Unix: `chmod 000` through `PermissionsExt` stages the undecided verdict,
    // and a Windows ACL denies the owner nothing the same way.
    // §REQ-cross-platform.3
    #[cfg(unix)]
    #[test]
    fn a_follower_keeps_reading_while_liveness_is_undecided() {
        use std::os::unix::fs::PermissionsExt;
        use super::run_descriptor_tests::{descriptor, workspace};

        let workspace = workspace();
        let run = descriptor("follow", &workspace.path, "2026-08-22T10:00:00Z");
        let path = run_descriptor_path(&workspace.path);
        fs::create_dir_all(path.parent().expect("runtime")).expect("runtime directory");
        fs::write(&path, serde_json::to_string(&run).expect("render")).expect("descriptor");
        drop(try_acquire_run_lock(&workspace.path).expect("lock"));
        let lock = workspace.path.join(".rhei").join("run.lock");
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o000)).expect("chmod 000");

        let mut ending = EndingWatch::default();
        assert!(matches!(ending.verdict(&run), Ending::Following), "undecided is not an end");
        // Reaching into the clock rather than sleeping out the real grace.
        ending.undecided.since = Some(Instant::now() - UNDECIDED_GRACE - Duration::from_millis(1));
        let verdict = ending.verdict(&run);
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o644)).expect("chmod 644");
        assert!(
            matches!(verdict, Ending::Undecided(_)),
            "past its grace it reports rather than following forever, got {}",
            match verdict {
                Ending::Following => "Following",
                Ending::Ended => "Ended",
                Ending::Undecided(_) => "Undecided",
            }
        );

        // A decided end still gets one more drain before the follower stops, so
        // the closing records written after `run_finished` are not lost.
        let mut ending = EndingWatch::default();
        let mut finished = run.clone();
        finished.status = RunStatus::Finished;
        fs::write(&path, serde_json::to_string(&finished).expect("render")).expect("descriptor");
        assert!(matches!(ending.verdict(&finished), Ending::Following), "one more drain");
        assert!(matches!(ending.verdict(&finished), Ending::Ended));
    }

    /// The exit status lands *after* the run stops being live: the run lock
    /// goes when the run command returns, the stamp comes from the process
    /// exit path behind it. A wait that reads the descriptor in that gap finds
    /// nothing on it, and reported a run that exited 0 as one that "did not
    /// end on its own" — the CI failure §FS-rhei-run-headless.5.3 exists to prevent.
    // §FS-rhei-run-headless.5.3
    #[test]
    fn the_wait_gives_a_late_exit_stamp_time_to_land() {
        use super::run_descriptor_tests::{descriptor, workspace};

        let workspace = workspace();
        let run = descriptor("settle", &workspace.path, "2026-08-22T10:00:00Z");
        let path = run_descriptor_path(&workspace.path);
        fs::create_dir_all(path.parent().expect("runtime")).expect("runtime directory");
        fs::write(&path, serde_json::to_string(&run).expect("render")).expect("descriptor");

        let stamped = {
            let mut ended = run.clone();
            ended.status = RunStatus::Finished;
            ended.exit_code = Some(0);
            serde_json::to_string(&ended).expect("render")
        };
        // The run's exit path, several polls behind the wait — the gap a loaded
        // machine widens until the wait loses the race to it.
        let late = std::thread::spawn(move || {
            std::thread::sleep(ATTACH_POLL * 3);
            fs::write(&path, stamped).expect("stamp");
        });

        let settled = settled_end(&run);
        late.join().expect("the stamping thread");
        assert_eq!(
            settled.exit_code,
            Some(0),
            "the wait outlasted the gap between the run going away and its stamp"
        );
    }
}
