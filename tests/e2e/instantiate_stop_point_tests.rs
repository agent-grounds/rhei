//! What a successful `rhei instantiate` says about where it stopped, and how it
//! labels the command it printed.
//!
//! Port of the #717 reproducer. A template's machine has one gating state,
//! `owner`, and one task authored in it; the other tasks start in `draft`, so
//! two of them are ready. The summary used to say the instantiation "stopped
//! at a human gate", hiding the ready tasks that §FS-rhei-templates.6.1.3 makes
//! the stop point, and it labelled the invocation it had just run "Instantiate
//! this template with:", a command its own `--output` now refuses
//! (§FS-rhei-templates.6.1.2).

use std::fs;
use std::path::Path;

use super::templates_tests::run_raw;
use super::*;

const STATES: &str = r#"name: country
version: 1
states:
  draft:
    description: Write the section
    initial: true
  owner:
    description: The owner decides
    gating: true
  done:
    description: Finished
    final: true
transitions:
  - from: draft
    to: done
  - from: owner
    to: done
"#;

/// Lay the issue's `country` template under `<dir>/.agent-grounds/rhei/templates`
/// with the given task files, and `values-hu.yaml` beside it in `dir`.
fn lay_country_template(dir: &Path, tasks: &[(&str, &str)]) {
    let template = dir.join(".agent-grounds/rhei/templates/country");
    fs::create_dir_all(template.join("tasks")).expect("create template dir");
    write_fixture_file(
        &template,
        "template.yaml",
        "name: country\nversion: 1.0.0\ndescription: A country report\ninputs:\n  - name: country\n    description: The country\n",
    );
    write_fixture_file(&template, "states.yaml", STATES);
    write_fixture_file(
        &template,
        "index.rhei.md",
        "# Rhei: The {{country}} report\n\nOne report per country.\n",
    );
    for (name, body) in tasks {
        write_fixture_file(&template.join("tasks"), name, body);
    }
    write_fixture_file(dir, "values-hu.yaml", "country: Hungary\n");
}

const DRAFT_TASK: &str =
    "### Task 1: Draft the {{country}} report\n**State:** draft\n\nWrite it.\n";

const SIGN_OFF_TASK: &str = r#"### Task 2: Sign off the {{country}} report
**State:** draft

#### Task 2.1: Collect the figures for {{country}}
**State:** draft

#### Task 2.2: The owner decides
**State:** owner

The report's owner accepts or returns it.
"#;

fn instantiate(dir: &Path, extra: &[&str]) -> CliRun {
    let mut args = vec!["instantiate", "country", "--values", "values-hu.yaml", "--output", "rep1"];
    args.extend_from_slice(extra);
    run_raw(&args, dir)
}

/// The text from `Stopped:` to the end of stdout.
fn stopped_section(stdout: &str) -> &str {
    let at = stdout
        .find("Stopped:\n")
        .unwrap_or_else(|| panic!("expected a Stopped section; got:\n{stdout}"));
    &stdout[at..]
}

/// A ready task is the stop point even when another task is authored at a
/// gate; the gated task is listed beside it, not in its place.
/// §FS-rhei-templates.6.1.3
#[test]
#[ignore = "red until #717: a gated task replaces the ready-task stop point; implement removes this"]
fn ready_task_is_the_stop_point_when_a_task_is_gated() {
    let dir = unique_temp_dir("instantiate-stop-point-gated");
    lay_country_template(&dir, &[("01-draft.md", DRAFT_TASK), ("02-sign-off.md", SIGN_OFF_TASK)]);
    let result = instantiate(&dir, &[]);
    assert_success(&result);
    let stopped = stopped_section(&result.stdout);
    assert!(
        stopped.starts_with(
            "Stopped:\n  instantiation stopped before execution; next ready task is \
             Task rep1.1: Draft the Hungary report [draft]. \
             Run `rhei run rep1` or claim it with `rhei next rep1`.\n"
        ),
        "the next ready task is the stop point:\n{stopped}"
    );
    assert!(
        stopped.contains(
            "\n  1 task waits on a person at a human gate: \
             Task rep1.2.2: The owner decides [owner].\n"
        ),
        "the gated task is still listed under Stopped:\n{stopped}"
    );
    assert!(
        !result.stdout.contains("stopped at a human gate"),
        "nothing stopped at a gate while tasks are ready:\n{}",
        result.stdout
    );
}

/// After a real write the invocation is a record of what ran, labelled in the
/// past tense; a dry run's is the write still to come. §FS-rhei-templates.6.1.2
#[test]
#[ignore = "red until #717: a real write labels its invocation as a command to run; implement removes this"]
fn real_write_labels_its_invocation_in_the_past_tense() {
    let dir = unique_temp_dir("instantiate-invocation-label");
    lay_country_template(&dir, &[("01-draft.md", DRAFT_TASK), ("02-sign-off.md", SIGN_OFF_TASK)]);

    let dry = instantiate(&dir, &["--dry-run"]);
    assert_success(&dry);
    assert!(
        dry.stdout.contains(
            "Instantiate this template with:\n  \
             rhei instantiate country --values values-hu.yaml --output rep1\n"
        ),
        "a dry run's invocation is the write still to come:\n{}",
        dry.stdout
    );
    assert!(!dry.stdout.contains("Instantiated with:"), "{}", dry.stdout);

    let real = instantiate(&dir, &[]);
    assert_success(&real);
    assert!(
        real.stdout.contains(
            "Instantiated with:\n  rhei instantiate country --values values-hu.yaml --output rep1\n"
        ),
        "a real write records what ran:\n{}",
        real.stdout
    );
    assert!(
        !real.stdout.contains("Instantiate this template with:"),
        "a real write offers no command its own output now refuses:\n{}",
        real.stdout
    );
}

/// With nothing ready and a task at a gate, the gate is the stop point.
/// §FS-rhei-templates.6.1.3
#[test]
fn gate_is_the_stop_point_when_no_task_is_ready() {
    let dir = unique_temp_dir("instantiate-stop-point-gate-only");
    let gated = "### Task 1: The owner decides on {{country}}\n**State:** owner\n\nDecide.\n";
    lay_country_template(&dir, &[("01-owner.md", gated)]);
    let result = instantiate(&dir, &[]);
    assert_success(&result);
    let stopped = stopped_section(&result.stdout);
    assert!(
        stopped.starts_with(
            "Stopped:\n  instantiation stopped at a human gate: \
             Task rep1.1: The owner decides on Hungary [owner].\n"
        ),
        "the gate is the whole reason when nothing is ready:\n{stopped}"
    );
    assert!(!stopped.contains("next ready task"), "{stopped}");
}
