//! When Codex reports that it overrode a setting Rhei asked for, every surface
//! that explains the invocation says so, so nobody reads a transcript to learn
//! why a worker exited 0 and wrote nothing (agent-grounds/rhei#509).
//!
//! The diagnostic is informational: the exit, the charge, the task's state and
//! the missing-output halt stay as they are. §FS-rhei-configuration-fallbacks

use super::codex_config_fallback_support::*;
use super::*;

/// The halt names the fallback beside the missing outputs, in the report, on
/// the console and on the slot release, and changes nothing else.
fn assert_fallback_explains_the_halt(name: &str, events: &[String], message: &str, parallel: bool) {
    let script = Script { events, ..Script::default() };
    let fixture = fallback_fixture(name, &script, parallel);
    let task = fixture.task_id();
    let run = fixture.run_plain();
    let context = format!("stdout:\n{}\nstderr:\n{}", run.stdout, run.stderr);

    // What the run does about the halt is unchanged. §FS-rhei-configuration-fallbacks
    assert_eq!(run.status.code(), Some(1), "{context}");
    let task_file = std::fs::read_to_string(fixture.plan.join("tasks/01.md")).unwrap();
    assert!(task_file.contains("**State:** working"), "{task_file}");
    assert!(!task_file.contains("providerLimits"), "{task_file}");
    let spawns = fixture.spawn_records();
    assert!(
        spawns.iter().all(|r| r["attempt_charged"] == true && r["ending"] != "provider_limited"),
        "{spawns:#?}"
    );

    let attention = section(&fixture.report(), "Attention");
    let row = attention.lines().find(|line| line.contains(&task)).unwrap_or_default();
    assert!(row.contains("worker exited 0 without finding"), "attention:\n{attention}\n{context}");
    assert!(
        row.contains(&format!("; {CLAUSE}{}", md(message))),
        "the attention row must name the fallback beside the missing outputs:\n{row}\n{context}"
    );
    let warning = format!(
        "warning: Codex configuration fallback for task {task} in state 'working': {message}"
    );
    assert!(run.stderr.contains(&warning) || run.stdout.contains(&warning), "{context}");

    let json = fallback_fixture(&format!("{name}-json"), &script, parallel);
    let (run, records) = json.run_json();
    let release = released(&records, &json.task_id());
    assert_eq!(
        release["configuration_fallbacks"],
        serde_json::json!([message]),
        "{release:#}\nstderr:\n{}",
        run.stderr
    );
    assert_eq!(release["exit_code"], 0, "{release:#}");
}

/// The report escapes a `|` and folds a newline, so a message is compared as
/// the cell spells it.
fn md(message: &str) -> String {
    message.replace('|', "\\|").replace('\n', " ")
}

#[test]
fn an_item_error_fallback_explains_a_sequential_halt() {
    let events = [item_error(PERMISSION_FALLBACK)];
    assert_fallback_explains_the_halt("codex-fallback-seq", &events, PERMISSION_FALLBACK, false);
}

#[test]
fn an_item_error_fallback_explains_a_parallel_halt() {
    let events = [item_error(PERMISSION_FALLBACK)];
    assert_fallback_explains_the_halt("codex-fallback-par", &events, PERMISSION_FALLBACK, true);
}

/// A top-level `error` event counts too, for any field. §FS-rhei-configuration-fallbacks
#[test]
fn a_top_level_error_fallback_for_another_field_explains_the_halt() {
    let events = [top_error(APPROVAL_FALLBACK)];
    assert_fallback_explains_the_halt("codex-fallback-top", &events, APPROVAL_FALLBACK, false);
}

/// A fallback on an invocation that still wrote everything it owed is shown
/// under Invocations and on the console, and gets no Attention row.
/// §FS-rhei-run-report.3.1 §FS-rhei-run-report.2
#[test]
fn a_successful_invocation_shows_its_fallback_only_under_invocations() {
    let events = [item_error(APPROVAL_FALLBACK)];
    let script = Script { events: &events, writes: true, ..Script::default() };
    let fixture = fallback_fixture("codex-fallback-success", &script, false);
    let task = fixture.task_id();
    let run = fixture.run_plain();
    let context = format!("stdout:\n{}\nstderr:\n{}", run.stdout, run.stderr);
    assert_success(&run);
    let report = fixture.report();
    assert!(!section(&report, "Attention").contains(CLAUSE), "{report}");
    assert!(
        section(&report, "Invocations").contains(&md(APPROVAL_FALLBACK)),
        "{report}\n{context}"
    );
    let warning =
        format!("warning: Codex configuration fallback for task {task} in state 'working': ");
    assert!(run.stderr.contains(&warning) || run.stdout.contains(&warning), "{context}");
}

/// A later attempt that halts without a fallback does not inherit the earlier
/// attempt's. §FS-rhei-configuration-fallbacks §FS-rhei-run-report.8
#[test]
fn a_later_attempt_does_not_inherit_an_earlier_fallback() {
    let events = [item_error(PERMISSION_FALLBACK)];
    let script = Script { events: &events, emit_attempts: 1, attempts: 2, ..Script::default() };
    let fixture = fallback_fixture("codex-fallback-retry", &script, false);
    let first = fixture.run_plain();
    let first_report = section(&fixture.report(), "Attention");
    assert!(
        first_report.contains(CLAUSE),
        "the first attempt carried a fallback:\n{first_report}\nstderr:\n{}",
        first.stderr
    );
    let second = fixture.run_plain();
    let second_report = fixture.report();
    assert!(
        second_report.contains("worker exited 0 without finding"),
        "{second_report}\n{}",
        second.stderr
    );
    assert!(!second_report.contains(CLAUSE), "attempt 2 emitted no fallback:\n{second_report}");
    assert!(
        !second.stderr.contains(CLAUSE) && !second.stdout.contains(CLAUSE),
        "{}",
        second.stderr
    );
}

/// Output that only looks like a fallback is not one: an assistant message
/// quoting it, an unrelated error, another family's output, and malformed JSON.
/// A guard. §FS-rhei-configuration-fallbacks
#[test]
fn output_that_only_looks_like_a_fallback_is_not_one() {
    let echo = serde_json::json!({"type": "item.completed", "item": {"id": "item_1", "type": "agent_message", "text": FALLBACK_LEAD}}).to_string();
    let unrelated = top_error("stream disconnected before completion");
    let malformed = format!(
        "{{\"type\":\"item.completed\",\"item\":{{\"type\":\"error\",\"message\":\"{FALLBACK_LEAD}"
    );
    let cases: [(&str, Vec<String>, &str); 4] = [
        ("echo", vec![echo], "codex"),
        ("unrelated", vec![unrelated], "codex"),
        ("malformed", vec![malformed], "codex"),
        ("other-family", vec![item_error(PERMISSION_FALLBACK)], "gemini"),
    ];
    for (name, events, family) in cases {
        let script = Script { events: &events, family, ..Script::default() };
        let fixture = fallback_fixture(&format!("codex-fallback-not-{name}"), &script, false);
        let run = fixture.run_plain();
        let report = fixture.report();
        assert!(
            report.contains("worker exited 0 without finding"),
            "{name}:\n{report}\n{}",
            run.stderr
        );
        assert!(!report.contains(CLAUSE), "{name} is not a fallback:\n{report}");
        assert!(
            !run.stderr.contains(CLAUSE) && !run.stdout.contains(CLAUSE),
            "{name}: {}",
            run.stderr
        );
    }
}
