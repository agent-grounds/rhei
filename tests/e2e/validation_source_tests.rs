//! State-machine source attribution in semantic validation failures.
//!
//! These are black-box contracts because the source summary must describe the
//! same resolution the command actually used, not a separately assembled
//! formatter input. §FS-rhei-validate.6

use std::fs;
use std::path::{Path, PathBuf};

use super::new_tests::flattened_output;
use super::*;

fn machine(name: &str) -> String {
    format!(
        "name: {name}\nversion: 1\nstates:\n  drafting:\n    description: Invoice is being \
         drafted\n    initial: true\n  filed:\n    description: Invoice is filed\n    final: \
         true\ntransitions:\n  - from: drafting\n    to: filed\n"
    )
}

fn write_member(
    project: &Path,
    id: &str,
    declared_machine: Option<&str>,
    machine_yaml: Option<&str>,
    state: &str,
) {
    let root = project.join(id);
    fs::create_dir_all(root.join("tasks")).expect("create rhei directories");
    let declaration =
        declared_machine.map(|name| format!("\n**States:** {name}")).unwrap_or_default();
    write_fixture_file(&root, "index.rhei.md", &format!("# Rhei: {id}{declaration}\n"));
    if let Some(machine_yaml) = machine_yaml {
        write_fixture_file(&root, "states.yaml", machine_yaml);
    }
    write_fixture_file(
        &root.join("tasks"),
        "one.md",
        &format!("### Task 1: {id}\n**State:** {state}\n"),
    );
}

/// Port of the historical rhei.69 fixture: the allowed states prove billing's
/// own machine judged the ticket, and the source list must say so.
fn historical_project(dir: &Path) -> PathBuf {
    let project = dir.join("project");
    fs::create_dir_all(&project).expect("create project");
    write_fixture_file(
        &project,
        "index.panta.md",
        "# Panta: Validation Source Fixture\n**States:** alpha\n",
    );
    write_fixture_file(
        &project,
        "states.yaml",
        "name: alpha\nversion: 1\nstates:\n  surveying:\n    description: Ticket is being \
         surveyed\n    initial: true\n  signed-off:\n    description: Ticket is signed \
         off\n    final: true\ntransitions:\n  - from: surveying\n    to: signed-off\n",
    );
    write_member(&project, "audit", None, None, "surveying");
    let custom = machine("custom");
    write_member(&project, "billing", Some("custom"), Some(&custom), "surveying");
    project
}

