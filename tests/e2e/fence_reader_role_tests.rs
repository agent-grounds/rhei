//! Where a document's frontmatter block stands depends on which file it is, not on its text: a
//! task file quoting a header, a block and a `## Tasks` line still has only a leading block, so a
//! raw-text edit reads its fences as the task-file parser does. §FS-rhei-plan-language.2.1
//! §FS-rhei-plan-language.1.4 §FS-rhei-complete.4.2

use std::fs;

use super::fence_reader_support::*;
use super::*;

// A fake header and block hold an open fence over a literal `## Tasks`; the real Task 2 is below.
const REFUSE: &str = "### Task 1: First\n\n**State:** open\n\n# Rhei: quoted\n\n---\n```\n---\n## Tasks\n```\n\n### Task 2: Second\n\n**State:** open\n";
// The same, with a quoted decoy of Task 2 inside the fence the fake block holds open.
const DECOY: &str = "### Task 1: First\n\n**State:** open\n\n# Rhei: quoted\n\n---\n```\n---\n## Tasks\n\n### Task 2: decoy\n\n**State:** decoy\n```\n\n### Task 2: Second\n\n**State:** open\n";

fn lay(text: &str) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir("rhei-622-role");
    write_fixture_file(&dir, "states.yaml", MACHINE);
    fs::create_dir_all(dir.join("tasks")).expect("tasks directory");
    write_fixture_file(&dir, "index.rhei.md", "# Rhei: Fence contract\n");
    let path = write_fixture_file(&dir, "tasks/a.md", text);
    (dir, path)
}

/// Runs `args` on Task 2 and returns the task file afterwards, the real Task 2 split off.
fn edit_task_2(text: &str, args: &[&str]) -> (String, String) {
    let (dir, path) = lay(text);
    assert_success(&run(&dir, &["validate", "index.rhei.md"]));
    let edited = run(&dir, args);
    assert!(edited.status.success(), "stderr:\n{}\nfile:\n{text}", edited.stderr);
    let after = fs::read_to_string(&path).expect("task file");
    let (above, below) = after.split_once("### Task 2: Second").expect("the real heading");
    assert_success(&run(&dir, &["validate", "index.rhei.md"]));
    (above.to_owned(), below.to_owned())
}

const COMPLETE: [&str; 7] =
    ["complete", "index.rhei.md", "--task", "2", "--result", "finished", "--no-callbacks"];
const CLAIM: [&str; 5] = ["next", "index.rhei.md", "--task", "2", "--no-callbacks"];

#[test]
fn fence_reader_622_complete_task_file_quoting_a_header_block_and_chapter() {
    let (above, below) = edit_task_2(REFUSE, &COMPLETE);
    assert!(below.contains("**State:** done"), "the real task must be done:\n{below}");
    assert!(!above.contains("**Result:**"), "{above}");
}

#[test]
fn fence_reader_622_complete_task_file_leaves_a_quoted_decoy_alone() {
    let (above, below) = edit_task_2(DECOY, &COMPLETE);
    assert!(above.contains("### Task 2: decoy\n\n**State:** decoy\n```"), "{above}");
    assert!(below.contains("**State:** done"), "the real task must be done:\n{below}");
    assert_eq!(below.matches("> **Result:**").count(), 1, "{below}");
}

#[test]
fn fence_reader_622_claim_task_file_leaves_a_quoted_decoy_alone() {
    let (above, below) = edit_task_2(DECOY, &CLAIM);
    assert!(!above.contains("**Assignee:**"), "the claim was written into a quotation:\n{above}");
    assert!(below.contains("**Assignee:**"), "the real task must be claimed:\n{below}");
}

