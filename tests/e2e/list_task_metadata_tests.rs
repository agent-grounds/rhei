// `rhei list --json` and the frontmatter an author wrote: which layer a query
// publishes, and what happens to a YAML value JSON cannot hold.
//
// The transcript of agent-grounds/rhei#317. A plan authors one field per task
// and a caller outside rhei reads it off the query surface instead of
// re-implementing rhei's id keying. What a *running* plan holds in the same flat
// map is rhei's own bookkeeping, so the case that separates the two layers — and
// the one a fresh example plan cannot distinguish — is a task whose stored map
// holds nothing else.

// §FS-rhei-list.4.2 §FS-rhei-transitions.2.5

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

/// A `review` state that counts visits, so a real `rhei transition` writes
/// `stateVisits` into the same map an author writes into and the layering test
/// does not have to author rhei's own key by hand.
// §FS-rhei-transitions.2.3
const MACHINE: &str = r#"name: list-metadata-e2e
version: 1
states:
  pending:
    initial: true
    description: Ready for work
  review:
    description: Review
    visits: 3
  completed:
    description: Done
    final: true
transitions:
  - { from: pending, to: review, description: Send it to review }
  - { from: review, to: review, description: Another round }
  - { from: review, to: completed, description: Done }
  - { from: "*", to: completed, description: Dropped }
"#;

/// The ticket's own plan: one task, two authored fields, nothing else.
// §FS-rhei-transitions.2
const AUTHORED_PLAN: &str = r#"# Rhei: demo

---
metadata:
  tasks:
    1:
      context: /home/me/checkouts/widget
      priority: high
---

## Tasks

### Task 1: Ship the widget
**State:** pending
"#;

fn single_file(prefix: &str, plan: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", MACHINE);
    (dir, plan_path, machine_path)
}

fn list_json(plan_path: &Path, machine_path: &Path) -> Vec<serde_json::Value> {
    let listed = run_cli("list", plan_path, machine_path, &["--json"]);
    assert_success(&listed);
    serde_json::from_str(&listed.stdout).expect("list JSON should parse")
}

fn element(tasks: &[serde_json::Value], id: &str) -> serde_json::Value {
    tasks
        .iter()
        .find(|task| task["id"] == id)
        .unwrap_or_else(|| panic!("{id} should be listed, got {tasks:#?}"))
        .clone()
}

/// The ask of the ticket: what rhei already parsed and keyed, read off the query
/// surface, with the author's own spellings. §FS-rhei-list.4.2
#[test]
fn list_json_carries_the_authored_metadata_of_a_task() {
    let (_dir, plan_path, machine_path) = single_file("list-meta-authored", AUTHORED_PLAN);

    let task = element(&list_json(&plan_path, &machine_path), "plan.1");

    assert_eq!(
        task["metadata"],
        serde_json::json!({ "context": "/home/me/checkouts/widget", "priority": "high" }),
        "the authored map, keys as authored"
    );
}

/// The case that proves which layer the field carries. `rhei transition` writes
/// `stateVisits` itself, the plan carries a `supervision` block beside it, and
/// the task's stored map holds nothing an author wrote — so the element is the
/// one it was before the field existed, with no `metadata` key at all rather
/// than an empty object. §FS-rhei-list.4.2 §FS-rhei-transitions.2.5
#[test]
fn list_json_omits_metadata_where_the_stored_map_is_only_rheis_own() {
    let plan = r#"# Rhei: demo

---
metadata:
  tasks:
    1:
      supervision:
        phase: released
---

## Tasks

### Task 1: Supervise the subtree
**State:** review
"#;
    let (_dir, plan_path, machine_path) = single_file("list-meta-runtime-only", plan);

    // Let rhei write the counter, so the map under test is rhei's own doing.
    assert_success(&run_transition(&plan_path, &machine_path, "plan.1", "review", "review"));

    let on_disk = fs::read_to_string(&plan_path).expect("plan should be readable");
    assert!(
        on_disk.contains("stateVisits:") && on_disk.contains("supervision:"),
        "both registered keys should be stored on disk, got:\n{on_disk}"
    );

    // And they are reachable: the export that publishes the stored map whole
    // shows exactly what the answer this test refuses would have emitted.
    // §FS-rhei-render.3.1
    let rendered = render_json(&plan_path, &machine_path);
    let stored = &rendered["frontmatter"]["metadata"]["tasks"]["plan.1"];
    assert_eq!(stored["stateVisits"]["review"], 2, "render carries the counter, in {stored:#?}");
    assert_eq!(stored["supervision"]["phase"], "released", "render carries the block");

    let task = element(&list_json(&plan_path, &machine_path), "plan.1");

    assert_eq!(
        task,
        serde_json::json!({
            "id": "plan.1",
            "kind": "task",
            "title": "Supervise the subtree",
            "state": "review-2",
            "assignee": null,
            "prior": [],
            "parent": null,
            "depth": 1,
        }),
        "a task whose stored map is only rhei's own emits the object it always did"
    );
}

