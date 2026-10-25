//! No shipped template asks Codex for `danger-full-access` unless the user
//! selects it, because a managed ChatGPT workspace refuses it, the worker falls
//! back to read-only, exits 0 and writes nothing (agent-grounds/rhei#509).
//!
//! Each template is laid as a user would lay it, on a machine that has
//! configured nothing, and every launch its states would make is resolved from
//! what was laid. §FS-rhei-templates.4.1

use std::fs;

use super::template_sandbox_sweep_support::*;
use super::template_timeout_tests::shipped_templates;
use super::*;

/// No state of a shipped template, laid with its defaults or with its example
/// values, resolves a Codex launch that asks for `danger-full-access`.
/// §FS-rhei-templates.4.1
#[test]
#[ignore = "red until #509: shipped templates launch Codex with danger-full-access; implement removes this"]
fn no_shipped_template_launch_asks_codex_for_full_access() {
    let dir = unique_temp_dir("template-sandbox-sweep");
    let mut offenders = Vec::new();
    for laid in lay_every_shipped_template(&dir) {
        for launch in launches(&laid) {
            if laid.is_codex(&launch.agent)
                && launch.argv.iter().any(|arg| arg.contains("danger-full-access"))
            {
                offenders.push(format!(
                    "{}: state '{}' launches {}[{}]",
                    laid.label(),
                    launch.state,
                    launch.agent,
                    launch.mode.as_deref().unwrap_or("no mode")
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "{} shipped Codex launch(es) ask for danger-full-access, which a managed workspace \
         refuses:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

/// Every Codex profile a laid template resolves that declares `yolo` also
/// declares `workspace`, with the built-in's flags. §FS-rhei-templates.4.1
/// §FS-rhei-agents.2
#[test]
#[ignore = "red until #509: no Codex profile declares a workspace mode; implement removes this"]
fn every_shipped_codex_profile_with_yolo_also_declares_workspace() {
    let dir = unique_temp_dir("template-sandbox-workspace");
    let expected: Vec<serde_json::Value> =
        WORKSPACE_FLAGS.iter().map(|flag| serde_json::json!(flag)).collect();
    let mut offenders = Vec::new();
    for laid in lay_every_shipped_template(&dir) {
        for agent in laid.codex_profiles() {
            let modes = laid.modes(&agent);
            if !modes.contains_key("yolo") {
                continue;
            }
            match modes.get("workspace").and_then(|flags| flags.as_array()) {
                Some(flags) if *flags == expected => {}
                Some(flags) => offenders
                    .push(format!("{}: agents.{agent}.modes.workspace is {flags:?}", laid.label())),
                None => offenders.push(format!(
                    "{}: agents.{agent} declares yolo but no workspace",
                    laid.label()
                )),
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "expected workspace = {WORKSPACE_FLAGS:?}:\n{}",
        offenders.join("\n")
    );
}

/// A shipped launch that asks for `workspace-write` resolves a profile with
/// `writable_roots`, or its worker could write neither the plan nor Rhei's
/// state. A guard: no shipped launch asks for `workspace-write` yet.
/// §FS-rhei-templates.4.1 §FS-rhei-agents.1.1.2
#[test]
fn every_shipped_workspace_write_launch_resolves_writable_roots() {
    let dir = unique_temp_dir("template-sandbox-roots");
    let mut offenders = Vec::new();
    for laid in lay_every_shipped_template(&dir) {
        for launch in launches(&laid) {
            if laid.is_codex(&launch.agent)
                && launch.argv.iter().any(|arg| arg == "workspace-write")
                && !laid.has_writable_roots(&launch.agent)
            {
                offenders.push(format!(
                    "{}: state '{}' launches {}[{}] without writable_roots",
                    laid.label(),
                    launch.state,
                    launch.agent,
                    launch.mode.as_deref().unwrap_or("no mode")
                ));
            }
        }
    }
    assert!(offenders.is_empty(), "{}", offenders.join("\n"));
}

/// A template that prepares a separate checkout for its final fix records it
/// where the checkout resolver reads it: `runtime/worktree-refs/<task-id>.yaml`.
/// §FS-rhei-templates.4.1 §FS-rhei-agents.4
#[test]
#[ignore = "red until #509: fix records its prepared checkout outside worktree-refs; implement removes this"]
fn shipped_fix_records_its_prepared_checkout_as_a_worktree_ref() {
    let dir = unique_temp_dir("template-sandbox-worktree-ref");
    let mut checked = Vec::new();
    let mut offenders = Vec::new();
    for (template, source) in shipped_templates() {
        if template != "fix" && template != "changeset-review" {
            continue;
        }
        let laid = lay(&dir, &template, &source, None, &["fix_prepare=worktree"]);
        for machine in files_named(&laid.root, "states.yaml") {
            let text = fs::read_to_string(&machine).expect("read laid machine");
            let doc: serde_yaml::Value = serde_yaml::from_str(&text).expect("machine parses");
            let Some(states) = doc["states"].as_mapping() else { continue };
            for (name, state) in states {
                let outputs = state["outputs"].as_sequence().cloned().unwrap_or_default();
                for output in outputs.iter().filter(|o| o["name"].as_str() == Some("workspace-ref"))
                {
                    let path = output["path"].as_str().unwrap_or_default();
                    let state = name.as_str().unwrap_or_default();
                    checked.push(format!("{template}: {state}"));
                    if path != "runtime/worktree-refs/{task_id}.yaml" {
                        offenders.push(format!("{template}: state '{state}' records it at {path}"));
                    }
                }
            }
        }
    }
    assert!(!checked.is_empty(), "no prepare-workspace output was found to check");
    assert!(
        offenders.is_empty(),
        "the prepared checkout must be recorded at runtime/worktree-refs/{{task_id}}.yaml:\n{}",
        offenders.join("\n")
    );
}
