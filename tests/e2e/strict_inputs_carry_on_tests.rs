//! What a serial run does with a ticket of another rhei that a non-concurrent
//! state deferred behind one that did not move: a refusal under
//! `--strict-inputs` carries on with it (§FS-rhei-seal.8.4), and so does a stall
//! with no seal involved (§FS-rhei-run.3.6). The controls are the cases that
//! already carry on: nothing refused, no deferral, and the worker pool.

use std::fs;
use std::path::{Path, PathBuf};

use super::strict_inputs_support::*;
use super::*;

const ONE_TICKET: &str = "### Task 1: Only ticket\n**State:** step\n";
const TWO_TICKETS: &str =
    "### Task 1: First\n**State:** step\n\n### Task 2: Second\n**State:** step\n";

struct TwoRheis {
    _dir: TestDir,
    project: PathBuf,
    home: PathBuf,
    a: PathBuf,
    b: PathBuf,
}

impl TwoRheis {
    /// A Panta project whose two member rheis each have their own machine, and
    /// whose tickets all sit in a state both machines name `step`.
    fn lay(prefix: &str, a_tasks: &str) -> Self {
        let dir = unique_temp_dir(prefix);
        let project = dir.join("project");
        fs::create_dir_all(&project).unwrap();
        write_fixture_file(&project, "index.panta.md", "# Panta: Two rheis\n");
        let shape = MachineShape::default();
        let a = lay_member(&project, "a", a_tasks, &shape);
        let b = lay_member(&project, "b", ONE_TICKET, &shape);
        let home = dir.join("home");
        Self { _dir: dir, project, home, a, b }
    }

    fn seal(&self) {
        let sealed = rhei_at(&self.home, &self.project, &["seal", "."]);
        assert!(sealed.status.success(), "rhei seal\nstderr:\n{}", sealed.stderr);
    }

    /// Seal the project, then change `a`'s machine, so a strict run refuses `a`.
    fn seal_and_change_a(&self) {
        self.seal();
        let machine = self.a.join("states.yaml");
        let text = fs::read_to_string(&machine).unwrap();
        fs::write(&machine, text + "# changed after the seal\n").unwrap();
    }

    fn run(&self, extra: &[&str]) -> CliRun {
        let mut args = vec!["run", ".", "--no-tui"];
        args.extend_from_slice(extra);
        rhei_at(&self.home, &self.project, &args)
    }
}

/// Rename the state of the member at `ws`, so the two rheis share no state name.
fn rename_step(ws: &Path, to: &str) {
    let machine = ws.join("states.yaml");
    let text = fs::read_to_string(&machine).unwrap();
    let renamed = text
        .replace("  step:\n", &format!("  {to}:\n"))
        .replace("from: step", &format!("from: {to}"));
    assert_ne!(text, renamed, "the fixture's machine should name the state");
    fs::write(&machine, renamed).unwrap();
    write_fixture_file(
        ws,
        "tasks/01-drift.md",
        &format!("### Task 1: Only ticket\n**State:** {to}\n"),
    );
}

fn refusals(run: &CliRun, id: &str) -> usize {
    run.stderr.matches(&format!("{id} was not started")).count()
}

/// `b.1` ran to its final state, and the run still exits 1 for what `a` left open.
#[track_caller]
fn assert_carried_on_with_b(run: &CliRun, project: &TwoRheis) {
    let log = format!("stdout:\n{}\nstderr:\n{}", run.stdout, run.stderr);
    assert_eq!(task_state(&project.b, 1), "completed", "nothing holds b.1 back\n{log}");
    assert!(evidence(&project.b).contains("ticket 1"), "b.1 should have run\n{log}");
    assert_eq!(run.status.code(), Some(1), "a's ticket is work for a person\n{log}");
}

/// §FS-rhei-seal.8.4 §FS-rhei-run.5
#[test]
#[ignore = "red until #585: a refusal ends the serial run with b.1 deferred and never started; implement removes this"]
fn a_refusal_carries_on_with_a_ticket_of_another_rhei_deferred_behind_it() {
    let project = TwoRheis::lay("strict-carry-on", ONE_TICKET);
    project.seal_and_change_a();

    let run = project.run(&["--strict-inputs"]);

    assert_refused(&run, "states.yaml");
    assert_eq!(refusals(&run, "a.1"), 1, "stderr:\n{}", run.stderr);
    assert_eq!(task_state(&project.a, 1), "step", "the refused ticket stays where it is");
    assert_eq!(spawned(&project.a, "a.1"), Vec::<String>::new());
    assert_carried_on_with_b(&run, &project);
}

