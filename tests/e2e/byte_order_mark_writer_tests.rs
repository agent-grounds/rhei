//! The commands that rewrite a plan read it as the loader does: after one leading
//! UTF-8 byte order mark, which the rewrite keeps (#630). §REQ-cross-platform.9
//!
//! Each test runs one command over two fixtures that differ only in the three
//! leading bytes `EF BB BF` of the file under test, and asks for the same outcome
//! from both: a transition through §FS-rhei-transition-cmd, the ticket-id
//! collision of §FS-rhei-library.4, and a subtask placed by §FS-rhei-new.3.1.

use std::path::Path;

use super::byte_order_mark_tests::{run_in, same_outcome, write_bytes, MARK};
use super::*;

const MACHINE: &str = "name: m\nversion: 1\nstates:\n  pending:\n    initial: true\n  completed:\n    final: true\ntransitions:\n  - {from: pending, to: completed}\n";
const PLAN: &str = "# Rhei: Example\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n";
const TASK_FILE: &str = "### Task 1: H\n**State:** pending\n";

/// The file at `name` after the command: the plain fixture's text, behind the
/// one mark the marked fixture started with. The budget ticket id a transition
/// mints is fresh on each run, so it is the one line not compared.
fn assert_rewritten_alike(plain: &Path, marked: &Path, name: &str) {
    let read = |dir: &Path| std::fs::read(dir.join(name)).expect("rewritten file");
    let (plain, marked) = (read(plain), read(marked));
    let marked = marked.strip_prefix(MARK).expect("the rewrite keeps the mark");
    let text = |bytes: &[u8]| {
        let text = String::from_utf8(bytes.to_vec()).expect("utf-8");
        text.lines().filter(|line| !line.contains("budgetTicketId:")).collect::<Vec<_>>().join("\n")
    };
    assert!(text(&plain).contains("**State:** completed"), "the transition was written");
    assert_eq!(text(marked), text(&plain), "the rewrite changes nothing else");
}

/// `rhei transition` on a single-file plan saved with the mark: the header is
/// found, and the plan is rewritten with its mark. §FS-rhei-transition-cmd.3
#[test]
fn a_transition_rewrites_a_plan_saved_with_a_byte_order_mark() {
    let (plain, marked) = same_outcome(
        "bom-transition-plan",
        |dir, marked| {
            write_bytes(dir, "states.yaml", MACHINE, false);
            write_bytes(dir, "plan.rhei.md", PLAN, marked);
        },
        &["transition", "plan.1", "--from", "pending", "--to", "completed", "--result", "done"],
    );
    assert_rewritten_alike(&plain, &marked, "plan.rhei.md");
}

/// `rhei transition` on a workspace whose task file, saved with the mark, opens
/// with the ticket's heading: the ticket's `**State:**` line is found and
/// rewritten. §FS-rhei-transition-cmd.3
#[test]
fn a_transition_rewrites_a_task_file_saved_with_a_byte_order_mark() {
    let (plain, marked) = same_outcome(
        "bom-transition-task-file",
        |dir, marked| {
            write_bytes(dir, "ws/index.rhei.md", "# Rhei: Host\n", false);
            write_bytes(dir, "ws/states.yaml", MACHINE, false);
            write_bytes(dir, "ws/tasks/01-h.md", TASK_FILE, marked);
        },
        &["transition", "ws.1", "--from", "pending", "--to", "completed", "--result", "done"],
    );
    assert_rewritten_alike(&plain, &marked, "ws/tasks/01-h.md");
}

