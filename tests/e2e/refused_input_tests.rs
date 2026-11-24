//! Port of the issue #770 reproducer (tool report rhei.139): a worker that
//! finished, held only by its destination's missing input, is reported as
//! stalled, not scheduled, or short of attempts - never as the refused edge.
// §FS-rhei-errors.1.5.2 §FS-rhei-run-report.3.1 §FS-rhei-retry.2 §FS-rhei-agents.3.2.1

use super::refused_input_support::{text, Outputs, Worker, World};
use super::*;

/// A visit with attempts left: the row names the input, and its remedy is to
/// write it and rerun, or cancel - never a retry, which grants no input.
fn unspent_row_gaps(world: &World, run: &str) -> Vec<String> {
    let (reason, next) = world.row();
    let mut gaps = world.naming_gaps(&format!("{run} Attention reason"), &reason);
    if !(next.contains("write") && next.contains("cancel")) {
        gaps.push(format!(
            "{run} Attention next action omits writing the input or cancelling: {next}"
        ));
    }
    let row = format!("{reason} | {next}");
    gaps.extend(World::misleading_gaps(&format!("{run} Attention"), &row, &["rhei retry"]));
    gaps
}

/// A spent visit: the row names the refused edge rather than an unmet
/// completion condition, and writing the input comes before the retry.
fn spent_row_gaps(world: &World, run: &str) -> Vec<String> {
    let (reason, next) = world.row();
    let mut gaps = world.naming_gaps(&format!("{run} Attention reason"), &reason);
    if !reason.contains("2 attempts spent on this visit") {
        gaps.push(format!("{run} Attention reason loses the spent attempts: {reason}"));
    }
    match (next.find("write"), next.find("rhei retry plan.1")) {
        (Some(write), Some(retry)) if write < retry => {}
        _ => gaps.push(format!(
            "{run} Attention next action must write the input, then `rhei retry plan.1`: {next}"
        )),
    }
    gaps.extend(World::misleading_gaps(
        &format!("{run} Attention"),
        &format!("{reason} | {next}"),
        &[],
    ));
    gaps
}

/// The halt line keeps the prefix `batch-loop.sh` parses and the ending of
/// §FS-rhei-retry.3, and names the refused edge between them.
fn halt_line_gaps(world: &World, result: &CliRun, run: &str, refusal: &[&str]) -> Vec<String> {
    let all = text(result);
    let halt = all
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("halting Task plan.1"))
        .unwrap_or_default()
        .to_string();
    let mut gaps = Vec::new();
    let prefix = "halting Task plan.1 in state 'draft': 2 attempts spent on this visit";
    if !halt.starts_with(prefix) {
        gaps.push(format!("{run} halt line lost its prefix {prefix:?}: {halt:?}\n{all}"));
    }
    for kept in ["The ticket stays in 'draft'.", "rhei retry plan.1"] {
        if !halt.contains(kept) {
            gaps.push(format!("{run} halt line lost {kept:?}: {halt}"));
        }
    }
    if refusal.is_empty() {
        gaps.extend(world.naming_gaps(&format!("{run} halt line"), &halt));
    }
    for named in refusal {
        if !halt.contains(named) {
            gaps.push(format!("{run} halt line does not name {named:?}: {halt}"));
        }
    }
    gaps.extend(World::misleading_gaps(&format!("{run} halt line"), &halt, &[]));
    gaps
}

/// The Task Final States line of a run that did not spawn the ticket carries
/// the run's one reading of it. §FS-rhei-run-report.3.1
fn final_state_line(world: &World) -> String {
    world
        .report()
        .lines()
        .find(|line| line.starts_with("- ! `plan.1`"))
        .unwrap_or_default()
        .to_string()
}

