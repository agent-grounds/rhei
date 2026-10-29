//! What evidence keeps of a prompt and a configuration object: the bytes under
//! `retain`, nothing under `hash-only`, the redactor's output when one is set,
//! and nothing — with the spawn still running — when the redactor fails.
//! §FS-rhei-evidence.5 §FS-rhei-evidence.5.2 §FS-rhei-evidence.5.3
//! §FS-rhei-agents.1.1.9

use super::evidence_support::{
    assert_store_exists, every_file, evidence_workspace, one_start, records, sha256_hex, Profile,
};

/// `hash-only` keeps the hashes of the prompt and of every configuration
/// object, and no bytes of either. §FS-rhei-evidence.5.2
#[test]
fn hash_only_keeps_the_hashes_and_stores_no_prompt_or_configuration() {
    let profile = Profile {
        settings: serde_json::json!({ "evidence": { "retention": "hash-only" } }),
        ..Profile::default()
    };
    let ws = evidence_workspace("evidence-hash-only", profile);
    ws.run();
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);

    let agent = one_start(&evidence, "agent");
    assert_eq!(agent["prompt"]["sha256"], sha256_hex(&ws.seen_prompt("plan.1", "work")));
    assert_eq!(agent["prompt"]["stored"], "none");
    for start in records(&evidence, "start") {
        assert_eq!(start["config"]["stored"], "none", "{start:#}");
        assert_eq!(start["config"]["sha256"].as_str().map(str::len), Some(64), "{start:#}");
    }
    let objects = every_file(&evidence.join("objects"));
    assert!(objects.is_empty(), "hash-only stores no object: {:?}", objects.iter().map(|o| &o.0));
}

/// An unknown retention value is a settings error, not a silent default.
/// §FS-rhei-agents.1.1.9
#[test]
fn an_unknown_retention_value_is_refused() {
    let profile = Profile {
        settings: serde_json::json!({ "evidence": { "retention": "forever" } }),
        ..Profile::default()
    };
    let ws = evidence_workspace("evidence-retention-unknown", profile);
    let result = super::run_cli("run", &ws.plan, &ws.machine, &["--no-tui", "--no-callbacks"]);
    assert!(!result.status.success(), "an unknown retention is refused:\n{}", result.stdout);
    assert!(result.stderr.contains("retention"), "the error names the key:\n{}", result.stderr);
}

#[cfg(unix)]
mod redaction {
    //! The redactor is `snapshots.redactor`, a path to one executable, so these
    //! scenarios run where a script is one: on Unix. §FS-rhei-snapshot-operations.4.2

    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use super::super::evidence_support::{
        assert_store_exists, evidence_workspace, object, one_start, Profile,
    };
    use super::super::python_command;
    use super::sha256_hex;

    const SECRET: &str = "SECRET-7f3a91";

    /// A redactor that replaces [`SECRET`] with `[redacted]`, or one that fails.
    fn write_redactor(dir: &Path, fails: bool) -> String {
        let path = dir.join(if fails { "failing-redactor" } else { "redactor" });
        let body = if fails {
            "import sys\nsys.stdin.buffer.read()\nsys.exit(3)\n".to_string()
        } else {
            format!(
                "import sys\ndata = sys.stdin.buffer.read()\nsys.stdout.buffer.write(data.replace(b'{SECRET}', b'[redacted]'))\n"
            )
        };
        fs::write(&path, format!("#!/usr/bin/env {}\n{body}", python_command()))
            .expect("write redactor");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod redactor");
        path.display().to_string()
    }

    /// The probe workspace with [`SECRET`] in the task body, so in the prompt, and
    /// in an MCP server's URL, so in the agent's configuration object.
    fn secret_workspace(
        prefix: &str,
        fails: bool,
    ) -> super::super::evidence_support::EvidenceWorkspace {
        let mut profile = Profile::default();
        profile.fields["mcp_flag"] = serde_json::json!("--mcp");
        profile.settings = serde_json::json!({
            "mcp_servers": { "hosted": { "url": format!("https://mcp.example/{SECRET}/sse"), "transport": "sse" } }
        });
        let ws = evidence_workspace(prefix, profile);
        let redactor = write_redactor(&ws.root, fails);
        let settings_path = ws.root.join(".agent-grounds/rhei/settings.json");
        let mut settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&settings_path).expect("settings"))
                .expect("JSON");
        settings["snapshots"] = serde_json::json!({ "redactor": redactor });
        fs::write(&settings_path, settings.to_string()).expect("write settings");
        let plan =
            fs::read_to_string(&ws.plan).expect("plan") + &format!("\nThe key is {SECRET}.\n");
        fs::write(&ws.plan, plan).expect("write plan");
        let machine = fs::read_to_string(&ws.machine).expect("machine");
        fs::write(
            &ws.machine,
            machine.replace("    agent: mock\n", "    agent: mock\n    mcp_servers: [hosted]\n"),
        )
        .expect("attach the hosted server");
        ws
    }

    /// The stored copies are the redactor's output; the record keeps the
    /// original's hash and says it holds a redacted copy. §FS-rhei-evidence.5.3
    #[test]
    fn a_redactor_stores_redacted_copies_and_the_record_keeps_the_original_hash() {
        let ws = secret_workspace("evidence-redacted", false);
        ws.run();
        let evidence = ws.evidence_dir();
        assert_store_exists(&evidence);

        let seen = ws.seen_prompt("plan.1", "work");
        assert!(
            String::from_utf8_lossy(&seen).contains(SECRET),
            "fixture: the agent saw the secret"
        );
        let agent = one_start(&evidence, "agent");
        for (field, original) in [("prompt", Some(sha256_hex(&seen))), ("config", None)] {
            let value = &agent[field];
            assert_eq!(value["stored"], "redacted", "{field}: {value:#}");
            if let Some(original) = original {
                assert_eq!(value["sha256"], original, "{field}: the original's hash");
            }
            let stored = value["stored_sha256"].as_str().expect("stored_sha256");
            assert_ne!(value["sha256"].as_str(), Some(stored), "{field}: two different copies");
            let bytes = object(&evidence, stored).expect("the redacted copy is stored");
            let text = String::from_utf8_lossy(&bytes);
            assert!(!text.contains(SECRET) && text.contains("[redacted]"), "{field}: {text}");
            let original = value["sha256"].as_str().expect("sha256");
            assert_eq!(object(&evidence, original), None, "{field}: no original copy is kept");
        }
    }

    /// A redactor that fails stores nothing (`withheld`), and the agent still
    /// runs to its result. §FS-rhei-evidence.5.3
    #[test]
    fn a_failing_redactor_withholds_the_copy_and_the_agent_still_runs() {
        let ws = secret_workspace("evidence-withheld", true);
        ws.run();
        let evidence = ws.evidence_dir();
        assert_store_exists(&evidence);

        assert!(!ws.seen_prompt("plan.1", "work").is_empty(), "the agent still ran");
        let agent = one_start(&evidence, "agent");
        assert_eq!(agent["prompt"]["stored"], "withheld", "{agent:#}");
        assert_eq!(agent["config"]["stored"], "withheld", "{agent:#}");
        let hash = agent["prompt"]["sha256"].as_str().expect("sha256");
        assert_eq!(object(&evidence, hash), None, "nothing is stored for a withheld value");
    }
}