/// A mixed map publishes the author's half and nothing else — including where
/// the author wrote a registered name, which keeps its runtime meaning and is
/// held back with the rest. §FS-rhei-transitions.2.5
#[test]
fn list_json_emits_the_authors_half_of_a_mixed_map() {
    let plan = r#"# Rhei: demo

---
metadata:
  tasks:
    1:
      context: /home/me/checkouts/widget
      stateVisits:
        review: 9
      priority: high
      budgetTicketId: not-yours-to-write
---

## Tasks

### Task 1: Ship the widget
**State:** pending
"#;
    let (_dir, plan_path, machine_path) = single_file("list-meta-mixed", plan);

    let task = element(&list_json(&plan_path, &machine_path), "plan.1");

    assert_eq!(
        task["metadata"],
        serde_json::json!({ "context": "/home/me/checkouts/widget", "priority": "high" }),
        "only the author's half, with every registered key held back"
    );
}

/// The map is published as stored, so a nested author value survives whole
/// rather than being flattened or stringified. §FS-rhei-list.4.2
#[test]
fn list_json_carries_a_nested_author_value_unchanged() {
    let plan = r#"# Rhei: demo

---
metadata:
  tasks:
    1:
      routes:
        primary:
          - alpha
          - beta
        fallback:
          - gamma
      snake_case_key: kept
---

## Tasks

### Task 1: Ship the widget
**State:** pending
"#;
    let (_dir, plan_path, machine_path) = single_file("list-meta-nested", plan);

    let task = element(&list_json(&plan_path, &machine_path), "plan.1");

    assert_eq!(
        task["metadata"],
        serde_json::json!({
            "routes": { "primary": ["alpha", "beta"], "fallback": ["gamma"] },
            "snake_case_key": "kept",
        }),
        "nested values and the author's own spelling both survive"
    );
}

/// A Directory Workspace keys its frontmatter by rhei-local id on disk, and the
/// field is published under the qualified id the listing prints — which is the
/// keying an outside reader had to re-implement. §FS-rhei-list.4.2
#[test]
fn list_json_carries_metadata_under_the_qualified_id_of_a_workspace() {
    let dir = unique_temp_dir("list-meta-workspace");
    let workspace = dir.join("widgets");
    fs::create_dir_all(workspace.join("tasks")).expect("create workspace");
    fs::write(
        workspace.join("index.rhei.md"),
        "# Rhei: Widgets\n\n---\nmetadata:\n  tasks:\n    1:\n      context: /checkouts/widget\n---\n",
    )
    .expect("write index");
    fs::write(
        workspace.join("tasks/01-work.md"),
        "### Task 1: Ship the widget\n**State:** pending\n",
    )
    .expect("write task file");
    let machine_path = write_fixture_file(&dir, "states.yaml", MACHINE);

    let task = element(&list_json(&workspace, &machine_path), "widgets.1");

    assert_eq!(task["metadata"], serde_json::json!({ "context": "/checkouts/widget" }));
}

