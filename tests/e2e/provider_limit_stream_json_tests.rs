//! Claude transport exposes result-text lines for strict refusal recognition.
//! §FS-rhei-agents.2.3 §FS-rhei-run.3.3

use super::provider_limit_support::*;
use super::*;

const INCIDENT: &str = include_str!("fixtures/provider_limit_stream_json/refusal.jsonl");

fn result_event(text: &str) -> String {
    let mut event: serde_json::Value =
        serde_json::from_str(INCIDENT.lines().last().unwrap()).unwrap();
    event["result"] = text.into();
    format!("{event}\n")
}

fn fixture(name: &str, stdout: &str, stderr: &str, code: i32) -> ProviderFixture {
    let machine = SIMPLE_MACHINE
        .replace("target: codex:openai:alpha", "target: cld:anthropic:alpha")
        .replace(
            "attempts: 1",
            "attempts: 2\n    outputs:\n      - name: finding\n        path: runtime/exports/{task_id}/finding.md",
        );
    let body = format!(
        r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'runtime' / 'starts.txt', env('RHEI_ATTEMPT') + '\n')
assert '--output-format' in sys.argv and 'stream-json' in sys.argv, sys.argv
sys.stdout.write({stdout:?})
sys.stdout.flush()
sys.stderr.write({stderr:?})
sys.stderr.flush()
raise SystemExit({code})
"#
    );
    let fixture = ProviderFixture::new(name, SINGLE_TASK, &machine, &body);
    fs::write(
        fixture.root.join(".agent-grounds/rhei/settings.json"),
        serde_json::json!({"agents": {"cld": {
            "family": "claude-code",
            "command": serde_json::from_str::<serde_json::Value>(&fixture_command(&fixture.agent)).unwrap(),
            "stdin_prompt": true,
            "timeout": "30s"
        }}}).to_string(),
    ).unwrap();
    fixture
}

fn start(fixture: &ProviderFixture, name: &str) -> RunningChild {
    let mut command = rhei_command(fixture.root.join(".home"));
    // A nested fixture owns its admission and accounting context. §FS-rhei-budgets.5
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("RHEI_") {
            command.env_remove(key);
        }
    }
    command.arg("--state-machine").arg(&fixture.machine).arg("run").arg(&fixture.root);
    command.args(["--no-tui", "--no-dashboard", "--json", "--continue-on-error", "--until-idle"]);
    RunningChild::spawn(&mut command, &fixture.dir, name)
}

fn assert_parks_and_reentry_is_suppressed(fixture: &ProviderFixture) {
    let mut first = start(fixture, "first");
    let status = first.wait_for_exit("until-idle refusal observation");
    let first_records = fixture.records();
    let first_wait =
        metadata(&fixture.root)["metadata"]["tasks"]["1"]["providerLimits"]["working"].clone();
    let mut restarted = start(fixture, "restarted");
    let restarted_status = restarted.wait_for_exit("until-idle re-entry before reset");
    let mut third = start(fixture, "third");
    let third_status = third.wait_for_exit("third until-idle pass of the same visit");
    let records = fixture.records();
    let last = records.last().expect("the fake agent produced a spawn record");
    assert_eq!(
        last["ending"],
        "provider_limited",
        "stream-json refusal must park without charging attempts; records={records:#?}; {}; {}; {}",
        first.output(),
        restarted.output(),
        third.output()
    );
    assert_eq!(status.code(), Some(3), "until-idle reports a provider wait; {}", first.output());
    assert_eq!(records.len(), 1, "{records:#?}");
    assert_eq!(last["attempt_charged"], false, "{last:#}");
    assert_eq!(last["charged"], 0, "{last:#}");
    assert_eq!(last["code"], 1, "{last:#}");

    let wait =
        metadata(&fixture.root)["metadata"]["tasks"]["1"]["providerLimits"]["working"].clone();
    assert_eq!(wait, first_wait, "the identity-qualified wait survives restart");
    assert_eq!(wait["identity"]["agent"], "cld", "{wait:#}");
    assert_eq!(wait["identity"]["provider"], "anthropic", "{wait:#}");
    assert_eq!(wait["signal"], LIMIT_SIGNAL, "{wait:#}");
    let deadline = wait["nextAttemptAt"].as_str().expect("durable UTC reset");
    assert!(deadline.ends_with(":21:00Z"), "{deadline}");
    assert!(deadline > utc_at(epoch_now()).as_str(), "{deadline}");
    assert!(wait["observedAt"].as_str().is_some(), "{wait:#}");
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
    assert_eq!(fs::read_to_string(fixture.root.join("runtime/starts.txt")).unwrap(), "1\n");

    assert_eq!(
        restarted_status.code(),
        Some(3),
        "until-idle must report the retained provider wait; {}",
        restarted.output()
    );
    assert_eq!(third_status.code(), Some(3), "{}", third.output());
    assert_eq!(records, first_records, "re-entry must retain the uncharged spawn");
    assert_eq!(fs::read_to_string(fixture.root.join("runtime/starts.txt")).unwrap(), "1\n");
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
    for output in [first.output(), restarted.output(), third.output()] {
        assert!(!output.contains("Re-spawning"), "{output}");
        assert!(!output.contains("attempts spent on this visit"), "{output}");
        assert!(!output.contains("halting Task"), "{output}");
    }
}

