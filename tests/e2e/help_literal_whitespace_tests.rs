//! A diagnostic's prose reads as the sentence its author wrote: no run of
//! spaces left inside it by a string literal whose `\` line continuation was
//! lost, so the next source line's indentation became part of the text.
//! §FS-rhei-errors.1.2
//!
//! Both runs are read with `NO_GRAPHICS=1`, which prints a diagnostic's message
//! and help raw. That is the form a log, a JSON wrapper or an agent reads, and
//! the one the graphical renderer hides: it breaks the line at the space run,
//! so the sentence only *looks* cut short there.

use std::path::Path;

use super::into_support::*;
use super::*;

/// Run the built binary with diagnostics printed raw rather than rendered.
fn run_raw(args: &[&str], dir: &Path) -> CliRun {
    let output = rhei_command(dir.join(".home"))
        .current_dir(dir)
        .env("NO_GRAPHICS", "1")
        .args(args)
        .output()
        .expect("rhei command should run");
    CliRun::from(&output)
}

/// The line of `stderr` that starts with `prefix`, with the prefix taken off.
fn line_after<'a>(result: &'a CliRun, prefix: &str) -> &'a str {
    result
        .stderr
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("expected a line starting {prefix:?}; got:\n{}", result.stderr))
}

fn assert_no_space_run(text: &str) {
    assert!(
        !text.contains("  "),
        "the text carries a run of two or more spaces, a lost `\\` continuation: {text:?}"
    );
}

/// A template whose `states.yaml` is valid YAML but not a state machine is
/// refused before it joins the host, with a help that is one sentence.
/// §FS-rhei-library.3.1 §FS-rhei-errors.1.2
#[test]
#[ignore = "red until #632: the unreadable-state-machine help carries 33 spaces; implement removes this"]
fn an_unreadable_template_machine_has_a_one_sentence_help() {
    let (dir, root) = host_workspace("help-unreadable-machine");
    let template = dir.join("bad");
    std::fs::create_dir_all(template.join("tasks")).expect("create the template");
    write_fixture_file(
        &template,
        "template.yaml",
        "name: bad\nversion: 1.0.0\ndescription: A machine that parses as YAML but is no machine.\n",
    );
    write_fixture_file(
        &template,
        "index.rhei.md",
        "# Rhei: bad\n\n## Task bad.1: Something\n\n**State:** pending\n",
    );
    write_fixture_file(
        &template,
        "states.yaml",
        "name: bad\nversion: 1\nstates: not-a-mapping\ntransitions: []\n",
    );
    let machine_before = read(&root.join("states.yaml"));

    let result = run_raw(&["instantiate", "./bad", "--into", "release"], &dir);

    assert!(!result.status.success(), "an unreadable template machine must be refused");
    assert_stderr_contains(&result, "template 'bad' has an unreadable state machine");
    let help = line_after(&result, "diagnostic help: ");
    assert_eq!(
        help,
        "every template stands alone, so its states.yaml is readable before it joins a machine."
    );
    assert_no_space_run(help);
    assert_eq!(machine_before, read(&root.join("states.yaml")), "the target is left alone");
}

/// A single-file template laid with `--output` next to a project's
/// `index.panta.md` is warned about, and the warning reads as prose.
/// §FS-rhei-templates.6.2 §FS-rhei-errors.1.2
#[test]
#[ignore = "red until #632: the single-file-in-a-project warning carries four 18-space runs; implement removes this"]
fn a_single_file_template_in_a_project_is_warned_about_in_plain_prose() {
    let dir = unique_temp_dir("help-single-file-in-project");
    let template = dir.join("one");
    std::fs::create_dir_all(&template).expect("create the template");
    write_fixture_file(
        &template,
        "template.yaml",
        "name: one\nversion: 1\ndescription: One file.\n",
    );
    write_fixture_file(
        &template,
        "plan.rhei.md",
        "# Rhei: One\n\n## Tasks\n\n### Task a: A\n**State:** pending\n",
    );
    let project = dir.join("proj");
    std::fs::create_dir_all(&project).expect("create the project");
    write_fixture_file(&project, "index.panta.md", "# Panta: P\n");

    let result = run_raw(&["instantiate", "./one", "--output", "proj/new"], &dir);

    assert_success(&result);
    // Read from after the two paths, so the separator a platform prints them with does not matter.
    let warning = result
        .stderr
        .lines()
        .find(|line| line.starts_with("warning: ") && line.contains("sits in the Panta project"))
        .and_then(|line| line.split_once(", but "))
        .map(|(_, rest)| rest)
        .unwrap_or_else(|| panic!("expected the single-file warning; got:\n{}", result.stderr));
    assert_eq!(
        warning,
        "a single-file template renders a plain directory, which discovery does not count \
         as a rhei — only `*.rhei.md` files and Directory Workspaces. The project will not list \
         it. Instantiate it outside the project (`--output new`), or move the rendered plan file \
         next to index.panta.md."
    );
    assert_no_space_run(warning);
}
