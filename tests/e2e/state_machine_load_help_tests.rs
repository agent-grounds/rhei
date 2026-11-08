//! §FS-rhei-errors.1.2.2: a state machine the scan rejects, one holding several
//! documents, one that cannot be read into a value and one with the wrong shape
//! each get their own help, under one headline, whether the refusal came from
//! `rhei states --state-machine` or `rhei validate`.

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

/// The plan `rhei validate` reads beside the machine.
const PLAN: &str = "# Rhei: Plan\n\n## Tasks\n\n### Task a: First\n\n**State:** open\n\nDo it.\n";

/// One state with neither `name` nor `version` beside it.
const OPEN_ONLY: &str = "states:\n  open:\n    initial: true\n";

/// Drive the built binary over `machine` both ways and hand back the help each
/// printed, having checked the exit code and that the headline is exactly the
/// one §FS-rhei-errors.1.2.2 keeps, with `cause` as its cause.
fn refusal_help(prefix: &str, machine: &str, cause: &str) -> Vec<(&'static str, String)> {
    let dir = unique_temp_dir(prefix);
    write_fixture_file(&dir, "states.yaml", machine);
    write_fixture_file(&dir, "plan.rhei.md", PLAN);
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
fn a_valid_mapping_loads_through_both_commands() {
    let dir = unique_temp_dir("load-help-named");
    let profiles = "profiles:\n  work:\n    initial: open\n    allowed: [open, done]\n\
                    node_policy:\n  root: work\n  default: work\n";
    // With profiles the initial state belongs to the profile alone.
    let body = NO_NAME.replace("    initial: true\n", "");
    write_fixture_file(&dir, "states.yaml", &format!("name: open-done\n{body}{profiles}"));
    write_fixture_file(&dir, "plan.rhei.md", PLAN);
    for args in
        [&["states", "--state-machine", "states.yaml"][..], &["validate", "plan.rhei.md"][..]]
    {
        let run = run_in(args, &dir);
        assert!(run.status.success(), "{args:?} accepts the mapping control: {}", run.stderr);
    }
}

#[test]
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

/// §FS-rhei-errors.1.2.2: the headline itself explains the missing mapping,
/// independently of the help that names its keys and the command to re-check it.
#[test]
#[ignore = "red until #633: non-mapping headlines must name the required mapping; implement removes this"]
fn a_document_with_no_mapping_gets_a_yaml_facing_headline_and_actionable_help() {
    let cases = [
        ("empty", ""),
        ("comment-only", "# nothing\n"),
        ("null", "null\n"),
        ("string", "just a string\n"),
        ("integer", "42\n"),
        ("sequence", "- a\n- b\n"),
        ("tagged sequence", "!foo\n- a\n- b\n"),
    ];
    let cause = "the document must be a mapping";
    let expected = format!("failed to parse state machine 'states.yaml': {cause}");
    let mut wrong_headlines = Vec::new();
    for (label, machine) in cases {
        let dir = unique_temp_dir("load-help-no-mapping");
        write_fixture_file(&dir, "states.yaml", machine);
        write_fixture_file(&dir, "plan.rhei.md", PLAN);
        for (command, args) in [
            ("states", &["states", "--state-machine", "states.yaml"][..]),
            ("validate", &["validate", "plan.rhei.md"][..]),
        ] {
            let run = run_in(args, &dir);
            assert_eq!(run.status.code(), Some(1), "{command} on {label}: {}", run.stderr);
            let headline = rendered_line(&run.stderr, "× ");
            if headline != expected {
                wrong_headlines.push(format!("{command} on {label}: {headline}"));
            }
            let help = rendered_line(&run.stderr, "help: ");
            assert_shape_help(command, cause, &help);
            assert!(help.contains("Write a mapping"), "`rhei {command}` on {label}: {help}");
            for key in ["name", "version", "states"] {
                assert_names_key(command, key, &help);
            }
        }
    }
    assert!(
        wrong_headlines.is_empty(),
        "expected headline for every non-mapping: {expected}\n{}",
        wrong_headlines.join("\n")
    );
}

/// A repeated key scans clean and cannot be told from a limit without reading the
/// message, so its help makes no claim about the YAML; the headline names the key,
/// and the position its help refers to is not there.
#[test]
fn a_duplicate_key_is_told_to_correct_what_the_headline_names() {
    let machine = "name: m\nname: n\nversion: 1\nstates:\n  open:\n    initial: true\n";
    let cause = "duplicate entry with key \"name\"";
    for (command, help) in refusal_help("load-help-duplicate", machine, cause) {
        assert!(!help.contains("not valid YAML"), "`rhei {command}`: {help}");
        assert!(!help.contains("Fix the syntax"), "`rhei {command}`: {help}");
        assert!(help.contains(RECHECK), "`rhei {command}`: {help}");
        assert!(!help.contains("position"), "`rhei {command}`: no position is printed: {help}");
    }
}

/// serde_yaml prints no mark at the first character, so this is a real syntax
/// failure whose help has no position to point at.
#[test]
fn a_syntax_failure_at_the_first_character_points_at_no_position() {
    let cause = "found character that cannot start any token, while scanning for the next token";
    for (command, help) in refusal_help("load-help-first-char", "@x\n", cause) {
        assert!(help.contains("not valid YAML"), "`rhei {command}`: {help}");
        assert!(help.contains(RECHECK), "`rhei {command}`: {help}");
        assert!(!help.contains("position"), "`rhei {command}`: no position is printed: {help}");
    }
}

/// A required key is absent from each of these, yet the headline names something
/// else, because an entry that fails is reported first. Correcting the absent key
/// would answer a question the headline never asked.
#[test]
fn a_wrong_shape_beside_an_absent_key_makes_no_top_level_claim() {
    let cases = [
        (
            "name: m\nstates:\n  open:\n    description: [a, b]\n".to_string(),
            "states.open.description: invalid type: sequence, expected a string \
             at line 4 column 18",
        ),
        (
            "version: 1\nstates: 3\n".to_string(),
            "states: invalid type: integer `3`, expected a map at line 2 column 9",
        ),
        (
            format!("name: m\n{OPEN_ONLY}    inputs:\n      - bogus: 1\n"),
            "states.open.inputs[0]: missing field `name` at line 6 column 9",
        ),
        (
            "name: [a]\nversion: 1\nstates: {}\n".to_string(),
            "name: invalid type: sequence, expected a string at line 1 column 7",
        ),
        (
            format!("version: 1\n{OPEN_ONLY}transitions: 3\n"),
            "transitions: invalid type: integer `3`, expected a sequence at line 5 column 14",
        ),
    ];
    for (machine, cause) in cases {
        for (command, help) in refusal_help("load-help-beside", &machine, cause) {
            assert_shape_help(command, cause, &help);
            assert!(!help.contains("top-level"), "`rhei {command}` on {cause}: {help}");
        }
    }
}

/// Nothing but `name` is wrong with these, however they spell the rest: a plain `3`
/// a string field reads from text, a second absent key the headline does not reach,
/// a tag on the document, a flow mapping.
#[test]
fn an_absent_name_is_found_however_the_document_is_written() {
    let cases = [
        format!("version: 1\ndescription: 3\n{OPEN_ONLY}"),
        OPEN_ONLY.to_string(),
        format!("!foo\nversion: 1\n{OPEN_ONLY}"),
        "{version: 1, states: {}}\n".to_string(),
    ];
    let cause = "missing field `name`";
    for machine in cases {
        for (command, help) in refusal_help("load-help-absent", &machine, cause) {
            assert_shape_help(command, cause, &help);
            assert_names_key(command, "name", &help);
            assert!(!help.contains("`version"), "`rhei {command}` on {machine:?}: {help}");
            assert!(help.contains("top-level"), "`rhei {command}` on {machine:?}: {help}");
        }
    }
}

#[test]
fn several_clean_documents_are_told_to_keep_one() {
    let cases = [
        "name: a\nversion: 1\nstates: {}\n---\nname: b\nversion: 1\nstates: {}\n",
        "name: a\nversion: 1\nstates: {}\n...\n---\n",
    ];
    let cause = "deserializing from YAML containing more than one document is not supported";
    for machine in cases {
        for (command, help) in refusal_help("load-help-documents", machine, cause) {
            assert!(help.contains("one document"), "`rhei {command}` on {machine:?}: {help}");
            assert!(help.contains(RECHECK), "`rhei {command}`: {help}");
            assert!(!help.contains("not valid YAML"), "`rhei {command}`: {help}");
            assert!(!help.to_lowercase().contains("syntax"), "`rhei {command}`: {help}");
        }
    }
}

/// The scan accepts each of these as one document and the read into a value still
/// refuses it, so neither "valid YAML" nor "not valid YAML" is the help's to say.
#[test]
fn a_well_formed_file_that_cannot_be_read_is_not_told_to_fix_its_syntax() {
    let mut bomb = "a0: &a0 [x,x,x,x,x,x,x,x,x,x]\n".to_string();
    for n in 1..8 {
        let items = vec![format!("*a{}", n - 1); 10].join(",");
        bomb.push_str(&format!("a{n}: &a{n} [{items}]\n"));
    }
    let cases = [
        (
            "name: m\nversion: 99999999999999999999999\nstates: {}\n".to_string(),
            "version: invalid type: integer `99999999999999999999999` as u128, \
             expected any YAML value at line 2 column 10",
        ),
        (
            format!("a: {}{}\n", "[".repeat(200), "]".repeat(200)),
            "recursion limit exceeded at line 1 column 131",
        ),
        (bomb, "repetition limit exceeded"),
    ];
    for (machine, cause) in cases {
        for (command, help) in refusal_help("load-help-unreadable", &machine, cause) {
            assert!(help.contains(RECHECK), "`rhei {command}` on {cause}: {help}");
            assert!(!help.contains("valid YAML"), "`rhei {command}` on {cause}: {help}");
            assert!(!help.to_lowercase().contains("syntax"), "`rhei {command}` on {cause}: {help}");
            assert!(!help.contains("position"), "`rhei {command}` on {cause}: {help}");
        }
    }
}

/// The headline says "more than one document" of a file whose second document does
/// not scan, so the scan and not that message has to decide.
#[test]
fn a_document_that_does_not_scan_is_a_syntax_failure_however_many_there_are() {
    let cause = "deserializing from YAML containing more than one document is not supported";
    for (command, help) in refusal_help("load-help-second-bad", "a: 1\n---\nb: [\n", cause) {
        assert!(help.contains("not valid YAML"), "`rhei {command}`: {help}");
        assert!(!help.contains("one document"), "`rhei {command}`: {help}");
    }
}

/// A byte-order mark holds one document, so it must not hear about documents. What
/// else happens to such a file is left open, so nothing else is asserted of it.
#[test]
fn a_byte_order_mark_is_not_told_about_documents() {
    let dir = unique_temp_dir("load-help-bom");
    write_fixture_file(&dir, "states.yaml", &format!("\u{feff}name: m\nversion: 1\n{OPEN_ONLY}"));
    write_fixture_file(&dir, "plan.rhei.md", PLAN);
    for args in
        [&["states", "--state-machine", "states.yaml"][..], &["validate", "plan.rhei.md"][..]]
    {
        let run = run_in(args, &dir);
        let help = run.stderr.lines().find_map(|line| line.trim_start().strip_prefix("help: "));
        assert!(
            !help.is_some_and(|help| help.contains("one document")),
            "{args:?}: {}",
            run.stderr
        );
    }
}
