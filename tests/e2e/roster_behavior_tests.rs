//! CLI, diagnostics, streams, and side-effect contract for roster inspection.
//! §FS-rhei-agents.1.1.7

use std::fs;
use std::process::Stdio;

use super::roster_support::*;
use super::*;

fn valid_plan(root: &std::path::Path) -> std::path::PathBuf {
    write_plan(
        root,
        "plan.rhei.md",
        "# Rhei: Roster behavior\n\n## Tasks\n\n### Task 1: Inert\n**State:** completed\n",
    )
}

/// Roster is one top-level command with one optional plan and `--json`; run
/// narrowing and execution overrides are deliberately outside this surface.
/// §FS-rhei-agents.1.1.7 §FS-rhei-usage.2.2
#[test]
fn roster_help_documents_the_surface_and_rejects_narrowing_or_run_overrides() {
    let root = unique_temp_dir("roster-help");
    let home = root.join("home");
    let plan = valid_plan(&root);

    let help = run_roster_args(&home, &root, &["--help"]);
    assert_success(&help);
    assert!(
        help.stdout.contains("Usage:")
            && help.stdout.contains("rhei roster")
            && help.stdout.contains("[RHEI_PLAN]"),
        "help was:\n{}",
        help.stdout
    );
    assert!(help.stdout.contains("--json"), "help was:\n{}", help.stdout);
    for forbidden in ["--rhei", "--agent", "--agent-mode", "--model"] {
        assert!(!help.stdout.contains(forbidden), "roster help exposed {forbidden}");
        let target = plan.display().to_string();
        let rejected = run_roster_args(&home, &root, &[&target, forbidden, "value"]);
        assert!(!rejected.status.success(), "roster unexpectedly accepted {forbidden}");
    }
    let target = plan.display().to_string();
    let rejected = run_roster_args(&home, &root, &[&target, &target]);
    assert!(!rejected.status.success(), "roster accepted a second positional target");
}

