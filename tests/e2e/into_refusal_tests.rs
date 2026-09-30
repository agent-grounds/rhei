//! What `--into` refuses, and the message each refusal prints.
//! §FS-rhei-library.3 §FS-rhei-library.4 §FS-rhei-library.7
//!
//! Each refusal fires on a composition the block compiler could not express at
//! all, so nothing that worked stops working — but a refusal that arrives as a
//! panic, a silent merge or a half-written target is the failure mode the rules
//! exist to prevent, which is why the target's bytes are asserted alongside the
//! message.
//!
//! All of these fail today with `error: unexpected argument '--into' found`.

use std::path::Path;

use super::into_support::*;
use super::*;

/// Give the template a state the host already defines, differing in an
/// operative field. Rule 1 refuses and names both sources and the field; it
/// does not pick a winner and does not qualify either name.
/// §FS-rhei-library.3.1
#[test]
fn a_same_named_state_that_differs_is_refused_naming_both_sources() {
    let (dir, root) = host_workspace("into-state-clash");
    let template = write_review_template(&dir);
    // `pending` is the host's, with different instructions.
    let machine = read(&template.join("states.yaml")).replace(
        "  decide:\n",
        "  pending:\n    description: Ready for work\n    instructions: |\n      Something else entirely.\n  decide:\n",
    );
    write_fixture_file(&template, "states.yaml", &machine);
    let machine_before = read(&root.join("states.yaml"));

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert!(!result.status.success(), "a differing same-named state must be refused");
    assert_stderr_contains(&result, "pending");
    assert_stderr_contains(&result, "instructions");
    assert_stderr_contains(&result, "review-loop");
    assert_eq!(
        machine_before,
        read(&root.join("states.yaml")),
        "nothing is written until the whole union validates"
    );
}

/// Two same-named profiles is the case that makes the shipped built-ins owe a
/// re-authoring: `code-review` and `fix` both declare `profiles.primary` with
/// different `initial` and different `allowed`, and `allowed` is wholesale.
/// §FS-rhei-library.3.1 §FS-rhei-states.8.2
#[test]
fn two_same_named_profiles_are_refused() {
    let (dir, root) = host_workspace("into-profile-clash");
    let template = write_review_template(&dir);
    let machine = read(&template.join("states.yaml")).replace(
        "  review-loop:\n    initial: review\n",
        "  host:\n    initial: review\n    allowed: [review, decide, completed, cancelled]\n  review-loop:\n    initial: review\n",
    );
    write_fixture_file(&template, "states.yaml", &machine);
    let machine_before = read(&root.join("states.yaml"));

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert!(!result.status.success(), "two differing same-named profiles must be refused");
    assert_stderr_contains(&result, "host");
    assert_eq!(machine_before, read(&root.join("states.yaml")), "the target is left alone");
}

/// A ticket id already taken in the target is refused before anything is
/// written, with the message the placement rule specifies verbatim.
/// §FS-rhei-library.4
#[test]
fn a_taken_ticket_id_is_refused_with_its_own_message() {
    let (dir, root) = host_workspace("into-id-collision");
    write_review_template(&dir);

    let first =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert_success(&first);
    let machine_before = read(&root.join("states.yaml"));
    let placed_before = read(&root.join("tasks/002-coordinate.md"));

    // The same template at the same parent: every id is taken.
    let second =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert!(!second.status.success(), "a taken ticket id must be refused");
    assert_stderr_contains(
        &second,
        "cannot place ticket 'coordinate': task id already exists in target",
    );
    assert_eq!(machine_before, read(&root.join("states.yaml")), "the machine is left alone");
    assert_eq!(
        placed_before,
        read(&root.join("tasks/002-coordinate.md")),
        "the ticket that was already there is not rewritten"
    );
}

/// Two states declaring one rhei-scoped artifact path — no `{task_id}` in it —
/// is a union-time collision, refused naming both states and the path. The
/// other case, one state walked by two tickets, is a warning rather than a
/// refusal, because whether it matters is the author's call.
/// §FS-rhei-library.7.2
#[test]
fn one_rhei_scoped_artifact_path_claimed_by_two_states_is_refused() {
    let (dir, root) = host_workspace("into-artifact-clash");
    let template = write_review_template(&dir);
    // The host's `pending` writes the note; so does the template's `review`.
    let host_machine = read(&root.join("states.yaml")).replace(
        "    instructions: |\n      Do the work and move on.\n",
        "    instructions: |\n      Do the work and move on.\n    outputs:\n      - name: plan-note\n        path: runtime/notes/plan.md\n        description: The one plan note\n",
    );
    write_fixture_file(&root, "states.yaml", &host_machine);
    let template_machine = read(&template.join("states.yaml")).replace(
        "      Review {{change_ref}} and write what you found.\n",
        "      Review {{change_ref}} and write what you found.\n    outputs:\n      - name: plan-note\n        path: runtime/notes/plan.md\n        description: The one plan note\n",
    );
    write_fixture_file(&template, "states.yaml", &template_machine);
    let machine_before = read(&root.join("states.yaml"));

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert!(!result.status.success(), "two states claiming one rhei-scoped path must be refused");
    assert_stderr_contains(&result, "runtime/notes/plan.md");
    assert_stderr_contains(&result, "pending");
    assert_stderr_contains(&result, "review");
    assert_eq!(machine_before, read(&root.join("states.yaml")), "the target is left alone");
}