/// Control from rhei.47, with the same family, identity, exit and budget.
/// §FS-rhei-agents.2.3 §FS-rhei-run.3.3
#[test]
fn claude_bare_refusal_parks_and_suppresses_reentry() {
    let fixture = fixture("claude-bare-control", &format!("{LIMIT_SIGNAL}\n"), "", 1);
    assert_parks_and_reentry_is_suppressed(&fixture);
}

/// The incident's rate-limit event and assistant echo accompany one result.
/// §FS-rhei-agents.2.3 §FS-rhei-run.3.3
#[test]
fn claude_stream_json_refusal_parks_and_suppresses_reentry() {
    let fixture = fixture("claude-stream-json-refusal", INCIDENT, "", 1);
    assert_parks_and_reentry_is_suppressed(&fixture);
}

/// Whole-line matching applies after splitting decoded result text.
/// §FS-rhei-agents.2.3
#[test]
fn claude_multiline_result_refusal_parks() {
    let fixture = fixture(
        "claude-multiline-refusal",
        &result_event(&format!("context\n  {LIMIT_SIGNAL}  \nmore context")),
        "",
        1,
    );
    assert_parks_and_reentry_is_suppressed(&fixture);
}

/// A refusal is independent of usage coverage. §FS-rhei-agents.2.3
#[test]
fn claude_result_refusal_without_usage_parks() {
    let event = serde_json::json!({"type": "result", "result": LIMIT_SIGNAL});
    let fixture = fixture("claude-refusal-no-usage", &format!("{event}\n"), "", 1);
    assert_parks_and_reentry_is_suppressed(&fixture);
}

/// Independently emitted result signals remain duplicates; other JSON and
/// prose do not acquire refusal semantics. These controls pass before the fix.
/// §FS-rhei-agents.2.3
#[test]
fn claude_stream_json_ordinary_results_remain_charged() {
    let result = result_event(LIMIT_SIGNAL);
    let cases = [
        ("two-results", result.repeat(2), String::new(), 1),
        (
            "two-logical-lines",
            result_event(&format!("{LIMIT_SIGNAL}\n{LIMIT_SIGNAL}")),
            String::new(),
            1,
        ),
        (
            "assistant-only",
            INCIDENT.lines().take(2).collect::<Vec<_>>().join("\n") + "\n",
            String::new(),
            1,
        ),
        (
            "unrelated-json",
            format!("{}\n", serde_json::json!({"type": "other", "result": LIMIT_SIGNAL})),
            String::new(),
            1,
        ),
        ("prose", result_event(&format!("quoted: {LIMIT_SIGNAL}")), String::new(), 1),
        ("exit-zero", result, String::new(), 0),
    ];
    for (name, stdout, stderr, code) in cases {
        let fixture = fixture(name, &stdout, &stderr, code);
        let mut run = start(&fixture, "ordinary");
        run.wait_for_exit("first ordinary result");
        let records = fixture.records();
        assert_eq!(records.len(), 1, "{name}: {records:#?}; {}", run.output());
        for record in &records {
            assert_eq!(record["ending"], "exited", "{name}: {record:#}");
            assert_eq!(record["attempt_charged"], true, "{name}: {record:#}");
        }
        assert_eq!(records[0]["charged"], 1, "{name}: {records:#?}");
        assert!(!markdown_text(&fixture.root).contains("providerLimits:"), "{name}");
        assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
    }
}

/// The stream-json signal and a separate bare signal are two real refusals,
/// even when identical. Raw capture currently sees only the bare one.
/// §FS-rhei-agents.2.3
#[test]
fn claude_result_and_plain_refusal_remain_charged() {
    let result = result_event(LIMIT_SIGNAL);
    for (name, stdout, stderr) in [
        ("result-and-stderr", result.clone(), format!("{LIMIT_SIGNAL}\n")),
        ("result-and-stdout", format!("{result}{LIMIT_SIGNAL}\n"), String::new()),
    ] {
        let fixture = fixture(name, &stdout, &stderr, 1);
        let mut run = start(&fixture, "duplicate");
        run.wait_for_exit("duplicate logical signals");
        let records = fixture.records();
        assert_eq!(records.len(), 1, "{name}: {records:#?}");
        assert_eq!(records[0]["ending"], "exited", "{name}: {records:#?}");
        assert_eq!(records[0]["attempt_charged"], true, "{name}: {records:#?}");
        assert_eq!(records[0]["charged"], 1, "{name}: {records:#?}");
        assert!(!markdown_text(&fixture.root).contains("providerLimits:"), "{name}");
    }
}
