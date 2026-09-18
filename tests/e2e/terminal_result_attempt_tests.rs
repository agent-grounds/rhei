//! Re-spawning a state that did not finish: which spawns are attempts at the
//! same visit, which are a fresh visit, and how many of the first kind a visit
//! gets before the run stops paying for them.
//!
//! The distinction is the whole subject. `visits:` bounds how many times a
//! ticket may *enter* a state; `attempts:` bounds how many times one entry may
//! be *spawned*. Conflating them is how a ticket that legitimately came back to
//! a state was narrated as a failed retry, and how a state that really was
//! stuck was re-spawned once per `rhei run`, forever.

// §FS-rhei-agents.3.2.3 §FS-rhei-agents.8.1 §FS-rhei-agents.8.4 §FS-rhei-run.3

use std::fs;

use super::operator_attended_tests::attended;
use super::operator_force_support::ForceFixture;
use super::terminal_result_tests::write_mock_agent_settings;
use super::*;

/// Publishes the state's declared output, counts the spawn, and exits 0 without
/// touching `RHEI_RESULT_PATH` — the shape of issue #105.
const OUTPUT_WITHOUT_RESULT_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
counter = root / ('attempts-' + env('RHEI_TASK_ID') + '.txt')
n = int(counter.read_text().strip()) + 1 if counter.exists() else 1
write(counter, str(n))
write(root / 'artifacts' / ('report-' + env('RHEI_TASK_ID') + '.md'), 'the report\n')
sys.stdout.write('ATTEMPT-{} OF-VISIT-{}\n'.format(env('RHEI_ATTEMPT'), env('RHEI_VISIT_COUNT')))
"#;

/// Writes the ticket's result every time, so every spawn finishes its state and
/// the only thing a second spawn can mean is a second *visit*.
const FINISHING_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
counter = root / ('spawns-' + env('RHEI_STATE') + '.txt')
n = int(counter.read_text().strip()) + 1 if counter.exists() else 1
write(counter, str(n))
result('done in ' + env('RHEI_STATE') + '\n')
sys.stdout.write('RAN-{}-{}\n'.format(env('RHEI_STATE'), env('RHEI_ATTEMPT')))
"#;

const ONE_TICKET_PLAN: &str = r#"# Rhei: Attempts

## Tasks

### Task 1: Implement
**State:** implement
"#;

fn result_only_missing_machine(extra_state_fields: &str) -> String {
    format!(
        r#"name: attempt-budget
version: 1
states:
  implement:
    initial: true
    description: Writes its declared output and never its result
    agent: mock
    agent_timeout: 20s
    concurrent: true
{extra_state_fields}    outputs:
      - name: report
        path: artifacts/report-{{task_id}}.md
  completed:
    final: true
    description: Done
transitions:
  - from: implement
    to: completed
"#
    )
}

fn setup(name: &str, plan: &str, machine: &str, agent_body: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(name);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let agent = write_python_agent(&dir, "mock-agent.py", agent_body);
    write_mock_agent_settings(&dir, &agent);
    (dir, plan_path, machine_path)
}

fn spawn_count(dir: &Path, name: &str) -> u32 {
    fs::read_to_string(dir.join(name)).map(|raw| raw.trim().parse().unwrap_or(0)).unwrap_or(0)
}