// The issue's plan, a ``` line in a frontmatter value, laid as a single-file plan in a workspace's
// `tasks/` directory: addressed directly, it is a plan, wherever it sits.
const PLAN_IN_TASKS: &str = "# Rhei: Plan\n\n---\nmetadata:\n  note: |\n    ```\n    not a fence\n---\n\n## Tasks\n\n### Task 1: First\n**State:** open\n";
// The issue's workspace task file, with the same line in its own leading block.
const WORKSPACE_TASK: &str = "---\nmetadata:\n  tasks:\n    \"1\":\n      note: |\n        ```\n        not a fence\n---\n\n### Task 1: First\n\n**State:** open\n\nDo.\n";
const VALUE: &str = "        ```\n        not a fence\n";

/// A workspace `ws` under a fresh root, and the file `name` under its `tasks/` holding `text`.
fn lay_in_tasks(name: &str, text: &str) -> (TestDir, PathBuf) {
    let root = unique_temp_dir("rhei-622-spelling");
    let ws = root.join("ws");
    fs::create_dir_all(ws.join("tasks")).expect("tasks directory");
    write_fixture_file(&ws, "index.rhei.md", "# Rhei: WS\n");
    write_fixture_file(&ws, "states.yaml", MACHINE);
    write_fixture_file(&ws, "tasks/states.yaml", MACHINE);
    let file = write_fixture_file(&ws, &format!("tasks/{name}"), text);
    (root, file)
}

/// Runs `args` from `cwd` and requires exit 0.
fn succeeds_from(root: &Path, cwd: &Path, args: &[&str]) {
    let output = command(root).current_dir(cwd).args(args).output().expect("rhei runs");
    let ran = CliRun::from(&output);
    assert!(ran.status.success(), "{args:?} from {}\nstderr:\n{}", cwd.display(), ran.stderr);
}

/// The plan's block is where the command that loaded it reads one, so neither the spelling of
/// its path nor the workspace around it changes what `rhei complete` reads. §FS-rhei-plan-language.2.1
#[test]
fn fence_reader_622_complete_a_plan_in_tasks_under_any_spelling() {
    for spelling in ["bare", "from-parent", "absolute"] {
        let (root, plan) = lay_in_tasks("plan.rhei.md", PLAN_IN_TASKS);
        let absolute = plan.display().to_string();
        let (cwd, target) = match spelling {
            "bare" => (root.join("ws").join("tasks"), "plan.rhei.md"),
            "from-parent" => (root.to_path_buf(), "ws/tasks/plan.rhei.md"),
            _ => (root.to_path_buf(), absolute.as_str()),
        };
        succeeds_from(&root, &cwd, &["validate", target]);
        succeeds_from(
            &root,
            &cwd,
            &["complete", target, "--task", "1", "--result", "finished", "--no-callbacks"],
        );
        let after = fs::read_to_string(&plan).expect("plan");
        assert!(after.contains("**State:** done"), "{spelling}: the task must be done:\n{after}");
        assert!(after.contains("    ```\n    not a fence\n"), "{spelling}: value kept:\n{after}");
    }
}

/// The workspace's own task file is a task file from every working directory and spelling.
/// §FS-rhei-plan-language.2.1 §FS-rhei-plan-language.1.4
#[test]
fn fence_reader_622_complete_a_workspace_task_file_under_any_spelling() {
    for spelling in ["dot", "dot-dot", "absolute"] {
        let (root, task) = lay_in_tasks("01-a.md", WORKSPACE_TASK);
        let ws = root.join("ws");
        let absolute = ws.display().to_string();
        let (cwd, target) = match spelling {
            "dot" => (ws.clone(), "."),
            "dot-dot" => (ws.join("tasks"), ".."),
            _ => (root.to_path_buf(), absolute.as_str()),
        };
        succeeds_from(
            &root,
            &cwd,
            &["complete", target, "--task", "1", "--result", "finished", "--no-callbacks"],
        );
        let after = fs::read_to_string(&task).expect("task file");
        assert!(after.contains("**State:** done"), "{spelling}: the task must be done:\n{after}");
        assert!(after.contains(VALUE), "{spelling}: value kept:\n{after}");
    }
}