/// (a) and (b): the run whose agent finished, then a run that reuses that work
/// without spawning it, with `draft` declaring its result as an output.
#[test]
#[ignore = "red until #770: Attention says 'stalled', then 'not scheduled - rerun', never the destination's missing input; implement removes this"]
fn issue_770_reused_visit_names_destination_input() {
    let world = World::new(Worker::Agent, Outputs::Declared);
    let finishing = world.run();
    world.assert_refused(&finishing);
    assert!(
        text(&finishing).contains("Missing required input artifact: note"),
        "the warning already names the input: {}",
        text(&finishing)
    );
    let mut gaps = unspent_row_gaps(&world, "finishing run");
    gaps.extend(World::misleading_gaps("finishing run console", &text(&finishing), &[]));
    gaps.extend(World::misleading_gaps("finishing run report", &world.report(), &[]));

    let later = world.run();
    world.assert_refused(&later);
    assert_eq!(world.calls("draft"), 1, "the finished visit is reused, never re-spawned");
    gaps.extend(unspent_row_gaps(&world, "later run"));
    gaps.extend(world.naming_gaps("later run Task Final States", &final_state_line(&world)));
    gaps.extend(World::misleading_gaps("later run console", &text(&later), &[]));
    gaps.extend(World::misleading_gaps("later run report", &world.report(), &[]));
    assert!(gaps.is_empty(), "{}", gaps.join("\n"));
}

