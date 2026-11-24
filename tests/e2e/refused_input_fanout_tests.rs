//! Issue #770 in a fanned-out state: one arm of the visit fails and the other
//! finishes, so the engine tries the edge on the finished arm's work, and the
//! destination refuses it. Every surface tells that one story whichever arm
//! ended last.
// §FS-rhei-errors.1.5.2 §FS-rhei-run-report.3.1 §FS-rhei-agents.3.2.1

use super::refused_input_support::{text, Outputs, Worker, World};
use super::*;

/// `draft` fanned out over `alpha`, which exits 3, and `beta`, which exits 0.
/// The arm named `last` ends only once the other has, and a second later, so
/// the order never races and the two records' end times differ.
fn fanned_out(last: &str) -> World {
    let world = World::new(Worker::Agent, Outputs::Bare);
    world.change_machine(
        "description: Draft the note\n    agent: mock",
        "description: Draft the note\n    all_targets: [\"mock:alpha\", \"mock:beta\"]",
    );
    write_fixture_file(&world.dir, "last-arm", last);
    write_python_agent(
        &world.dir,
        "worker.py",
        r#"agent_prompt()
root = pathlib.Path(env('RHEI_ROOT'))
runtime = root / 'runtime'
model, attempt = env('RHEI_MODEL'), env('RHEI_ATTEMPT')
append(runtime / (env('RHEI_STATE') + '-calls'), model + '\n')
if model == (root / 'last-arm').read_text().strip():
    other = runtime / ('ended-' + ('beta' if model == 'alpha' else 'alpha') + '-' + attempt)
    deadline = time.time() + 30
    while not other.exists() and time.time() < deadline:
        time.sleep(0.05)
    time.sleep(1.1)
append(runtime / 'ended', model + '\n')
write(runtime / ('ended-' + model + '-' + attempt), '')
raise SystemExit(3 if model == 'alpha' else 0)
"#,
    );
    world
}

/// One run that keeps going past the failing arm, established as refusing the
/// edge before anything is read.
fn run(world: &World) -> CliRun {
    let result =
        world.cli(&["run", "plan.rhei.md", "--no-tui", "--no-callbacks", "--continue-on-error"]);
    let all = text(&result);
    assert_eq!(result.status.code(), Some(1), "the run must halt: {all}");
    assert_eq!(world.state(), "draft", "the edge into 'read' must be refused: {all}");
    assert_eq!(world.calls("read"), 0, "'read' must never run: {all}");
    result
}

/// The row with this world's directory taken out, so two worlds compare.
fn row(world: &World) -> (String, String) {
    let (mut reason, next) = world.row();
    for path in world.note_paths() {
        reason = reason.replace(&path, "<note>");
    }
    (reason, next)
}

/// The row names the refused edge, with `remedy` in order in the next action,
/// and nothing that leaves the ticket where it is. Where the Transition Ledger
/// and Task Final States give a reason, it is the row's; a run that spawned
/// nothing gives one in both.
fn report_gaps(world: &World, run: &str, remedy: &[&str]) -> Vec<String> {
    let (reason, next) = world.row();
    let mut gaps = world.naming_gaps(&format!("{run} Attention reason"), &reason);
    let mut from = 0;
    for step in remedy {
        match next[from..].find(step) {
            Some(at) => from += at + step.len(),
            None => gaps.push(format!("{run} Attention next action lacks {step:?}: {next}")),
        }
    }
    gaps.extend(World::misleading_gaps(
        &format!("{run} Attention"),
        &format!("{reason} | {next}"),
        &[],
    ));
    let report = world.report();
    let ledger = report.lines().find(|line| line.starts_with("| plan.1 | draft | - | blocked |"));
    let last = report.lines().find(|line| line.starts_with("- ! `plan.1` (draft) — "));
    let last = last.filter(|line| !line.contains("agent×"));
    let zero_spawn = run.ends_with("halted run");
    if zero_spawn && (ledger.is_none() || last.is_none()) {
        gaps.push(format!("{run} gives no Ledger or Task Final States reason: {report}"));
    }
    if ledger.is_some_and(|line| !line.ends_with(&format!("| {reason} |"))) {
        gaps.push(format!("{run} Transition Ledger does not give the row's reason: {ledger:?}"));
    }
    if last.is_some_and(|line| !line.ends_with(&reason)) {
        gaps.push(format!("{run} Task Final States does not give the row's reason: {last:?}"));
    }
    gaps
}

/// Both orders: the row is the same in every run, both spent arms' halt lines
/// name the refused edge, and only the arm that finished says its last attempt
/// met the condition.
#[test]
fn issue_770_fanned_out_visit_names_refused_edge_whichever_arm_ends_last() {
    let mut told = Vec::new();
    for last in ["alpha", "beta"] {
        let world = fanned_out(last);
        let first = if last == "alpha" { "beta" } else { "alpha" };
        let mut gaps = Vec::new();
        let mut rows = Vec::new();
        for (name, remedy) in
            [("run 1", ["write", "cancel"]), ("spending run", ["write", "rhei retry plan.1"])]
        {
            gaps.extend(World::misleading_gaps(
                &format!("{last} last, {name} console"),
                &text(&run(&world)),
                &[],
            ));
            gaps.extend(report_gaps(&world, &format!("{last} last, {name}"), &remedy));
            rows.push(row(&world));
        }
        let ended: Vec<String> = world.read("runtime/ended").lines().map(String::from).collect();
        assert_eq!(ended, [first, last, first, last], "{last} ended last in both runs");

        let calls = world.calls("draft");
        let halted = text(&run(&world));
        assert_eq!(world.calls("draft"), calls, "{last} last: a spent visit is not spawned again");
        let halts: Vec<&str> = halted
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("halting Task plan.1"))
            .collect();
        assert_eq!(halts.len(), 2, "one halt line per spent arm: {halted}");
        let prefix = "halting Task plan.1 in state 'draft': 2 attempts spent on this visit and ";
        let mut subjects = Vec::new();
        for halt in &halts {
            gaps.extend(world.naming_gaps(&format!("{last} last, halt line"), halt));
            gaps.extend(World::misleading_gaps(&format!("{last} last, halt line"), halt, &[]));
            if !(halt.starts_with(prefix) && halt.contains("The ticket stays in 'draft'.")) {
                gaps.push(format!("halt line lost its prefix or its ending: {halt}"));
            }
            subjects
                .push(halt.trim_start_matches(prefix).split(" met ").next().unwrap_or_default());
        }
        subjects.sort();
        assert_eq!(subjects, ["another invocation of this visit", "the last one"], "{halts:?}");
        gaps.extend(report_gaps(
            &world,
            &format!("{last} last, halted run"),
            &["write", "rhei retry plan.1"],
        ));
        rows.push(row(&world));
        assert!(gaps.is_empty(), "{}", gaps.join("\n"));
        told.push(rows);
    }
    assert_eq!(told[0], told[1], "the rows must not depend on which arm ended last");
}
