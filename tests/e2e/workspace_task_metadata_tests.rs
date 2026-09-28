// A workspace task file's own metadata frontmatter block: the per-task custom
// field written beside the task's `**State:**` rather than in the one file every
// other run also writes (agent-grounds/rhei#318).
//
// The acceptance criterion is byte-identity. The two-task workspace whose fields
// live in two task files must render exactly the map the same two fields written
// in `index.rhei.md` render, because anything less than identity means readers
// have two placements to understand instead of one.

// §FS-rhei-plan-language.1.4 §FS-rhei-authoring.4.6

use std::fs;
use std::path::{Path, PathBuf};

use super::new_tests::flattened_output;
use super::*;

/// A machine that reads a task's authored metadata into its instructions and
/// counts visits into `review`, so one fixture serves the read surface
/// (§FS-rhei-states.4.1) and the runtime's own write (§FS-rhei-transitions.2.3).
const METADATA_MACHINE: &str = r#"name: workspace-task-metadata
version: 1
states:
  pending:
    initial: true
    description: Ready for work
    instructions: |
      CONTEXT=[{meta.context}] PRIORITY=[{meta.priority}]
  review:
    description: Under review
    visits: 3
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: review
  - from: review
    to: completed
"#;

const FIRST_BLOCK: &str = "---\nmetadata:\n  tasks:\n    first:\n      context: oracle-labs\n---\n";
const SECOND_BLOCK: &str = "---\nmetadata:\n  tasks:\n    second:\n      context: personal\n---\n";
const FIRST_BODY: &str = "\n### Task first: first item\n**State:** pending\n";
const SECOND_BODY: &str = "\n### Task second: second item\n**State:** pending\n";

/// A workspace whose directory is named `wp`, so the qualified ids read as the
/// ticket writes them — `wp.first`, `wp.second`.
fn workspace(
    prefix: &str,
    index: &str,
    task_files: &[(&str, &str)],
) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let ws = dir.join("wp");
    fs::create_dir_all(ws.join("tasks")).expect("create workspace dirs");
    fs::write(ws.join("index.rhei.md"), index).expect("write index");
    for (name, content) in task_files {
        fs::write(ws.join("tasks").join(name), content).expect("write task file");
    }
    let machine = write_fixture_file(&dir, "states.yaml", METADATA_MACHINE);
    (dir, ws, machine)
}

/// The ticket's own workspace: one field per task, in the file that defines it.
fn authored_in_task_files(prefix: &str) -> (TestDir, PathBuf, PathBuf) {
    workspace(
        prefix,
        "# Rhei: Workspace\n",
        &[
            ("01-first.md", &format!("{FIRST_BLOCK}{FIRST_BODY}")),
            ("02-second.md", &format!("{SECOND_BLOCK}{SECOND_BODY}")),
        ],
    )
}

/// The same two fields in the one shared file — the placement this change is
/// asked to make unnecessary, and the reference the other form must match.
fn authored_in_the_index(prefix: &str) -> (TestDir, PathBuf, PathBuf) {
    workspace(
        prefix,
        "# Rhei: Workspace\n\n---\nmetadata:\n  tasks:\n    first:\n      context: oracle-labs\n    \
         second:\n      context: personal\n---\n",
        &[("01-first.md", FIRST_BODY.trim_start()), ("02-second.md", SECOND_BODY.trim_start())],
    )
}

fn rendered_metadata(ws: &Path, machine: &Path) -> serde_json::Value {
    render_json(ws, machine)["frontmatter"].clone()
}

/// The leading `---` block of a task file, or `None` when it carries none. What
/// the guards below compare: a transition rewrites `**State:**` in the same
/// file, so whole-file identity would assert the wrong thing.
fn metadata_block(path: &Path) -> Option<String> {
    let source = fs::read_to_string(path).expect("read task file");
    let rest = source.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    Some(format!("---\n{}\n---\n", &rest[..end]))
}

