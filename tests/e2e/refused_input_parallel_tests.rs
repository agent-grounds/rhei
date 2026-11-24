//! Issue #770 under `--parallel`: the pool completes and spawns workers on paths
//! of its own, and a finished worker whose edge the destination refused must
//! read there exactly as it does in a sequential run.
// §FS-rhei-errors.1.5.2 §FS-rhei-run-report.3.1 §FS-rhei-agents.3.2.1

use super::refused_input_support::{text, Outputs, Worker, World};
use super::*;

/// Two tickets, each in a file of its own, in a `concurrent` state: one ready
/// agent ticket runs alone, a plan in one file falls back to sequential, and a
/// state that is not concurrent is deferred, so only this fills the pool.
fn pooled(worker: Worker, outputs: Outputs) -> World {
    let world = World::new(worker, outputs);
    world.change_machine(
        "description: Draft the note",
        "description: Draft the note\n    concurrent: true",
    );
    fs::remove_file(world.dir.join("plan.rhei.md")).unwrap();
    write_fixture_file(&world.dir, "index.rhei.md", "# Rhei: Refused edge\n");
    fs::create_dir_all(world.dir.join("tasks")).unwrap();
    for n in [1, 2] {
        let ticket = format!("### Task {n}: Draft\n**State:** draft\n");
        write_fixture_file(&world.dir, &format!("tasks/{n}.md"), &ticket);
    }
    world
}

/// One pooled run, established as refusing the edge before anything is read.
fn run_parallel(world: &World) -> CliRun {
    let result = world.cli(&["run", ".", "--no-tui", "--no-callbacks", "--parallel", "2"]);
    let all = text(&result);
    assert!(!all.contains("Falling back to sequential"), "{all}");
    assert_eq!(result.status.code(), Some(1), "the run must halt: {all}");
    assert!(world.read("tasks/1.md").contains("**State:** draft"), "edge refused: {all}");
    assert_eq!(world.calls("read"), 0, "'read' must never run: {all}");
    result
}

/// The row names the destination and the input, and nothing that leaves the
/// ticket where it is; `remedy` must appear, in order, in its next action.
fn row_gaps(world: &World, run: &str, remedy: &[&str]) -> Vec<String> {
    let (reason, next) = world.row();
    let mut gaps = world.naming_gaps(&format!("{run} Attention reason"), &reason);
    let mut from = 0;
    for step in remedy {
        match next[from..].find(step) {
            Some(at) => from += at + step.len(),
            None => gaps.push(format!("{run} Attention next action lacks {step:?}: {next}")),
        }
    }
    let row = format!("{reason} | {next}");
    gaps.extend(World::misleading_gaps(&format!("{run} Attention"), &row, &[]));
    gaps
}

/// The finishing run, through the pool's completion path, then a run that
/// reuses that work without spawning it.
#[test]
fn issue_770_parallel_reused_visit_names_destination_input() {
    let world = pooled(Worker::Agent, Outputs::Declared);
    let mut gaps = Vec::new();
    for run in ["finishing run", "later run"] {
        let result = run_parallel(&world);
        gaps.extend(row_gaps(&world, run, &["write", "cancel"]));
        gaps.extend(World::misleading_gaps(&format!("{run} console"), &text(&result), &[]));
    }
    assert_eq!(world.calls("draft"), 2, "each finished visit is reused, never re-spawned");
    assert!(gaps.is_empty(), "{}", gaps.join("\n"));
}

/// The pool's agent and program spawn sites print the spent visit's halt line,
/// which names the refused edge between its prefix and its ending.
#[test]
fn issue_770_parallel_spent_visit_names_refused_edge() {
    for worker in [Worker::Agent, Worker::Program] {
        let world = pooled(worker, Outputs::Bare);
        while world.calls("draft") < 4 {
            assert!(run_parallel(&world).status.code() == Some(1));
        }
        assert_eq!(world.calls("draft"), 4, "{worker:?}: both tickets spent both attempts");
        let halted = run_parallel(&world);
        assert_eq!(world.calls("draft"), 4, "{worker:?}: a spent visit is not spawned again");
        let all = text(&halted);
        let halt = all.lines().map(str::trim).find(|line| line.starts_with("halting Task plan.1"));
        let halt = halt.unwrap_or_else(|| panic!("{worker:?}: no halt line\n{all}"));
        let prefix = "halting Task plan.1 in state 'draft': 2 attempts spent on this visit";
        let mut gaps = world.naming_gaps("halt line", halt);
        gaps.extend(World::misleading_gaps("halt line", halt, &[]));
        if !(halt.starts_with(prefix) && halt.contains("The ticket stays in 'draft'.")) {
            gaps.push(format!("halt line lost its prefix or its ending: {halt}"));
        }
        gaps.extend(row_gaps(&world, "halted run", &["write", "rhei retry plan.1"]));
        assert!(gaps.is_empty(), "{worker:?}\n{}", gaps.join("\n"));
    }
}
