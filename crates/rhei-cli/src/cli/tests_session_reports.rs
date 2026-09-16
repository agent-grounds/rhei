// The session-report and metrics contract: what the renderer says a log said,
// and which measurement a session's work is bound to.
//
// Its own part because both surfaces are pure readers of recorded facts, and
// a wrong answer here shows up three layers away as a report that lies about
// a session or credits its delta to the wrong window.

// §AR-source-file-size.3 §FS-rhei-session-reports.2 §FS-rhei-metrics.2
// §FS-rhei-metrics.3

mod session_reports {
    use super::super::*;

    /// A minimal but complete Pi session log: header, prompt, one tool call
    /// with its result, a written file, a final message, and the exit footer.
    fn pi_log() -> String {
        let mut body = String::from("=== rhei agent log v1 ===\n");
        body.push_str("agent: pi\ntask: plan.1\nstate: cover\n");
        body.push_str("started: 2026-09-16T10:00:00Z\n===\n\n");
        let events = [
            serde_json::json!({"type": "session", "version": 3, "id": "s-1"}),
            serde_json::json!({"type": "message_end", "message": {
                "role": "user",
                "content": [{"type": "text", "text": "# Task plan.1\ncover the API"}],
            }}),
            serde_json::json!({"type": "message_end", "message": {
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "find the suite"},
                    {"type": "toolCall", "id": "c1", "name": "read",
                     "arguments": {"path": "suite/build.gradle"}},
                ],
            }}),
            serde_json::json!({"type": "tool_execution_end", "toolCallId": "c1",
                "toolName": "read",
                "result": {"content": [{"type": "text", "text": "plugins { }"}]}}),
            serde_json::json!({"type": "message_end", "message": {
                "role": "assistant",
                "content": [{"type": "toolCall", "id": "c2", "name": "write",
                    "arguments": {"path": "suite/Test.java", "content": "class Test {}"}}],
            }}),
            serde_json::json!({"type": "tool_execution_end", "toolCallId": "c2",
                "toolName": "write",
                "result": {"content": [{"type": "text", "text": "ok"}]}}),
            serde_json::json!({"type": "message_end", "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "Covered the API."}],
                "usage": {"totalTokens": 12},
            }}),
        ];
        for event in events {
            body.push_str(&event.to_string());
            body.push('\n');
        }
        body.push_str("\n=== exit ===\ncode: 0\nduration: 2m\n");
        body.push_str("ended: 2026-09-16T10:02:00Z\n===\n");
        body
    }

    fn workspace_with_log(log_name: &str, content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tmpdir");
        let logs = dir.path().join("runtime").join("logs");
        fs::create_dir_all(&logs).expect("logs dir");
        let log = logs.join(log_name);
        fs::write(&log, content).expect("log");
        (dir, log)
    }

    /// The report carries every section the spec names, in the log's order,
    /// and never invents an exit for a session that recorded one.
    // §FS-rhei-session-reports.2
    #[test]
    fn a_session_log_renders_prompt_actions_files_and_outcome() {
        let (dir, log) = workspace_with_log("task-plan.1-cover.log", &pi_log());
        let runtime = dir.path().join("runtime");
        let report = render_session_report(&log, &runtime, false).expect("rendered");
        let text = fs::read_to_string(&report).expect("report");
        assert!(report.ends_with("reports/task-plan.1-cover.md"));
        assert!(text.contains("# plan.1 — cover"));
        assert!(text.contains("**Duration**: 2m · **Exit**: 0"));
        assert!(text.contains("# Task plan.1\ncover the API"));
        assert!(text.contains("**read** `suite/build.gradle`"));
        assert!(text.contains("plugins { }"));
        assert!(text.contains("## Files produced"));
        assert!(text.contains("### `suite/Test.java`"));
        assert!(text.contains("class Test {}"));
        assert!(text.contains("Covered the API."));
        assert!(text.contains("totalTokens"));
    }

    /// A log without an exit footer says so; the renderer never infers one.
    // §FS-rhei-session-reports.5
    #[test]
    fn a_log_without_an_exit_footer_is_reported_as_such() {
        let content = pi_log();
        let truncated = &content[..content.find("=== exit ===").expect("footer")];
        let (dir, log) = workspace_with_log("task-plan.1-cover.log", truncated);
        let report =
            render_session_report(&log, &dir.path().join("runtime"), false).expect("rendered");
        let text = fs::read_to_string(&report).expect("report");
        assert!(text.contains("session ended without an exit record"));
    }

    /// A non-Pi event stream renders header and notice, never a wrong body.
    // §FS-rhei-session-reports.6
    #[test]
    fn an_unsupported_stream_is_named_not_guessed() {
        let mut content = String::from("=== rhei agent log v1 ===\ntask: plan.1\n===\n");
        content.push_str("{\"type\":\"result\",\"session_id\":\"x\"}\n");
        let (dir, log) = workspace_with_log("task-plan.1-cover.log", &content);
        let report =
            render_session_report(&log, &dir.path().join("runtime"), false).expect("rendered");
        let text = fs::read_to_string(&report).expect("report");
        assert!(text.contains("not a Pi session stream"));
        assert!(!text.contains("## Agent actions"));
    }

    /// Oversized tool output is cut at the rendering limit with a pointer to
    /// the log; `full` disables the cut. Capture is never changed.
    // §FS-rhei-session-reports.3
    #[test]
    fn tool_output_truncates_at_the_limit_unless_full() {
        let big = "x".repeat(SESSION_REPORT_OUTPUT_LIMIT + 100);
        let mut content = String::from("=== rhei agent log v1 ===\ntask: plan.1\n===\n");
        for event in [
            serde_json::json!({"type": "session", "id": "s"}),
            serde_json::json!({"type": "message_end", "message": {"role": "assistant",
                "content": [{"type": "toolCall", "id": "c1", "name": "bash",
                             "arguments": {"command": "cat big"}}]}}),
            serde_json::json!({"type": "tool_execution_end", "toolCallId": "c1",
                "toolName": "bash",
                "result": {"content": [{"type": "text", "text": big}]}}),
        ] {
            content.push_str(&event.to_string());
            content.push('\n');
        }
        let (dir, log) = workspace_with_log("task-plan.1-cover.log", &content);
        let runtime = dir.path().join("runtime");
        let cut = fs::read_to_string(
            render_session_report(&log, &runtime, false).expect("rendered"),
        )
        .expect("report");
        assert!(cut.contains("bytes truncated"));
        assert!(cut.contains("full output in the log at line"));
        let full = fs::read_to_string(
            render_session_report(&log, &runtime, true).expect("rendered"),
        )
        .expect("report");
        assert!(!full.contains("bytes truncated"));
    }
}

