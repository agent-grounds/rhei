// The per-stream session-report contract: how each extractor's log reads
// back, how the parser tells streams, plain output, and unknown streams
// apart without ever rendering a body wrongly, and what the live display
// shows of the Claude stream the report reads.
//
// Its own part because stream detection and the Claude Code and Codex
// collectors are their own surface beside the base report contract.

// §AR-source-file-size.3 §FS-rhei-session-reports.6

mod session_report_streams {
    use super::super::*;
    use super::session_reports::workspace_with_log;

    /// Prose that quotes the odd JSON line is still plain output, never an
    /// unsupported stream or an empty report.
    // §FS-rhei-session-reports.6.1 §FS-rhei-session-reports.6.4
    #[test]
    fn quoted_json_inside_plain_output_stays_plain() {
        let mut content = String::from("=== rhei agent log v1 ===\n");
        content.push_str("agent: claude-code\ntask: plan.5\nstate: present\n===\n\n");
        content.push_str("The stream format looks like this:\n");
        content.push_str("{\"type\":\"module\",\"name\":\"x\"}\n");
        content.push_str("and every event carries a type field.\n");
        content.push_str("I documented it in the summary.\n");
        content.push_str("\n=== exit ===\ncode: 0\nduration: 10s\n===\n");
        let (dir, log) = workspace_with_log("task-plan.5-present.log", &content);
        let report = render_session_report(&log, &dir.path().join("runtime"), false)
            .expect("rendered");
        let text = fs::read_to_string(&report).expect("report");
        assert!(text.contains("## Session output"));
        assert!(text.contains("{\"type\":\"module\",\"name\":\"x\"}"));
        assert!(!text.contains("not one this renderer reads"));
    }

    /// A stream that dies before its marker still renders through the agent
    /// the header records — the one error line is the whole point of the
    /// report.
    // §FS-rhei-session-reports.6.1
    #[test]
    fn a_markerless_stream_falls_back_to_the_recorded_agent() {
        let mut content = String::from("=== rhei agent log v1 ===\n");
        content.push_str("agent: codex\ntask: plan.6\nstate: build\n===\n");
        content.push_str("{\"type\":\"error\",\"message\":\"401 Unauthorized\"}\n");
        content.push_str("\n=== exit ===\ncode: 1\nduration: 1s\n===\n");
        let (dir, log) = workspace_with_log("task-plan.6-build.log", &content);
        let report = render_session_report(&log, &dir.path().join("runtime"), false)
            .expect("rendered");
        let text = fs::read_to_string(&report).expect("report");
        assert!(text.contains("codex error: 401 Unauthorized"));
        assert!(!text.contains("not one this renderer reads"));
    }

    /// A JSON stream the fallback cannot read either is reported as
    /// unsupported, never pasted as plain output or rendered empty.
    // §FS-rhei-session-reports.6.1
    #[test]
    fn a_fallback_that_collects_nothing_is_unsupported() {
        let mut content = String::from("=== rhei agent log v1 ===\n");
        content.push_str("agent: codex\ntask: plan.7\nstate: build\n===\n");
        content.push_str("{\"id\":\"0\",\"msg\":{\"type\":\"agent_message\",\"text\":\"hi\"}}\n");
        content.push_str("{\"id\":\"1\",\"msg\":{\"type\":\"task_complete\"}}\n");
        let (dir, log) = workspace_with_log("task-plan.7-build.log", &content);
        let report = render_session_report(&log, &dir.path().join("runtime"), false)
            .expect("rendered");
        let text = fs::read_to_string(&report).expect("report");
        assert!(text.contains("not one this renderer reads"));
        assert!(!text.contains("## Session output"));
        assert!(!text.contains("## Agent actions"));
    }