/// `rhei instantiate --into` a workspace whose task file, saved with the mark,
/// already declares the ticket the template places: the same id collision is
/// refused, not a duplicate the union was about to write. §FS-rhei-library.4
#[test]
fn a_union_sees_a_ticket_declared_in_a_task_file_saved_with_a_byte_order_mark() {
    for marked in [false, true] {
        let dir = unique_temp_dir(if marked { "bom-union-marked" } else { "bom-union-plain" });
        write_bytes(
            &dir,
            "tpl/template.yaml",
            "name: tpl\nversion: 1.0.0\ndescription: T.\n",
            false,
        );
        write_bytes(
            &dir,
            "tpl/plan.rhei.md",
            "# Rhei: Part\n\n## Tasks\n\n### Task 1: Part\n**State:** pending\n",
            false,
        );
        write_bytes(&dir, "tpl/states.yaml", MACHINE, false);
        write_bytes(&dir, "host/index.rhei.md", "# Rhei: Host\n", false);
        write_bytes(&dir, "host/states.yaml", MACHINE, false);
        write_bytes(&dir, "host/tasks/01-h.md", TASK_FILE, marked);
        let run = run_in(&dir, &["instantiate", "./tpl", "--into", "host"]);
        assert!(!run.status.success(), "marked: {marked}\nstdout:\n{}", run.stdout);
        assert!(
            run.stderr.contains("cannot place ticket '1': task id already exists in target"),
            "marked: {marked}\nstderr:\n{}",
            run.stderr
        );
    }
}

/// `rhei new --under` a ticket whose heading opens a task file saved with the
/// mark: the parent is found, the subtask lands under it, and the file keeps its
/// mark. §FS-rhei-new.3.1
#[test]
fn a_subtask_lands_under_a_heading_that_opens_a_task_file_saved_with_a_byte_order_mark() {
    let (plain, marked) = same_outcome(
        "bom-new-under",
        |dir, marked| {
            write_bytes(dir, "ws/index.rhei.md", "# Rhei: Host\n", false);
            write_bytes(dir, "ws/states.yaml", MACHINE, false);
            write_bytes(dir, "ws/tasks/01-h.md", TASK_FILE, marked);
        },
        &["new", "Gamma", "--project", "ws", "--under", "ws.1"],
    );
    let read = |dir: &Path| std::fs::read(dir.join("ws/tasks/01-h.md")).expect("task file");
    let (plain, marked) = (read(&plain), read(&marked));
    assert!(String::from_utf8_lossy(&plain).contains("Task 1.1: Gamma"), "the subtask was written");
    assert_eq!(marked.strip_prefix(MARK).expect("the write keeps the mark"), plain.as_slice());
}

/// `rhei instantiate --into ws.1` a host whose files are saved with the mark: the
/// parent heading that opens its task file is found, and the task file, index and
/// machine the union extends come back with their marks, as the plain host's bytes
/// with the same lines added. §FS-rhei-library.7.1
#[test]
fn a_union_under_a_ticket_keeps_the_marks_of_the_files_it_extends() {
    let (plain, marked) = same_outcome(
        "bom-union-under",
        |dir, marked| {
            write_bytes(
                dir,
                "tpl/template.yaml",
                "name: tpl\nversion: 1.0.0\ndescription: T.\n",
                false,
            );
            write_bytes(dir, "tpl/plan.rhei.md", PLAN.replace("Example", "Part").as_str(), false);
            write_bytes(dir, "tpl/states.yaml", MACHINE, false);
            write_bytes(dir, "ws/index.rhei.md", "# Rhei: Host\n", marked);
            write_bytes(dir, "ws/states.yaml", MACHINE, marked);
            write_bytes(dir, "ws/tasks/01-h.md", TASK_FILE, marked);
        },
        &["instantiate", "./tpl", "--into", "ws.1"],
    );
    for name in ["ws/tasks/01-h.md", "ws/index.rhei.md", "ws/states.yaml"] {
        let read = |dir: &Path| std::fs::read(dir.join(name)).expect("host file");
        let (plain, marked) = (read(&plain), read(&marked));
        let marked = marked.strip_prefix(MARK).expect("the union keeps the mark");
        assert_eq!(marked, plain.as_slice(), "{name}");
    }
}
