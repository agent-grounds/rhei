//! Every shipped template instantiates into a workspace `rhei validate`
//! accepts, on a machine that has configured nothing.
//!
//! A template is the worked example a new project copies, so a template that
//! resolves no finite `agent_timeout` for one of its agent states hands that
//! state to everyone who instantiates it. Nothing measured that before: the
//! template tests check instantiation mechanics for a handful of templates,
//! never that all of them validate clean, which is how three of them drifted.
//! §FS-rhei-templates.6 §FS-rhei-validate.4 §FS-rhei-agents.3.2.2

use std::fs;
use std::path::Path;

use super::templates_tests::run_raw;
use super::*;

/// What a required input with no default is given, so the guard reaches
/// validation rather than stopping at the prompt for a value.
const PLACEHOLDER: &str = "placeholder";

/// The templates as they ship, read from the tree rather than from a list in
/// this file — a list would go stale the moment a template is added, which is
/// the one case this guard exists for.
fn shipped_templates() -> Vec<String> {
    let dir = repo_root().join("crates/rhei-cli/templates");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        .map(|entry| entry.expect("template entry"))
        .filter(|entry| entry.path().join("template.yaml").is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
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

#[test]
fn every_shipped_template_instantiates_into_a_workspace_validate_accepts() {
    let dir = unique_temp_dir("template-validates");
    // The command searches upwards for a rhei home, so a template of the same
    // name above the temporary directory would be the one instantiated.
    fs::create_dir_all(dir.join(".agent-grounds/rhei/templates")).expect("create rhei home");

    for template in shipped_templates() {
        let output = format!("out-{template}");
        let mut args = vec!["instantiate".to_string(), template.clone()];
        args.extend(required_inputs(&dir, &template));
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
