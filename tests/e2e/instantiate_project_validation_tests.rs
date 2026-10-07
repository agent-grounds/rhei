use std::fs;
use std::path::Path;

use super::instantiate_project_validation_support::*;
use super::*;

/// Port the intake control and both broken-sibling modes. Check mutations
/// before the parity assertion so a false success cannot hide a write.
/// §FS-rhei-templates.6.1.2
#[test]
fn dry_run_refuses_existing_sibling_like_real_write() {
    let scenario = Scenario::new();
    scenario.clean_control();
    scenario.add_incoming_settings();
    scenario.add_broken_sibling();
    let before = snapshot(&scenario.project);
    let mut previews = Vec::new();
    for flags in [&["--dry-run"][..], &["--dry-run", "--keep-on-error"][..]] {
        let preview = scenario.output_run(flags);
        assert_eq!(snapshot(&scenario.project), before, "dry run changed project/settings bytes");
        assert!(!scenario.output().exists(), "dry run never publishes or retains output");
        previews.push(preview);
    }
    let real = scenario.output_run(&[]);
    assert_refused_for_sibling(&real);
    assert_eq!(snapshot(&scenario.project), before, "real refusal rolls back staged output");
    for preview in previews {
        assert_eq!(
            preview.status.code(),
            real.status.code(),
            "dry run must perform the same strict project parse as a real write; \
             clean control succeeded, real refused the sibling\ndry stdout:\n{}\nreal stderr:\n{}",
            preview.stdout,
            real.stderr
        );
        assert_refused_for_sibling(&preview);
        assert_sibling_remedy(&scenario, &preview);
    }
}

/// Diagnostic coverage is separate from parity: today's real refusal already
/// exits 1, but its unnamed remedy incorrectly directs attention to the output.
/// §FS-rhei-templates.6.1.2
#[test]
fn real_write_refusal_names_existing_sibling_and_repair_command() {
    let scenario = Scenario::new();
    scenario.clean_control();
    scenario.add_broken_sibling();
    let before = snapshot(&scenario.project);
    let real = scenario.output_run(&[]);
    assert_eq!(snapshot(&scenario.project), before);
    assert_sibling_remedy(&scenario, &real);
    let sibling = scenario.project.join("broken-sibling.rhei.md");
    let targets = [sibling, Path::new("..").join("project").join("broken-sibling.rhei.md")];
    assert_remedy_reproduces(&scenario, &real, &targets);
}

/// A clean preview uses both the project's registry and the template's new
/// model, with project values winning conflicts. Neither document alone is
/// enough. §FS-rhei-templates.6.1.2 §FS-rhei-templates.6.2
#[test]
fn dry_run_uses_reconciled_project_settings_without_committing_them() {
    let scenario = Scenario::new();
    let settings = scenario.project.join(".agent-grounds/rhei/settings.json");
    let existing = r#"{
  "agents": {"project-agent": {"command": ["unused"], "modes": {"review": []}}},
  "models": {"incoming-model": {"provider": "mock", "model": "kept"}}
}
"#;
    fs::write(&settings, existing).expect("write the project's registry");
    write_fixture_file(
        &scenario.template,
        "settings.json",
        r#"{
  "models": {"incoming-model": {"provider": "wrong-provider", "model": "incoming",
                                  "default_agent": "project-agent"}}
}
"#,
    );
    let machine = fs::read_to_string(scenario.template.join("states.yaml")).unwrap();
    fs::write(
        scenario.template.join("states.yaml"),
        machine.replace("    initial: true", "    initial: true\n    target: project-agent[review]:mock:incoming-model\n    agent_timeout: 5s"),
    )
    .expect("require the combined project registry");
    let real = scenario.output_run(&[]);
    assert!(
        real.status.success(),
        "clean real write must validate with reconciled settings:\n{}",
        real.stderr
    );
    let merged: serde_json::Value = serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
    assert_eq!(merged["models"]["incoming-model"]["provider"], "mock");
    assert_eq!(merged["models"]["incoming-model"]["default_agent"], "project-agent");
    fs::remove_dir_all(scenario.output()).unwrap();
    fs::write(&settings, existing).unwrap();
    let before = snapshot(&scenario.project);
    for flags in [&["--dry-run"][..], &["--dry-run", "--keep-on-error"][..]] {
        let preview = scenario.output_run(flags);
        assert_eq!(snapshot(&scenario.project), before, "dry run commits no settings merge");
        assert!(
            preview.status.success(),
            "dry run must use reconciled project settings:\n{}",
            preview.stderr
        );
        assert!(
            preview.stdout.contains("new.1"),
            "uses intended member identity:\n{}",
            preview.stdout
        );
    }
}

