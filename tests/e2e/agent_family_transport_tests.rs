//! What a declared family gives a wrapped agent besides a token count: the
//! launch arguments its extractor requires, a log that says which family wrote
//! it, a rendered session report, and the prohibition on reading Claude usage
//! out of log text. A wrapped profile also parks on a recognized refusal, which
//! it owes to its resolved provider rather than to its family
//! ([§FS-rhei-agents.2.3](../../docs/functional-spec/rhei-agents.spec.md#23-recognized-provider-refusals)).
//!
//! One of these is not an under-delivery if it is left keyed on the id. The
//! log-fallback guard is the sentence
//! [§FS-rhei-cost-accounting.4](../../docs/functional-spec/rhei-cost-accounting.spec.md#4-extraction-flow)
//! closes with a flat prohibition, so a wrapped Claude Code that falls through
//! to the log scraper is forbidden behaviour rather than a missing feature.
// §FS-rhei-agents.1.1.2 §FS-rhei-agents.8.2 §FS-rhei-cost-accounting.4
// §FS-rhei-session-reports.6.2 §FS-rhei-agents.2.3

use std::fs;

use super::agent_family_support::*;
use super::provider_limit_support::*;
use super::*;

/// The family supplies the launch arguments its extractor requires, so the
/// operator writes none of them and a wrapper that forwards `"$@"` is measured.
// §FS-rhei-cost-accounting.4
#[test]
fn a_family_supplies_the_launch_arguments_its_extractor_requires() {
    let workspace = wrapped_workspace(
        "family-launch-args",
        CODEX_DIALECT_AGENT,
        serde_json::json!({ "family": "codex" }),
        "openai",
        "gpt-contract",
    );
    let run = run_cli("run", &workspace.plan, &workspace.machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&run);
    let argv = wrapped_argv(&workspace);
    assert!(
        argv.iter().any(|arg| arg == "--json"),
        "the codex family asks its agent for JSONL; got: {argv:?}"
    );
}

/// A wrapped Claude Code is launched for its event stream, its log says which
/// family wrote it, and the log renders as a Claude session report rather than
/// as the plain output nothing asked to structure.
// §FS-rhei-agents.8.2 §FS-rhei-session-reports.6.2 §FS-rhei-cost-accounting.4
#[test]
fn the_claude_family_is_logged_and_reported_as_claude() {
    let workspace = wrapped_workspace(
        "family-claude-stream",
        CLAUDE_DIALECT_AGENT,
        serde_json::json!({ "family": "claude-code" }),
        "anthropic",
        "claude-opus-5",
    );
    let run = run_cli("run", &workspace.plan, &workspace.machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&run);

    let argv = wrapped_argv(&workspace);
    for expected in ["--output-format", "stream-json", "--verbose"] {
        assert!(
            argv.iter().any(|arg| arg == expected),
            "every launch of the claude-code family requests the event stream; got: {argv:?}"
        );
    }

    // The header line a reader of the artifact resolves the family from: a log
    // is read without the registry that produced it. §FS-rhei-agents.8.2
    let log = workspace.agent_log();
    assert!(
        log.contains(&format!("agent: {WRAPPED_AGENT}\n")),
        "the log still names the profile that ran:\n{log}"
    );
    assert!(
        log.contains("family: claude-code\n"),
        "and the family it belongs to, because they differ:\n{log}"
    );

    let record = one_record(&workspace);
    assert_eq!(record["agent_family"], "claude-code");
    assert_eq!(record["extraction_status"], "measured");
    assert_eq!(record["tokens"]["input"]["total"]["value"], 657);

    let report = rhei_command(workspace.root.join(".home"))
        .arg("report")
        .arg(&workspace.root)
        .output()
        .expect("rhei report runs");
    assert!(report.status.success(), "{}", stderr(&report));
    let reports = workspace.runtime_files("runtime/reports", "md");
    assert_eq!(reports.len(), 1, "one session, one report: {reports:?}");
    let rendered = fs::read_to_string(&reports[0]).expect("read session report");
    assert!(
        rendered.contains("The wrapper spoke Claude."),
        "the Claude stream's assistant text is the report:\n{rendered}"
    );
    assert!(
        !rendered.contains("not one this renderer reads"),
        "and the stream is not read as unsupported:\n{rendered}"
    );
}

/// Claude usage comes only from the typed result envelope. A wrapped Claude
/// Code whose log carries usage-shaped prose is `no-usage-emitted`, never a
/// record scraped out of that prose.
// §FS-rhei-cost-accounting.4 §FS-rhei-cost-accounting.3.2
#[test]
fn the_claude_family_never_reads_usage_from_log_text() {
    let workspace = wrapped_workspace(
        "family-claude-log-text",
        CLAUDE_LOG_TEXT_ONLY_AGENT,
        serde_json::json!({ "family": "claude-code" }),
        "anthropic",
        "claude-opus-5",
    );
    let run = run_cli("run", &workspace.plan, &workspace.machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&run);

    // There was something to scrape. A guard that passed because the log was
    // empty would pin nothing. §FS-rhei-cost-accounting.4
    let log = workspace.agent_log();
    assert!(
        log.contains("tokens used") && log.contains("12,345"),
        "the usage-shaped prose reached the log a scraper would read:\n{log}"
    );

    let record = one_record(&workspace);
    assert_eq!(record["agent_family"], "claude-code");
    assert_eq!(
        record["extraction_status"], "no-usage-emitted",
        "the log scraper is closed to the whole claude-code family: {record:#}"
    );
    assert!(
        record["tokens"]["total"]["value"].is_null(),
        "nothing was measured, so no total was written: {record:#}"
    );
}

/// One task, one attempt, on a literal target naming the wrapper.
fn limit_fixture(name: &str, family: &str, provider: &str) -> ProviderFixture {
    let machine = SIMPLE_MACHINE.replace(
        "target: codex:openai:alpha",
        &format!("target: {WRAPPED_AGENT}:{provider}:alpha"),
    );
    let fixture = ProviderFixture::new(
        name,
        SINGLE_TASK,
        &machine,
        &format!(
            r#"print({LIMIT_SIGNAL:?}, file=sys.stderr, flush=True)
raise SystemExit(1)
"#
        ),
    );
    let settings = fixture.root.join(".agent-grounds/rhei/settings.json");
    let profile = serde_json::json!({
        "family": family,
        "command": serde_json::from_str::<serde_json::Value>(&fixture_command(&fixture.agent))
            .expect("fixture command is JSON"),
        "stdin_prompt": true,
        "timeout": "12s"
    });
    fs::write(&settings, serde_json::json!({ "agents": { WRAPPED_AGENT: profile } }).to_string())
        .expect("write wrapped settings");
    fixture
}

/// A wrapped profile parks end to end: recognition is keyed on the resolved
/// provider, so a refusal under a declared family is a calm wait rather than a
/// failed attempt, under whatever id the operator gave the entry.
// §FS-rhei-agents.2.3 §FS-rhei-agents.1.1.2
#[test]
fn the_codex_family_parks_on_a_recognized_refusal() {
    let fixture = limit_fixture("family-limit-openai", "codex", "openai");
    let mut run = fixture.start(&[]);
    fixture.parked(&mut run);
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
}
