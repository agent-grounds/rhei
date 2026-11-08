//! §FS-rhei-states.1.1: a `name` or `version` whose value is not of the type its
//! row names is refused when the machine loads, naming the field, and the help
//! does not call the valid YAML around it invalid (§FS-rhei-errors.1.2.2).

use std::path::Path;

use super::*;

/// The re-check every refusal ends in, as the fixtures sit in the working directory.
const RECHECK: &str = "rhei states --state-machine states.yaml";

/// The valid body every fixture shares; only `name:` and `version:` vary.
const BODY: &str = "states:
  open:
    initial: true
  done:
    final: true
transitions:
  - from: open
    to: done
";

/// The plan `rhei validate` reads beside the machine.
const PLAN: &str = "# Rhei: Plan\n\n## Tasks\n\n### Task a: First\n\n**State:** open\n\nDo it.\n";

fn run_in(args: &[&str], cwd: &Path) -> CliRun {
    let home = unique_temp_dir("top-level-type-home");
    let output =
        rhei_command(&home).current_dir(cwd).args(args).output().expect("rhei command should run");
    CliRun::from(&output)
}

/// The text after `marker` on the one rendered line that opens with it.
fn rendered_line(stderr: &str, marker: &str) -> String {
    stderr
        .lines()
        .find_map(|line| line.trim_start().strip_prefix(marker))
        .unwrap_or_else(|| panic!("no `{marker}` line in:\n{stderr}"))
        .trim_end()
        .to_string()
}

fn machine(top: &str) -> String {
    format!("{top}{BODY}")
}

/// Drive `rhei states --state-machine` and `rhei validate` over `top` + [`BODY`],
/// and hold each to a refusal whose headline names `key` and whose help neither
/// calls the file invalid YAML nor asks for a syntax fix.
fn assert_refused_naming(prefix: &str, top: &str, key: &str) {
    let dir = unique_temp_dir(prefix);
    write_fixture_file(&dir, "states.yaml", &machine(top));
    write_fixture_file(&dir, "plan.rhei.md", PLAN);
    for args in
        [&["states", "--state-machine", "states.yaml"][..], &["validate", "plan.rhei.md"][..]]
    {
        let run = run_in(args, &dir);
        assert_eq!(
            run.status.code(),
            Some(1),
            "{args:?} refuses {top:?}\nstdout:\n{}\nstderr:\n{}",
            run.stdout,
            run.stderr
        );
        let headline = rendered_line(&run.stderr, "× ");
        assert!(
            headline.contains(&format!("{key}:")) || headline.contains(&format!("`{key}`")),
            "{args:?} on {top:?}: the refusal names `{key}`: {headline}"
        );
        let help = rendered_line(&run.stderr, "help: ");
        assert!(help.contains(RECHECK), "{args:?} on {top:?}: the help re-checks the file: {help}");
        assert!(!help.contains("not valid YAML"), "{args:?} on {top:?}: the YAML is valid: {help}");
        assert!(!help.to_lowercase().contains("syntax"), "{args:?} on {top:?}: no syntax: {help}");
    }
}

#[test]
#[ignore = "red until #629: a null name loads as an empty name; implement removes this"]
fn a_null_name_is_refused_naming_the_field() {
    assert_refused_naming("top-level-null-name", "name:\nversion: 1\n", "name");
}

#[test]
#[ignore = "red until #629: a null version loads and shows as null; implement removes this"]
fn a_null_version_is_refused_naming_the_field() {
    assert_refused_naming("top-level-null-version", "name: m\nversion:\n", "version");
}

#[test]
#[ignore = "red until #629: a version that is a map, a list or a bool loads; implement removes this"]
fn a_version_that_is_neither_number_nor_string_is_refused_naming_the_field() {
    for version in ["{a: 1}", "[1, 2]", "true"] {
        assert_refused_naming(
            "top-level-shape-version",
            &format!("name: m\nversion: {version}\n"),
            "version",
        );
    }
}

/// The type check `name` already had stays, and its help stays the shape help.
#[test]
fn a_name_that_is_a_list_or_a_map_is_still_refused() {
    for name in ["[a, b]", "{a: 1}"] {
        assert_refused_naming(
            "top-level-shape-name",
            &format!("name: {name}\nversion: 1\n"),
            "name",
        );
    }
}

/// A number or a string is a version, and every surface shows it as written.
#[test]
fn a_version_that_is_a_number_or_a_string_loads_as_written() {
    for (version, shown) in [("1", "1"), ("1.5", "1.5"), ("\"1.2\"", "1.2"), ("v2", "v2")] {
        let dir = unique_temp_dir("top-level-version-ok");
        write_fixture_file(
            &dir,
            "states.yaml",
            &machine(&format!("name: m\nversion: {version}\n")),
        );
        let run = run_in(&["states", "--state-machine", "states.yaml"], &dir);
        assert!(run.status.success(), "version: {version} loads: {}", run.stderr);
        assert!(
            run.stdout.contains(&format!("State machine: m (version: {shown})")),
            "version: {version} is shown as written:\n{}",
            run.stdout
        );
    }
}

/// `""` is a string, so the spec's row holds of it; only a null is refused.
#[test]
fn an_empty_string_name_still_loads() {
    let dir = unique_temp_dir("top-level-empty-name");
    write_fixture_file(&dir, "states.yaml", &machine("name: \"\"\nversion: 1\n"));
    let run = run_in(&["states", "--state-machine", "states.yaml"], &dir);
    assert!(run.status.success(), "name: \"\" loads: {}", run.stderr);
}
