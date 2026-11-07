//! Introduced project load failures retain original and proposed machine
//! provenance even before validation can resolve the proposal. §FS-rhei-library.7.5

use std::fs;
use std::path::Path;

use super::into_validation_refusal_support::{Host, Scenario};
use super::into_validation_refusal_tests::assert_report;
use super::*;

fn assert_context_path(scenario: &Scenario, report: &str, label: &str, path: &Path) {
    let named = report
        .split_once(label)
        .and_then(|(_, context)| context.split('\'').nth(1))
        .unwrap_or_else(|| panic!("missing {label}:\n{report}"))
        .replace('\\', "/");
    let relative = path.strip_prefix(&scenario.cwd).unwrap_or(path);
    assert!(
        [path, relative].iter().any(|path| named == path.to_string_lossy().replace('\\', "/")),
        "{label} should name {} from the invocation directory:\n{report}",
        path.display(),
    );
}

fn project_load_refusal(replacement: bool, dry_run: bool) {
    let scenario = Scenario::new(Host::Member);
    let (parse_file, line) = if replacement {
        fs::remove_file(scenario.template.join("index.rhei.md")).unwrap();
        fs::remove_dir_all(scenario.template.join("tasks")).unwrap();
        write_fixture_file(&scenario.template, "index.panta.md", "# Panta: Replacement\n");
        fs::copy(&scenario.machine, scenario.template.join("states.yaml")).unwrap();
        write_fixture_file(
            &scenario.template,
            "introduced.rhei.md",
            "# Rhei: Introduced\n\n## Tasks\n\n### Task only: Only\n**State:** pending\n\n## Appendix\n",
        );
        (scenario.scope.join("introduced.rhei.md"), 8)
    } else {
        write_fixture_file(
            &scenario.template,
            "tasks/001-only.md",
            "### Task only: Only\n**State:** todo\n\n## Appendix\n",
        );
        (scenario.scope.join("leak/tasks/001-only.md"), 4)
    };
    let validate = || scenario.run(&["validate", scenario.scope.to_str().unwrap()]);
    assert_success(&validate());
    let before = scenario.authored_bytes();
    let refused = scenario.place_in_project(dry_run);
    assert_eq!(refused.status.code(), Some(1), "{}\n{}", refused.stdout, refused.stderr);
    assert_eq!(scenario.authored_bytes(), before, "load refusal preserves all files and artifacts");
    scenario.assert_scratch_discarded();
    assert!(!scenario.scope.join("leak").exists(), "no proposed member was published");
    assert!(!scenario.scope.join("introduced.rhei.md").exists(), "no bundled plan was published");
    assert!(!scenario.scope.join("scripts").exists(), "no bundled artifact was published");
    let validation = validate();
    assert_success(&validation);
    assert!(validation.stdout.contains("Validation succeeded"));

    let report = format!("{}\n{}", refused.stdout, refused.stderr);
    let folded = report.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_report(&scenario, &report);
    assert!(scenario.names_original(&folded, &scenario.scope), "original project:\n{report}");
    assert_context_path(&scenario, &report, "Original project state machine:", &scenario.machine);
    assert!(folded.contains("project default; rhei: member"), "original ownership:\n{report}");
    let parse_path = parse_file.strip_prefix(&scenario.cwd).unwrap();
    let parse_paths =
        [parse_file.clone(), Path::new(".").join(parse_path), parse_path.to_path_buf()];
    assert!(
        parse_paths.iter().any(|path| folded.contains(&format!("in {}", path.display()))),
        "parse frame should name {} from the invocation directory:\n{report}",
        parse_file.display(),
    );
    for detail in [
        "-- PARSE ERROR".to_owned(),
        format!("partway through line {line}"),
        format!("{line}| ## Appendix"),
        "^".to_owned(),
        "Tasks section must be the final".to_owned(),
    ] {
        assert!(folded.contains(&detail), "missing parse detail {detail}:\n{report}");
    }
    if !replacement {
        assert!(folded.contains("proposed member"), "proposed destination:\n{report}");
        assert!(scenario.names_original(&folded, &scenario.scope.join("leak")), "{report}");
        assert_context_path(
            &scenario,
            &report,
            "Proposed member state machine (not published):",
            &scenario.scope.join("leak/states.yaml"),
        );
        assert!(folded.contains("not been published"), "discarded member provenance:\n{report}");
    }
}

#[test]
fn proposed_member_load_refusal_names_original_and_proposed_machines() {
    project_load_refusal(false, false);
}

#[test]
fn proposed_member_preview_load_refusal_names_original_and_proposed_machines() {
    project_load_refusal(false, true);
}

#[test]
fn replacement_load_refusal_names_original_machine() {
    project_load_refusal(true, false);
}

#[test]
fn replacement_preview_load_refusal_names_original_machine() {
    project_load_refusal(true, true);
}