    /// One stream's generic event names never leak into another's report:
    /// a Pi session with a stray top-level error event stays a Pi report.
    // §FS-rhei-session-reports.6.1
    #[test]
    fn foreign_event_names_stay_out_of_other_streams() {
        let mut content = String::from("=== rhei agent log v1 ===\n");
        content.push_str("agent: pi\ntask: plan.8\nstate: cover\n===\n");
        for event in [
            serde_json::json!({"type": "session", "id": "s-8"}),
            serde_json::json!({"type": "error", "message": "rate limited"}),
            serde_json::json!({"type": "message_end", "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "Done."}]}}),
        ] {
            content.push_str(&event.to_string());
            content.push('\n');
        }
        let (dir, log) = workspace_with_log("task-plan.8-cover.log", &content);
        let report = render_session_report(&log, &dir.path().join("runtime"), false)
            .expect("rendered");
        let text = fs::read_to_string(&report).expect("report");
        assert!(text.contains("Done."));
        assert!(!text.contains("codex error"));
    }

    /// Text riding along a tool result is injected context, never the
    /// prompt: an unechoed prompt renders the explicit marker.
    // §FS-rhei-session-reports.6.2
    #[test]
    fn injected_text_beside_tool_results_is_not_the_prompt() {
        let mut content = String::from("=== rhei agent log v1 ===\n");
        content.push_str("agent: claude-code\ntask: plan.9\nstate: fix\n===\n");
        for event in [
            serde_json::json!({"type": "system", "subtype": "init", "session_id": "c-9"}),
            serde_json::json!({"type": "assistant", "message": {"role": "assistant",
                "content": [{"type": "tool_use", "id": "t1", "name": "Bash",
                    "input": {"command": "ls"}}]}}),
            serde_json::json!({"type": "user", "message": {"role": "user",
                "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "src"},
                    {"type": "text", "text": "<system-reminder>plans changed</system-reminder>"},
                ]}}),
        ] {
            content.push_str(&event.to_string());
            content.push('\n');
        }
        let (dir, log) = workspace_with_log("task-plan.9-fix.log", &content);
        let report = render_session_report(&log, &dir.path().join("runtime"), false)
            .expect("rendered");
        let text = fs::read_to_string(&report).expect("report");
        assert!(text.contains("(no prompt recorded)"));
        assert!(text.contains("### Step 1 · Bash — ok\n\n**Command:** `ls`"));
    }

    /// A kept result envelope adds its text when the transcript does not
    /// already end with it, and stays silent when it merely repeats the
    /// final message.
    // §FS-rhei-session-reports.6.2
    #[test]
    fn a_kept_result_envelope_never_loses_its_conclusion() {
        let log_for = |result_text: &str| {
            let mut content = String::from("=== rhei agent log v1 ===\n");
            content.push_str("agent: claude-code\ntask: plan.10\nstate: fix\n===\n");
            for event in [
                serde_json::json!({"type": "system", "subtype": "init",
                    "session_id": "c-10"}),
                serde_json::json!({"type": "assistant", "message": {"role": "assistant",
                    "content": [{"type": "text", "text": "Let me check the tests."}]}}),
                serde_json::json!({"type": "result", "subtype": "success",
                    "result": result_text,
                    "usage": {"input_tokens": 3, "output_tokens": 1}}),
            ] {
                content.push_str(&event.to_string());
                content.push('\n');
            }
            content
        };
        let (dir, log) =
            workspace_with_log("task-plan.10-fix.log", &log_for("3 tests still fail."));
        let text = fs::read_to_string(
            render_session_report(&log, &dir.path().join("runtime"), false)
                .expect("rendered"),
        )
        .expect("report");
        assert!(text.contains("3 tests still fail."));

        let (dir, log) =
            workspace_with_log("task-plan.10-fix.log", &log_for("Let me check the tests."));
        let text = fs::read_to_string(
            render_session_report(&log, &dir.path().join("runtime"), false)
                .expect("rendered"),
        )
        .expect("report");
        assert_eq!(text.matches("Let me check the tests.").count(), 1);
    }

    /// Plain output quoting rhei's own log markers stays in the body: only
    /// the log's final block is the exit footer.
    // §FS-rhei-session-reports.6.4
    #[test]
    fn quoted_log_markers_never_steal_the_exit_footer() {
        let mut content = String::from("=== rhei agent log v1 ===\n");
        content.push_str("agent: claude-code\ntask: plan.11\nstate: present\n===\n\n");
        content.push_str("The failing run's log ended with:\n");
        content.push_str("=== exit ===\n");
        content.push_str("code: 1\n");
        content.push_str("===\n");
        content.push_str("so the crash happened before the retry.\n");
        content.push_str("\n=== exit ===\ncode: 0\nduration: 12s\n===\n");
        let (dir, log) = workspace_with_log("task-plan.11-present.log", &content);
        let report = render_session_report(&log, &dir.path().join("runtime"), false)
            .expect("rendered");
        let text = fs::read_to_string(&report).expect("report");
        assert!(text.contains("**Exit**: 0"));
        assert!(text.contains("code: 1"));
        assert!(text.contains("so the crash happened before the retry."));
    }

    /// Output a detached reader appended after the footer never costs the
    /// session its exit record; the late lines are body again.
    // §FS-rhei-session-reports.5 §FS-rhei-session-reports.6.4
    #[test]
    fn late_output_after_the_footer_keeps_the_exit_record() {
        let mut content = String::from("=== rhei agent log v1 ===\n");
        content.push_str("agent: claude-code\ntask: plan.12\nstate: present\n===\n\n");
        content.push_str("Summary written.\n");
        content.push_str("\n=== exit ===\ncode: 0\nduration: 5s\n===\n");
        content.push_str("background watcher flushed its last line\n");
        let (dir, log) = workspace_with_log("task-plan.12-present.log", &content);
        let report = render_session_report(&log, &dir.path().join("runtime"), false)
            .expect("rendered");
        let text = fs::read_to_string(&report).expect("report");
        assert!(text.contains("**Duration**: 5s · **Exit**: 0"));
        assert!(!text.contains("session ended without an exit record"));
        assert!(text.contains("background watcher flushed its last line"));
    }

    /// A Claude Code stream renders the same sections a Pi stream does:
    /// prompt, thinking, tool calls with matched results and errors marked,
    /// files with their edits, the final message, and the last usage.
    // §FS-rhei-session-reports.6.2
    #[test]
    fn a_claude_stream_log_renders_actions_files_and_outcome() {
        let mut body = String::from("=== rhei agent log v1 ===\n");
        body.push_str("agent: claude-code\ntask: plan.2\nstate: fix\n");
        body.push_str("started: 2026-09-22T10:00:00Z\n===\n\n");
        for event in [
            serde_json::json!({"type": "system", "subtype": "init",
                "session_id": "c-1", "model": "claude-opus-5"}),
            serde_json::json!({"type": "user", "message": {"role": "user",
                "content": [{"type": "text", "text": "# Task plan.2\nfix the bug"}]}}),
            serde_json::json!({"type": "assistant", "message": {"role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "inspect the failing test"},
                    {"type": "tool_use", "id": "t1", "name": "Bash",
                     "input": {"command": "cargo test"}},
                ],
                "usage": {"input_tokens": 4, "output_tokens": 2}}}),
            serde_json::json!({"type": "user", "message": {"role": "user",
                "content": [{"type": "tool_result", "tool_use_id": "t1",
                    "content": [{"type": "text", "text": "1 test failed"}],
                    "is_error": true}]}}),
            serde_json::json!({"type": "assistant", "message": {"role": "assistant",
                "content": [{"type": "tool_use", "id": "t2", "name": "Edit",
                    "input": {"file_path": "src/lib.rs",
                              "old_string": "a + 2", "new_string": "a + 1"}}]}}),
            serde_json::json!({"type": "user", "message": {"role": "user",
                "content": [{"type": "tool_result", "tool_use_id": "t2",
                    "content": "edited", "is_error": false}]}}),
            serde_json::json!({"type": "assistant", "message": {"role": "assistant",
                "content": [{"type": "tool_use", "id": "t3", "name": "Grep",
                    "input": {"pattern": "off_by_one", "path": "crates/"}}]}}),
            serde_json::json!({"type": "user", "message": {"role": "user",
                "content": [{"type": "tool_result", "tool_use_id": "t3",
                    "content": "src/lib.rs:12", "is_error": false}]}}),
            serde_json::json!({"type": "assistant", "message": {"role": "assistant",
                "content": [{"type": "text", "text": "Fixed the off-by-one."}],
                "usage": {"input_tokens": 9, "cache_read_input_tokens": 7,
                          "output_tokens": 3}}}),
        ] {
            body.push_str(&event.to_string());
            body.push('\n');
        }
        // The result envelope reaches the log as its display text.
        // §FS-rhei-cost-accounting.4
        body.push_str("Fixed the off-by-one.\n");
        body.push_str("\n=== exit ===\ncode: 0\nduration: 1m\n===\n");
        let (dir, log) = workspace_with_log("task-plan.2-fix.log", &body);
        let report = render_session_report(&log, &dir.path().join("runtime"), false)
            .expect("rendered");
        let text = fs::read_to_string(&report).expect("report");
        assert!(text.contains("# plan.2 — fix"));
        assert!(text.contains("# Task plan.2\nfix the bug"));
        assert!(text.contains("inspect the failing test"));
        assert!(text.contains("### Step 1 · Bash — error\n\n**Command:** `cargo test`"));
        assert!(text.contains("<b>Output of step 1</b> · 1 line"));
        assert!(text.contains("1 test failed"));
        // A search identifies itself by its pattern, not the directory.
        assert!(text.contains("### Step 3 · Grep — ok\n\n**Pattern:** `off_by_one`"));
        assert!(text.contains("### `src/lib.rs` — edited in step 2"));
        assert!(text.contains("Step 2 replaced:"));
        // The agent's words are set apart from tool traffic.
        assert!(text.contains("> **Agent**\n>\n> Fixed the off-by-one."));
        assert!(text.contains("a + 2"));
        assert!(text.contains("a + 1"));
        assert!(text.contains("Fixed the off-by-one."));
        assert!(text.contains("cache_read_input_tokens"));
        assert!(!text.contains("## Session output"));
    }

