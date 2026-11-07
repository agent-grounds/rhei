//! A frontmatter block is YAML to every reader, so a raw-text task edit reads the plan the parser
//! reads. §FS-rhei-plan-language.2.1 §FS-rhei-plan-language.2.3 §FS-rhei-complete.4.2

use std::fs;

use super::fence_reader_support::*;
use super::*;

const HEADING: &str = "### Task 1: First";
const TASKS: &str = "## Tasks\n\n### Task 1: First\n**State:** open\n";
const FILE_TASK: &str = "### Task 1: First\n\n**State:** open\n\nDo.\n";

// The issue's `report` plan: one fence-shaped line in a block scalar, the task below it.
const FENCE_YAML: &str = "---\nmetadata:\n  note: |\n    ```\n    not a fence\n---\n\n";
// The issue's `workspace` task file: the same value in the file's leading block.
const FENCE_FILE: &str =
    "---\nmetadata:\n  tasks:\n    \"1\":\n      note: |\n        ```\n        not a fence\n---\n\n";
// Indentation alone keeps these block scalar lines from matching the raw heading scan.
const INDENTED_DECOY_YAML: &str =
    "---\nmetadata:\n  note: |\n    ### Task 1: decoy\n    **State:** decoy\n---\n\n";
const INDENTED_DECOY_FILE: &str = "---\nmetadata:\n  tasks:\n    \"1\":\n      note: |\n        ### Task 1: decoy\n        **State:** decoy\n---\n\n";
// A quoted scalar may continue at column 0, where the same lines do match the raw scan.
const COLUMN_0_DECOY_YAML: &str =
    "---\nmetadata:\n  note: \"\n### Task 1: decoy\n\n**State:** decoy\n\"\n---\n\n";
const COLUMN_0_DECOY_FILE: &str =
    "---\nmetadata:\n  tasks:\n    \"1\":\n      note: \"\n### Task 1: decoy\n\n**State:** decoy\n\"\n---\n\n";
const BALANCED_YAML: &str =
    "---\nmetadata:\n  note: |\n    ```\n    closed again\n    ```\n---\n\n";
const BALANCED_FILE: &str = "---\nmetadata:\n  tasks:\n    \"1\":\n      note: |\n        ```\n        closed again\n        ```\n---\n\n";

fn plan(frontmatter: &str) -> String {
    format!("# Rhei: Plan\n\n{frontmatter}{TASKS}")
}

fn task_file(frontmatter: &str) -> String {
    format!("{frontmatter}{FILE_TASK}")
}

fn lay(workspace: bool, text: &str) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir("rhei-622-frontmatter");
    write_fixture_file(&dir, "states.yaml", MACHINE);
    let path = if workspace {
        fs::create_dir_all(dir.join("tasks")).expect("tasks directory");
        write_fixture_file(&dir, "index.rhei.md", "# Rhei: Fence contract\n");
        write_fixture_file(&dir, "tasks/a.md", text)
    } else {
        write_fixture_file(&dir, "plan.rhei.md", text)
    };
    (dir, path)
}

/// What must survive the edit: a task file's leading block as bytes, since no command writes it
/// (§FS-rhei-plan-language.1.4); a plan's value as the parser reads it, since a frontmatter write
/// re-serializes the block around the key it adds.
fn value_of(dir: &Path, workspace: bool, path: &Path) -> String {
    if workspace {
        let text = fs::read_to_string(path).expect("task file");
        let mut delimiters = 0;
        return text
            .split_inclusive('\n')
            .take_while(|line| {
                let open = delimiters < 2;
                delimiters += usize::from(line.trim_end() == "---");
                open
            })
            .collect();
    }
    let rendered = run(dir, &["render", target(workspace), "--format", "json"]);
    assert_success(&rendered);
    let json: serde_json::Value = serde_json::from_str(&rendered.stdout).expect("render json");
    json["frontmatter"]["metadata"]["note"].to_string()
}