fn run_in(cwd: &Path, args: &[&str]) -> CliRun {
    let output = rhei_command(cwd.join(".home"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("rhei command should run");
    CliRun::from(&output)
}

fn assert_billing_state_error(result: &CliRun) -> String {
    assert_eq!(result.status.code(), Some(1), "stderr:\n{}", result.stderr);
    let said = flattened_output(result);
    assert!(
        said.contains("Task billing.1 has invalid state 'surveying'. Allowed: [drafting, filed]"),
        "the semantic failure must establish that billing's machine was used; got:\n{said}"
    );
    said
}

fn quoted(path: &Path) -> String {
    format!("'{}'", path.display())
}

#[test]
fn validate_failure_lists_every_resolved_machine_source() {
    let dir = unique_temp_dir("validate-source-attribution");
    let project = historical_project(&dir);
    let project_arg = project.display().to_string();

    let result = run_in(&dir, &["validate", &project_arg]);
    let said = assert_billing_state_error(&result);
    let default =
        format!("{} (project default; rhei: audit)", quoted(&project.join("states.yaml")));
    let billing = format!("{} (rhei: billing)", quoted(&project.join("billing/states.yaml")));
    assert!(
        said.contains("I validated this plan using these state-machine sources:")
            && said.contains(&default)
            && said.contains(&billing),
        "the failure must name both actual sources and their owners; got:\n{said}"
    );
    let misleading = format!(
        "I validated this plan using {}, but found a problem.",
        quoted(&project.join("states.yaml"))
    );
    assert!(
        !said.contains(&misleading),
        "a heterogeneous pass must not claim it used only the project default; got:\n{said}"
    );
}

#[test]
fn validation_sources_are_ordered_and_default_owners_are_grouped() {
    let dir = unique_temp_dir("validate-source-order");
    let project = dir.join("project");
    fs::create_dir_all(&project).expect("create project");
    write_fixture_file(&project, "index.panta.md", "# Panta: Ordered Sources\n**States:** alpha\n");
    write_fixture_file(
        &project,
        "states.yaml",
        "name: alpha\nversion: 1\nstates:\n  surveying:\n    description: Surveying\n    \
         initial: true\n  signed-off:\n    description: Signed off\n    final: true\ntransitions:\n  \
         - from: surveying\n    to: signed-off\n",
    );
    write_member(&project, "zeta", None, None, "surveying");
    write_member(&project, "audit", None, None, "surveying");
    let ledger_machine = machine("ledger-flow");
    write_member(&project, "ledger", Some("ledger-flow"), Some(&ledger_machine), "drafting");
    let billing_machine = machine("billing-flow");
    write_member(&project, "billing", Some("billing-flow"), Some(&billing_machine), "surveying");

    let result = run_in(&dir, &["validate", &project.display().to_string()]);
    let said = assert_billing_state_error(&result);
    let entries = [
        format!("{} (project default; rhei: audit, zeta)", quoted(&project.join("states.yaml"))),
        format!("{} (rhei: billing)", quoted(&project.join("billing/states.yaml"))),
        format!("{} (rhei: ledger)", quoted(&project.join("ledger/states.yaml"))),
    ];
    let positions = entries
        .iter()
        .map(|entry| said.find(entry).unwrap_or_else(|| panic!("missing {entry:?} in:\n{said}")))
        .collect::<Vec<_>>();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "sources must be default-first, then ordered by owning rhei; got:\n{said}"
    );
}

#[test]
fn identical_machine_files_remain_separate_sources() {
    let dir = unique_temp_dir("validate-identical-source-files");
    let project = dir.join("project");
    fs::create_dir_all(&project).expect("create project");
    write_fixture_file(&project, "index.panta.md", "# Panta: Separate Sources\n");
    let custom = machine("custom");
    write_member(&project, "billing", Some("custom"), Some(&custom), "surveying");
    write_member(&project, "treasury", Some("custom"), Some(&custom), "drafting");

    let result = run_in(&dir, &["validate", &project.display().to_string()]);
    let said = assert_billing_state_error(&result);
    let built_in = "the built-in default state machine (project default)";
    let billing = format!("{} (rhei: billing)", quoted(&project.join("billing/states.yaml")));
    let treasury = format!("{} (rhei: treasury)", quoted(&project.join("treasury/states.yaml")));
    assert!(
        said.contains(built_in) && said.contains(&billing) && said.contains(&treasury),
        "content-identical files are still separate sources; got:\n{said}"
    );
    assert!(
        !said.contains("project default; rhei:")
            && !said.contains(&quoted(&project.join("states.yaml"))),
        "an ownerless built-in default must claim no rhei and invent no path; got:\n{said}"
    );
}

#[test]
fn builtin_default_source_names_its_inheriting_rhei_without_a_path() {
    let dir = unique_temp_dir("validate-builtin-source");
    let project = dir.join("project");
    fs::create_dir_all(&project).expect("create project");
    write_fixture_file(&project, "index.panta.md", "# Panta: Built-in Source\n");
    write_member(&project, "audit", None, None, "pending");
    let custom = machine("custom");
    write_member(&project, "billing", Some("custom"), Some(&custom), "surveying");

    let result = run_in(&dir, &["validate", &project.display().to_string()]);
    let said = assert_billing_state_error(&result);
    assert!(
        said.contains("the built-in default state machine (project default; rhei: audit)")
            && !said.contains(&quoted(&project.join("states.yaml"))),
        "the built-in source must name its owner without fabricating a file; got:\n{said}"
    );
}

#[test]
fn next_validation_failure_lists_every_resolved_machine_source() {
    let dir = unique_temp_dir("next-source-attribution");
    let project = historical_project(&dir);

    let result = run_in(&dir, &["next", &project.display().to_string(), "--no-callbacks"]);
    let said = assert_billing_state_error(&result);
    assert!(
        said.contains(&quoted(&project.join("states.yaml")))
            && said.contains(&quoted(&project.join("billing/states.yaml"))),
        "a persistent-source caller must receive the shared source summary; got:\n{said}"
    );
}

#[test]
fn builtin_only_failure_keeps_its_single_source_sentence() {
    let dir = unique_temp_dir("validate-builtin-only-source");
    write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Built-in Only\n\n## Tasks\n\n### Task 1: Invalid\n**State:** surveying\n",
    );

    let result = run_in(&dir, &["validate", "plan.rhei.md"]);
    assert_eq!(result.status.code(), Some(1), "stderr:\n{}", result.stderr);
    let said = flattened_output(&result);
    assert!(
        said.contains(
            "I validated this plan using the built-in default state machine, but found a problem."
        ) && !said.contains("using these state-machine sources"),
        "a built-in-only pass must retain its existing sentence; got:\n{said}"
    );
}

#[test]
fn instantiate_failure_keeps_its_single_source_presentation() {
    let dir = unique_temp_dir("instantiate-validation-source");
    let template = dir.join(".agent-grounds/rhei/templates/invalid-source");
    fs::create_dir_all(template.join("tasks")).expect("create template");
    write_fixture_file(
        &template,
        "template.yaml",
        "name: invalid-source\nversion: 1.0.0\ndescription: Invalid source fixture\n",
    );
    write_fixture_file(&template, "index.rhei.md", "# Rhei: Invalid Source\n**States:** custom\n");
    write_fixture_file(&template, "states.yaml", &machine("custom"));
    write_fixture_file(
        &template.join("tasks"),
        "one.md",
        "### Task 1: Invalid\n**State:** surveying\n",
    );

    let result = run_in(&dir, &["instantiate", "invalid-source", "--output", "rendered"]);
    assert_eq!(result.status.code(), Some(1), "stderr:\n{}", result.stderr);
    let said = flattened_output(&result);
    assert!(
        said.contains("Task rendered.1 has invalid state 'surveying'. Allowed: [drafting, filed]")
            && said.contains(
                "I validated this plan using 'rendered/states.yaml', but found a problem."
            ),
        "instantiate must retain its existing generated-source sentence; got:\n{said}"
    );
    assert!(!dir.join("rendered").exists(), "failed generated output should be removed");
}