fn log_names(dir: &Path) -> Vec<String> {
    let mut names = fs::read_dir(dir.join("runtime/logs"))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// A budget is per *visit* and persisted with it, so it cannot be refreshed by
/// starting another `rhei run`. Before this, "one spawn per run" was unbounded
/// across runs, which is exactly how a cycling machine reached 903 transcripts.
// §FS-rhei-agents.3.2.3 §FS-rhei-run.3
#[test]
fn a_visit_is_spawned_at_most_its_attempt_budget_across_separate_runs() {
    let (dir, plan_path, machine_path) = setup(
        "attempts-budget",
        ONE_TICKET_PLAN,
        &result_only_missing_machine(""),
        OUTPUT_WITHOUT_RESULT_AGENT,
    );

    for _ in 0..4 {
        let run = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
        assert!(!run.status.success(), "the ticket never finishes, so no run succeeds");
    }

    assert_eq!(
        spawn_count(&dir, "attempts-plan.1.txt"),
        2,
        "four runs, one visit, the built-in budget of two spawns"
    );
    assert_task_state(&plan_path, &machine_path, "1", "implement");
    assert_eq!(
        log_names(&dir),
        vec!["task-plan.1-implement-attempt2.log", "task-plan.1-implement.log"],
        "and two transcripts, not one per run"
    );
}

/// The halt has to say what it spent and what is still owed, at the moment it
/// stops spawning — an operator who only learns at the end of the run that the
/// ticket never moved has no idea which of the two bounds applied.
// §FS-rhei-agents.3.2.3
#[test]
fn an_exhausted_budget_halts_the_ticket_where_it_is_and_says_what_it_owes() {
    let (_dir, plan_path, machine_path) = setup(
        "attempts-halt",
        ONE_TICKET_PLAN,
        &result_only_missing_machine(""),
        OUTPUT_WITHOUT_RESULT_AGENT,
    );

    for _ in 0..2 {
        run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    }
    let spent = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    let combined = format!("{}{}", spent.stdout, spent.stderr);

    assert!(
        combined
            .contains("halting Task plan.1 in state 'implement': 2 attempts spent on this visit"),
        "the halt names the ticket, the state, and the attempts; got:\n{combined}"
    );
    assert!(
        combined.contains("result ("),
        "and the artifact the completion condition still owes; got:\n{combined}"
    );
    assert!(!spent.status.success(), "a run that ends with the ticket unfinished exits non-zero");
    // No error edge, no timeout edge, no `cancelled`: the ticket keeps its
    // state exactly as any other stall leaves it. §FS-rhei-run.3
    assert_task_state(&plan_path, &machine_path, "1", "implement");
}

/// The re-spawn line is the only place an operator sees the loop while it is
/// running, so it carries the attempt, the budget, and *why* the attempt before
/// it did not finish — not one canned rule for every ending.
// §FS-rhei-agents.3.2.1
#[test]
fn a_respawn_names_the_attempt_the_budget_and_what_ended_the_previous_one() {
    let (_dir, plan_path, machine_path) = setup(
        "attempts-note",
        ONE_TICKET_PLAN,
        &result_only_missing_machine(""),
        OUTPUT_WITHOUT_RESULT_AGENT,
    );

    run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    let second = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    let combined = format!("{}{}", second.stdout, second.stderr);

    assert!(
        combined.contains(
            "Re-spawning Task plan.1 in state 'implement': attempt 2 of 2; the previous attempt \
             exited 0 without meeting this state's completion condition"
        ),
        "the note says which attempt, out of how many, and what happened; got:\n{combined}"
    );
}

/// The halt line predicts what the run will do next, so it has to be wrong in
/// neither direction. While attempts remain it promises another pass; on the
/// run that *spends the last one* it must not, because no later pass will spawn
/// the state again — and the run after it says exactly that. Promising a retry
/// and then silently never making one is the defect this whole change exists to
/// remove, one message over.
// §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3
#[test]
fn the_run_that_spends_the_last_attempt_does_not_promise_another() {
    let (_dir, plan_path, machine_path) = setup(
        "attempts-last-promise",
        ONE_TICKET_PLAN,
        &result_only_missing_machine(""),
        OUTPUT_WITHOUT_RESULT_AGENT,
    );
    let halt_line = |run: &CliRun| -> String {
        format!("{}{}", run.stdout, run.stderr)
            .lines()
            .find(|line| line.contains("halting Task plan.1"))
            .unwrap_or_default()
            .to_string()
    };
    let run =
        || halt_line(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));

    // Attempt 1 of 2: a later pass really does run the state again.
    let first = run();
    assert!(
        first.contains("a later pass runs the state again"),
        "with an attempt left the promise is true and stays; got:\n{first}"
    );

    // Attempt 2 of 2: the budget is spent by the attempt that just finished.
    let second = run();
    assert!(
        !second.contains("a later pass runs the state again"),
        "the last attempt must not promise a pass that will not spawn; got:\n{second}"
    );
    assert!(
        second.contains("2 attempts spent on this visit"),
        "it says the budget is spent, at the moment it becomes true; got:\n{second}"
    );

    // And the pass that declines to spawn says the very same thing, because it
    // is the very same fact.
    assert_eq!(second, run(), "one sentence with two moments, not two that disagree");
}

