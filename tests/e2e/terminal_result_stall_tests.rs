//! A worker that exits 0 owing the ticket's result, and what the run says
//! about it.
//!
//! The report has to name the artifact that is missing rather than fall back to
//! "stalled in non-terminal state", and one stalled ticket must not take its
//! siblings — or the rest of the run — down with it.

// §FS-rhei-states.3.3 §FS-rhei-run-report.3.1 §FS-rhei-run.3

use std::fs;

use super::terminal_result_fanout_tests::{write_fanout_agent_settings, FANOUT_PLAN};
use super::terminal_result_tests::{
    write_exiting_agent, write_mock_agent_settings, write_silent_agent,
};
use super::*;

/// `rhei run` spawns a program once per ticket, whatever the state declares, so
/// a program state writes the ticket's result file and is never asked for a
/// fragment per declared target — files nothing could write.
// §FS-rhei-states.3.3 §FS-rhei-programs.2
fn program_fanout_machine(command: &str) -> String {
    format!(
        r#"name: program-fanout
version: 1
models:
  - alpha
  - beta
states:
  review:
    initial: true
    description: One program, many declared targets
    all_targets:
      - "mock:mock:alpha"
      - "mock:mock:beta"
    program:
      command: {command}
    program_timeout: 20s
  completed:
    final: true
    description: Done
transitions:
  - from: review
    to: completed
    exit_code: 0
"#
    )
}

#[test]
fn a_program_state_with_declared_targets_writes_the_ticket_result() {
    let dir = unique_temp_dir("terminal-result-program-fanout");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", FANOUT_PLAN);
    let program = write_python_agent(
        &dir,
        "the-program.py",
        r#"result('the program did it.\n')
"#,
    );
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        &program_fanout_machine(&fixture_command(&program)),
    );
    let agent = write_silent_agent(&dir);
    write_fanout_agent_settings(&dir, &agent);

    assert_success(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    let result =
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("ticket result file");
    assert!(result.contains("the program did it."), "got:\n{result}");
    assert!(
        !dir.join("runtime/results/plan.1").exists(),
        "a program state files no per-invocation fragments"
    );
}

/// A program is a worker: when it exits 0 owing the ticket's result, the run
/// report must name the file, not fall back to "stalled in non-terminal state".
// §FS-rhei-run-report.3.1 §FS-rhei-agents.3.2.1
fn silent_program_machine(command: &str) -> String {
    format!(
        r#"name: silent-program
version: 1
states:
  probe:
    initial: true
    description: Exits 0 and writes nothing
    program:
      command: {command}
    program_timeout: 20s
  completed:
    final: true
    description: Done
transitions:
  - from: probe
    to: completed
    exit_code: 0
"#
    )
}

const SILENT_PROGRAM_PLAN: &str = r#"# Rhei: Silent Program

## Tasks

### Task 1: Probe
**State:** probe
"#;

#[test]
fn a_program_that_owes_the_result_is_reported_as_missing_outputs() {
    for parallel in ["1", "2"] {
        let dir = unique_temp_dir(&format!("terminal-result-program-stall-{parallel}"));
        let plan_path = write_fixture_file(&dir, "plan.rhei.md", SILENT_PROGRAM_PLAN);
        let probe = write_exiting_agent(&dir, "probe.py", 0);
        let machine_path = write_fixture_file(
            &dir,
            "states.yaml",
            &silent_program_machine(&fixture_command(&probe)),
        );

        let result = run_cli(
            "run",
            &plan_path,
            &machine_path,
            &["--no-tui", "--no-callbacks", "--parallel", parallel],
        );
        assert!(!result.status.success(), "--parallel {parallel}: the run halts on the stall");
        let combined = format!("{}{}", result.stdout, result.stderr);
        assert!(
            combined.contains("program exited 0 but required outputs are missing"),
            "--parallel {parallel}: the console still says `program`; got:\n{combined}"
        );
        let report = fs::read_to_string(dir.join("runtime/run-report.md")).expect("run report");
        assert!(
            report.contains("worker exited 0 without result ("),
            "--parallel {parallel}: the report names the artifact the program owed; got:\n{report}"
        );
        assert!(
            !report.contains("stalled in non-terminal state"),
            "--parallel {parallel}: not the nameless stall; got:\n{report}"
        );
        assert!(
            report.contains(&format!(
                "{} --task plan.1 --from probe",
                shell_quote(&plan_path.display().to_string())
            )),
            "--parallel {parallel}: the suggested command carries the plan; got:\n{report}"
        );
    }
}