    /// A Codex stream renders completed items only: reasoning as thinking,
    /// commands with their bundled output, file changes by path and kind,
    /// the agent message, and the last turn's usage.
    // §FS-rhei-session-reports.6.3
    #[test]
    fn a_codex_stream_log_renders_completed_items() {
        let mut body = String::from("=== rhei agent log v1 ===\n");
        body.push_str("agent: codex\ntask: plan.3\nstate: build\n");
        body.push_str("started: 2026-09-22T11:00:00Z\n===\n\n");
        for event in [
            serde_json::json!({"type": "thread.started", "thread_id": "t-9"}),
            serde_json::json!({"type": "turn.started"}),
            serde_json::json!({"type": "item.started", "item": {"id": "item_1",
                "type": "command_execution", "command": "cargo build",
                "aggregated_output": "", "status": "in_progress"}}),
            serde_json::json!({"type": "item.completed", "item": {"id": "item_0",
                "type": "reasoning", "text": "look at the parser first"}}),
            serde_json::json!({"type": "item.completed", "item": {"id": "item_1",
                "type": "command_execution", "command": "cargo build",
                "aggregated_output": "error[E0308]: mismatched types",
                "exit_code": 101, "status": "failed"}}),
            serde_json::json!({"type": "item.completed", "item": {"id": "item_2",
                "type": "file_change",
                "changes": [{"path": "src/parse.rs", "kind": "update"},
                            {"path": "src/dead.rs", "kind": "delete"}],
                "status": "completed"}}),
            serde_json::json!({"type": "item.completed", "item": {"id": "item_3",
                "type": "agent_message", "text": "Build fixed."}}),
            serde_json::json!({"type": "turn.completed", "usage": {
                "input_tokens": 50, "cached_input_tokens": 10, "output_tokens": 8}}),
        ] {
            body.push_str(&event.to_string());
            body.push('\n');
        }
        body.push_str("\n=== exit ===\ncode: 0\nduration: 3m\n===\n");
        let (dir, log) = workspace_with_log("task-plan.3-build.log", &body);
        let report = render_session_report(&log, &dir.path().join("runtime"), false)
            .expect("rendered");
        let text = fs::read_to_string(&report).expect("report");
        assert!(text.contains("look at the parser first"));
        assert!(text.contains("### Step 1 · command — error\n\n**Command:** `cargo build`"));
        assert!(text.contains("error[E0308]: mismatched types"));
        // The started delta of the same command never renders a second call.
        assert_eq!(text.matches("· command —").count(), 1);
        // The action line names the touched paths; the deletion stays there
        // and never lands under files produced.
        assert!(text.contains("### Step 2 · file_change — ok\n\n**Files:** `src/parse.rs, src/dead.rs`"));
        assert!(text.contains("### `src/parse.rs` — updated in step 2"));
        assert!(!text.contains("### `src/dead.rs`"));
        assert!(!text.contains("Replace:"));
        assert!(text.contains("Build fixed."));
        assert!(text.contains("cached_input_tokens"));
    }