/// The budget resolves through the same shape a timeout does: the state's own
/// field first, then `defaults.attempts`, then the built-in.
// §FS-rhei-agents.3.2.3
#[test]
fn a_state_level_attempts_field_wins_over_the_settings_default() {
    let (dir, plan_path, machine_path) = setup(
        "attempts-resolution",
        ONE_TICKET_PLAN,
        &result_only_missing_machine("    attempts: 3\n"),
        OUTPUT_WITHOUT_RESULT_AGENT,
    );
    // A settings default of 1 would stop after the first spawn; the state's
    // own `attempts: 3` is the more specific level and wins.
    let settings = dir.join(".agent-grounds/rhei/settings.json");
    let raw = fs::read_to_string(&settings).expect("read settings");
    fs::write(
        &settings,
        raw.replace(
            r#""defaults": { "agent": "mock""#,
            r#""defaults": { "attempts": 1, "agent": "mock""#,
        ),
    )
    .expect("write settings");

    for _ in 0..5 {
        run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    }

    assert_eq!(
        spawn_count(&dir, "attempts-plan.1.txt"),
        3,
        "the state's own budget is the one spent"
    );
}

/// The settings default applies where a state declares no `attempts:` of its
/// own — the second level of the chain, and the only one an operator can set
/// once for a whole project.
// §FS-rhei-agents.3.2.3
#[test]
fn a_settings_default_supplies_the_budget_a_state_does_not_declare() {
    let (dir, plan_path, machine_path) = setup(
        "attempts-settings-default",
        ONE_TICKET_PLAN,
        &result_only_missing_machine(""),
        OUTPUT_WITHOUT_RESULT_AGENT,
    );
    let settings = dir.join(".agent-grounds/rhei/settings.json");
    let raw = fs::read_to_string(&settings).expect("read settings");
    fs::write(
        &settings,
        raw.replace(
            r#""defaults": { "agent": "mock""#,
            r#""defaults": { "attempts": 1, "agent": "mock""#,
        ),
    )
    .expect("write settings");

    for _ in 0..3 {
        run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    }

    assert_eq!(
        spawn_count(&dir, "attempts-plan.1.txt"),
        1,
        "one spawn, and no informed retry, because the project asked for none"
    );
}

const CYCLE_PLAN: &str = r#"# Rhei: Re-entry

## Tasks

### Task 1: Loop
**State:** a
"#;

/// `a → b → done`; an operator can send the ticket round again with the
/// attended missing-edge recovery path. Every state finishes its work, so a
/// second spawn of `a` can only mean a second *visit* — which is the case the
/// engine used to read as a retry.
// §FS-rhei-agents.8.1
const CYCLE_MACHINE: &str = r#"name: reentry
version: 1
states:
  a:
    initial: true
    description: First
    agent: mock
    agent_timeout: 20s
  b:
    description: Second
    agent: mock
    agent_timeout: 20s
  done:
    final: true
    description: Done
transitions:
  - from: a
    to: b
  - from: b
    to: done
"#;

/// Entering a state again is a new visit, so it starts over: the plain log name,
/// no retry narration, and a fresh `attempts:` budget. This is the seam between
/// the two bounds — `visits:` ticks here, and an `attempts:` budget that did not
/// reset with it would make a ticket sent round the loop unrunnable on its
/// second lap.
// §FS-rhei-agents.3.2.3 §FS-rhei-agents.8.1
#[test]
fn re_entering_a_state_is_a_new_visit_with_a_fresh_attempt_budget() {
    let (dir, plan_path, machine_path) =
        setup("attempts-reentry", CYCLE_PLAN, CYCLE_MACHINE, FINISHING_AGENT);
    let fixture = ForceFixture { dir, plan: plan_path.clone(), machine: machine_path.clone() };

    let first = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert_success(&first);
    assert_task_state(&plan_path, &machine_path, "1", "done");

    // Send it round again through the attended operator recovery path. The
    // recovery operation removes only the terminal task link while retaining
    // the result history; no fixture-side ledger or plan edit is needed.
    let (success, transcript) = attended(
        &fixture,
        &[
            "--state-machine",
            machine_path.to_str().expect("machine path"),
            "transition",
            plan_path.to_str().expect("plan path"),
            "--task",
            "1",
            "--from",
            "done",
            "--to",
            "a",
            "--force",
            "--reason",
            "reopen visit",
        ],
        "force plan.1 done -> a\r\n",
    );
    assert!(success, "{transcript}");
    let after_recovery = fs::read_to_string(&plan_path).expect("read plan after recovery");
    assert!(!after_recovery.contains("> **Result:**"), "terminal task link is removed");

    let second = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    let combined = format!("{}{}", second.stdout, second.stderr);
    assert_success(&second);
    assert_task_state(&plan_path, &machine_path, "1", "done");

    assert_eq!(spawn_count(&fixture.dir, "spawns-a.txt"), 2, "the second lap ran state 'a' again");
    assert!(
        !combined.contains("Re-spawning"),
        "a fresh entry is not a retry of the last one; got:\n{combined}"
    );
    assert!(
        !combined.contains("attempts spent"),
        "and it did not arrive with the first lap's budget already gone; got:\n{combined}"
    );
    assert_eq!(
        log_names(&fixture.dir),
        vec!["task-plan.1-a.log", "task-plan.1-b.log"],
        "each visit writes the plain name; `-attempt` is for retries within one visit"
    );
}