/// The same, one level further out: a Panta project merges every rhei's
/// frontmatter, and the field answers under the project-qualified id.
// §FS-rhei-list.4.2
#[test]
fn list_json_carries_metadata_under_the_qualified_id_in_a_project() {
    let dir = unique_temp_dir("list-meta-project");
    let project = dir.join("project");
    fs::create_dir_all(project.join("billing/tasks")).expect("create project");
    fs::write(project.join("index.panta.md"), "# Panta: Metadata Probe\n").expect("write manifest");
    fs::write(
        project.join("billing/index.rhei.md"),
        "# Rhei: Billing\n\n---\nmetadata:\n  tasks:\n    1:\n      context: /checkouts/billing\n---\n",
    )
    .expect("write member index");
    fs::write(
        project.join("billing/tasks/01-work.md"),
        "### Task 1: Bill it\n**State:** pending\n",
    )
    .expect("write member task");
    let machine_path = write_fixture_file(&dir, "states.yaml", MACHINE);

    let task = element(&list_json(&project, &machine_path), "billing.1");

    assert_eq!(task["metadata"], serde_json::json!({ "context": "/checkouts/billing" }));
}

/// Absence is the default, so a plan that authored no frontmatter emits exactly
/// the object it emitted before this field existed. §FS-rhei-list.4.2
#[test]
fn list_json_is_unchanged_on_a_plan_with_no_frontmatter() {
    let plan = "# Rhei: demo\n\n## Tasks\n\n### Task 1: Ship the widget\n**State:** pending\n";
    let (_dir, plan_path, machine_path) = single_file("list-meta-bare", plan);

    let task = element(&list_json(&plan_path, &machine_path), "plan.1");

    assert_eq!(
        task,
        serde_json::json!({
            "id": "plan.1",
            "kind": "task",
            "title": "Ship the widget",
            "state": "pending",
            "assignee": null,
            "prior": [],
            "parent": null,
            "depth": 1,
        })
    );
}

/// A key JSON cannot name and a number JSON cannot hold: both are named errors
/// on the query surface, reported together, rather than a value quietly
/// replaced by `null`. §FS-rhei-render.3.1.1
#[test]
fn list_json_names_every_value_json_cannot_hold() {
    let (_dir, plan_path, machine_path) = single_file("list-meta-unrepresentable", UNHOLDABLE_PLAN);

    let listed = run_cli("list", &plan_path, &machine_path, &["--json"]);

    assert!(
        !listed.status.success(),
        "a value with no JSON form is an error, not a silent substitution\nstdout:\n{}",
        listed.stdout
    );
    assert_unrepresentable_report(&listed.stderr);
}

/// The same two shapes on the export that publishes the map whole. Both are
/// silent today, and the sequence key is the worse of the two: it drops the
/// entire `frontmatter` value, so the document loses everything the caller came
/// for. §FS-rhei-render.3.1.1
#[test]
fn render_json_names_every_value_json_cannot_hold() {
    let (_dir, plan_path, machine_path) =
        single_file("render-meta-unrepresentable", UNHOLDABLE_PLAN);

    let rendered = run_cli("render", &plan_path, &machine_path, &["--format", "json"]);

    assert!(
        !rendered.status.success(),
        "a value with no JSON form is an error, not a dropped document\nstdout:\n{}",
        rendered.stdout
    );
    assert_unrepresentable_report(&rendered.stderr);
}

/// One plan holding both shapes, so a single run has to report both.
// §FS-rhei-errors.1.1
const UNHOLDABLE_PLAN: &str = r#"# Rhei: demo

---
metadata:
  tasks:
    1:
      ? [alpha, beta]
      : keyed by a sequence
    2:
      ratio: .inf
---

## Tasks

### Task 1: Ship the widget
**State:** pending

### Task 2: Ship the other widget
**State:** pending
"#;

/// Every unrepresentable value in the document, named by plan, qualified task id
/// and key, in one run. §FS-rhei-render.3.1.1 §FS-rhei-errors.1.1
fn assert_unrepresentable_report(stderr: &str) {
    for expected in ["plan.rhei.md", "plan.1", "sequence", "plan.2", "ratio"] {
        assert!(stderr.contains(expected), "the report should name {expected:?}, got:\n{stderr}");
    }
}