/// "Starts nothing more in that rhei" beside "carries on with the others".
/// §FS-rhei-seal.8.4
#[test]
#[ignore = "red until #585: the first refusal ends the serial run before a.2 or b.1 gets a pass; implement removes this"]
fn every_ticket_of_the_refused_rhei_is_refused_once_and_the_other_rhei_still_runs() {
    let project = TwoRheis::lay("strict-carry-on-two", TWO_TICKETS);
    project.seal_and_change_a();

    let run = project.run(&["--strict-inputs"]);

    assert_refused(&run, "states.yaml");
    for (n, id) in [(1, "a.1"), (2, "a.2")] {
        assert_eq!(refusals(&run, id), 1, "{id} is refused once\nstderr:\n{}", run.stderr);
        assert_eq!(task_state(&project.a, n), "step");
        assert_eq!(spawned(&project.a, id), Vec::<String>::new());
    }
    assert_eq!(evidence(&project.a), "", "no ticket of the refused rhei is started");
    assert_carried_on_with_b(&run, &project);
}

/// Control: the same project with nothing changed. §FS-rhei-seal.8.4
#[test]
fn an_unchanged_sealed_project_runs_both_rheis_to_their_final_states() {
    let project = TwoRheis::lay("strict-carry-on-clean", ONE_TICKET);
    project.seal();

    let run = project.run(&["--strict-inputs"]);

    assert_eq!(run.status.code(), Some(0), "stderr:\n{}", run.stderr);
    assert_eq!(task_state(&project.a, 1), "completed");
    assert_eq!(task_state(&project.b, 1), "completed");
}

/// Control: with no state name shared, nothing defers `b.1` behind the refusal.
/// §FS-rhei-seal.8.4 §FS-rhei-run.5
#[test]
fn a_refusal_leaves_another_rhei_running_when_no_state_name_is_shared() {
    let project = TwoRheis::lay("strict-carry-on-renamed", ONE_TICKET);
    rename_step(&project.b, "other");
    project.seal_and_change_a();

    let run = project.run(&["--strict-inputs"]);

    assert_refused(&run, "states.yaml");
    assert!(!run.stdout.contains("Deferred"), "stdout:\n{}", run.stdout);
    assert_eq!(task_state(&project.a, 1), "step");
    assert_carried_on_with_b(&run, &project);
}

/// Control: the worker pool already asks for another pass after the refusal.
/// §FS-rhei-seal.8.4 §FS-rhei-run.5
#[test]
fn a_refusal_under_parallel_carries_on_with_a_ticket_deferred_behind_it() {
    let project = TwoRheis::lay("strict-carry-on-parallel", ONE_TICKET);
    project.seal_and_change_a();

    let run = project.run(&["--strict-inputs", "--parallel", "2"]);

    assert_refused(&run, "states.yaml");
    assert_eq!(refusals(&run, "a.1"), 1, "stderr:\n{}", run.stderr);
    assert_eq!(task_state(&project.a, 1), "step");
    assert_carried_on_with_b(&run, &project);
}

/// The same exit with no seal: a program that exits 0 without its result stalls
/// its ticket, and the ticket deferred behind it still gets its pass. §FS-rhei-run.3.6
#[test]
#[ignore = "red until #585: a stall ends the serial run with b.1 deferred and never started; implement removes this"]
fn a_stalled_program_carries_on_with_a_ticket_of_another_rhei_deferred_behind_it() {
    let project = TwoRheis::lay("stall-carry-on", ONE_TICKET);
    on_ticket(&project.a, 1, "sys.exit(0)\n");

    let run = project.run(&[]);

    let output = format!("{}{}", run.stdout, run.stderr);
    assert!(output.contains("required outputs are missing"), "a.1 should stall\n{output}");
    assert_eq!(task_state(&project.a, 1), "step", "no transition fires");
    assert_carried_on_with_b(&run, &project);
}