mod metric_declarations {
    use super::super::*;

    fn machine_yaml(metrics: &str) -> String {
        format!(
            "name: m\nversion: 1\nstates:\n  measure:\n    program: \"true\"\n    \
             initial: true\n  cover:\n    description: work\n  done:\n    final: true\n\
             transitions:\n  - {{from: measure, to: cover}}\n  - {{from: cover, to: measure}}\n  \
             - {{from: measure, to: done}}\n{metrics}"
        )
    }

    /// The examples the spec declares parse, and `measured_by` is a list.
    // §FS-rhei-metrics.1
    #[test]
    fn a_valid_metric_declaration_loads() {
        let machine = rhei_validator::StateMachine::from_yaml_str(&machine_yaml(
            "metrics:\n  coverage:\n    label: API coverage\n    measured_by: [measure]\n    \
             drivers: [cover]\n    artifact: \"runtime/report-{iteration}.json\"\n    \
             pointer: /summary/percent\n    unit: \"%\"\n    goal: increase\n",
        ))
        .expect("valid metric loads");
        let metric = machine.metrics.get("coverage").expect("declared");
        assert_eq!(metric.measured_by, vec!["measure".to_string()]);
        assert_eq!(metric.kind, rhei_validator::MetricKind::Number);
    }

    /// Metrics referencing unknown states, ambiguous value sources, or a
    /// boundary without `{iteration}` are rejected at load.
    // §FS-rhei-metrics.1
    #[test]
    fn invalid_metric_declarations_are_rejected() {
        for (metrics, expected) in [
            (
                "metrics:\n  m:\n    measured_by: [missing]\n    \
                 artifact: \"a-{iteration}\"\n    pointer: /x\n",
                "does not declare",
            ),
            (
                "metrics:\n  m:\n    measured_by: [measure]\n    \
                 artifact: \"a-{iteration}\"\n    pointer: /x\n    program: echo\n",
                "exactly one value source",
            ),
            (
                "metrics:\n  m:\n    measured_by: [measure]\n    \
                 artifact: \"a-{iteration}\"\n",
                "exactly one value source",
            ),
            (
                "metrics:\n  m:\n    measured_by: [measure]\n    \
                 artifact: \"a.json\"\n    pointer: /x\n",
                "{iteration}",
            ),
            ("metrics:\n  m:\n    measured_by: []\n    artifact: \"a-{iteration}\"\n    pointer: /x\n", "empty `measured_by`"),
        ] {
            let error = rhei_validator::StateMachine::from_yaml_str(&machine_yaml(metrics))
                .expect_err("invalid metric rejected");
            assert!(
                error.to_string().contains(expected),
                "expected '{expected}' in '{error}'"
            );
        }
    }
}

mod metric_recording {
    use super::super::*;