/// (c): the visit's record is on disk and its work is reusable, so there is
/// nothing to refill; the refusal says that and names the refused edge.
#[test]
#[ignore = "red until #770: rhei retry says 'no spawn record for its current visit' though it is on disk; implement removes this"]
fn issue_770_retry_of_reused_visit_says_nothing_to_refill() {
    let world = World::new(Worker::Agent, Outputs::Declared);
    world.assert_refused(&world.run());
    world.assert_refused(&world.run());
    let spawns = world.dir.join("runtime/spawns");
    let records = || -> Vec<(PathBuf, Vec<u8>)> {
        let mut records: Vec<_> = fs::read_dir(&spawns)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .map(|path| {
                let bytes = fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect();
        records.sort();
        records
    };
    let before = records();
    assert!(!before.is_empty(), "the finished visit's spawn record is on disk");
    let plan = world.read("plan.rhei.md");

    let retry = world.cli(&["retry", "plan.1"]);
    let said = text(&retry);
    assert!(!retry.status.success(), "retry must refuse: {said}");
    assert_eq!(records(), before, "a refused retry writes nothing");
    assert_eq!(world.read("plan.rhei.md"), plan, "a refused retry writes nothing");
    let mut gaps = Vec::new();
    if !said.contains("nothing to refill") {
        gaps.push(format!("retry does not say there is nothing to refill: {said}"));
    }
    gaps.extend(world.naming_gaps("retry", &said));
    gaps.extend(World::misleading_gaps("retry", &said, &[]));
    assert!(gaps.is_empty(), "{}", gaps.join("\n"));
}

/// (d): a worker without reusable work is spawned again on a successful visit
/// (§FS-rhei-agents.3.2), so its attempts run out while only the edge is refused.
fn spent_visit_names_refused_edge(worker: Worker, outputs: Outputs) {
    let world = World::new(worker, outputs);
    let first = world.run();
    world.assert_refused(&first);
    let mut gaps = unspent_row_gaps(&world, "run 1");
    gaps.extend(World::misleading_gaps("run 1 console", &text(&first), &[]));

    let spending = world.run();
    world.assert_refused(&spending);
    assert_eq!(world.calls("draft"), 2, "both attempts ran, and each exited 0");
    gaps.extend(spent_row_gaps(&world, "spending run"));
    gaps.extend(World::misleading_gaps("spending run report", &world.report(), &[]));

    let halted = world.run();
    world.assert_refused(&halted);
    assert_eq!(world.calls("draft"), 2, "a spent visit is not spawned again");
    gaps.extend(halt_line_gaps(&world, &halted, "halted run", &[]));
    gaps.extend(spent_row_gaps(&world, "halted run"));
    gaps.extend(World::misleading_gaps("halted run report", &world.report(), &[]));
    assert!(gaps.is_empty(), "{worker:?} {outputs:?}\n{}", gaps.join("\n"));
}

#[test]
#[ignore = "red until #770: the spent visit says 'completion condition is still unmet: ... names nothing on disk', never the refused edge; implement removes this"]
fn issue_770_agent_spent_visit_names_refused_edge() {
    spent_visit_names_refused_edge(Worker::Agent, Outputs::Bare);
}

#[test]
#[ignore = "red until #770: a program's finished visit reads 'stalled', then 'names nothing on disk', never the refused edge; implement removes this"]
fn issue_770_program_spent_visit_names_refused_edge() {
    spent_visit_names_refused_edge(Worker::Program, Outputs::Bare);
}

#[test]
#[ignore = "red until #770: a program with its outputs on disk still reads 'names nothing on disk', never the refused edge; implement removes this"]
fn issue_770_program_with_outputs_spent_visit_names_refused_edge() {
    spent_visit_names_refused_edge(Worker::Program, Outputs::Declared);
}

/// #764's copy of the same clause: the destination refused the finished
/// worker's edge for a spent `visits:` budget, and the state declares no
/// outputs, so the spent attempts must name that budget. §FS-rhei-errors.1.5.1
#[test]
#[ignore = "red until #770: a spent visit behind a spent visits: budget says 'still unmet: ... names nothing on disk'; implement removes this"]
fn issue_770_spent_visit_behind_spent_destination_visits_names_budget() {
    let world = World::new(Worker::Agent, Outputs::Bare);
    world.change_machine(
        "    agent: mock\n    inputs: [{ name: note, path: 'runtime/{task_id}.note.md' }]",
        &format!(
            "    visits: 1\n    program: {{ command: {} }}",
            fixture_command(&world.dir.join("worker.py"))
        ),
    );
    world.change_machine(
        "  - { from: read, to: completed, description: Read }",
        "  - { from: read, to: draft, description: Again, exit_code: 0 }\n  - { from: read, to: completed, description: Read, exit_code: 1 }",
    );
    let budget = "visit budget for state 'read' is exhausted (1/1 visits)";
    let spending = world.run();
    assert_eq!(spending.status.code(), Some(1), "{}", text(&spending));
    assert_eq!(world.state(), "draft", "{}", text(&spending));
    assert_eq!(world.calls("read"), 1, "one real entry into 'read'");
    assert_eq!(world.calls("draft"), 3, "one visit before 'read', two attempts after it");
    assert!(text(&spending).contains(budget), "the warning names the budget: {}", text(&spending));

    let row_gaps = |run: &str| {
        let (reason, next) = world.row();
        let mut gaps = Vec::new();
        if !(reason.contains(budget) && reason.contains("2 attempts spent on this visit")) {
            gaps.push(format!("{run} Attention reason does not name {budget:?}: {reason}"));
        }
        match (next.find("raise `visits:`"), next.find("rhei retry plan.1")) {
            (Some(raise), Some(retry)) if raise < retry && next.contains("'read'") => {}
            _ => gaps.push(format!(
                "{run} Attention next action must raise `visits:` on 'read', then `rhei retry plan.1`: {next}"
            )),
        }
        gaps.extend(World::misleading_gaps(
            &format!("{run} Attention"),
            &format!("{reason} | {next}"),
            &[],
        ));
        gaps
    };
    let mut gaps = row_gaps("spending run");
    let halted = world.run();
    assert_eq!(halted.status.code(), Some(1), "{}", text(&halted));
    assert_eq!(world.calls("draft"), 3, "a spent visit is not spawned again");
    gaps.extend(halt_line_gaps(&world, &halted, "halted run", &["'read'", budget]));
    gaps.extend(row_gaps("halted run"));
    assert!(gaps.is_empty(), "{}", gaps.join("\n"));
}