/// Sequential mode is the default, and a stall there used to end the whole run:
/// the pass broke out of the loop, so a healthy ticket mid-workflow never got
/// its next state and no second pass happened.
// §FS-rhei-run.3 §FS-rhei-agents.5.2.1
fn sequential_stall_machine(command: &str) -> String {
    format!(
        r#"name: sequential-stall
version: 1
states:
  probe:
    initial: true
    description: Advances the ticket into work
    program:
      command: {command}
    program_timeout: 20s
  work:
    description: Agent work
    agent: mock
    agent_timeout: 10s
  completed:
    final: true
    description: Done
transitions:
  - from: probe
    to: work
    exit_code: 0
  - from: work
    to: completed
"#
    )
}

const SEQUENTIAL_STALL_PLAN: &str = r#"# Rhei: Sequential Stall

## Tasks

### Task 1: Probe then work
**State:** probe

### Task 2: Silent worker
**State:** work
"#;

#[test]
fn a_sequential_stall_does_not_end_the_run() {
    let dir = unique_temp_dir("terminal-result-sequential-stall");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", SEQUENTIAL_STALL_PLAN);
    let probe = write_exiting_agent(&dir, "probe.py", 0);
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        &sequential_stall_machine(&fixture_command(&probe)),
    );
    // Task 2 is the silent one; task 1 writes its result and finishes.
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"task = env('RHEI_TASK_ID')
if task != 'plan.2':
    result('{} finished.\n'.format(task))
"#,
    );
    write_mock_agent_settings(&dir, &agent);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(!result.status.success(), "the run still halts on the ticket that stalled");
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(combined.contains("Pass 2"), "the pass after the stall happens; got:\n{combined}");
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert_task_state(&plan_path, &machine_path, "2", "work");
    let report = fs::read_to_string(dir.join("runtime/run-report.md")).expect("run report");
    let row = report.lines().find(|line| line.starts_with("| plan.2 | work |")).unwrap();
    for expected in [
        "2 attempts spent on this visit".to_string(),
        format!("result ({})", dir.join("runtime").join("results").join("plan.2.md").display()),
        "`rhei retry plan.2`, then rerun".to_string(),
    ] {
        assert!(row.contains(&expected), "the stalled ticket lacks {expected:?}: {row}");
    }
}

const THREE_WORKER_PLAN: &str = r#"# Rhei: Three Workers

## Tasks

### Task 1: Worker one
**State:** work

### Task 2: Silent worker
**State:** work

### Task 3: Worker three
**State:** work
"#;

/// One ticket stalling must not take its siblings down with it, including the
/// ones a non-concurrent state deferred behind it.
// §FS-rhei-run.3
#[test]
fn a_sequential_stall_leaves_its_siblings_claimable() {
    let dir = unique_temp_dir("terminal-result-sequential-siblings");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", THREE_WORKER_PLAN);
    let probe = write_exiting_agent(&dir, "probe.py", 0);
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        &sequential_stall_machine(&fixture_command(&probe)),
    );
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"task = env('RHEI_TASK_ID')
if task != 'plan.2':
    result('{} finished.\n'.format(task))
"#,
    );
    write_mock_agent_settings(&dir, &agent);

    let result = run_cli(
        "run",
        &plan_path,
        &machine_path,
        &["--no-tui", "--no-callbacks", "--parallel", "1"],
    );
    assert!(!result.status.success(), "the silent ticket still halts the run");
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert_task_state(&plan_path, &machine_path, "2", "work");
    assert_task_state(&plan_path, &machine_path, "3", "completed");
}

const RESULT_ONLY_MISSING_PLAN: &str = r#"# Rhei: Result Only Missing

## Tasks

### Task 1: Implement
**State:** implement
"#;

/// The shape of issue #105: the state declares an output, the agent writes it
/// and exits 0, and the edge out of the state is terminal — so the ticket's
/// result is the one artifact of the completion condition still owed.
// §FS-rhei-agents.3.2 §FS-rhei-states.3.3
const RESULT_ONLY_MISSING_MACHINE: &str = r#"name: result-only-missing
version: 1
states:
  implement:
    initial: true
    description: Writes its declared output and never its result
    agent: mock
    agent_timeout: 20s
    outputs:
      - name: report
        path: artifacts/report-{task_id}.md
  completed:
    final: true
    description: Done
transitions:
  - from: implement
    to: completed
"#;

/// Publishes the declared output, counts the attempt so the test can tell one
/// spawn from the next, and exits 0 without touching `RHEI_RESULT_PATH`.
const OUTPUT_WITHOUT_RESULT_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
counter = root / 'attempts.txt'
n = int(counter.read_text().strip()) + 1 if counter.exists() else 1
write(counter, str(n))
write(root / 'artifacts' / ('report-' + env('RHEI_TASK_ID') + '.md'), 'the report\n')
sys.stdout.write('ATTEMPT-{}\n'.format(n))
"#;