const TWO_TICKET_PLAN: &str = r#"# Rhei: Attempts In Parallel

## Tasks

### Task 1: Implement one
**State:** implement

### Task 2: Implement two
**State:** implement
"#;

/// The worker pool schedules through its own code path, and `--parallel`
/// defaults to 1, so every other test in this suite exercises the sequential
/// one. The completion condition, the attempt log, the retry narration and the
/// budget all have to hold in the pool too — they are one rule, and a rule with
/// one tested caller is a rule with one caller that works.
// §FS-rhei-agents.3.2 §FS-rhei-agents.3.2.3 §FS-rhei-run.5
#[test]
fn the_worker_pool_respawns_and_bounds_the_same_way_the_sequential_path_does() {
    let (dir, plan_path, machine_path) = setup(
        "attempts-parallel",
        TWO_TICKET_PLAN,
        &result_only_missing_machine(""),
        OUTPUT_WITHOUT_RESULT_AGENT,
    );
    let args = ["--no-tui", "--no-callbacks", "--parallel", "2"];

    let first = run_cli("run", &plan_path, &machine_path, &args);
    assert!(!first.status.success(), "both tickets owe their result");
    assert_task_state(&plan_path, &machine_path, "1", "implement");
    assert_task_state(&plan_path, &machine_path, "2", "implement");

    let second = run_cli("run", &plan_path, &machine_path, &args);
    let combined = format!("{}{}", second.stdout, second.stderr);
    for task in ["plan.1", "plan.2"] {
        assert!(
            combined
                .contains(&format!("Re-spawning Task {task} in state 'implement': attempt 2 of 2")),
            "the pool re-spawns {task} rather than advancing it; got:\n{combined}"
        );
    }

    assert!(
        !combined.contains("a later pass runs the state again"),
        "and neither ticket is promised a pass the pool will not run; got:\n{combined}"
    );

    let third = run_cli("run", &plan_path, &machine_path, &args);
    let combined = format!("{}{}", third.stdout, third.stderr);
    for task in ["plan.1", "plan.2"] {
        assert!(
            combined
                .contains(&format!("halting Task {task} in state 'implement': 2 attempts spent")),
            "and the pool honours the same budget; got:\n{combined}"
        );
    }
    assert_eq!(spawn_count(&dir, "attempts-plan.1.txt"), 2);
    assert_eq!(spawn_count(&dir, "attempts-plan.2.txt"), 2);
}

/// A spawn that never started leaves a complete-looking log header behind — the
/// engine writes it before the subprocess exists. Crediting that log to a worker
/// is the mirror of the bug this whole change fixes, so the evidence is the
/// spawn record, which only a subprocess that ran can produce.
// §FS-rhei-agents.8.4 §FS-rhei-run.3
#[test]
fn a_spawn_that_never_started_is_not_evidence_that_a_worker_ran() {
    let dir = unique_temp_dir("attempts-unspawnable");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", ONE_TICKET_PLAN);
    // No declared `outputs:`, so the `--no-agent` pass below can take the edge
    // and reach the sentence under test. §FS-rhei-states.1.4
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: unspawnable
version: 1
states:
  implement:
    initial: true
    description: An agent command that does not exist
    agent: mock
    agent_timeout: 20s
  completed:
    final: true
    description: Done
transitions:
  - from: implement
    to: completed
"#,
    );
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    fs::write(
        settings_dir.join("settings.json"),
        r#"{
  "defaults": { "agent": "mock", "agent_timeout": "10s" },
  "agents": {
    "mock": { "command": ["rhei-no-such-binary-anywhere"], "timeout": "10s" }
  }
}"#,
    )
    .expect("write settings");

    let failed = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(!failed.status.success(), "the agent command does not exist");
    assert!(
        dir.join("runtime/logs/task-plan.1-implement.log").exists(),
        "the header was written before the spawn was attempted, which is the trap"
    );

    let advanced =
        run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks", "--no-agent"]);
    assert_success(&advanced);
    let recorded =
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("read result file");
    assert!(
        recorded.contains("No agent or program ran in that state"),
        "nothing ran, and the engine says so; got:\n{recorded}"
    );
}