/// 1 · The acceptance criterion. Two task files carry one field each, and the
/// rendered map is the one the index form renders — same qualified keys, same
/// values, same bytes. §FS-rhei-render.3.1
#[test]
fn two_task_files_render_the_map_the_index_renders() {
    let (_dir, ws, machine) = authored_in_task_files("wtm-render");
    let (_ref_dir, reference_ws, reference_machine) = authored_in_the_index("wtm-render-reference");

    let reference = rendered_metadata(&reference_ws, &reference_machine);
    assert_eq!(
        reference,
        serde_json::json!({
            "metadata": {
                "tasks": {
                    "wp.first": { "context": "oracle-labs" },
                    "wp.second": { "context": "personal" },
                }
            }
        }),
        "the reference form is the ticket's third transcript: qualified keys, one field each"
    );
    assert_eq!(
        rendered_metadata(&ws, &machine),
        reference,
        "two task files must render exactly what the one index file renders"
    );
}

/// 2 · The field reaches an agent, not only a renderer: `{meta.context}`
/// resolves for the task whose own file authored it. §FS-rhei-states.4.1
#[test]
fn meta_resolves_from_the_task_files_own_block() {
    let (_dir, ws, machine) = authored_in_task_files("wtm-meta-variable");

    let peek = run_cli("next", &ws, &machine, &["--peek"]);
    assert_success(&peek);
    assert!(
        peek.stdout.contains("CONTEXT=[oracle-labs]"),
        "the claimed task's prompt should carry its own context; got:\n{}",
        peek.stdout
    );
}

/// 3 · Disjoint keys merge. The index holds `priority` for `first`, the task
/// file holds `context` for `first`, and both arrive under `wp.first`.
#[test]
fn the_index_and_a_task_file_merge_by_key() {
    let (_dir, ws, machine) = workspace(
        "wtm-merge",
        "# Rhei: Workspace\n\n---\nmetadata:\n  tasks:\n    first:\n      priority: high\n---\n",
        &[("01-first.md", &format!("{FIRST_BLOCK}{FIRST_BODY}"))],
    );

    let first = &rendered_metadata(&ws, &machine)["metadata"]["tasks"]["wp.first"];
    assert_eq!(first["context"], "oracle-labs", "the task file's key should arrive");
    assert_eq!(first["priority"], "high", "and so should the index's");

    let peek = run_cli("next", &ws, &machine, &["--peek"]);
    assert_success(&peek);
    assert!(
        peek.stdout.contains("CONTEXT=[oracle-labs] PRIORITY=[high]"),
        "both keys should resolve through {{meta.<key>}}; got:\n{}",
        peek.stdout
    );
}

/// 5 · **Guard.** `rhei reset` clears the index's runtime keys and leaves an
/// authored task-file block exactly as it found it. §FS-rhei-reset.2
#[test]
fn reset_clears_the_index_and_leaves_the_task_file_block_alone() {
    let (_dir, ws, machine) = authored_in_task_files("wtm-reset");
    let task_file = ws.join("tasks/01-first.md");
    let before = metadata_block(&task_file).expect("the task file opens with a block");

    assert_success(&run_transition(&ws, &machine, "wp.first", "pending", "review"));
    assert!(
        fs::read_to_string(ws.join("index.rhei.md")).expect("read index").contains("stateVisits"),
        "the transition should have recorded a visit in the index"
    );

    let reset = run_cli("reset", &ws, &machine, &["--yes"]);
    assert_success(&reset);

    assert!(
        !fs::read_to_string(ws.join("index.rhei.md")).expect("read index").contains("stateVisits"),
        "reset should clear the index's runtime keys"
    );
    assert_eq!(
        metadata_block(&task_file).as_deref(),
        Some(before.as_str()),
        "reset must not read or rewrite an authored task-file metadata block"
    );
}