fn setup_result_only_missing(name: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(name);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", RESULT_ONLY_MISSING_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", RESULT_ONLY_MISSING_MACHINE);
    let agent = write_python_agent(&dir, "mock-agent.py", OUTPUT_WITHOUT_RESULT_AGENT);
    write_mock_agent_settings(&dir, &agent);
    (dir, plan_path, machine_path)
}

/// A ticket that failed the completion condition on one pass was read on the
/// next as having nothing left to do — its declared outputs were on disk — and
/// was advanced into its terminal state carrying a result that said no agent had
/// run. The scheduler asks the whole condition now, so the recovery is the one
/// the execution loop prescribes: run the state again.
// §FS-rhei-agents.3.2 §FS-rhei-run.3
#[test]
fn an_agent_owing_only_the_result_is_respawned_rather_than_advanced() {
    let (dir, plan_path, machine_path) = setup_result_only_missing("terminal-result-only-missing");

    let first = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(!first.status.success(), "the first run halts on the result the agent owes");
    assert_task_state(&plan_path, &machine_path, "1", "implement");

    let second = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    let combined = format!("{}{}", second.stdout, second.stderr);
    assert!(
        combined.contains("Re-spawning Task plan.1 in state 'implement'"),
        "the next pass runs the state again, and says why; got:\n{combined}"
    );
    assert!(!second.status.success(), "a second silent attempt halts the same way");
    assert_task_state(&plan_path, &machine_path, "1", "implement");
    assert_eq!(
        fs::read_to_string(dir.join("attempts.txt")).expect("attempt counter").trim(),
        "2",
        "the agent was spawned again rather than skipped"
    );

    // Asserting on the file's *contents* would pass against an empty string
    // whatever the engine had said; the fact under test is that the engine wrote
    // nothing for a ticket it did not finish. §FS-rhei-states.3.3
    let result_path = dir.join("runtime/results/plan.1.md");
    assert!(
        !result_path.exists(),
        "no transition fired, so no result was recorded; got:\n{}",
        fs::read_to_string(&result_path).unwrap_or_default()
    );
}

/// The re-spawn used to truncate the log of the attempt it was retrying, which
/// is the one file that says why that attempt did not finish.
///
/// Asserting only that `-attempt2` appeared would not have caught the other
/// half of the same mistake: an `-attempt2` written for a spawn that is not a
/// retry at all. So the whole listing is checked — two spawns of one visit,
/// two transcripts, and no third name from anywhere else.
// §FS-rhei-agents.8.1
#[test]
fn a_respawn_keeps_the_earlier_attempts_transcript() {
    let (dir, plan_path, machine_path) = setup_result_only_missing("terminal-result-attempt-logs");

    for _ in 0..2 {
        run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    }

    let logs = dir.join("runtime/logs");
    let first = fs::read_to_string(logs.join("task-plan.1-implement.log"))
        .expect("the first attempt keeps the unsuffixed name");
    let second = fs::read_to_string(logs.join("task-plan.1-implement-attempt2.log"))
        .expect("the re-spawn writes its own attempt log");
    assert!(first.contains("ATTEMPT-1"), "attempt 1's transcript survives; got:\n{first}");
    assert!(second.contains("ATTEMPT-2"), "attempt 2 wrote its own file; got:\n{second}");

    let mut names = fs::read_dir(&logs)
        .expect("read logs")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec!["task-plan.1-implement-attempt2.log", "task-plan.1-implement.log"],
        "two spawns of one visit leave two transcripts and nothing else"
    );
}

/// `--no-agent` walks an edge with no worker spawned under it, but a worker may
/// well have run in that state on an earlier run. The engine's own account says
/// what the log proves rather than that no agent ran.
// §FS-rhei-run.3 §FS-rhei-agents.8.1
#[test]
fn callback_only_advancement_names_an_agent_that_ran_earlier() {
    let (dir, plan_path, machine_path) = setup_result_only_missing("terminal-result-no-agent-stub");

    let first = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(!first.status.success(), "the agent ran and left the result unwritten");

    let advanced =
        run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks", "--no-agent"]);
    assert_success(&advanced);
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    let recorded =
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("read result file");
    assert!(
        recorded.contains("agent 'mock' ran in that state earlier"),
        "the account names the agent the log proves ran; got:\n{recorded}"
    );
    assert!(
        recorded.contains("task-plan.1-implement.log"),
        "and where that agent's transcript is; got:\n{recorded}"
    );
    assert!(
        !recorded.contains("No agent or program ran"),
        "which is the opposite of what it used to say; got:\n{recorded}"
    );
}
