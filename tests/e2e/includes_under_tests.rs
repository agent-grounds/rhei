//! `includes:` in `template.yaml`, and `under:` on an entry: a template built
//! out of templates, with an included template's tickets placed beneath a task
//! of the host.
//! §FS-rhei-library.14 §FS-rhei-library.14.1
//!
//! `under:` is the `<task>` half of `--into <rhei>.<task>` applied one level in,
//! so what these tests pin is that it is the *same* mechanism: the same
//! re-parenting, the same heading deepening, the same `**Prior:**` rewrite, the
//! same writer and the same refusals. A second placement code path would pass
//! some of these and be the defect the design exists to prevent.
//! §AR-rhei-library.6.2
//!
//! Today every one of these fails: `includes:` is not a manifest field, so the
//! including template is refused at manifest validation.

use std::path::{Path, PathBuf};

use super::into_support::*;
use super::*;

/// The including template: its own supervisor state, its own `ticket` task, and
/// one entry placing `review-loop`'s tickets under it.
fn write_including_template(dir: &Path, includes: &str) -> PathBuf {
    let template = dir.join(".agent-grounds/rhei/templates/grounded");
    std::fs::create_dir_all(template.join("tasks")).expect("create including template");
    write_fixture_file(
        &template,
        "template.yaml",
        &format!(
            "name: grounded\nversion: 1.0.0\ndescription: A ticket with a review loop under it\ninputs:\n  - name: change_ref\n    description: The change to review\n    type: string\nincludes:\n{includes}"
        ),
    );
    // The host writes the edge into the included template's entry state and the
    // profile whose `allowed` spans both: a path across templates is the one
    // thing no single template knows. §FS-rhei-library.11.5
    write_fixture_file(
        &template,
        "states.yaml",
        r#"name: host
version: 1
states:
  pending:
    description: Ready for work
    instructions: |
      Do the work and move on.
  completed:
    final: true
    description: Done
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: pending
    to: completed
  - from: "*"
    to: cancelled
profiles:
  host:
    initial: pending
    allowed: [pending, completed, cancelled]
node_policy:
  root: host
  default: host
  by_type:
    task: host
"#,
    );
    write_fixture_file(
        &template,
        "index.rhei.md",
        "# Rhei: Grounded {{change_ref}}\n**States:** host\n\n---\nstructure:\n  maxLevels: 2\n  nodeKinds:\n  - task\n---\n\n## Overview\n\nA ticket with a review loop under it.\n",
    );
    write_fixture_file(
        &template,
        "tasks/001-ticket.md",
        "### Task ticket: Work {{change_ref}}\n**State:** pending\n\nThe supervisor's own ticket.\n",
    );
    template
}

/// `under: ticket` places the included template's tickets beneath the host's
/// `ticket` task: placed ids, deepened headings, the included template's own
/// `**Prior:**` rewritten to the composed id, and `rhei validate` clean.
/// §FS-rhei-library.14.1
#[test]
fn under_places_an_included_templates_tickets_beneath_a_host_task() {
    let dir = unique_temp_dir("includes-under");
    write_review_template(&dir);
    write_including_template(&dir, "  - { template: review-loop, under: ticket }\n");

    let result =
        run_into(&["instantiate", "grounded", "change_ref=HEAD~1", "--output", "out"], &dir);
    assert_success(&result);

    let root = dir.join("out");
    let ticket = read(&root.join("tasks/001-ticket.md"));
    // The placed id is `ticket.coordinate`, not `coordinate`.
    assert!(
        ticket.contains("#### Step ticket.coordinate: Coordinate review of HEAD~1"),
        "the included ticket should be re-parented and its heading deepened; got:\n{ticket}"
    );
    assert!(
        ticket.contains("#### Step ticket.record:"),
        "every included ticket is re-parented; got:\n{ticket}"
    );
    // The `**Prior:**` the included template wrote names a template task, so it
    // is rewritten to the composed id.
    assert!(
        ticket.contains("**Prior:** ticket.coordinate"),
        "an included `**Prior:**` should be rewritten to the placed id; got:\n{ticket}"
    );
    assert!(
        ticket.contains("### Task ticket: Work HEAD~1"),
        "the including template's own ticket is the parent; got:\n{ticket}"
    );

    // The machine is the union: the host's states and the included one's, each
    // under the name its author wrote.
    let machine = read(&root.join("states.yaml"));
    for state in ["pending:", "review:", "decide:"] {
        assert!(machine.contains(state), "the union should carry {state}; got:\n{machine}");
    }
    let index = read(&root.join("index.rhei.md"));
    assert!(index.contains("- step"), "the included kind joins nodeKinds; got:\n{index}");
    assert!(index.contains("- task"), "the host's kinds stay; got:\n{index}");

    let validate = run_into(&["validate", "out"], &dir);
    assert_success(&validate);
}

