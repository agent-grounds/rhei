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
