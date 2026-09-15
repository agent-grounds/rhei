//! Observable publication guarantees for a template instantiated as a Panta
//! member. The implementation may choose its unique hidden staging name; the
//! public contract is that only the final member can become discoverable.

use std::fs;
use std::path::Path;

use super::*;

fn run_in(cwd: &Path, args: &[&str]) -> CliRun {
    let output = rhei_command(cwd.join(".home"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("rhei command should run");
    CliRun::from(&output)
}

fn write_member_template(project: &Path, name: &str, task_state: &str) {
    let template = project.join(".agent-grounds/rhei/templates").join(name);
    fs::create_dir_all(template.join("tasks")).expect("create member template");
    write_fixture_file(
        &template,
        "template.yaml",
        &format!("name: {name}\nversion: 1.0.0\ndescription: Member publication fixture\n"),
    );
    write_fixture_file(
        &template,
        "index.rhei.md",
        &format!("# Rhei: {name}\n**States:** publication-machine\n"),
    );
    write_fixture_file(
        &template,
        "states.yaml",
        r#"name: publication-machine
version: 1
states:
  ready:
    initial: true
    description: Ready
  done:
    final: true
    description: Done
transitions:
  - from: ready
    to: done
"#,
    );
    write_fixture_file(
        &template,
        "tasks/01-work.md",
        &format!("### Task 1: Work\n**State:** {task_state}\n"),
    );
}

fn visible_member_names(project: &Path) -> Vec<String> {
    let mut names = fs::read_dir(project)
        .expect("read project")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// Successful project-context validation has one publication point: after the
/// command returns, the intended member is the only visible rendered output.
// §FS-rhei-templates.6.1.2 §FS-rhei-templates.6.2
#[test]
fn issue_205_valid_project_member_is_published_once_after_project_validation() {
    let project = unique_temp_dir("member-publication-success");
    write_fixture_file(&project, "index.panta.md", "# Panta: Publication\n");
    write_member_template(&project, "review", "ready");

    let instantiated = run_in(&project, &["instantiate", "review", "--output", "published-review"]);
    assert_success(&instantiated);
    assert_eq!(visible_member_names(&project), vec!["published-review"]);

    let listed = run_in(&project, &["list"]);
    assert_success(&listed);
    assert_eq!(listed.stdout.matches("published-review.1").count(), 1);
    assert_success(&run_in(&project, &["validate"]));
}

/// Failed output is undiscoverable by default. `--keep-on-error` deliberately
/// exposes it at the requested name, after which strict project loading must
/// reject the invalid visible member rather than silently retain an old graph.
// §FS-rhei-templates.6.1.2 §FS-rhei-templates.6.2
#[test]
fn issue_205_invalid_project_member_is_cleaned_or_deliberately_retained_for_strict_loading() {
    let project = unique_temp_dir("member-publication-failure");
    write_fixture_file(&project, "index.panta.md", "# Panta: Publication Failure\n");
    write_member_template(&project, "broken", "not-a-state");

    let discarded = run_in(&project, &["instantiate", "broken", "--output", "discarded"]);
    assert!(!discarded.status.success(), "invalid output must fail validation");
    assert!(!project.join("discarded").exists(), "failed output is removed by default");
    assert!(visible_member_names(&project).is_empty(), "no staged output is discoverable");

    let retained =
        run_in(&project, &["instantiate", "broken", "--output", "retained", "--keep-on-error"]);
    assert!(!retained.status.success(), "retention does not turn validation green");
    assert!(project.join("retained").is_dir(), "requested diagnostic output is retained");
    assert_eq!(visible_member_names(&project), vec!["retained"]);

    let strict = run_in(&project, &["validate"]);
    assert!(!strict.status.success(), "an invalid visible member must fail strict loading");
    assert!(
        strict.stderr.contains("not-a-state") || strict.stderr.contains("invalid state"),
        "strict-load diagnostic should name the invalid retained member:\n{}",
        strict.stderr
    );
}
