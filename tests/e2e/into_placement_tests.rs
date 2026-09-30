//! `rhei instantiate <template> --into <rhei>[.<task>]`: a template's states,
//! edges, profile, kind and tickets added to a plan that already exists, under
//! the names their authors wrote.
//! §FS-rhei-library.9 §FS-rhei-library.10 §FS-rhei-library.12
//!
//! Every test here fails today with `error: unexpected argument '--into'
//! found`, because the flag does not exist. That is absence rather than a
//! misdirected pin, and it is the right failure for a capability that is not
//! there: the assertions describe what the union must do, not what the argument
//! parser currently says about it.

use super::into_support::*;
use super::*;

/// The whole of a placement at a rhei's top level: the machine gains the
/// template's states, its edge set, its profile and its `by_type` route; the
/// template's own wildcard is written *scoped* to its own states; the tickets
/// land where `rhei new` writes them with their `**Prior:**` rewritten; the
/// fence records the inclusion; and the host's own bytes do not move.
/// §FS-rhei-library.10 §FS-rhei-library.11.2 §FS-rhei-library.15.1
#[test]
fn into_a_rhei_adds_the_templates_graph_and_tickets() {
    let (dir, root) = host_workspace("into-top-level");
    write_review_template(&dir);
    let index_before = read(&root.join("index.rhei.md"));
    let host_ticket_before = read(&root.join("tasks/001-ticket.md"));

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert_success(&result);

    let machine = read(&root.join("states.yaml"));
    // The names the author wrote, not `m6_review__review`. §FS-rhei-library.9
    for state in ["review:", "decide:"] {
        assert!(machine.contains(state), "machine should gain state {state}; got:\n{machine}");
    }
    assert!(
        machine.contains("review-loop:"),
        "machine should gain the template's profile; got:\n{machine}"
    );
    assert!(
        machine.contains("step: review-loop"),
        "machine should gain the template's by_type route; got:\n{machine}"
    );
    // The host's own unscoped wildcard is left as written and spans the union.
    assert!(
        machine.contains("- from: \"*\"\n    to: cancelled\n"),
        "the host's own wildcard must stay unscoped; got:\n{machine}"
    );
    // The template's wildcard is scoped to the template's own states, so its
    // cancel-from-anywhere edge cannot capture the host's. §FS-rhei-library.11.2
    assert!(
        machine.contains("sources:"),
        "the template's wildcard must be written with sources:; got:\n{machine}"
    );
    for scoped in ["review", "decide"] {
        assert!(
            machine.contains(scoped),
            "the scoped source set should name {scoped}; got:\n{machine}"
        );
    }
    assert!(
        !machine.contains("pending\n      - completed"),
        "the template's scoped wildcard must not list the host's states; got:\n{machine}"
    );

    // One fence comment per inclusion is the whole of provenance.
    assert!(
        machine.contains("# --- review-loop 1.0.0 src:sha256:"),
        "states.yaml should carry one fence comment; got:\n{machine}"
    );
    assert!(machine.contains("inputs: {"), "the fence should record the inputs; got:\n{machine}");
    assert!(
        !root.join(".agent-grounds/rhei/composition.lock.json").exists(),
        "a union writes no composition lock"
    );

    // Tickets land where `rhei new` writes a top-level ticket. §FS-rhei-new.3.1
    let coordinate = read(&root.join("tasks/002-coordinate.md"));
    assert!(
        coordinate.contains("### Step coordinate: Coordinate review of HEAD~1"),
        "placed ticket should keep its relative id at a rhei's top level; got:\n{coordinate}"
    );
    let record = read(&root.join("tasks/003-record.md"));
    assert!(
        record.contains("**Prior:** coordinate"),
        "a top-level placement leaves the ids alone; got:\n{record}"
    );

    // The frontmatter unions: the template's kind joins the host's.
    let index = read(&root.join("index.rhei.md"));
    assert!(index.contains("- step"), "nodeKinds should gain `step`; got:\n{index}");
    assert!(index.contains("- task"), "nodeKinds must keep the host's kinds; got:\n{index}");

    // The host's own ticket is untouched, byte for byte.
    assert_eq!(
        host_ticket_before,
        read(&root.join("tasks/001-ticket.md")),
        "a placement must not rewrite the host's own ticket"
    );
    // The host's index keeps every line it had; the union only inserts.
    for line in index_before.lines() {
        assert!(index.contains(line), "the host's index lost the line {line:?}; got:\n{index}");
    }

    let validate = run_into(&["validate", "release"], &dir);
    assert_success(&validate);
}

