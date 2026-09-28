//! A wrapped agent is measured under its own id once its profile names the
//! built-in family it belongs to — and a profile that names none is unchanged.
//!
//! The first case is this ticket's reproducer: fourteen states ran on a wrapped
//! Claude Code, all fourteen worked, and `runtime/accounting/` was never
//! created, because extractor resolution was keyed on the registry id and
//! nothing else. The second is the floor the whole change rests on.
// §FS-rhei-agents.1.1.2 §FS-rhei-cost-accounting.3 §FS-rhei-cost-accounting.3.2
// §FS-rhei-cost-accounting.4

use super::agent_family_support::*;
use super::*;

const UNMEASURED: &str = "Token accounting was not measured for this run.";

/// The wrapped run is measured: the capture contract reached it, its family's
/// extractor read its stream, and the record names both the profile and the
/// family it belongs to.
// §FS-rhei-agents.1.1.2 §FS-rhei-cost-accounting.3 §FS-rhei-cost-accounting.4
#[test]
fn a_declared_family_measures_the_wrapped_run() {
    let workspace = wrapped_workspace(
        "family-measured",
        CODEX_DIALECT_AGENT,
        serde_json::json!({ "family": "codex" }),
        "openai",
        "gpt-contract",
    );
    let run = run_cli("run", &workspace.plan, &workspace.machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&run);

    // The capture path is set only for an invocation whose family has an
    // extractor, so its existence is the gate the ticket reports closed.
    // §FS-rhei-cost-accounting.4
    assert_eq!(
        workspace.runtime_files("runtime/accounting/captures", "jsonl").len(),
        1,
        "the wrapped run should have written one usage capture"
    );

    let record = one_record(&workspace);
    assert_eq!(record["agent"], WRAPPED_AGENT, "the record names the profile's own id");
    assert_eq!(record["agent_family"], "codex", "beside the family it resolved to");
    assert_eq!(record["extraction_status"], "measured");
    // Only the Codex extractor reads a `thread.started` event, so the session
    // identity proves the family selected the extractor rather than merely
    // enabling a record. §FS-rhei-cost-accounting.4
    assert_eq!(record["cli_session"]["id"], "thread-family-332");
    assert_eq!(record["tokens"]["input"]["total"]["value"], 120);
    assert_eq!(record["tokens"]["output"]["total"]["value"], 30);

    let summary = run_cli("summary", &workspace.plan, &workspace.machine, &["--details"]);
    assert_success(&summary);
    assert!(
        !summary.stdout.contains(UNMEASURED),
        "the run was measured, so the summary must not say it was not:\n{}",
        summary.stdout
    );
    assert!(
        summary.stdout.contains("1 agent invocation across 1 model"),
        "the summary should count the wrapped invocation:\n{}",
        summary.stdout
    );
    assert!(
        summary.stdout.contains("| Accounting | Value |"),
        "and print the token table rather than the unmeasured line:\n{}",
        summary.stdout
    );
}

/// The compatibility floor: the same script under the same id, declaring no
/// family, is exactly as unmeasured as it is today. Nine behavioural branches
/// move underneath this promise, so it is a test rather than a claim.
// §FS-rhei-agents.1.1.2 §FS-rhei-cost-accounting.3.2
#[test]
fn a_profile_without_a_family_stays_unmeasured() {
    let workspace = wrapped_workspace(
        "family-absent",
        CODEX_DIALECT_AGENT,
        serde_json::json!({}),
        "openai",
        "gpt-contract",
    );
    let run = run_cli("run", &workspace.plan, &workspace.machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&run);

    assert_missing(
        &workspace.accounting_root(),
        "a custom agent with no family has no extractor, so nothing is accounted",
    );
    assert!(workspace.records().is_empty());

    let summary = run_cli("summary", &workspace.plan, &workspace.machine, &["--details"]);
    assert_success(&summary);
    assert!(
        summary.stdout.contains(UNMEASURED),
        "an unmeasured run still says so:\n{}",
        summary.stdout
    );
}

/// `rhei roster` is where an operator reads what a family supplied: the
/// resolved profile, with `family` in it, under one origin for the entry.
// §FS-rhei-agents.1.1.7 §FS-rhei-agents.1.1.2
#[test]
fn the_roster_reports_the_resolved_profile_of_a_family() {
    let workspace = wrapped_workspace(
        "family-roster",
        CODEX_DIALECT_AGENT,
        serde_json::json!({ "family": "codex" }),
        "openai",
        "gpt-contract",
    );
    let roster = run_cli("roster", &workspace.plan, &workspace.machine, &["--json"]);
    assert_success(&roster);
    let roster: serde_json::Value =
        serde_json::from_str(&roster.stdout).expect("roster JSON parses");
    let profile = &roster["agents"][WRAPPED_AGENT];

    assert_eq!(profile["family"], "codex", "the resolved profile carries its family");
    // Written wins: the wrapper's own command is not the built-in's.
    assert_eq!(profile["command"][0], python_command());
    // Omitted inherits: `--mcp` is the `codex` built-in's MCP wiring, and the
    // wrapper never wrote it. §FS-rhei-agents.1.1.2
    assert_eq!(profile["mcp_flag"], "--mcp");
    assert_eq!(profile["modes"]["yolo"][0], "--sandbox");
    // One origin for the whole entry, and no new provenance leaf.
    assert_eq!(roster["provenance"]["agents"][WRAPPED_AGENT], "project");
    assert_eq!(roster["schema_version"], 1);
}

/// An unknown family is a settings error, raised where the registry's other
/// self-validation is raised and naming the ids that are families.
// §FS-rhei-agents.1.1.2
#[test]
fn an_unknown_family_is_refused_and_names_the_known_ones() {
    let workspace = wrapped_workspace(
        "family-unknown",
        CODEX_DIALECT_AGENT,
        serde_json::json!({ "family": "claude" }),
        "anthropic",
        "claude-opus-5",
    );
    let validate = run_cli("validate", &workspace.plan, &workspace.machine, &[]);
    assert!(
        !validate.status.success(),
        "a family that is not a built-in id must be refused\nstdout:\n{}\nstderr:\n{}",
        validate.stdout,
        validate.stderr
    );
    assert_stderr_contains(&validate, "agent 'cld' declares family 'claude'");
    for known in ["claude-code", "codex", "gemini", "cursor", "kilocode", "pi"] {
        assert_stderr_contains(&validate, known);
    }
}
