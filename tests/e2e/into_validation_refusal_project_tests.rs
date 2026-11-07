//! Discarded project-default and member placements share union recovery;
//! retained output keeps its repairable validation target. §FS-rhei-library.7.5

use std::fs;

use super::into_validation_refusal_support::{Host, Scenario};
use super::into_validation_refusal_tests::assert_report;
use super::*;

fn project_refusal(replacement: bool, dry_run: bool) {
    let scenario = Scenario::new(Host::Member);
    if replacement {
        // Review's counterexample: the new default drops the finite timeout,
        // while leaving the ticket's state valid, reaching general validation.
        let machine = r#"name: default
version: 1
states:
  work:
    description: Work
    target: codex:openai:gpt-5.5
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#;
        write_fixture_file(&scenario.template, "states.yaml", machine);
        write_fixture_file(
            &scenario.scope,
            "states.yaml",
            &machine.replace("    target:", "    agent_timeout: 1h\n    target:"),
        );
        write_fixture_file(
            &scenario.scope,
            "member.rhei.md",
            "# Rhei: Member\n\n## Tasks\n\n### Task one: Work\n**State:** work\n",
        );
        fs::remove_file(scenario.template.join("index.rhei.md")).unwrap();
        fs::remove_dir_all(scenario.template.join("tasks")).unwrap();
        write_fixture_file(&scenario.template, "index.panta.md", "# Panta: Replacement\n");
    }
    let validate = || scenario.run(&["validate", scenario.scope.to_str().unwrap()]);
    assert_success(&validate());
    let before = scenario.authored_bytes();
    let refused = scenario.place_in_project(dry_run);
    assert_eq!(refused.status.code(), Some(1), "{}\n{}", refused.stdout, refused.stderr);
    assert_eq!(scenario.authored_bytes(), before, "refused placement preserves all project bytes");
    scenario.assert_scratch_discarded();
    let validation = validate();
    assert_success(&validation);
    assert!(validation.stdout.contains("Validation succeeded"));
    let report = format!("{}\n{}", refused.stdout, refused.stderr);
    let folded = report.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_report(&scenario, &report);
    if replacement {
        assert!(folded.contains("no `agent_timeout` resolves"), "{report}");
        assert!(folded.contains("state 'work'"), "{report}");
    } else {
        for detail in ["Task leak.only", "depends on missing Task mini.77", "**Prior:**", "mini.77"]
        {
            assert!(folded.contains(detail), "missing {detail}:\n{report}");
        }
        assert!(scenario.names_original(&folded, &scenario.scope.join("leak/states.yaml")));
        assert!(folded.contains("proposed member"), "the removed member is proposed:\n{report}");
        assert!(folded.contains("not been published"), "its machine is also proposed:\n{report}");
        assert!(folded.contains("project default; rhei: member"), "existing provenance:\n{report}");
    }
}

#[test]
fn replacement_refusal_names_original_files_and_retry() {
    project_refusal(true, false);
}

#[test]
fn replacement_preview_refusal_names_original_files_and_retry() {
    project_refusal(true, true);
}

#[test]
fn proposed_member_refusal_names_original_files_and_retry() {
    project_refusal(false, false);
}

#[test]
fn proposed_member_preview_refusal_names_original_files_and_retry() {
    project_refusal(false, true);
}

#[test]
fn retained_member_refusal_keeps_validate_recovery() {
    let scenario = Scenario::new(Host::Member);
    let output = scenario.scope.join("leak");
    let refused = scenario.run(&[
        "instantiate",
        "leak",
        "--output",
        output.to_str().unwrap(),
        "--keep-on-error",
    ]);
    assert_eq!(refused.status.code(), Some(1), "{}", refused.stderr);
    assert!(output.join("index.rhei.md").is_file(), "retained output is available to repair");
    scenario.assert_scratch_discarded();
    let report = refused.stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        report.contains(&format!("rhei validate {}", shell_quote(&output.to_string_lossy()))),
        "retained output keeps its real validation command:\n{report}"
    );
    assert!(!report.to_lowercase().contains("nothing was written"), "{report}");
    let validation = scenario.run(&["validate", output.to_str().unwrap()]);
    assert_eq!(validation.status.code(), Some(1));
    assert!(validation.stderr.contains("depends on missing Task mini.77"));
}

#[test]
fn mirror_assertion_distinguishes_original_paths_and_relative_copies() {
    for host in [Host::Single, Host::Member, Host::Directory] {
        let scenario = Scenario::new(host);
        assert!(!scenario.exposes_mirror(&scenario.machine.display().to_string()));
        let scratch = scenario.cwd.join(if matches!(host, Host::Directory) {
            "scratch"
        } else {
            "../scratch"
        });
        let scratch = fs::canonicalize(scratch).unwrap().join(".tmpProbe/states.yaml");
        assert!(scenario.exposes_mirror(&format!("in '{}'", scratch.display())));
        let relative = if matches!(host, Host::Directory) {
            "scratch/.tmpProbe/states.yaml"
        } else {
            "../scratch/.tmpProbe/states.yaml"
        };
        assert!(scenario.exposes_mirror(&format!("in '{relative}'")));
        assert!(scenario.exposes_mirror(&format!("in '{}'", relative.replace('/', "\\"))));
    }
}