/// `--into <rhei>.<task>` re-parents the ids, deepens the headings, rewrites
/// the `**Prior:**` inside the template to the placed id, appends to the
/// parent's file because a task file owns its subtree, and grows the host's
/// `maxLevels` to the depth the placement needs.
/// §FS-rhei-library.12 §FS-rhei-library.12.1
#[test]
fn into_a_task_reparents_the_placed_tickets() {
    let (dir, root) = host_workspace("into-under-task");
    write_review_template(&dir);

    let result = run_into(
        &["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release.ticket"],
        &dir,
    );
    assert_success(&result);

    // A subtask is appended to the parent's file, not given a file of its own.
    assert!(
        !root.join("tasks/002-coordinate.md").exists(),
        "a subtask placement writes into the parent's file"
    );
    let parent = read(&root.join("tasks/001-ticket.md"));
    assert!(
        parent.contains("#### Step ticket.coordinate: Coordinate review of HEAD~1"),
        "the placed id should be re-parented and the heading deepened; got:\n{parent}"
    );
    assert!(
        parent.contains("**Prior:** ticket.coordinate"),
        "a `**Prior:**` naming a template task should be rewritten to the placed id; got:\n{parent}"
    );
    assert!(
        parent.contains("### Task ticket: Ship it"),
        "the host's own ticket must keep its heading; got:\n{parent}"
    );

    let index = read(&root.join("index.rhei.md"));
    assert!(
        index.contains("maxLevels: 2"),
        "depth 2 is what this placement needs, so maxLevels stays; got:\n{index}"
    );

    let validate = run_into(&["validate", "release"], &dir);
    assert_success(&validate);
}

/// Into a single-file rhei the tickets go inside `## Tasks`, which is the one
/// placement rule that differs by workspace shape. §FS-rhei-new.3.1
#[test]
fn into_a_single_file_rhei_writes_inside_the_tasks_section() {
    let (dir, _root) = host_single_file("into-single-file");
    write_review_template(&dir);
    let plan_path = dir.join("release.rhei.md");

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert_success(&result);

    let plan = read(&plan_path);
    let tasks_at = plan.find("## Tasks").expect("the plan should keep its Tasks section");
    let placed_at =
        plan.find("### Step coordinate:").expect("the placed ticket should be in the plan");
    assert!(placed_at > tasks_at, "a placed ticket goes inside `## Tasks`; got:\n{plan}");
    assert!(
        plan.contains("### Task ticket: Ship it"),
        "the host's own ticket must survive; got:\n{plan}"
    );

    let validate = run_into(&["validate", "release.rhei.md"], &dir);
    assert_success(&validate);
}

/// `--dry-run` prints the insertion and writes nothing: the diff a caller reads
/// before landing a union, and the promise that a refusal leaves the target
/// byte-identical. §FS-rhei-library.10
#[test]
fn into_dry_run_prints_the_diff_and_writes_nothing() {
    let (dir, root) = host_workspace("into-dry-run");
    write_review_template(&dir);
    let machine_before = read(&root.join("states.yaml"));
    let index_before = read(&root.join("index.rhei.md"));

    let result = run_into(
        &["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release", "--dry-run"],
        &dir,
    );
    assert_success(&result);

    assert!(
        result.stdout.contains("release/states.yaml"),
        "the dry run should name the machine it would write; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("review") && result.stdout.contains("decide"),
        "the dry run should name the states it would add; got:\n{}",
        result.stdout
    );
    assert_eq!(machine_before, read(&root.join("states.yaml")), "--dry-run writes nothing");
    assert_eq!(index_before, read(&root.join("index.rhei.md")), "--dry-run writes nothing");
    assert!(!root.join("tasks/002-coordinate.md").exists(), "--dry-run writes no ticket");
}

/// Placing the same template twice adds nothing to the machine and refuses only
/// the tickets whose ids are taken — so under *another* task it succeeds, and
/// the machine is unchanged because every definition is already there and
/// identical. §FS-rhei-library.11.1 §FS-rhei-library.13
#[test]
fn the_same_template_twice_adds_the_machine_once() {
    let (dir, root) = host_workspace("into-twice");
    write_review_template(&dir);

    let first =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert_success(&first);
    let machine_after_first = read(&root.join("states.yaml"));

    let second = run_into(
        &["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release.ticket"],
        &dir,
    );
    assert_success(&second);

    let machine_after_second = read(&root.join("states.yaml"));
    assert_eq!(
        machine_after_first.matches("  review:\n").count(),
        machine_after_second.matches("  review:\n").count(),
        "a definition already present and identical is added once, not twice"
    );
    let parent = read(&root.join("tasks/001-ticket.md"));
    assert!(
        parent.contains("ticket.coordinate"),
        "the second placement's tickets land under the other parent; got:\n{parent}"
    );

    let validate = run_into(&["validate", "release"], &dir);
    assert_success(&validate);
}
