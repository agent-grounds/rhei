//! Refusing an unwritten union must leave an inspectable diagnostic and a
//! truthful recovery, in both write and preview modes. §FS-rhei-library.7.5

use super::into_validation_refusal_support::{Host, Scenario};
use super::*;

fn assert_refusal(host: Host, dry_run: bool, load_failure: bool) {
    let scenario = Scenario::new(host);
    if load_failure {
        // A template task with a chapter after it reaches the strict scope
        // parser only once placed; the union cannot load, rather than merely
        // having an unresolved dependency. No generated YAML corruption needed.
        write_fixture_file(
            &scenario.template,
            "tasks/001-only.md",
            "### Task only: Only\n**State:** todo\n\n## Appendix\n\nMisplaced chapter.\n",
        );
    }
    assert_success(&scenario.validate());
    let before = scenario.authored_bytes();
    let refused = scenario.place(dry_run);
    assert_eq!(refused.status.code(), Some(1), "{}\n{}", refused.stdout, refused.stderr);
    assert_eq!(scenario.authored_bytes(), before, "refusal must preserve all host bytes");
    scenario.assert_scratch_discarded();
    let validation = scenario.validate();
    assert_success(&validation);
    assert!(validation.stdout.contains("Validation succeeded"));
    let report = format!("{}\n{}", refused.stdout, refused.stderr);
    let folded = report.split_whitespace().collect::<Vec<_>>().join(" ");
    if load_failure {
        assert!(folded.contains("Tasks section must be the final"), "wrong refusal:\n{report}");
        assert!(folded.contains("## Appendix"), "parser detail was lost:\n{report}");
    } else {
        for detail in ["only", "depends on missing Task mini.77", "**Prior:**", "mini.77"] {
            assert!(folded.contains(detail), "missing dependency detail {detail}:\n{report}");
        }
    }

    assert_report(&scenario, &report);
}

pub(super) fn assert_report(scenario: &Scenario, report: &str) {
    let folded = report.split_whitespace().collect::<Vec<_>>().join(" ");
    // Collect violations so each red run demonstrates both symptoms even if
    // the path assertion fails first. These are semantic requirements, not an
    // exact rendering or an obligation to print a concrete retry command.
    let lower = folded.to_lowercase();
    let mut violations = Vec::new();
    if scenario.exposes_mirror(report) || report.contains(".rhei-instantiate-") {
        violations.push("report exposes the disposable validation mirror");
    }
    if !scenario.names_original(&folded, &scenario.target)
        && !scenario.names_original(&folded, &scenario.scope)
    {
        violations.push("report omits the original target/scope path");
    }
    if !scenario.names_original(&folded, &scenario.machine) {
        violations.push("report omits the original machine path");
    }
    if lower.contains("rhei validate") {
        violations.push("validate cannot re-check the unwritten proposal");
    }
    if ![
        "nothing was written",
        "nothing has been written",
        "not written",
        "no changes were written",
        "unwritten",
    ]
    .iter()
    .any(|text| lower.contains(text))
    {
        violations.push("report does not explain that the placement was not written");
    }
    if !lower.contains("placement") && !lower.contains("union") {
        violations.push("report does not identify the attempted placement");
    }
    if !lower.contains("instantiate")
        || !["retry", "re-run", "rerun", "again"].iter().any(|text| lower.contains(text))
    {
        violations.push("recovery does not direct retrying instantiate");
    }
    if !["template", "input", "value"].iter().any(|text| lower.contains(text))
        || !["correct", "fix", "repair", "edit"].iter().any(|text| lower.contains(text))
    {
        violations.push("recovery does not direct correcting the placement inputs");
    }
    assert!(violations.is_empty(), "{}\n{report}", violations.join("\n"));
}

#[test]
fn single_file_union_refusal_names_original_files_and_retry() {
    assert_refusal(Host::Single, false, false);
}

#[test]
fn single_file_preview_refusal_names_original_files_and_retry() {
    assert_refusal(Host::Single, true, false);
}

#[test]
fn member_union_refusal_names_original_files_and_retry() {
    // A passing control inside the red regression protects the existing
    // inherited-error exception and ordinary validate's separate recovery.
    let control = Scenario::new(Host::Member);
    write_fixture_file(
        &control.scope,
        "untouched.rhei.md",
        "# Rhei: Untouched\n\n## Tasks\n\n### Task 1: Existing\n**State:** pending\n**Prior:** mini.77\n",
    );
    write_fixture_file(
        &control.template,
        "tasks/001-only.md",
        "### Task only: Only\n**State:** todo\n",
    );
    let ordinary = control.run(&["validate", control.scope.to_str().unwrap()]);
    assert_eq!(ordinary.status.code(), Some(1));
    assert!(ordinary.stderr.contains("untouched.1 depends on missing Task mini.77"));
    assert!(ordinary.stderr.contains("rhei validate"), "ordinary validate keeps its re-check");
    assert_success(&control.place(true));
    assert_success(&control.place(false));
    assert_refusal(Host::Member, false, false);
}

#[test]
fn member_preview_refusal_names_original_files_and_retry() {
    assert_refusal(Host::Member, true, false);
}

#[test]
fn directory_union_refusal_names_original_files_and_retry() {
    assert_refusal(Host::Directory, false, false);
}

#[test]
fn directory_preview_refusal_names_original_files_and_retry() {
    assert_refusal(Host::Directory, true, false);
}

#[test]
fn introduced_load_refusal_names_original_files_and_retry() {
    assert_refusal(Host::Single, false, true);
}

#[test]
fn introduced_preview_load_refusal_names_original_files_and_retry() {
    assert_refusal(Host::Single, true, true);
}
