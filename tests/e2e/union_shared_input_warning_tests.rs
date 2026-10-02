//! Two or more states reading one rhei-scoped path that no state declares
//! writing: `rhei instantiate` warns, once, on the completed machine, and never
//! refuses. §FS-rhei-library.7.2.3
//!
//! "Completed" is the whole of it. The warning is read off the machine the
//! command actually writes — every `includes:` entry joined, the target and the
//! placed template together under `--into` — so a writer any part brings clears
//! it and nesting cannot print it twice. Its caveat is part of the line: a
//! missing `outputs:` declaration does not mean nothing produces the file.
//!
//! Before the change there was no such warning, and a reader on each side of a
//! union was refused as two states claiming one path.

use super::into_support::run_into;
use super::union_artifact_paths_support::*;
use super::*;

/// The supervisor and two workers of one template all read the plan note and
/// none declares writing it: one line, readers sorted, optional one included.
/// §FS-rhei-library.7.2.3
#[test]
fn readers_with_no_writer_inside_one_template_warn_once_word_for_word() {
    let dir = unique_temp_dir("shared-input-q5");
    Template {
        name: "solo",
        kind: Some("task"),
        states: &[
            ("supervising", &[MayRead(PLAN)]),
            ("review", &[Reads(PLAN)]),
            ("implement", &[Reads(PLAN)]),
        ],
        tickets: &["ticket"],
        ..Template::default()
    }
    .write(&dir);

    let result = run_into(&["instantiate", "solo", "--output", "out"], &dir);
    assert_success(&result);
    assert_eq!(
        shared_input_warnings(&result),
        [shared_input_line(PLAN, &["implement", "review", "supervising"])],
        "stderr:\n{}",
        result.stderr
    );
}

/// Readers on both sides of an `includes:` union: one line for the whole
/// composition, not one per entry. §FS-rhei-library.7.2.3
#[test]
fn readers_across_includes_warn_once() {
    let dir = unique_temp_dir("shared-input-q6-includes");
    ticket_host(
        &dir,
        &[MayRead(PLAN)],
        &["implementing", "reviewing"],
        &["../implement", "../review"],
    );
    part(&dir, "implement", &[("implementing", &[Reads(PLAN)])]);
    part(&dir, "review", &[("reviewing", &[Reads(PLAN)])]);

    let result = run_into(&["instantiate", "ticket-host", "--output", "out"], &dir);
    assert_success(&result);
    assert_eq!(
        shared_input_warnings(&result),
        [shared_input_line(PLAN, &["implementing", "reviewing", "supervising"])],
        "stderr:\n{}",
        result.stderr
    );
}

/// A reader in the target and one in the placed template: the target is part of
/// the completed machine, so they are two readers of one path.
/// §FS-rhei-library.7.2.3
#[test]
fn readers_across_into_warn_once() {
    let (dir, _root) = target_with("shared-input-q6-into", &[("pending", &[Reads(PLAN)])]);
    placed(&dir, "reader", &[("review", &[Reads(PLAN)])]);

    let result = run_into(&["instantiate", "reader", "--into", "release"], &dir);
    assert_success(&result);
    assert_eq!(
        shared_input_warnings(&result),
        [shared_input_line(PLAN, &["pending", "review"])],
        "stderr:\n{}",
        result.stderr
    );
}

/// Two entries read the path and a later entry writes it. Read part by part the
/// first two would warn; read whole, the file has a producer. §FS-rhei-library.7.2.3
#[test]
fn a_later_include_bringing_a_writer_clears_the_warning() {
    let dir = unique_temp_dir("shared-input-later-writer");
    ticket_host(
        &dir,
        &[],
        &["triaging", "planning", "writing"],
        &["../first-reader", "../second-reader", "../writer"],
    );
    part(&dir, "first-reader", &[("triaging", &[Reads(PLAN)])]);
    part(&dir, "second-reader", &[("planning", &[Reads(PLAN)])]);
    part(&dir, "writer", &[("writing", &[Writes(PLAN)])]);

    let result = run_into(&["instantiate", "ticket-host", "--output", "out"], &dir);
    assert_success(&result);
    assert!(shared_input_warnings(&result).is_empty(), "stderr:\n{}", result.stderr);
}

