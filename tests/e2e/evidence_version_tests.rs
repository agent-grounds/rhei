//! The agent CLI version an invocation ran with: `version_args` appended to the
//! profile's whole command, probed once per run per command and again when the
//! executable changes, and `null` with a note when there is none.
//! §FS-rhei-evidence.7 §FS-rhei-agents.1.1.2

use std::fs;

use super::evidence_support::{
    assert_store_exists, evidence_workspace, one_start, records, EvidenceWorkspace, Profile,
};
use super::python_command;

/// Two tickets that both start on the `mock` agent.
const TWO_TICKETS: &str = r#"# Rhei: Two probes

## Tasks

### Task 1: First
**State:** work

### Task 2: Second
**State:** work
"#;

const TWO_TICKETS_MACHINE: &str = r#"name: two-probes
version: 1
states:
  work:
    initial: true
    description: An agent step
    agent: mock
    agent_timeout: 60s
    attempts: 1
  done:
    description: Done
    final: true
transitions:
  - { from: work, to: done, description: Worked }
"#;

/// The lines the stub appended for each `--version` it answered.
fn probes(ws: &EvidenceWorkspace) -> Vec<String> {
    fs::read_to_string(ws.root.join("version-probes.txt"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// One distinct command is probed once for the whole run, and both spawns
/// record what it answered. §FS-rhei-evidence.7
#[test]
#[ignore = "red until #520: no agent CLI version is probed or recorded; implement removes this"]
fn one_command_is_probed_once_for_two_spawns() {
    let mut profile = Profile::default();
    profile.fields["version_args"] = serde_json::json!(["--version"]);
    let ws = evidence_workspace("evidence-version-once", profile);
    fs::write(&ws.plan, TWO_TICKETS).expect("write plan");
    fs::write(&ws.machine, TWO_TICKETS_MACHINE).expect("write machine");
    ws.run();
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);

    let agents: Vec<_> =
        records(&evidence, "start").into_iter().filter(|r| r["kind"] == "agent").collect();
    assert_eq!(agents.len(), 2, "{agents:#?}");
    for agent in &agents {
        assert_eq!(agent["agent_cli_version"], "stub-cli 1.2.3", "{agent:#}");
    }
    assert_eq!(probes(&ws), ["--version"], "probed once, on the whole command");
}

/// A profile with no `version_args` and no family records `null` and says why.
/// §FS-rhei-evidence.7
#[test]
#[ignore = "red until #520: no start record carries agent_cli_version; implement removes this"]
fn a_profile_without_version_args_records_null_with_a_note() {
    let ws = evidence_workspace("evidence-version-none", Profile::default());
    ws.run();
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);

    let agent = one_start(&evidence, "agent");
    assert!(agent["agent_cli_version"].is_null(), "{agent:#}");
    assert_eq!(agent["agent_cli_version_note"], "no version_args");
    assert!(probes(&ws).is_empty(), "nothing was probed");
}

/// The command a built-in's `family` profile runs is asked, not its
/// interpreter: `[python, stub, exec]` answers `stub exec --version`, where
/// probing the first element alone would record Python's version.
fn assert_built_in_probes_its_whole_command(family: &str, trailing: &[&str], expected_probe: &str) {
    let ws = evidence_workspace(&format!("evidence-version-{family}"), Profile::default());
    let mut command = vec![python_command().to_string(), ws.agent.display().to_string()];
    command.extend(trailing.iter().map(|arg| (*arg).to_string()));
    let settings_path = ws.root.join(".agent-grounds/rhei/settings.json");
    let mut settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&settings_path).expect("settings")).expect("JSON");
    settings["agents"]["mock"] = serde_json::json!({
        "family": family, "command": command, "stdin_prompt": true, "timeout": "60s",
    });
    fs::write(&settings_path, settings.to_string()).expect("write settings");

    let roster = ws.rhei(&["roster", "--json"]);
    assert!(roster.status.success(), "{}", roster.stderr);
    let roster: serde_json::Value = serde_json::from_str(&roster.stdout).expect("roster JSON");
    assert_eq!(
        roster["agents"][family]["version_args"],
        serde_json::json!(["--version"]),
        "the built-in `{family}` declares a probe its own command accepts"
    );

    ws.run();
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);
    let agent = one_start(&evidence, "agent");
    assert_eq!(agent["agent_cli_version"], "stub-cli 1.2.3", "not the interpreter's: {agent:#}");
    assert_eq!(probes(&ws), [expected_probe], "the whole command was asked");
}

