//! What each process a run starts leaves as evidence: a start and an end
//! record, the prompt's hash and bytes, the real argv and cwd, the hash of the
//! configuration consumed, the CLI version and every log's hash.
//! §FS-rhei-evidence.1 §FS-rhei-evidence.2.1 §FS-rhei-evidence.3
//! §FS-rhei-evidence.4 §FS-rhei-evidence.5.1 §FS-rhei-evidence.6

use std::fs;

use super::evidence_support::{
    assert_nowhere_under, assert_store_exists, assert_valid, end_for, evidence_workspace,
    evidence_workspace_with, object, one_start, only_manifest, records, sha256_hex, Profile,
};
use super::{python_command, run_cli};

/// The triage reproducer of #520, ported: a program, then a stub agent, and
/// every gap it named. §FS-rhei-evidence.3.1 §FS-rhei-evidence.4 §FS-rhei-verify.5
#[test]
fn a_program_then_an_agent_leave_the_evidence_the_reproducer_asks_for() {
    let mut profile = Profile::default();
    profile.fields["version_args"] = serde_json::json!(["--version"]);
    let ws = evidence_workspace("evidence-repro", profile);
    ws.run();
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);

    // 1. The sha256 of the exact prompt the agent read.
    let agent = one_start(&evidence, "agent");
    let seen = ws.seen_prompt("plan.1", "work");
    assert_eq!(agent["prompt"]["sha256"], sha256_hex(&seen), "{agent:#}");
    assert_eq!(agent["prompt"]["delivery"], "stdin");

    // 2. Every log's sha256, taken after exit, in an end record naming it.
    let ends = records(&evidence, "end");
    for entry in fs::read_dir(ws.root.join("runtime/logs")).expect("logs").flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("log") {
            continue;
        }
        let hash = sha256_hex(&fs::read(&path).expect("read log"));
        let relative = format!("runtime/logs/{}", entry.file_name().to_string_lossy());
        assert!(
            ends.iter().any(|end| end["log"]["sha256"] == hash && end["log"]["path"] == relative),
            "no end record carries {relative} at {hash}: {ends:#?}"
        );
    }

    // 3. The program's resolved argv and working directory.
    let program = one_start(&evidence, "program");
    let expected_argv =
        serde_json::json!([python_command(), ws.program.display().to_string(), "--flag"]);
    assert_eq!(program["argv"], expected_argv, "the real list, not the joined label");
    let cwd = program["cwd"].as_str().expect("cwd is a string");
    assert!(std::path::Path::new(cwd).is_absolute(), "cwd is absolute: {cwd}");

    // 4. The configuration hash and the agent CLI version.
    let config = agent["config"]["sha256"].as_str().expect("config.sha256");
    assert_eq!(config.len(), 64, "a full sha256: {config}");
    assert_eq!(agent["agent_cli_version"], "stub-cli 1.2.3");

    // 5. `rhei verify` accepts the workspace.
    let verify = ws.verify(&[]);
    assert!(verify.status.success(), "{}\n{}", verify.stdout, verify.stderr);
}

/// Each process has exactly one start and one end record, each valid against
/// the published schema, under the key its id derives. §FS-rhei-evidence.2.1
/// §FS-rhei-evidence.1.1 §FS-rhei-evidence.9.1
#[test]
fn each_process_leaves_one_start_and_one_end_record_under_its_key() {
    let ws = evidence_workspace("evidence-records", Profile::default());
    ws.run();
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);

    for kind in ["agent", "program"] {
        let start = one_start(&evidence, kind);
        assert_valid("rhei.evidence.invocation.v1", &start);
        let end = end_for(&evidence, &start);
        assert_valid("rhei.evidence.invocation.v1", &end);
        assert_eq!(end["phase"], "end");
        assert_eq!(end["exit_code"], 0);
        assert_eq!(end["ending"], "exited");

        let id = start["invocation_id"].as_str().expect("invocation_id");
        let key = &sha256_hex(id.as_bytes())[..32];
        let run_id = start["run_id"].as_str().expect("run_id");
        let dir = evidence.join("runs").join(run_id);
        assert!(dir.join(format!("{key}.start.json")).is_file(), "{kind}: keyed by {key}");
        assert!(dir.join(format!("{key}.end.json")).is_file(), "{kind}: keyed by {key}");
    }
    let program = one_start(&evidence, "program");
    let id = program["invocation_id"].as_str().expect("program invocation_id");
    assert!(id.starts_with("plan.1::measure::program::visit-"), "shared spelling: {id}");
    assert!(id.contains("::attempt-1"), "shared spelling: {id}");

    let (_, manifest) = only_manifest(&evidence);
    assert_valid("rhei.evidence.manifest.v1", &manifest);
    assert_eq!(manifest["status"], "finished");
    assert_eq!(manifest["invocations"].as_array().map(Vec::len), Some(2), "{manifest:#}");
}

/// The prompt's bytes are retained under their hash, and the stored object is
/// exactly what the agent read on stdin. §FS-rhei-evidence.5 §FS-rhei-evidence.5.2
#[test]
fn the_retained_prompt_is_the_bytes_the_agent_read() {
    let ws = evidence_workspace("evidence-prompt", Profile::default());
    ws.run();
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);

    let agent = one_start(&evidence, "agent");
    let seen = ws.seen_prompt("plan.1", "work");
    let hash = sha256_hex(&seen);
    assert_eq!(agent["prompt"]["sha256"], hash);
    assert_eq!(agent["prompt"]["bytes"], seen.len());
    assert_eq!(agent["prompt"]["stored"], "original");
    assert_eq!(object(&evidence, &hash), Some(seen), "the object holds the prompt's bytes");
}