/// `outer` includes `inner`, which includes the two readers. The inner union is
/// finished before the outer one, and the warning still prints exactly once.
/// §FS-rhei-library.7.2.3
#[test]
fn nesting_does_not_repeat_the_warning() {
    let dir = unique_temp_dir("shared-input-nesting");
    let readers = ["triaging", "planning"];
    Template {
        name: "outer",
        kind: Some("task"),
        states: &[("supervising", &[])],
        spans: &readers,
        includes: &["../inner"],
        tickets: &["ticket"],
        ..Template::default()
    }
    .write(&dir);
    Template {
        name: "inner",
        states: &[("gathering", &[])],
        spans: &readers,
        includes: &["../first-reader", "../second-reader"],
        ..Template::default()
    }
    .write(&dir);
    part(&dir, "first-reader", &[("triaging", &[Reads(PLAN)])]);
    part(&dir, "second-reader", &[("planning", &[Reads(PLAN)])]);

    let result = run_into(&["instantiate", "outer", "--output", "out"], &dir);
    assert_success(&result);
    assert_eq!(
        shared_input_warnings(&result),
        [shared_input_line(PLAN, &["planning", "triaging"])],
        "stderr:\n{}",
        result.stderr
    );
}

/// Guard: one reader is a file someone hands in, not a shared one, and never
/// warns. §FS-rhei-library.7.2.3
#[test]
fn one_reader_never_warns() {
    let dir = unique_temp_dir("shared-input-one-reader");
    Template {
        name: "solo",
        kind: Some("task"),
        states: &[("implement", &[Reads(PLAN)])],
        tickets: &["ticket"],
        ..Template::default()
    }
    .write(&dir);

    let result = run_into(&["instantiate", "solo", "--output", "out"], &dir);
    assert_success(&result);
    assert!(shared_input_warnings(&result).is_empty(), "stderr:\n{}", result.stderr);
}

/// An optional reader counts: a supervisor that may read its own plan note and
/// a worker that must are two readers. §FS-rhei-library.7.2.3
#[test]
fn one_optional_and_one_required_reader_warn_and_both_are_named() {
    let dir = unique_temp_dir("shared-input-optional");
    Template {
        name: "solo",
        kind: Some("task"),
        states: &[("supervising", &[MayRead(PLAN)]), ("implement", &[Reads(PLAN)])],
        tickets: &["ticket"],
        ..Template::default()
    }
    .write(&dir);

    let result = run_into(&["instantiate", "solo", "--output", "out"], &dir);
    assert_success(&result);
    assert_eq!(
        shared_input_warnings(&result),
        [shared_input_line(PLAN, &["implement", "supervising"])],
        "stderr:\n{}",
        result.stderr
    );
}

/// `--dry-run` prints the diagnostics a real placement would and writes
/// nothing. §FS-rhei-library.7.2.3
#[test]
fn into_dry_run_prints_the_warning_and_writes_nothing() {
    let (dir, root) = target_with("shared-input-dry-run", &[("pending", &[Reads(PLAN)])]);
    placed(&dir, "reader", &[("review", &[Reads(PLAN)])]);
    let before = snapshot(&root);

    let result = run_into(&["instantiate", "reader", "--into", "release", "--dry-run"], &dir);
    assert_success(&result);
    assert_eq!(
        shared_input_warnings(&result),
        [shared_input_line(PLAN, &["pending", "review"])],
        "stderr:\n{}",
        result.stderr
    );
    assert_eq!(before, snapshot(&root), "a dry run writes nothing");
}

/// Guard: the warning belongs to the person composing the template, so
/// `rhei validate` on the laid rhei does not repeat it. §FS-rhei-library.7.2.3
#[test]
fn rhei_validate_does_not_repeat_the_warning() {
    let dir = unique_temp_dir("shared-input-validate");
    Template {
        name: "solo",
        kind: Some("task"),
        states: &[("supervising", &[MayRead(PLAN)]), ("implement", &[Reads(PLAN)])],
        tickets: &["ticket"],
        ..Template::default()
    }
    .write(&dir);
    assert_success(&run_into(&["instantiate", "solo", "--output", "out"], &dir));

    let validate = run_into(&["validate", "out"], &dir);
    assert_success(&validate);
    assert!(shared_input_warnings(&validate).is_empty(), "stderr:\n{}", validate.stderr);
}