    /// A body with no event stream at all is the agent's plain output; it
    /// renders verbatim instead of as an empty report.
    // §FS-rhei-session-reports.6.4
    #[test]
    fn a_plain_output_body_renders_verbatim() {
        let mut content = String::from("=== rhei agent log v1 ===\n");
        content.push_str("agent: claude-code\ntask: plan.4\nstate: present\n===\n\n");
        content.push_str("Wrote `runtime/summary.md` with the requested sections.\n");
        content.push_str("\n=== exit ===\ncode: 0\nduration: 30s\n===\n");
        let (dir, log) = workspace_with_log("task-plan.4-present.log", &content);
        let report = render_session_report(&log, &dir.path().join("runtime"), false)
            .expect("rendered");
        let text = fs::read_to_string(&report).expect("report");
        assert!(text.contains("## Session output"));
        assert!(text.contains("Wrote `runtime/summary.md` with the requested sections."));
        assert!(!text.contains("## Prompt"));
        assert!(!text.contains("no prompt recorded"));
    }
}

mod claude_stream_display {
    use super::super::*;

    /// The live surfaces show a Claude stream the way they show Pi's: the
    /// session start and the assistant's text; tool traffic, thinking, and
    /// notices stay in the log for the report, never as raw JSON on screen.
    // §FS-rhei-session-reports.6.2 §FS-rhei-cost-accounting.4
    #[test]
    fn a_claude_stream_displays_text_and_hides_events() {
        let show = |event: serde_json::Value| {
            display_output_line(AgentUsageExtractor::Claude, &event.to_string())
        };
        assert!(matches!(
            show(serde_json::json!({"type": "system", "subtype": "init", "session_id": "s-7"})),
            AgentOutputLine::Replace(text) if text == "claude-code session started: s-7"
        ));
        assert!(matches!(
            show(serde_json::json!({"type": "assistant", "message": {"role": "assistant",
                "content": [{"type": "text", "text": "Covering slugify next."}]}})),
            AgentOutputLine::Replace(text) if text == "Covering slugify next."
        ));
        for hidden in [
            serde_json::json!({"type": "assistant", "message": {"role": "assistant",
                "content": [{"type": "tool_use", "id": "t1", "name": "Bash",
                             "input": {"command": "ls"}}]}}),
            serde_json::json!({"type": "assistant", "message": {"role": "assistant",
                "content": [{"type": "thinking", "thinking": "plan"}]}}),
            serde_json::json!({"type": "user", "message": {"role": "user",
                "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "src"}]}}),
            serde_json::json!({"type": "rate_limit_event", "rate_limit_info": {}}),
        ] {
            assert!(matches!(show(hidden), AgentOutputLine::Suppress));
        }
        assert!(matches!(
            display_output_line(AgentUsageExtractor::Claude, "plain stderr-ish text"),
            AgentOutputLine::Passthrough
        ));
    }
}

mod session_report_paths {
    use super::super::*;
    use super::session_reports::workspace_with_log;

    /// Paths the agent spelled absolutely read relative to the session's
    /// working root; a path outside it stays as written.
    // §FS-rhei-session-reports.1
    #[test]
    fn absolute_paths_read_relative_to_the_session_root() {
        let mut content = String::from("=== rhei agent log v1 ===\n");
        content.push_str("agent: claude-code\ntask: plan.13\nstate: cover\n");
        content.push_str("checkout_root: /work/repo\n===\n");
        for event in [
            serde_json::json!({"type": "system", "subtype": "init", "session_id": "c-13"}),
            serde_json::json!({"type": "assistant", "message": {"role": "assistant",
                "content": [
                    {"type": "tool_use", "id": "t1", "name": "Read",
                     "input": {"file_path": "/work/repo/src/lib.rs"}},
                    {"type": "tool_use", "id": "t2", "name": "Write",
                     "input": {"file_path": "/work/repo/test/a.test.mjs", "content": "x"}},
                    {"type": "tool_use", "id": "t3", "name": "Read",
                     "input": {"file_path": "/etc/hosts"}},
                ]}}),
        ] {
            content.push_str(&event.to_string());
            content.push('\n');
        }
        let (dir, log) = workspace_with_log("task-plan.13-cover.log", &content);
        let report = render_session_report(&log, &dir.path().join("runtime"), false)
            .expect("rendered");
        let text = fs::read_to_string(&report).expect("report");
        assert!(text.contains("### Step 1 · Read — no result recorded\n\n**File:** `src/lib.rs`"));
        assert!(text.contains("### `test/a.test.mjs` — written in step 2"));
        assert!(text.contains("**File:** `/etc/hosts`"));
        assert!(!text.contains("/work/repo/src/lib.rs"));
    }

    /// The prompt record wins over a stream's own echo, and a plain-output
    /// report shows its recorded prompt too. §FS-rhei-session-reports.1.1
    #[test]
    fn a_recorded_prompt_is_the_report_prompt() {
        let mut stream = String::from("=== rhei agent log v1 ===\nagent: pi\ntask: plan.14\n===\n");
        for event in [
            serde_json::json!({"type": "session", "id": "s-14"}),
            serde_json::json!({"type": "message_end", "message": {"role": "user",
                "content": [{"type": "text", "text": "echoed prompt"}]}}),
        ] {
            stream.push_str(&event.to_string());
            stream.push('\n');
        }
        let (dir, log) = workspace_with_log("task-plan.14-cover.log", &stream);
        fs::create_dir_all(dir.path().join("runtime/prompts")).expect("prompts dir");
        fs::write(session_prompt_record_path(&log), "recorded prompt").expect("record");
        let text = fs::read_to_string(
            render_session_report(&log, &dir.path().join("runtime"), false).expect("rendered"),
        )
        .expect("report");
        assert!(text.contains("recorded prompt"));
        assert!(!text.contains("echoed prompt"));

        let plain = "=== rhei agent log v1 ===\nagent: claude-code\ntask: plan.15\n===\n\nDone.\n";
        let (dir, log) = workspace_with_log("task-plan.15-present.log", plain);
        assert!(session_prompt_record_path(&log).ends_with("runtime/prompts/task-plan.15-present.md"));
        write_session_prompt_record(&log, "the delivered prompt");
        let text = fs::read_to_string(
            render_session_report(&log, &dir.path().join("runtime"), false).expect("rendered"),
        )
        .expect("report");
        assert!(text.contains("## Prompt") && text.contains("the delivered prompt"));
        assert!(text.contains("## Session output"));
    }
}