/// `codex exec --version`: the codex built-in's command is `codex exec`, and
/// its probe is appended after `exec`. §FS-rhei-evidence.7
#[test]
#[ignore = "red until #520: built-ins declare no version_args and nothing is probed; implement removes this"]
fn the_codex_built_in_probes_its_whole_command() {
    assert_built_in_probes_its_whole_command("codex", &["exec"], "exec --version");
}

/// `claude --version`, inherited by a `family: claude-code` wrapper.
/// §FS-rhei-evidence.7
#[test]
#[ignore = "red until #520: built-ins declare no version_args and nothing is probed; implement removes this"]
fn the_claude_code_built_in_probes_its_whole_command() {
    assert_built_in_probes_its_whole_command("claude-code", &[], "--version");
}

/// The probe is taken again when the resolved executable changes mid-run.
/// The stub must be the executable itself for its edit to count, so this runs
/// where a script is one: on Unix. §FS-rhei-evidence.7
#[cfg(unix)]
#[test]
#[ignore = "red until #520: no agent CLI version is probed or recorded; implement removes this"]
fn a_changed_executable_is_probed_again_mid_run() {
    use std::os::unix::fs::PermissionsExt;

    let ws = evidence_workspace("evidence-version-changed", Profile::default());
    let script = fs::read_to_string(&ws.agent).expect("stub");
    fs::write(&ws.agent, format!("#!/usr/bin/env {}\n{script}", python_command()))
        .expect("shebang");
    fs::set_permissions(&ws.agent, fs::Permissions::from_mode(0o755)).expect("chmod stub");
    let settings_path = ws.root.join(".agent-grounds/rhei/settings.json");
    let mut settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&settings_path).expect("settings")).expect("JSON");
    settings["agents"]["mock"]["command"] = serde_json::json!([ws.agent.display().to_string()]);
    settings["agents"]["mock"]["version_args"] = serde_json::json!(["--version"]);
    fs::write(&settings_path, settings.to_string()).expect("write settings");

    // Between the two agent steps a program grows the stub and changes its answer.
    let retouch = super::write_python_agent(
        &ws.root,
        "retouch.py",
        &format!(
            "append({:?}, '\\n# retouched\\n')\nwrite({:?}, 'stub-cli 2.0.0\\n')\nresult('## Result\\n\\nRetouched.\\n')\n",
            ws.agent.display().to_string(),
            ws.root.join("stub-version.txt").display().to_string(),
        ),
    );
    let machine = format!(
        r#"name: version-changed
version: 1
states:
  work:
    initial: true
    description: First agent step
    agent: mock
    agent_timeout: 60s
    attempts: 1
  retouch:
    description: Change the stub
    program:
      command: {}
    program_timeout: 60s
  rework:
    description: Second agent step
    agent: mock
    agent_timeout: 60s
    attempts: 1
  done:
    description: Done
    final: true
transitions:
  - {{ from: work, to: retouch, description: Worked }}
  - {{ from: retouch, to: rework, description: Retouched, exit_code: 0 }}
  - {{ from: rework, to: done, description: Reworked }}
"#,
        super::fixture_command(&retouch)
    );
    fs::write(&ws.machine, machine).expect("write machine");
    fs::write(&ws.plan, "# Rhei: Changed\n\n## Tasks\n\n### Task 1: Probe\n**State:** work\n")
        .expect("write plan");
    ws.run();
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);

    let mut versions: Vec<(String, String)> = records(&evidence, "start")
        .into_iter()
        .filter(|r| r["kind"] == "agent")
        .map(|r| {
            (r["state"].as_str().unwrap_or("").to_string(), r["agent_cli_version"].to_string())
        })
        .collect();
    versions.sort();
    assert_eq!(
        versions,
        [
            ("rework".to_string(), "\"stub-cli 2.0.0\"".to_string()),
            ("work".to_string(), "\"stub-cli 1.2.3\"".to_string())
        ]
    );
    assert_eq!(probes(&ws).len(), 2, "probed again after the change");
}
