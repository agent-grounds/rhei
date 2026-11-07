//! §FS-rhei-errors.1.2.2: a state machine that does not parse and one that
//! parses with the wrong shape get different help, under one headline, whether
//! the refusal came from `rhei states --state-machine` or `rhei validate`.

use std::path::Path;

use super::*;

/// The re-check every refusal ends in. The fixtures sit in the working directory,
/// so the path both commands print is the bare file name.
const RECHECK: &str = "rhei states --state-machine states.yaml";

/// The machine from the ticket, which `rhei` accepts once `name:` is added.
const NO_NAME: &str = "version: 1.0
states:
  open:
    description: Not finished
    initial: true
  done:
    description: Finished
    final: true
transitions:
  - from: open
    to: done
";

/// Drive the built binary over `machine` both ways and hand back the help each
/// printed, having checked the exit code and that the headline is exactly the
/// one §FS-rhei-errors.1.2.2 keeps, with `cause` as its cause.
fn refusal_help(prefix: &str, machine: &str, cause: &str) -> Vec<(&'static str, String)> {
    let dir = unique_temp_dir(prefix);
    write_fixture_file(&dir, "states.yaml", machine);
    write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Plan\n\n## Tasks\n\n### Task a: First\n\n**State:** open\n\nDo it.\n",
    );
    [
        ("states", &["states", "--state-machine", "states.yaml"][..]),
        ("validate", &["validate", "plan.rhei.md"][..]),
    ]
    .into_iter()
    .map(|(command, args)| {
        let run = run_in(args, &dir);
        assert_eq!(
            run.status.code(),
            Some(1),
            "`rhei {command}` refuses this machine\nstdout:\n{}\nstderr:\n{}",
            run.stdout,
            run.stderr
        );
        let headline = rendered_line(&run.stderr, "× ");
        assert_eq!(
            headline,
            format!("failed to parse state machine 'states.yaml': {cause}"),
            "`rhei {command}` keeps the headline; got:\n{}",
            run.stderr
        );
        (command, rendered_line(&run.stderr, "help: "))
    })
    .collect()
}