/// The basin holds unfiled tickets that run under the project default and has
/// no machine of its own, so it is never a `--into` target.
/// §FS-rhei-library.2.1
#[test]
fn the_basin_is_never_a_target() {
    let dir = unique_temp_dir("into-basin");
    write_fixture_file(&dir, "index.panta.md", "# Panta: Work\n**States:** host\n");
    write_fixture_file(&dir, "states.yaml", HOST_MACHINE);
    std::fs::create_dir_all(dir.join("basin")).expect("create basin");
    write_fixture_file(&dir.join("basin"), "001-loose.md", HOST_TICKET);
    write_review_template(&dir);

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "basin"], &dir);
    assert!(!result.status.success(), "the basin must be refused as a target");
    assert_stderr_contains(&result, "basin");
}

/// Each flag `--into` cannot be combined with is an error naming the pair,
/// rather than a meaning invented for the combination.
/// §FS-rhei-library.7.3
#[test]
fn into_refuses_the_flags_it_cannot_combine_with() {
    let (dir, _root) = host_workspace("into-flag-clashes");
    write_review_template(&dir);

    let cases: &[(&[&str], &str)] = &[
        (&["--output", "out"], "--output"),
        (&["--execute"], "--execute"),
        (&["--keep-on-error"], "--keep-on-error"),
        (&["--state-machine", "release/states.yaml"], "--state-machine"),
    ];
    for (extra, named) in cases {
        let mut args: Vec<&str> =
            vec!["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"];
        args.extend_from_slice(extra);
        let result = run_into(&args, &dir);
        assert!(!result.status.success(), "--into with {named} must be refused");
        assert_stderr_contains(&result, named);
        assert_stderr_contains(&result, "--into");
    }
}

/// `--mount`, `--seam` and `--pass` are gone, and each says what replaced it:
/// a removed flag that errors with `unexpected argument` teaches nothing.
/// §FS-rhei-library.1
#[test]
fn the_removed_composition_flags_name_their_replacement() {
    let dir = unique_temp_dir("into-removed-flags");

    // A composition that *succeeds* today: the point is that the flag is
    // accepted at all, so the failure after the change is the refusal rather
    // than a complaint about an input.
    let cases: &[&[&str]] = &[
        &["instantiate", "--mount", "review=code-review", "--set", "review.change_ref=HEAD~1"],
        &[
            "instantiate",
            "--mount",
            "review=code-review",
            "--mount",
            "fix=fix",
            "--set",
            "review.change_ref=HEAD~1",
            "--seam",
            "review.done=fix.entry",
        ],
        &[
            "instantiate",
            "--mount",
            "review=code-review",
            "--mount",
            "fix=fix",
            "--set",
            "review.change_ref=HEAD~1",
            "--pass",
            "review.decision=fix.decision",
        ],
    ];
    for args in cases {
        let result = run_into(args, &dir);
        assert!(!result.status.success(), "the removed composition flags must be gone: {args:?}");
        assert_stderr_contains(&result, "--into");
        assert_stderr_contains(&result, "includes:");
    }
}

/// An `includes:` cycle is an error naming the chain, so the message says which
/// templates form the loop rather than that recursion ran out.
/// §FS-rhei-library.6
#[test]
fn an_includes_cycle_names_the_chain() {
    let dir = unique_temp_dir("into-includes-cycle");
    write_including_pair(&dir, "alpha", "beta");
    write_including_pair(&dir, "beta", "alpha");

    let result = run_into(&["instantiate", "alpha", "--output", "out"], &dir);
    assert!(!result.status.success(), "an includes cycle must be refused");
    assert_stderr_contains(&result, "alpha");
    assert_stderr_contains(&result, "beta");
    assert_stderr_contains(&result, "cycle");
}

/// A states-only template whose manifest includes another by name.
fn write_including_pair(dir: &Path, name: &str, includes: &str) {
    let template = dir.join(format!(".agent-grounds/rhei/templates/{name}"));
    std::fs::create_dir_all(template.join("tasks")).expect("create template");
    write_fixture_file(
        &template,
        "template.yaml",
        &format!(
            "name: {name}\nversion: 1.0.0\ndescription: Includes {includes}\ninputs: []\nincludes:\n  - {includes}\n"
        ),
    );
    write_fixture_file(&template, "states.yaml", HOST_MACHINE);
    write_fixture_file(
        &template,
        "index.rhei.md",
        "# Rhei: Including\n**States:** host\n\n---\nstructure:\n  maxLevels: 2\n  nodeKinds:\n  - task\n---\n\n## Overview\n\nAn including template.\n",
    );
    write_fixture_file(
        &template,
        "tasks/001-own.md",
        &format!(
            "### Task own: {name}'s own ticket\n**State:** pending\n\nThe host's own ticket.\n"
        ),
    );
}
