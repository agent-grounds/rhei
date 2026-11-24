//! Issue #770 in a fanned-out state: `alpha` fails every attempt and `beta`
//! finishes. The run tries the edge only once the visit's last invocation to
//! end met its condition, so which arm ended last decides whether the
//! destination's refusal is what holds the ticket, and the surfaces tell the
//! story that is true in that order: with `beta` last, the refused edge and a
//! remedy that releases it; with `alpha` last, the unmet hold, naming neither
//! `read` nor its input.
// §FS-rhei-errors.1.5.2 §FS-rhei-run-report.3.1 §FS-rhei-agents.3.2.1

use super::refused_input_support::{text, Outputs, Worker, World};
use super::*;

/// What a spent visit whose edge was never tried says, on its halt lines and in
/// its row alike. §FS-rhei-agents.3.2.1
const UNMET: &str = "still unmet: this state's completion condition names nothing on disk";

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

/// One run that keeps going past the failing arm, established as leaving the
/// ticket in `draft` before anything is read.
fn run(world: &World) -> CliRun {
    let result =
        world.cli(&["run", "plan.rhei.md", "--no-tui", "--no-callbacks", "--continue-on-error"]);
    let all = text(&result);
    assert_eq!(result.status.code(), Some(1), "the run must halt: {all}");
    assert_eq!(world.state(), "draft", "the ticket must stay in 'draft': {all}");
    assert_eq!(world.calls("read"), 0, "'read' must never run: {all}");
    result
}

/// Two runs that spend both arms' attempts, each checked by `check` with its
/// name and console, then the zero-spawn run that halts on the spent visit.
/// Returns that run's halt lines, one per spent arm.
fn spend(world: &World, last: &str, mut check: impl FnMut(&str, &str)) -> Vec<String> {
    for name in ["run 1", "spending run"] {
        check(name, &text(&run(world)));
    }
    let first = if last == "alpha" { "beta" } else { "alpha" };
    let ended: Vec<String> = world.read("runtime/ended").lines().map(String::from).collect();
    assert_eq!(ended, [first, last, first, last], "{last} ended last in both runs");
    let calls = world.calls("draft");
    let halted = text(&run(world));
    assert_eq!(world.calls("draft"), calls, "{last} last: a spent visit is not spawned again");
    let halts: Vec<String> = halted
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("halting Task plan.1"))
        .map(String::from)
        .collect();
    assert_eq!(halts.len(), 2, "one halt line per spent arm: {halted}");
    check("halted run", &halted);
    halts
}

/// The Attention row's reason and next action, and every other line of the
/// report that explains `plan.1`. Where the Transition Ledger and Task Final
/// States give a reason, it is the row's; a run that spawned nothing gives one
/// in both.
fn report(world: &World, run: &str, gaps: &mut Vec<String>) -> (String, String, Vec<String>) {
    let (reason, next) = world.row();
    let report = world.report();
    let ledger = report.lines().find(|line| line.starts_with("| plan.1 | draft | - | blocked |"));
    let last = report.lines().find(|line| line.starts_with("- ! `plan.1` (draft) — "));
    let last = last.filter(|line| !line.contains("agent×"));
    if run.ends_with("halted run") && (ledger.is_none() || last.is_none()) {
        gaps.push(format!("{run} gives no Ledger or Task Final States reason: {report}"));
    }
    if ledger.is_some_and(|line| !line.ends_with(&format!("| {reason} |"))) {
        gaps.push(format!("{run} Transition Ledger does not give the row's reason: {ledger:?}"));
    }
    if last.is_some_and(|line| !line.ends_with(&reason)) {
        gaps.push(format!("{run} Task Final States does not give the row's reason: {last:?}"));
    }
    (reason, next, ledger.into_iter().chain(last).map(String::from).collect())
}

/// What a surface says about `read` or its input, line by line.
fn edge_named(world: &World, surface: &str, said: &str) -> Vec<String> {
    let paths = world.note_paths();
    said.lines()
        .filter(|line| {
            line.contains("'read'")
                || line.contains("note (")
                || paths.iter().any(|path| line.contains(path.as_str()))
        })
        .map(|line| format!("{surface} names the edge it never tried: {}", line.trim()))
        .collect()
}