/// 6 · **Guard.** The shared-write hotspot closes rather than moves: a
/// transition writes `stateVisits` into `index.rhei.md`, never into the task
/// file that authored the block it read. §FS-rhei-plan-language.1.4
#[test]
fn a_transition_writes_visits_to_the_index_not_to_the_task_file() {
    let (_dir, ws, machine) = authored_in_task_files("wtm-transition");
    let task_file = ws.join("tasks/01-first.md");
    let before = metadata_block(&task_file).expect("the task file opens with a block");

    assert_success(&run_transition(&ws, &machine, "wp.first", "pending", "review"));

    let index = fs::read_to_string(ws.join("index.rhei.md")).expect("read index");
    assert!(index.contains("stateVisits"), "the index should carry the counter; got:\n{index}");
    assert_eq!(
        metadata_block(&task_file).as_deref(),
        Some(before.as_str()),
        "the task file's metadata block must be byte-identical after a transition"
    );
    let body = fs::read_to_string(&task_file).expect("read task file");
    assert!(
        !body.trim_start_matches(before.as_str()).contains("stateVisits"),
        "and no runtime key may appear in the task file at all; got:\n{body}"
    );
}

/// 7 · A `basin/` ticket file has no metadata document of its own, so a block
/// there is a load error — and `rhei list` still lists every other rhei.
/// §FS-rhei-panta.2 §FS-rhei-validate.4.4
#[test]
fn a_basin_ticket_files_block_is_a_load_error_that_list_survives() {
    let dir = unique_temp_dir("wtm-basin");
    fs::create_dir_all(dir.join("basin")).expect("create basin");
    fs::create_dir_all(dir.join("other/tasks")).expect("create sibling rhei");
    fs::write(dir.join("index.panta.md"), "# Panta: Project\n").expect("write manifest");
    fs::write(
        dir.join("basin/01-loose.md"),
        "---\nmetadata:\n  tasks:\n    3:\n      context: oracle-labs\n---\n\n\
         ### Task 3: loose ticket\n**State:** pending\n",
    )
    .expect("write basin ticket");
    fs::write(dir.join("other/index.rhei.md"), "# Rhei: Other\n").expect("write sibling index");
    fs::write(dir.join("other/tasks/01-a.md"), "### Task a: an item\n**State:** pending\n")
        .expect("write sibling ticket");
    let machine = write_fixture_file(&dir, "states.yaml", METADATA_MACHINE);

    let validate = run_cli("validate", &dir, &machine, &[]);
    let reported = flattened_output(&validate);
    assert!(!validate.status.success(), "a basin block should fail the load; got:\n{reported}");
    assert!(
        reported.contains("01-loose.md") && reported.contains("index.panta.md"),
        "the error should name the file and where basin metadata lives; got:\n{reported}"
    );

    let list = run_cli("list", &dir, &machine, &[]);
    assert_success(&list);
    assert!(
        list.stdout.contains("Task other.a: an item"),
        "`rhei list` skips what it cannot load and lists the rest; got:\n{}",
        list.stdout
    );
    assert!(
        flattened_output(&list).contains("01-loose.md"),
        "and warns naming the file it skipped; got:\n{}",
        flattened_output(&list)
    );
}

/// 14 · Composition rewrites task ids, so a block task file may not carry an
/// authored metadata block: the compiler refuses it and names the block's own
/// `index.rhei.md`. Carrying it through qualification is a separate contract.
/// §FS-rhei-library.4
#[test]
fn block_composition_refuses_a_task_file_carrying_a_metadata_block() {
    use super::block_composition_support::*;

    let dir = unique_temp_dir("wtm-block-composition");
    let fixtures = write_composition_fixtures(&dir);
    let task_file = fixtures.review.join("tasks/review.md");
    let authored = fs::read_to_string(&task_file).expect("read block task file");
    fs::write(
        &task_file,
        format!(
            "---\nmetadata:\n  tasks:\n    job:\n      context: oracle-labs\n---\n\n{authored}"
        ),
    )
    .expect("write block task file");

    let review = mount_arg("review", &fixtures.review);
    let result = run_compose(&dir, &["instantiate", "--mount", review.as_str()]);
    assert_failed_with(&result, &["review.md", "index.rhei.md"]);
}