/// `under:` omitted places at the including template's top level — the shape a
/// wrapper that chains two templates as siblings needs. §FS-rhei-library.14.1
#[test]
fn an_entry_without_under_places_at_the_top_level() {
    let dir = unique_temp_dir("includes-no-under");
    write_review_template(&dir);
    write_including_template(&dir, "  - review-loop\n");

    let result =
        run_into(&["instantiate", "grounded", "change_ref=HEAD~1", "--output", "out"], &dir);
    assert_success(&result);

    let root = dir.join("out");
    let ticket = read(&root.join("tasks/001-ticket.md"));
    assert!(
        !ticket.contains("ticket.coordinate"),
        "with no `under:` nothing is re-parented; got:\n{ticket}"
    );
    let placed_path = root.join("tasks/002-coordinate.md");
    assert!(
        placed_path.exists(),
        "the included template's ticket should have been placed at the top level"
    );
    let placed = read(&placed_path);
    assert!(
        placed.contains("### Step coordinate: Coordinate review of HEAD~1"),
        "a top-level placement keeps the template's own id; got:\n{placed}"
    );

    let validate = run_into(&["validate", "out"], &dir);
    assert_success(&validate);
}

/// `under:` resolves against the task tree as it stands when the entry is
/// reached, so a name no earlier entry has placed is an error naming the entry,
/// the id, and the ids that *are* available — a message about order rather than
/// a mystery. §FS-rhei-library.14.1
#[test]
fn an_under_naming_an_unplaced_task_lists_what_is_available() {
    let dir = unique_temp_dir("includes-forward-ref");
    write_review_template(&dir);
    write_including_template(&dir, "  - { template: review-loop, under: nowhere }\n");

    let result =
        run_into(&["instantiate", "grounded", "change_ref=HEAD~1", "--output", "out"], &dir);
    assert!(!result.status.success(), "an `under:` naming no task must be refused");
    assert_stderr_contains(&result, "nowhere");
    assert_stderr_contains(&result, "review-loop");
    // The ids that *are* available, so the author reads it as an ordering
    // problem rather than as a missing template.
    assert_stderr_contains(&result, "ticket");
}

/// The same template under two parents is legal — different parents, different
/// ids — which is "once per parent" with a way to say the parent. Twice under
/// one parent is the id collision. §FS-rhei-library.14.1 §FS-rhei-library.12
#[test]
fn the_same_template_under_two_parents_is_legal_and_twice_under_one_is_not() {
    let dir = unique_temp_dir("includes-two-parents");
    write_review_template(&dir);
    let template = write_including_template(
        &dir,
        "  - { template: review-loop, under: ticket }\n  - { template: review-loop, under: second }\n",
    );
    write_fixture_file(
        &template,
        "tasks/002-second.md",
        "### Task second: A second parent\n**State:** pending\n\nAnother parent.\n",
    );

    let result =
        run_into(&["instantiate", "grounded", "change_ref=HEAD~1", "--output", "out"], &dir);
    assert_success(&result);
    let root = dir.join("out");
    assert!(
        read(&root.join("tasks/001-ticket.md")).contains("ticket.coordinate"),
        "the first parent should carry its own copy"
    );
    assert!(
        read(&root.join("tasks/002-second.md")).contains("second.coordinate"),
        "the second parent should carry its own copy"
    );

    // Twice under one parent is the id collision, with the placement message.
    write_fixture_file(
        &template,
        "template.yaml",
        "name: grounded\nversion: 1.0.0\ndescription: A ticket with a review loop under it\ninputs:\n  - name: change_ref\n    description: The change to review\n    type: string\nincludes:\n  - { template: review-loop, under: ticket }\n  - { template: review-loop, under: ticket }\n",
    );
    let twice =
        run_into(&["instantiate", "grounded", "change_ref=HEAD~1", "--output", "out2"], &dir);
    assert!(!twice.status.success(), "twice under one parent must be refused");
    assert_stderr_contains(
        &twice,
        "cannot place ticket 'ticket.coordinate': task id already exists in target",
    );
}