fn run_in(args: &[&str], cwd: &Path) -> CliRun {
    let home = unique_temp_dir("load-help-home");
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

/// The claims the ticket found false of a shape failure, and the one thing every
/// help owes its reader. A position may be mentioned only where one is printed.
fn assert_shape_help(command: &str, cause: &str, help: &str) {
    assert!(help.contains(RECHECK), "`rhei {command}`: the help re-checks the file: {help}");
    assert!(help.contains("is valid YAML"), "`rhei {command}`: the YAML is valid: {help}");
    assert!(!help.contains("not valid YAML"), "`rhei {command}`: the YAML is valid: {help}");
    assert!(!help.to_lowercase().contains("syntax"), "`rhei {command}`: no syntax to fix: {help}");
    if !cause.contains(" at line ") {
        assert!(!help.contains("position"), "`rhei {command}`: no position is printed: {help}");
    }
}

/// `key` as help spells a key: opened by a backtick, so `name` is not found in
/// some other word.
fn assert_names_key(command: &str, key: &str, help: &str) {
    let spelled = format!("`{key}");
    assert!(help.contains(&spelled), "`rhei {command}`: the help names {spelled}`: {help}");
}

#[test]
fn a_file_that_does_not_parse_keeps_its_help_word_for_word() {
    let help = refusal_help(
        "load-help-broken",
        "name: x\nstates: [a, b\nfoo: 1\n",
        "did not find expected ',' or ']' at line 3 column 4, \
         while parsing a flow sequence at line 2 column 9",
    );
    for (command, help) in help {
        assert_eq!(
            help,
            "'states.yaml' is not valid YAML. Fix the syntax at the position above, then \
             re-check it with: rhei states --state-machine states.yaml",
            "`rhei {command}`"
        );
    }
}

#[test]
fn the_tickets_machine_loads_once_it_has_a_name() {
    let dir = unique_temp_dir("load-help-named");
    write_fixture_file(&dir, "states.yaml", &format!("name: open-done\n{NO_NAME}"));
    let run = run_in(&["states", "--state-machine", "states.yaml"], &dir);
    assert!(run.status.success(), "the missing name is the only fault: {}", run.stderr);
}

#[test]
#[ignore = "red until #615: a missing top-level field is told to fix YAML syntax; implement removes this"]
fn a_missing_top_level_field_gets_the_edit_that_adds_it() {
    let machine_without = |key: &str| match key {
        "name" => NO_NAME.to_string(),
        "version" => format!("name: m\n{}", NO_NAME.trim_start_matches("version: 1.0\n")),
        "states" => "name: m\nversion: 1\n".to_string(),
        other => panic!("{other} is not a required top-level field"),
    };
    // §FS-rhei-states.1.1 lists these three as the required ones, with these shapes.
    for (key, shape) in [("name", "string"), ("version", "number"), ("states", "map")] {
        let cause = format!("missing field `{key}`");
        let machine = machine_without(key);
        for (command, help) in refusal_help("load-help-missing", &machine, &cause) {
            assert_shape_help(command, &cause, &help);
            assert_names_key(command, key, &help);
            assert!(help.contains("top-level"), "`rhei {command}`: says where it goes: {help}");
            assert!(help.contains(shape), "`rhei {command}`: says {key} is a {shape}: {help}");
        }
    }
}

/// Each of these is refused at a key path the headline prints, so "top-level" is
/// false of most and owed to none. The `name` missing inside an input is the one
/// a classifier reading only the field's name would send to the top level.
#[test]
#[ignore = "red until #615: a wrong shape below the top level is told to fix YAML syntax; implement removes this"]
fn a_wrong_shape_elsewhere_makes_no_top_level_claim() {
    let states = "name: m\nversion: 1\nstates:\n  open:\n    initial: true\n";
    let cases = [
        (
            "name: m\nversion: 1\nstates: 3\n".to_string(),
            "states: invalid type: integer `3`, expected a map at line 3 column 9",
        ),
        (
            "name: m\nversion: 1\nstates:\n  open:\n    description: [a, b]\n".to_string(),
            "states.open.description: invalid type: sequence, expected a string \
             at line 5 column 18",
        ),
        (
            format!("{states}    effort: bogus\n"),
            "states.open.effort: unknown variant `bogus`, expected one of `off`, `minimal`, \
             `low`, `medium`, `high`, `xhigh`, `max` at line 6 column 13",
        ),
        (
            format!("{states}transitions:\n  - from: open\n"),
            "transitions[0]: missing field `to` at line 7 column 5",
        ),
        (
            format!("{states}    inputs:\n      - bogus: 1\n"),
            "states.open.inputs[0]: missing field `name` at line 7 column 9",
        ),
        (
            format!("{states}    snapshot:\n      bogus: 1\n"),
            "states.open.snapshot: unknown field `bogus`, expected `emit` or `inherit` \
             at line 7 column 7",
        ),
    ];
    for (machine, cause) in cases {
        for (command, help) in refusal_help("load-help-nested", &machine, cause) {
            assert_shape_help(command, cause, &help);
            assert!(!help.contains("top-level"), "`rhei {command}` on {cause}: {help}");
        }
    }
}

/// None of these has a top-level mapping to add one field to, so the help asks for
/// the mapping and everything it must hold. The comment-only file is the one
/// missing `name` that prints a position.
#[test]
#[ignore = "red until #615: a document with no mapping is told to fix YAML syntax; implement removes this"]
fn a_document_with_no_mapping_is_told_what_it_must_hold() {
    let cases = [
        ("", "missing field `name`"),
        ("# nothing\n", "missing field `name` at line 2 column 1"),
        ("null\n", "invalid type: unit value, expected struct StateMachine"),
        ("just a string\n", "invalid type: string \"just a string\", expected struct StateMachine"),
        ("- a\n- b\n", "invalid type: sequence, expected struct StateMachine"),
    ];
    for (machine, cause) in cases {
        for (command, help) in refusal_help("load-help-no-mapping", machine, cause) {
            assert_shape_help(command, cause, &help);
            assert!(help.contains("mapping"), "`rhei {command}` on {machine:?}: {help}");
            for key in ["name", "version", "states"] {
                assert_names_key(command, key, &help);
            }
        }
    }
}

/// A repeated key is refused by the parse itself, and YAML does call it invalid, so
/// this stays a syntax failure; only the position it points at is not there.
#[test]
#[ignore = "red until #615: a duplicate key is sent to a position no line prints; implement removes this"]
fn a_syntax_failure_with_no_position_points_at_none() {
    let machine = "name: m\nname: n\nversion: 1\nstates:\n  open:\n    initial: true\n";
    let cause = "duplicate entry with key \"name\"";
    for (command, help) in refusal_help("load-help-duplicate", machine, cause) {
        assert!(help.contains("not valid YAML"), "`rhei {command}`: {help}");
        assert!(help.contains(RECHECK), "`rhei {command}`: {help}");
        assert!(!help.contains("position"), "`rhei {command}`: no position is printed: {help}");
    }
}