/// `--into <project>` lays a member through the same helper, so it must not
/// regain the isolated dry-run validation. §FS-rhei-templates.6.1.2
#[test]
fn into_project_dry_run_refuses_existing_sibling() {
    let scenario = Scenario::new();
    scenario.clean_control();
    scenario.add_broken_sibling();
    let project = scenario.project.to_str().unwrap();
    let before = snapshot(&scenario.project);
    let preview = scenario.run(&["--into", project, "--dry-run"]);
    assert_eq!(snapshot(&scenario.project), before, "--into preview changes no project bytes");
    let real = scenario.run(&["--into", project]);
    assert_refused_for_sibling(&real);
    assert!(!scenario.project.join("mini").exists(), "real refusal removes its member");
    assert_eq!(
        preview.status.code(),
        real.status.code(),
        "--into project must share strict validation; dry stdout:\n{}\nreal stderr:\n{}",
        preview.stdout,
        real.stderr
    );
    assert_sibling_remedy(&scenario, &preview);
}

/// The documented existence exception must still preview in the intended
/// project context, preserving all existing destination bytes on either result.
/// §FS-rhei-templates.6.1.2
#[test]
fn dry_run_preserves_existing_output_on_success_and_sibling_refusal() {
    let scenario = Scenario::new();
    scenario.clean_control();
    scenario.add_incoming_settings();
    fs::remove_dir_all(scenario.project.join(".agent-grounds")).unwrap();
    fs::create_dir(scenario.output()).unwrap();
    write_fixture_file(
        &scenario.output(),
        "sentinel.txt",
        "the existing destination belongs to its owner\n",
    );
    let before = snapshot(&scenario.project);
    let clean = scenario.output_run(&["--dry-run", "--keep-on-error"]);
    assert_eq!(snapshot(&scenario.project), before, "success preserves existing destination");
    assert_success(&clean);
    scenario.add_broken_sibling();
    let before = snapshot(&scenario.project);
    let broken = scenario.output_run(&["--dry-run", "--keep-on-error"]);
    assert_eq!(snapshot(&scenario.project), before, "refusal preserves existing destination");
    assert_refused_for_sibling(&broken);
    assert_sibling_remedy(&scenario, &broken);
}

/// A sibling parse cannot establish that invalid rendered output validated.
/// This guard already passes; it prevents the new diagnostic inventing that
/// claim when output validation did not complete. §FS-rhei-templates.6.1.2
#[test]
fn sibling_refusal_does_not_claim_invalid_output_validated() {
    let scenario = Scenario::new();
    scenario.clean_control();
    write_fixture_file(
        &scenario.template,
        "tasks/001-only.md",
        "### Task 1: Only\n**State:** not-a-state\n",
    );
    scenario.add_broken_sibling();
    let before = snapshot(&scenario.project);
    for flags in [&["--dry-run", "--keep-on-error"][..], &[][..]] {
        let refused = scenario.output_run(flags);
        assert_eq!(snapshot(&scenario.project), before);
        assert_eq!(refused.status.code(), Some(1));
        let diagnostic = format!("{}\n{}", refused.stdout, refused.stderr).to_lowercase();
        for claim in [
            "output itself validated",
            "output validated",
            "output is valid",
            "output passed validation",
            "validation succeeded",
        ] {
            assert!(
                !diagnostic.contains(claim),
                "invalid rendered output cannot support '{claim}':\n{diagnostic}"
            );
        }
    }
}