fn complete_finishes(workspace: bool, text: &str) {
    let (dir, path) = lay(workspace, text);
    assert_success(&run(&dir, &["validate", target(workspace)]));
    let value = value_of(&dir, workspace, &path);
    let args =
        ["complete", target(workspace), "--task", "1", "--result", "finished", "--no-callbacks"];
    let completed = run(&dir, &args);
    assert!(
        completed.status.success(),
        "complete must finish a plan validate accepts; stderr:\n{}\nplan:\n{text}",
        completed.stderr
    );
    let after = fs::read_to_string(&path).expect("rewritten plan");
    let (above, below) = after.split_once(HEADING).expect("the task heading survives");
    assert!(below.contains("**State:** done"), "the task itself must be done:\n{after}");
    assert!(!above.contains("**Result:**"), "the result link went above the task:\n{after}");
    assert_eq!(below.matches("> **Result:**").count(), 1, "{after}");
    assert_eq!(value_of(&dir, workspace, &path), value, "the YAML value moved:\n{after}");
    assert_success(&run(&dir, &["validate", target(workspace)]));
}

fn claim_succeeds(workspace: bool, text: &str) {
    let (dir, path) = lay(workspace, text);
    assert_success(&run(&dir, &["validate", target(workspace)]));
    let value = value_of(&dir, workspace, &path);
    let claimed = run(&dir, &["next", target(workspace), "--task", "1", "--no-callbacks"]);
    assert!(
        claimed.status.success(),
        "a claim must take a plan validate accepts; stderr:\n{}\nplan:\n{text}",
        claimed.stderr
    );
    let after = fs::read_to_string(&path).expect("rewritten plan");
    let (above, below) = after.split_once(HEADING).expect("the task heading survives");
    assert!(below.contains("**Assignee:**"), "the task itself must be claimed:\n{after}");
    assert!(!above.contains("**Assignee:**"), "the claim was written into YAML:\n{after}");
    assert_eq!(value_of(&dir, workspace, &path), value, "the YAML value moved:\n{after}");
    assert_success(&run(&dir, &["validate", target(workspace)]));
}

/// Completes Task 2, which sits below a quotation of itself the edit must leave alone.
fn complete_below_quotation(workspace: bool, text: &str) {
    let (dir, path) = lay(workspace, text);
    assert_success(&run(&dir, &["validate", target(workspace)]));
    let args =
        ["complete", target(workspace), "--task", "2", "--result", "finished", "--no-callbacks"];
    let completed = run(&dir, &args);
    assert!(completed.status.success(), "stderr:\n{}\nplan:\n{text}", completed.stderr);
    let after = fs::read_to_string(&path).expect("rewritten plan");
    assert!(after.contains("### Task 2: quoted\n**State:** quoted\n"), "{after}");
    let below = after.split_once("### Task 2: Second").expect("the real heading").1;
    assert!(below.contains("**State:** done"), "the real task must be done:\n{after}");
    assert_eq!(below.matches("> **Result:**").count(), 1, "{after}");
}

// The issue's two shapes, on the command it ran.

#[test]
fn fence_reader_622_complete_fence_value_single() {
    complete_finishes(false, &plan(FENCE_YAML));
}

#[test]
fn fence_reader_622_complete_fence_value_workspace() {
    complete_finishes(true, &task_file(FENCE_FILE));
}

// The claim scans the same way; the issue inferred this from the code and never ran it.

#[test]
fn fence_reader_622_claim_fence_value_single() {
    claim_succeeds(false, &plan(FENCE_YAML));
}

#[test]
fn fence_reader_622_claim_fence_value_workspace() {
    claim_succeeds(true, &task_file(FENCE_FILE));
}

// The reverse direction: YAML read as the task. These exit 0 today and damage the plan.

