//! Black-box JSON contract for the effective roster. §FS-rhei-agents.1.1.7

use std::collections::BTreeSet;

use serde_json::{json, Value};

use super::roster_support::*;
use super::*;

fn keys(value: &Value) -> BTreeSet<&str> {
    value.as_object().expect("JSON value should be an object").keys().map(String::as_str).collect()
}

/// The zero-settings result is enough for a dispatcher to discover every
/// built-in transport and its complete named-mode map without copying Rhei.
/// §FS-rhei-agents.1.1.7 §FS-rhei-agents.2.2
#[test]
fn roster_json_exposes_the_versioned_shape_and_complete_built_in_modes() {
    let root = unique_temp_dir("roster-built-ins");
    let home = root.join("home");
    let plan = write_plan(
        &root,
        "plan.rhei.md",
        "# Rhei: Built-ins\n\n## Tasks\n\n### Task 1: Inert\n**State:** completed\n",
    );

    let result = run_roster(&home, &root, Some(&plan), true);
    let payload = roster_json(&result);

    assert_eq!(
        keys(&payload),
        BTreeSet::from([
            "agents",
            "defaults",
            "models",
            "project_root",
            "provenance",
            "schema_version",
            "sources",
        ])
    );
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["project_root"], resolved(&root));
    assert_eq!(payload["sources"]["built_in"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(payload["sources"]["global"].is_null());
    assert!(payload["sources"]["project"].is_null());
    assert_eq!(payload["models"], json!({}));
    assert_eq!(payload["defaults"], json!({}));
    assert_eq!(payload["provenance"]["defaults"], json!({}));

    assert_eq!(
        keys(&payload["agents"]),
        BTreeSet::from(["claude-code", "codex", "cursor", "gemini", "kilocode", "pi"])
    );
    assert_eq!(
        payload["agents"]["claude-code"]["modes"],
        json!({"yolo": ["--permission-mode", "bypassPermissions"]})
    );
    assert_eq!(
        payload["agents"]["codex"]["modes"],
        json!({
            "yolo": [
                "--sandbox",
                "danger-full-access",
                "--skip-git-repo-check",
                "-c",
                "approval_policy=\"never\""
            ]
        })
    );
    assert_eq!(payload["agents"]["cursor"]["modes"], json!({"yolo": ["--force"]}));
    assert_eq!(payload["agents"]["gemini"]["modes"], json!({"yolo": ["--approval-mode", "yolo"]}));
    assert_eq!(payload["agents"]["kilocode"]["modes"], json!({"yolo": ["--yolo"]}));
    assert!(
        payload["agents"]["pi"].get("modes").is_none(),
        "an optional mode map never supplied by a layer is omitted"
    );
    for id in ["claude-code", "codex", "cursor", "gemini", "kilocode", "pi"] {
        assert_eq!(payload["provenance"]["agents"][id], "built_in");
    }
    assert!(result.stderr.is_empty(), "no settings warning was selected: {}", result.stderr);
}

/// Global and project values are merged once, with provenance at exactly the
/// units that merge independently. Clears stay distinct from omission.
/// §FS-rhei-agents.1.1.7 §FS-rhei-agents.1.3
#[test]
fn roster_json_reports_layered_values_clears_and_field_provenance() {
    let root = unique_temp_dir("roster-layered");
    let home = root.join("home");
    let plan = write_plan(
        &root,
        "plan.rhei.md",
        "# Rhei: Layered roster\n\n## Tasks\n\n### Task 1: Inert\n**State:** completed\n",
    );
    let global = write_settings(
        &home,
        ".config/rhei/settings.json",
        r#"{
  "agents": {
    "global-only": { "command": ["global-only"], "modes": { "safe": ["--safe"] } },
    "shared": { "command": ["global-shared"], "prompt_flag": "--prompt", "modes": { "safe": ["--safe"] } }
  },
  "models": {
    "review": {
      "provider": "openai",
      "model": "gpt-global",
      "default_agent": "shared",
      "agents": {
        "shared": {
          "args": ["--global"],
          "autonomous_args": ["--global-auto"],
          "timeout": "10m"
        }
      }
    },
    "sparse": { "provider": "local", "model": "small", "agents": { "shared": {} } }
  },
  "defaults": {
    "model": "review",
    "agent": "global-only",
    "agent_mode": "safe",
    "agent_timeout": "30m",
    "program_timeout": "10m",
    "attempts": 3,
    "mcp_servers": ["global-mcp"],
    "skills": ["global-skill"]
  }
}"#,
    );
    let project = write_settings(
        &root,
        CURRENT_SETTINGS,
        r#"{
  "agents": {
    "project-only": { "command": ["project-only"] },
    "shared": { "command": ["project-shared"], "modes": { "review": ["--review"] } }
  },
  "models": {
    "review": {
      "model": "gpt-project",
      "agents": { "shared": { "args": [], "timeout": null } }
    }
  },
  "defaults": { "agent": null, "agent_mode": null, "skills": [] }
}"#,
    );

    let result = run_roster(&home, &root, Some(&plan), true);
    let payload = roster_json(&result);

    assert_eq!(payload["sources"]["global"]["path"], resolved(&global));
    assert_eq!(payload["sources"]["project"]["path"], resolved(&project));
    assert_eq!(payload["sources"]["project"]["home"], "current");
    assert_eq!(payload["agents"]["global-only"]["command"], json!(["global-only"]));
    assert_eq!(payload["agents"]["project-only"]["command"], json!(["project-only"]));
    assert_eq!(
        payload["agents"]["shared"],
        json!({"command": ["project-shared"], "modes": {"review": ["--review"]}}),
        "the project agent replaces its global namesake wholesale"
    );
    assert_eq!(payload["provenance"]["agents"]["global-only"], "global");
    assert_eq!(payload["provenance"]["agents"]["project-only"], "project");
    assert_eq!(payload["provenance"]["agents"]["shared"], "project");

    assert_eq!(payload["models"]["review"]["provider"], "openai");
    assert_eq!(payload["models"]["review"]["model"], "gpt-project");
    assert_eq!(payload["models"]["review"]["default_agent"], "shared");
    assert_eq!(payload["models"]["review"]["agents"]["shared"]["args"], json!([]));
    assert_eq!(
        payload["models"]["review"]["agents"]["shared"]["autonomous_args"],
        json!(["--global-auto"])
    );
    assert!(payload["models"]["review"]["agents"]["shared"]["timeout"].is_null());
    assert_eq!(payload["provenance"]["models"]["review"]["provider"], "global");
    assert_eq!(payload["provenance"]["models"]["review"]["model"], "project");
    assert_eq!(payload["provenance"]["models"]["review"]["default_agent"], "global");
    assert_eq!(
        payload["provenance"]["models"]["review"]["agents"]["shared"],
        json!({
            "args": "project",
            "autonomous_args": "global",
            "timeout": "project"
        })
    );
    assert_eq!(payload["models"]["sparse"]["agents"]["shared"], json!({}));
    assert_eq!(payload["provenance"]["models"]["sparse"]["agents"]["shared"], json!({}));

    assert_eq!(payload["defaults"]["model"], "review");
    assert!(payload["defaults"]["agent"].is_null());
    assert!(payload["defaults"]["agent_mode"].is_null());
    assert_eq!(payload["defaults"]["mcp_servers"], json!(["global-mcp"]));
    assert_eq!(payload["defaults"]["skills"], json!([]));
    assert_eq!(payload["provenance"]["defaults"]["model"], "global");
    assert_eq!(payload["provenance"]["defaults"]["agent"], "project");
    assert_eq!(payload["provenance"]["defaults"]["agent_mode"], "project");
    assert_eq!(payload["provenance"]["defaults"]["mcp_servers"], "global");
    assert_eq!(payload["provenance"]["defaults"]["skills"], "project");
    assert!(result.stderr.is_empty(), "current settings do not warn: {}", result.stderr);
}