/// The two re-parentings compose and both depth limits are checked **once** on
/// the final ids: `under: ticket` inside the template plus
/// `--into <rhei>.<task>` outside it puts the included ticket three deep and
/// grows the host's `maxLevels` to reach it. §FS-rhei-library.12.1
#[test]
fn under_and_into_a_task_compose_into_one_depth_check() {
    let dir = unique_temp_dir("includes-compose-depth");
    write_review_template(&dir);
    write_including_template(&dir, "  - { template: review-loop, under: ticket }\n");
    let (_host_dir, root) = host_workspace_in(&dir, "release");

    let result = run_into(
        &["instantiate", "grounded", "change_ref=HEAD~1", "--into", "release.ticket"],
        &dir,
    );
    assert_success(&result);

    let parent = read(&root.join("tasks/001-ticket.md"));
    assert!(
        parent.contains("##### Step ticket.ticket.coordinate:"),
        "the two re-parentings should compose into one placed id; got:\n{parent}"
    );
    let index = read(&root.join("index.rhei.md"));
    assert!(
        index.contains("maxLevels: 3"),
        "maxLevels should grow to the depth the placement needs; got:\n{index}"
    );

    let validate = run_into(&["validate", "release"], &dir);
    assert_success(&validate);
}

/// Depth 4 is a hard ceiling, because `######` is the deepest heading Markdown
/// gives, and it wins over growing `maxLevels`: a placement past it is refused
/// naming the id that overflows. §FS-rhei-library.12.1
#[test]
fn a_placement_past_depth_four_is_refused_naming_the_id() {
    let dir = unique_temp_dir("includes-depth-ceiling");
    write_review_template(&dir);
    write_including_template(&dir, "  - { template: review-loop, under: ticket }\n");
    let (_host_dir, root) = host_workspace_in(&dir, "release");
    // Deepen the host so the composed placement lands at depth 5.
    write_fixture_file(
        &root.join("tasks"),
        "001-ticket.md",
        "### Task ticket: Ship it\n**State:** pending\n\nThe host's ticket.\n\n#### Task ticket.inner: One level in\n**State:** pending\n\nInner.\n\n##### Task ticket.inner.deeper: Two levels in\n**State:** pending\n\nDeeper.\n",
    );
    write_fixture_file(
        &root,
        "index.rhei.md",
        "# Rhei: Release\n**States:** host\n\n---\nstructure:\n  maxLevels: 4\n  nodeKinds:\n  - task\n---\n\n## Overview\n\nA deep host.\n",
    );
    let index_before = read(&root.join("index.rhei.md"));

    let result = run_into(
        &["instantiate", "grounded", "change_ref=HEAD~1", "--into", "release.ticket.inner.deeper"],
        &dir,
    );
    assert!(!result.status.success(), "a placement past depth 4 must be refused");
    assert_stderr_contains(&result, "ticket.inner.deeper.ticket.coordinate");
    assert_eq!(
        index_before,
        read(&root.join("index.rhei.md")),
        "the ceiling refuses rather than growing maxLevels"
    );
}

/// An including template shows its inputs as the **union** of its own and its
/// parts', so it is used exactly as a flat template is.
/// §FS-rhei-library.11.4
#[test]
fn an_including_template_lists_the_unioned_inputs() {
    let dir = unique_temp_dir("includes-input-union");
    let review = write_review_template(&dir);
    // An input only the *included* template declares.
    write_fixture_file(
        &review,
        "template.yaml",
        "name: review-loop\nversion: 1.0.0\ndescription: Review a change and decide\ninputs:\n  - name: change_ref\n    description: The change to review\n    type: string\n  - name: reviewers\n    description: How many reviewers\n    type: number\n    default: 2\n",
    );
    write_including_template(&dir, "  - { template: review-loop, under: ticket }\n");

    let result = run_into(&["instantiate", "grounded", "--list-inputs"], &dir);
    assert_success(&result);
    assert!(
        result.stdout.contains("change_ref"),
        "the including template's own input should be listed; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("reviewers"),
        "an included template's input joins the union; got:\n{}",
        result.stdout
    );
}

/// A host workspace inside an existing directory, for tests that need both a
/// template tier and a target in one tree.
fn host_workspace_in(dir: &Path, name: &str) -> (PathBuf, PathBuf) {
    let root = dir.join(name);
    std::fs::create_dir_all(root.join("tasks")).expect("create host workspace");
    write_fixture_file(&root, "index.rhei.md", HOST_INDEX);
    write_fixture_file(&root, "states.yaml", HOST_MACHINE);
    write_fixture_file(&root.join("tasks"), "001-ticket.md", HOST_TICKET);
    (dir.to_path_buf(), root)
}