/// A prompt passed with `prompt_flag` records `delivery: argument`, shows
/// `<prompt>` in argv in its place, and hashes the same bytes.
/// §FS-rhei-evidence.5.1 §FS-rhei-evidence.3.2
#[test]
fn a_prompt_passed_as_an_argument_is_a_placeholder_in_argv() {
    let profile = Profile {
        fields: serde_json::json!({ "prompt_flag": "--prompt", "timeout": "60s" }),
        ..Profile::default()
    };
    let ws = evidence_workspace("evidence-prompt-arg", profile);
    ws.run();
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);

    let agent = one_start(&evidence, "agent");
    assert_eq!(agent["prompt"]["delivery"], "argument");
    assert_eq!(agent["prompt"]["sha256"], sha256_hex(&ws.seen_prompt("plan.1", "work")));
    let argv: Vec<&str> = agent["argv"]
        .as_array()
        .expect("argv")
        .iter()
        .map(|arg| arg.as_str().expect("string arg"))
        .collect();
    let at = argv.iter().position(|arg| *arg == "--prompt").expect("the prompt flag");
    assert_eq!(argv.get(at + 1), Some(&"<prompt>"), "argv: {argv:?}");
    assert!(!argv.iter().any(|arg| arg.starts_with("# Task ")), "no prompt text in argv");
}

/// `config.sha256` is the sha256 of the stored configuration object, which is
/// one canonical JSON document. §FS-rhei-evidence.6.2 §FS-rhei-evidence.1.2
#[test]
fn the_configuration_hash_is_the_hash_of_the_stored_object() {
    let ws = evidence_workspace("evidence-config", Profile::default());
    ws.run();
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);

    for kind in ["agent", "program"] {
        let start = one_start(&evidence, kind);
        let hash = start["config"]["sha256"].as_str().expect("config.sha256").to_string();
        assert_eq!(start["config"]["stored"], "original", "{kind}: {start:#}");
        let bytes = object(&evidence, &hash).expect("the configuration object is stored");
        assert_eq!(sha256_hex(&bytes), hash, "{kind}: sha256sum recomputes it");
        assert_eq!(start["config"]["bytes"], bytes.len());
        let text = String::from_utf8(bytes).expect("UTF-8");
        assert!(text.ends_with('\n') && !text.trim_end_matches('\n').contains('\n'), "{text}");
        let value: serde_json::Value = serde_json::from_str(&text).expect("JSON");
        assert!(value.is_object(), "{kind}: one object: {value:#}");
    }
}

/// Every `env` map is a sorted list of names: no value, and no hash of a
/// value, appears anywhere under `runtime/evidence/`. §FS-rhei-evidence.6.1
#[test]
fn env_maps_are_recorded_as_names_without_values_or_their_hashes() {
    const PROGRAM_SECRET: &str = "program-secret-0b5e7d";
    const MCP_SECRET: &str = "mcp-secret-91c4aa";
    let mut profile = Profile::default();
    profile.fields["mcp_flag"] = serde_json::json!("--mcp");
    profile.settings = serde_json::json!({
        "mcp_servers": {
            "probe": {
                "command": [python_command(), "-c", "pass"],
                "env": { "ZETA_TOKEN": MCP_SECRET, "ALPHA_HOST": "localhost" }
            }
        }
    });
    let extra = format!("      env:\n        PROGRAM_TOKEN: \"{PROGRAM_SECRET}\"\n");
    let ws = evidence_workspace_with("evidence-env-names", profile, &extra);
    let machine = fs::read_to_string(&ws.machine).expect("machine");
    fs::write(
        &ws.machine,
        machine.replace("    agent: mock\n", "    agent: mock\n    mcp_servers: [probe]\n"),
    )
    .expect("attach the MCP server");
    let result = run_cli("run", &ws.plan, &ws.machine, &["--no-tui", "--no-callbacks"]);
    assert!(result.status.success(), "fixture run: {}\n{}", result.stdout, result.stderr);

    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);
    for secret in [PROGRAM_SECRET, MCP_SECRET, "localhost"] {
        assert_nowhere_under(&evidence, secret, "an env value");
        assert_nowhere_under(&evidence, &sha256_hex(secret.as_bytes()), "the hash of an env value");
    }

    let program = one_start(&evidence, "program");
    let program_config = object(&evidence, program["config"]["sha256"].as_str().expect("hash"))
        .expect("program configuration stored");
    let program_config: serde_json::Value =
        serde_json::from_slice(&program_config).expect("program configuration JSON");
    assert_eq!(program_config["program"]["env"], serde_json::json!(["PROGRAM_TOKEN"]));

    let agent = one_start(&evidence, "agent");
    let agent_config = object(&evidence, agent["config"]["sha256"].as_str().expect("hash"))
        .expect("agent configuration stored");
    let text = String::from_utf8(agent_config).expect("UTF-8");
    assert!(text.contains(r#"["ALPHA_HOST","ZETA_TOKEN"]"#), "sorted names only: {text}");
}