/// `beta` ends last, so the run tries the edge on its work and `read` refuses
/// it: every surface names the edge and the remedy, both spent arms' halt lines
/// name it, only `beta`'s says its own last attempt met the condition, and
/// doing what the spent row says reaches `read`.
#[test]
fn issue_770_fanned_out_visit_names_refused_edge_when_its_success_ends_last() {
    let world = fanned_out("beta");
    let mut gaps = Vec::new();
    let halts = spend(&world, "beta", |name, console| {
        let remedy = match name {
            "run 1" => ["write", "cancel"],
            _ => ["write", "rhei retry plan.1"],
        };
        if name != "halted run" {
            gaps.extend(World::misleading_gaps(&format!("{name} console"), console, &[]));
        }
        let (reason, next, _) = report(&world, name, &mut gaps);
        gaps.extend(world.naming_gaps(&format!("{name} Attention reason"), &reason));
        let mut from = 0;
        for step in remedy {
            match next[from..].find(step) {
                Some(at) => from += at + step.len(),
                None => gaps.push(format!("{name} Attention next action lacks {step:?}: {next}")),
            }
        }
        let row = format!("{reason} | {next}");
        gaps.extend(World::misleading_gaps(&format!("{name} Attention"), &row, &[]));
    });
    let prefix = "halting Task plan.1 in state 'draft': 2 attempts spent on this visit and ";
    let mut subjects = Vec::new();
    for halt in &halts {
        gaps.extend(world.naming_gaps("halt line", halt));
        gaps.extend(World::misleading_gaps("halt line", halt, &[]));
        if !(halt.starts_with(prefix) && halt.contains("The ticket stays in 'draft'.")) {
            gaps.push(format!("halt line lost its prefix or its ending: {halt}"));
        }
        subjects.push(halt.trim_start_matches(prefix).split(" met ").next().unwrap_or_default());
    }
    subjects.sort();
    assert_eq!(subjects, ["another invocation of this visit", "the last one"], "{halts:?}");
    assert!(gaps.is_empty(), "{}", gaps.join("\n"));

    // The spent row's remedy, in its order: write the file, retry, rerun.
    world.write_note();
    let retry = world.cli(&["retry", "plan.1"]);
    assert!(retry.status.success(), "retry must grant the spent visit: {}", text(&retry));
    let released =
        world.cli(&["run", "plan.rhei.md", "--no-tui", "--no-callbacks", "--continue-on-error"]);
    assert!(
        world.calls("read") > 0 && world.state() != "draft",
        "following the remedy must reach 'read', now {}: {}",
        world.state(),
        text(&released)
    );
}

/// `alpha` ends last, so the run never tries the edge: the note is not what
/// holds the ticket, and no surface names `read` or the note. The rows say
/// what the halt lines say: stalled after the first run, then the spent visit's
/// unmet hold, which here is the truthful one.
#[test]
fn issue_770_fanned_out_visit_whose_failure_ends_last_names_no_edge() {
    let world = fanned_out("alpha");
    let mut gaps = Vec::new();
    let halts = spend(&world, "alpha", |name, console| {
        gaps.extend(edge_named(&world, &format!("{name} console"), console));
        let (reason, next, others) = report(&world, name, &mut gaps);
        let row = format!("{reason} | {next}");
        gaps.extend(edge_named(&world, &format!("{name} Attention"), &row));
        for other in &others {
            gaps.extend(edge_named(&world, &format!("{name} report"), other));
        }
        let held = if name == "run 1" { "stalled" } else { UNMET };
        if !reason.contains(held) {
            gaps.push(format!("{name} Attention reason does not say {held:?}: {reason}"));
        }
    });
    let prefix = "halting Task plan.1 in state 'draft': 2 attempts spent on this visit and ";
    for halt in &halts {
        gaps.extend(edge_named(&world, "halt line", halt));
        if !(halt.starts_with(prefix) && halt.contains(UNMET)) {
            gaps.push(format!("halt line does not keep the spent visit's unmet hold: {halt}"));
        }
        if !halt.contains("The ticket stays in 'draft'.") {
            gaps.push(format!("halt line lost its ending: {halt}"));
        }
    }
    assert!(gaps.is_empty(), "{}", gaps.join("\n"));
}