#[test]
fn fence_reader_622_complete_column_0_decoy_single() {
    complete_finishes(false, &plan(COLUMN_0_DECOY_YAML));
}

#[test]
fn fence_reader_622_complete_column_0_decoy_workspace() {
    complete_finishes(true, &task_file(COLUMN_0_DECOY_FILE));
}

#[test]
fn fence_reader_622_claim_column_0_decoy_single() {
    claim_succeeds(false, &plan(COLUMN_0_DECOY_YAML));
}

#[test]
fn fence_reader_622_claim_column_0_decoy_workspace() {
    claim_succeeds(true, &task_file(COLUMN_0_DECOY_FILE));
}

// Where the block stands: every place the parsers find it. §FS-rhei-plan-language.2.3

#[test]
fn fence_reader_622_complete_block_past_a_fence() {
    complete_finishes(false, &plan(&format!("```\nquoted\n```\n\n{FENCE_YAML}")));
}

#[test]
fn fence_reader_622_complete_block_below_blank_lines_workspace() {
    complete_finishes(true, &task_file(&format!("\n\n{FENCE_FILE}")));
}

#[test]
fn fence_reader_622_complete_crlf_block() {
    complete_finishes(false, &plan(FENCE_YAML).replace('\n', "\r\n"));
}

/// A header and a `---` quoted above the real header are content, so the block is the one below.
#[test]
fn fence_reader_622_complete_block_below_a_quoted_header() {
    let quoted = "```\n# Rhei: quoted\n---\nquoted: true\n---\n```\n\n";
    complete_finishes(false, &format!("{quoted}{}", plan(FENCE_YAML)));
}

// Guards: these pass today and must still pass after the change.

#[test]
fn fence_reader_622_balanced_value_guard() {
    complete_finishes(false, &plan(BALANCED_YAML));
    complete_finishes(true, &task_file(BALANCED_FILE));
    claim_succeeds(false, &plan(BALANCED_YAML));
    claim_succeeds(true, &task_file(BALANCED_FILE));
}

#[test]
fn fence_reader_622_indented_decoy_guard() {
    complete_finishes(false, &plan(INDENTED_DECOY_YAML));
    complete_finishes(true, &task_file(INDENTED_DECOY_FILE));
    claim_succeeds(false, &plan(INDENTED_DECOY_YAML));
    claim_succeeds(true, &task_file(INDENTED_DECOY_FILE));
}

#[test]
fn fence_reader_622_fence_below_the_block_still_quotes_guard() {
    let body = "### Task 1: First\n**State:** open\n\n```\n### Task 2: quoted\n**State:** quoted\n```\n\n### Task 2: Second\n**State:** open\n";
    let frontmatter = "---\nmetadata:\n  note: plain\n---\n\n";
    complete_below_quotation(false, &format!("# Rhei: Plan\n\n{frontmatter}## Tasks\n\n{body}"));
    let leading = "---\nmetadata:\n  tasks:\n    \"1\":\n      note: plain\n---\n\n";
    complete_below_quotation(true, &format!("{leading}{body}"));
}

/// A `---` anywhere else is content, so a fence opened between two of them is still a fence.
#[test]
fn fence_reader_622_rule_in_a_body_is_content_guard() {
    let body = "### Task 1: First\n**State:** open\n\n---\n```\n---\n### Task 2: quoted\n**State:** quoted\n```\n\n### Task 2: Second\n**State:** open\n";
    complete_below_quotation(false, &format!("# Rhei: Plan\n\n## Tasks\n\n{body}"));
    complete_below_quotation(true, body);
}

/// A `---` pair below prose under the header is not frontmatter, so its lines are plan text.
#[test]
fn fence_reader_622_rule_below_prose_is_content_guard() {
    let text = format!("# Rhei: Plan\n\nProse.\n\n---\nnote: not frontmatter\n---\n\n{TASKS}");
    complete_finishes(false, &text);
    claim_succeeds(false, &text);
}