    fn machine_with_metric() -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(
            "name: m\nversion: 1\nstates:\n  measure:\n    program: \"true\"\n    \
             initial: true\n  cover:\n    description: work\n  fix:\n    description: repair\n  \
             done:\n    final: true\n\
             transitions:\n  - {from: measure, to: cover}\n  - {from: cover, to: measure}\n  \
             - {from: measure, to: fix}\n  - {from: fix, to: measure}\n  \
             - {from: measure, to: done}\n\
             metrics:\n  coverage:\n    label: API coverage\n    measured_by: [measure]\n    \
             drivers: [cover]\n    artifact: \"runtime/validation/report-{iteration}.json\"\n    \
             pointer: /summary/percent\n    detail: \"{/summary/covered}/{/summary/total}\"\n    \
             unit: \"%\"\n    goal: increase\n",
        )
        .expect("machine loads")
    }

    fn write_measurement(root: &Path, iteration: u64, percent: f64, covered: u64) {
        let dir = root.join("runtime").join("validation");
        fs::create_dir_all(&dir).expect("validation dir");
        fs::write(
            dir.join(format!("report-{iteration}.json")),
            serde_json::json!({"summary": {"percent": percent, "covered": covered,
                "total": 801}})
            .to_string(),
        )
        .expect("measurement");
    }

    fn note_session(root: &Path, state: &str, moves: u64) {
        let runtime = root.join("runtime");
        let log = runtime.join("logs").join(format!("task-plan.1-{state}.log"));
        note_metric_pending_session(
            &runtime,
            "plan.1",
            state,
            moves,
            1,
            &log,
            Some(0),
            "exited",
        );
    }

    /// The full loop: a baseline measurement with no sessions, a driver
    /// session confirmed into iteration 1 with its delta, a failed
    /// measurement advancing nothing, and a shared window attributing the
    /// cover and fix sessions together — never collapsed onto one.
    // §FS-rhei-metrics.2 §FS-rhei-metrics.3 §FS-rhei-metrics.4
    #[test]
    fn iterations_bind_sessions_by_measurement_windows() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let root = dir.path();
        let machine = machine_with_metric();
        let record_path = root.join("runtime").join("metrics").join("coverage.jsonl");

        // Baseline: measurement 0 exists before any driver session ran.
        write_measurement(root, 0, 3.23, 23);
        confirm_metric_iterations(root, &machine, "plan.1", "measure");
        let records = read_metric_records(&record_path);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].iteration, 0);
        assert_eq!(records[0].detail.as_deref(), Some("23/801"));
        assert!(records[0].sessions.is_empty());

        // One driver session, then a successful measurement: iteration 1.
        note_session(root, "cover", 1);
        write_measurement(root, 1, 76.37, 543);
        confirm_metric_iterations(root, &machine, "plan.1", "measure");
        let records = read_metric_records(&record_path);
        assert_eq!(records.len(), 2);
        assert_eq!(records[1].sessions.len(), 1);
        assert!(records[1].sessions[0].driver);
        assert_eq!(records[1].sessions[0].state, "cover");

        // A failed measurement: no report-2.json, so nothing advances.
        note_session(root, "cover", 2);
        confirm_metric_iterations(root, &machine, "plan.1", "measure");
        assert_eq!(read_metric_records(&record_path).len(), 2);

        // The repair session joins the same window as the cover it repairs.
        note_session(root, "fix", 2);
        write_measurement(root, 2, 94.66, 673);
        confirm_metric_iterations(root, &machine, "plan.1", "measure");
        let records = read_metric_records(&record_path);
        assert_eq!(records.len(), 3);
        let window: Vec<(&str, bool)> = records[2]
            .sessions
            .iter()
            .map(|session| (session.state.as_str(), session.driver))
            .collect();
        assert_eq!(window, vec![("cover", true), ("fix", false)]);

        // The summary is regenerated from the records alone.
        let summary = fs::read_to_string(
            root.join("runtime").join("reports").join("metrics-summary.md"),
        )
        .expect("summary");
        assert!(summary.contains("## API coverage — `plan.1`"));
        assert!(summary.contains("| 0 | (baseline) | 3.23% (23/801) | — |"));
        assert!(summary.contains("▲ +73.14%"));
        assert!(summary.contains("94.66% (673/801)"));
    }

    /// Another task's sessions never enter this task's windows.
    // §FS-rhei-metrics.2
    #[test]
    fn sessions_of_other_tasks_stay_out_of_the_window() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let root = dir.path();
        let machine = machine_with_metric();
        let runtime = root.join("runtime");
        note_metric_pending_session(
            &runtime,
            "plan.2",
            "cover",
            1,
            1,
            &runtime.join("logs").join("task-plan.2-cover.log"),
            Some(0),
            "exited",
        );
        write_measurement(root, 0, 1.0, 1);
        confirm_metric_iterations(root, &machine, "plan.1", "measure");
        let records =
            read_metric_records(&runtime.join("metrics").join("coverage.jsonl"));
        assert_eq!(records.len(), 1);
        assert!(records[0].sessions.is_empty());
    }

    /// The environment names the iteration a measuring state would confirm,
    /// and says nothing when two metrics make the number ambiguous.
    // §FS-rhei-metrics.2
    #[test]
    fn next_iteration_is_deterministic_and_unambiguous() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let root = dir.path();
        let machine = machine_with_metric();
        assert_eq!(next_metric_iteration(root, &machine, "measure"), Some(0));
        assert_eq!(next_metric_iteration(root, &machine, "cover"), None);
        write_measurement(root, 0, 1.0, 1);
        confirm_metric_iterations(root, &machine, "plan.1", "measure");
        assert_eq!(next_metric_iteration(root, &machine, "measure"), Some(1));
    }
}
