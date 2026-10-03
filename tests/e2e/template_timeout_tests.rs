//! Every shipped template instantiates into a workspace `rhei validate`
//! accepts, on a machine that has configured nothing.
//!
//! A template is the worked example a new project copies, so a template that
//! resolves no finite `agent_timeout` for one of its agent states hands that
//! state to everyone who instantiates it. Nothing measured that before: the
//! template tests check instantiation mechanics for a handful of templates,
//! never that all of them validate clean, which is how three of them drifted.
//! §FS-rhei-templates.6 §FS-rhei-validate.4 §FS-rhei-agents.3.2.2
//!
//! What it gates is the shipped template and nothing else: a same-named
//! template in a rhei home above the test's directory must never stand in for
//! it, so the fixture plants one there and the guard has to pass regardless.
//! §REQ-test-isolation.4

use std::fs;
use std::path::{Path, PathBuf};

use super::templates_tests::run_raw;
use super::*;

/// What a required input with no default is given, so the guard reaches
/// validation rather than stopping at the prompt for a value.
const PLACEHOLDER: &str = "placeholder";

/// The templates as they ship, read from the tree rather than from a list in
/// this file — a list would go stale the moment a template is added, which is
/// the one case this guard exists for. Each is its name and its source path:
/// the path is what the commands are given, because a bare name walks every
/// rhei home above the test and the user tier first. §REQ-test-isolation.4
fn shipped_templates() -> Vec<(String, PathBuf)> {
    let dir = repo_root().join("crates/rhei-cli/templates");
    let mut names: Vec<(String, PathBuf)> = fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        .map(|entry| entry.expect("template entry"))
        .filter(|entry| entry.path().join("template.yaml").is_file())
        .map(|entry| (entry.file_name().to_string_lossy().into_owned(), entry.path()))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no shipped templates found under {}", dir.display());
    names
}

/// `name=placeholder` for every input the template requires and gives no
/// default, taken from the command's own JSON rather than from `template.yaml`.
fn required_inputs(dir: &Path, template: &str) -> Vec<String> {
    let detail = run_raw(&["templates", template, "--json"], dir);
    assert_success(&detail);
    let value: serde_json::Value =
        serde_json::from_str(&detail.stdout).expect("template detail should be valid JSON");
    value["inputs"]
        .as_array()
        .expect("inputs array")
        .iter()
        .filter(|input| input["required"] == true && input["default"].is_null())
        .map(|input| format!("{}={PLACEHOLDER}", input["name"].as_str().expect("input name")))
        .collect()
}

/// A rhei home at `fixture`, holding an `agora` whose one required input is an
/// array the built-in `agora` does not have, so instantiating it with the
/// placeholder fails as the stand-in and never as the shipped template.
fn plant_ancestor_agora(fixture: &Path) {
    let agora = fixture.join(".agent-grounds/rhei/templates/agora");
    fs::create_dir_all(&agora).expect("create the ancestor agora");
    fs::write(
        agora.join("template.yaml"),
        "name: agora\nversion: 1.0.0\ndescription: not the built-in agora\ninputs:\n  \
         - name: participants\n    description: the discussants\n    type: array\n    \
         required: true\n    items:\n      type: string\n",
    )
    .expect("write the ancestor agora's manifest");
    fs::write(
        agora.join("plan.rhei.md"),
        "# Rhei: Ancestor agora\n\n## Tasks\n\n### Task 1: Discuss\n**State:** pending\n",
    )
    .expect("write the ancestor agora's plan");
}

#[test]
fn every_shipped_template_instantiates_into_a_workspace_validate_accepts() {
    let fixture = unique_temp_dir("template-validates");
    // A rhei home above where the commands run, with an incompatible same-named
    // template, as a developer's home above TMPDIR may hold. §REQ-test-isolation.4
    plant_ancestor_agora(&fixture);
    let dir = fixture.join("work");
    fs::create_dir_all(&dir).expect("create the working directory");

    for (template, source) in shipped_templates() {
        let output = format!("out-{template}");
        let source = source.to_string_lossy().into_owned();
        let mut args = vec!["instantiate".to_string(), source.clone()];
        args.extend(required_inputs(&dir, &source));
        args.push("--output".to_string());
        args.push(output.clone());
        let args: Vec<&str> = args.iter().map(String::as_str).collect();

        let instantiated = run_raw(&args, &dir);
        assert!(
            instantiated.status.success(),
            "template '{template}' should instantiate\nstdout:\n{}\nstderr:\n{}",
            instantiated.stdout,
            instantiated.stderr
        );

        let validated = run_raw(&["validate", &output], &dir);
        assert!(
            validated.status.success(),
            "a workspace from template '{template}' should validate on a machine that has \
             configured nothing\nstdout:\n{}\nstderr:\n{}",
            validated.stdout,
            validated.stderr
        );
    }
}