/// A selected malformed settings file fails before stdout and keeps the
/// machine-readable error-with-help contract on stderr.
/// §FS-rhei-agents.1.1.7 §FS-rhei-usage.2.2
#[test]
fn roster_invalid_settings_emit_one_json_error_and_no_partial_payload() {
    let root = unique_temp_dir("roster-invalid-settings");
    let home = root.join("home");
    let plan = valid_plan(&root);
    let settings = write_settings(&root, CURRENT_SETTINGS, "{ not valid JSON");

    let text = run_roster(&home, &root, Some(&plan), false);
    assert!(!text.status.success(), "invalid settings must be refused in text mode");
    assert!(text.stdout.is_empty(), "text failure emitted a partial roster: {}", text.stdout);
    assert!(text.stderr.contains("failed to parse settings"));

    let result = run_roster(&home, &root, Some(&plan), true);
    assert!(!result.status.success(), "invalid settings must be refused");
    assert!(result.stdout.is_empty(), "failure emitted a partial roster: {}", result.stdout);
    let error: serde_json::Value =
        serde_json::from_str(result.stderr.trim()).unwrap_or_else(|why| {
            panic!("stderr must be one JSON error object: {why}\n{}", result.stderr)
        });
    assert!(
        error["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("failed to parse settings")),
        "JSON error did not classify the selected settings: {error:#}"
    );
    assert!(
        error["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains(&settings.display().to_string())),
        "JSON error did not name the selected file: {error:#}"
    );
    assert!(error["error"]["help"].as_str().is_some(), "JSON error omitted help: {error:#}");
}

/// A selected path that cannot be read as a file has the same atomic output
/// boundary as malformed contents. A directory is a portable unreadable-file
/// fixture and does not depend on Unix permission bits. §FS-rhei-agents.1.1.7
#[test]
fn roster_unreadable_settings_emit_one_json_error_and_no_partial_payload() {
    let root = unique_temp_dir("roster-unreadable-settings");
    let home = root.join("home");
    let plan = valid_plan(&root);
    let settings = root.join(CURRENT_SETTINGS);
    fs::create_dir_all(&settings).expect("create a directory where the settings file belongs");

    let result = run_roster(&home, &root, Some(&plan), true);
    assert!(!result.status.success(), "unreadable settings must be refused");
    assert!(result.stdout.is_empty(), "failure emitted a partial roster: {}", result.stdout);
    let error: serde_json::Value =
        serde_json::from_str(result.stderr.trim()).unwrap_or_else(|why| {
            panic!("stderr must be one JSON error object: {why}\n{}", result.stderr)
        });
    assert!(
        error["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("failed to read settings")),
        "JSON error did not classify the selected settings: {error:#}"
    );
    assert!(
        error["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains(&settings.display().to_string())),
        "JSON error did not name the selected file: {error:#}"
    );
    assert!(error["error"]["help"].as_str().is_some(), "JSON error omitted help: {error:#}");
}

/// A deprecated-home warning must not prefix the JSON parse failure.
/// §FS-rhei-agents.1.1.7 §FS-rhei-usage.2.2
#[test]
fn roster_malformed_deprecated_settings_emit_only_one_json_error() {
    let root = unique_temp_dir("roster-malformed-deprecated-settings");
    let home = root.join("home");
    let plan = valid_plan(&root);
    let settings = write_settings(&root, DEPRECATED_SETTINGS, "{ not valid JSON");

    let result = run_roster(&home, &root, Some(&plan), true);
    assert!(!result.status.success(), "malformed settings must be refused");
    assert!(result.stdout.is_empty(), "failure emitted a partial roster: {}", result.stdout);
    let error: serde_json::Value = serde_json::from_str(result.stderr.trim())
        .unwrap_or_else(|why| panic!("stderr must be one JSON object: {why}\n{}", result.stderr));
    assert!(
        error["error"]["message"].as_str().is_some_and(|message| {
            message.contains("failed to parse settings")
                && message.contains(&settings.display().to_string())
        }),
        "JSON error did not identify the malformed deprecated file: {error:#}"
    );
    assert!(error["error"]["help"].as_str().is_some(), "JSON error omitted help: {error:#}");
}

/// Intrinsic validation also completes before a deprecated warning is emitted,
/// leaving the complete stderr stream as one JSON object on failure.
/// §FS-rhei-agents.1.1.7 §FS-rhei-usage.2.2
#[test]
fn roster_invalid_deprecated_settings_emit_only_one_json_error() {
    let root = unique_temp_dir("roster-invalid-deprecated-settings");
    let home = root.join("home");
    let plan = valid_plan(&root);
    write_settings(&root, DEPRECATED_SETTINGS, r#"{"agents":{"invalid":{"command":[]}}}"#);

    let result = run_roster(&home, &root, Some(&plan), true);
    assert!(!result.status.success(), "intrinsically invalid settings must be refused");
    assert!(result.stdout.is_empty(), "failure emitted a partial roster: {}", result.stdout);
    let error: serde_json::Value = serde_json::from_str(result.stderr.trim())
        .unwrap_or_else(|why| panic!("stderr must be one JSON object: {why}\n{}", result.stderr));
    assert!(
        error["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("invalid merged settings")
                && message.contains("empty 'command'")),
        "JSON error did not report intrinsic validation: {error:#}"
    );
    assert!(error["error"]["help"].as_str().is_some(), "JSON error omitted help: {error:#}");
}

/// Text is a compact, ordered human summary: sources and defaults first,
/// agents and modes next, models and binding field origins last.
/// §FS-rhei-agents.1.1.7
#[test]
fn roster_text_summarizes_sources_defaults_agents_modes_models_and_bindings() {
    let root = unique_temp_dir("roster-text");
    let home = root.join("home");
    let plan = valid_plan(&root);
    write_settings(
        &root,
        CURRENT_SETTINGS,
        r#"{
  "agents": { "runner": { "command": ["runner"], "modes": { "safe": ["--safe"] } } },
  "models": {
    "review": {
      "provider": "openai",
      "model": "gpt-test",
      "default_agent": "runner",
      "agents": { "runner": { "timeout": "5m" } }
    }
  },
  "defaults": { "model": "review", "agent": "runner" }
}"#,
    );

    let result = run_roster(&home, &root, Some(&plan), false);
    assert_success(&result);
    let positions = ["Sources", "Defaults", "Agents", "Models"].map(|heading| {
        result
            .stdout
            .find(heading)
            .unwrap_or_else(|| panic!("missing {heading}:\n{}", result.stdout))
    });
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "sections out of order");
    for text in ["current", "review", "runner", "safe", "openai", "gpt-test", "timeout", "project"]
    {
        assert!(result.stdout.contains(text), "text roster omitted {text:?}:\n{}", result.stdout);
    }
    assert!(result.stderr.is_empty(), "text diagnostics leaked: {}", result.stderr);
}

/// Both renderings only inspect settings. Even malformed task markdown must
/// remain byte-for-byte untouched and no runtime tree may appear.
/// §FS-rhei-agents.1.1.7
#[test]
fn roster_text_and_json_leave_the_filesystem_unchanged_and_spawn_nothing() {
    let root = unique_temp_dir("roster-read-only");
    let home = root.join("home");
    let plan = write_plan(
        &root,
        "plan.rhei.md",
        "# Rhei: Broken body\n\n## Tasks\n\n### Task nope: malformed\n**State:** pending\n",
    );
    write_settings(
        &root,
        CURRENT_SETTINGS,
        r#"{"agents":{"must-not-spawn":{"command":["definitely-not-a-command"]}}}"#,
    );

    // Constructing any test command creates the isolated state-home directory;
    // do that once before the behavior snapshot.
    drop(rhei_command(&home));
    let before = tree_snapshot(&root);
    for json in [false, true] {
        let result = run_roster(&home, &root, Some(&plan), json);
        assert_success(&result);
    }
    assert_eq!(tree_snapshot(&root), before, "roster changed the fixture tree");
    assert!(!root.join("runtime").exists(), "roster created a runtime tree");
}

/// Closing the consumer end of stdout is ordinary pipeline termination in
/// both renderings, never a panic or diagnostic. §FS-rhei-usage.2.2
#[test]
fn roster_treats_early_stdout_closure_as_success() {
    let root = unique_temp_dir("roster-broken-pipe");
    let home = root.join("home");
    let plan = valid_plan(&root);
    for json in [false, true] {
        let mut command = rhei_command(&home);
        command.current_dir(&root).arg("roster").arg(&plan);
        if json {
            command.arg("--json");
        }
        let mut child =
            command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("spawn roster");
        drop(child.stdout.take());
        let output = child.wait_with_output().expect("wait for roster");
        let result = CliRun::from(&output);
        assert!(
            result.status.success(),
            "early stdout closure must be success; json={json}\nstderr:\n{}",
            result.stderr
        );
        assert!(result.stderr.is_empty(), "broken pipe emitted a diagnostic: {}", result.stderr);
    }
}
