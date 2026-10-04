//! A poll state re-entered through the ledger, and a spawn that never started,
//! both leave the next entry or run a name it can take (agent-grounds/rhei#309).

// §FS-rhei-agents.8.1 §FS-rhei-programs.5.1

use std::fs;

use super::entry_log_support::*;
use super::*;

const POLL_MACHINE: &str = r#"name: entry-poll
version: 1
states:
  wait:
    initial: true
    description: Wait for checks
    poll: { interval: 1s, max_attempts: 5 }
    program:
      command: WAIT
    program_timeout: 20s
  fix:
    description: Fix what the checks found
    program:
      command: FIX
    program_timeout: 20s
  done:
    description: Done
    final: true
transitions:
  - { from: wait, to: wait, exit_code: 1, description: Not yet }
  - { from: wait, to: fix, exit_code: 0, description: Checks failed }
  - { from: wait, to: done, exit_code: 3, description: Checks passed }
  - { from: fix, to: wait, description: Wait again }
"#;

/// Spawn 1 sends the ticket to `fix`, spawn 2 polls again, spawn 3 finishes.
const WAIT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
counter = root / 'runtime' / 'wait-spawns.txt'
n = int(counter.read_text().strip()) + 1 if counter.exists() else 1
write(counter, str(n))
sys.stdout.write('WAIT-{}\n'.format(n))
if n >= 3:
    result('checks passed\n')
raise SystemExit({1: 0, 2: 1}.get(n, 3))
"#;

/// A `poll:` state is numbered by its entries like any uncounted state: the
/// re-entry through `fix` writes `-2`, and the poll retry inside that entry,
/// which writes no ledger line, is `-2-attempt2`.
// §FS-rhei-agents.8.1 §FS-rhei-states.2
#[test]
fn a_poll_state_reentered_through_the_ledger_writes_its_own_entry() {
    let dir = unique_temp_dir("entry-log-poll");
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        &ONE_TICKET.replace("**State:** measure", "**State:** wait"),
    );
    let wait = write_python_agent(&dir, "wait.py", WAIT);
    let fix = write_python_agent(&dir, "fix.py", "sys.stdout.write('FIXED\\n')\n");
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &POLL_MACHINE
            .replace("WAIT", &fixture_command(&wait))
            .replace("FIX", &fixture_command(&fix)),
    );

    let run = run_cli("run", &plan, &machine, &RUN);
    assert_success(&run);
    assert_task_state(&plan, &machine, "1", "done");
    assert_eq!(
        names_in(&dir, "runtime/logs"),
        [
            "task-plan.1-fix.log",
            "task-plan.1-wait-2-attempt2.log",
            "task-plan.1-wait-2.log",
            "task-plan.1-wait.log",
        ],
    );
    assert!(read(&dir, "runtime/logs/task-plan.1-wait.log").contains("WAIT-1"));
    assert!(read(&dir, "runtime/logs/task-plan.1-wait-2.log").contains("WAIT-2"));
    assert!(read(&dir, "runtime/logs/task-plan.1-wait-2-attempt2.log").contains("WAIT-3"));
}

/// Settings whose `mock` agent is a command that does not exist.
fn missing_agent_settings(root: &std::path::Path) {
    let dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&dir).expect("create settings directory");
    fs::write(
        dir.join("settings.json"),
        r#"{
  "defaults": { "agent": "mock", "agent_timeout": "20s" },
  "agents": { "mock": { "command": ["rhei-no-such-binary-anywhere"], "timeout": "20s" } }
}"#,
    )
    .expect("write settings");
}

/// An agent whose command cannot start leaves no log behind, so once the
/// settings are corrected the next run spawns at the same name and finishes.
// §FS-rhei-agents.8.1 §FS-rhei-agents.8.4
#[test]
fn an_agent_that_never_started_leaves_its_name_free() {
    let (dir, plan, machine) = gated_fixture("entry-log-unstarted-agent");
    let agent = dir.join("work.py");
    missing_agent_settings(&dir);

    let failed = run_cli("run", &plan, &machine, &RUN);
    assert!(!failed.status.success(), "the agent command does not exist");
    assert_eq!(names_in(&dir, "runtime/logs"), Vec::<String>::new(), "no log is left");

    settings(&dir, &agent);
    let run = run_cli("run", &plan, &machine, &RUN);
    let combined = format!("{}{}", run.stdout, run.stderr);
    assert!(!combined.contains("refusing to spawn"), "nothing to refuse:\n{combined}");
    assert_success(&run);
    assert_task_state(&plan, &machine, "1", "verifying");
    assert!(read(&dir, "runtime/logs/task-plan.1-work.log").contains("WORK-plan.1-1"));
}

/// A program whose command cannot start leaves no log behind either.
// §FS-rhei-programs.5.1 §FS-rhei-agents.8.1
#[test]
fn a_program_that_never_started_leaves_its_name_free() {
    let dir = unique_temp_dir("entry-log-unstarted-program");
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        &ONE_TICKET.replace("**State:** measure", "**State:** wait"),
    );
    let missing = POLL_MACHINE.replace("WAIT", r#"["rhei-no-such-binary-anywhere"]"#);
    let machine = write_fixture_file(&dir, "states.yaml", &missing);

    let failed = run_cli("run", &plan, &machine, &RUN);
    assert!(!failed.status.success(), "the program command does not exist");
    assert_eq!(names_in(&dir, "runtime/logs"), Vec::<String>::new(), "no log is left");

    let wait = write_python_agent(&dir, "wait.py", "result('passed\\n')\nraise SystemExit(3)\n");
    fs::write(&machine, POLL_MACHINE.replace("WAIT", &fixture_command(&wait)))
        .expect("correct the machine");
    assert_success(&run_cli("run", &plan, &machine, &RUN));
    assert_task_state(&plan, &machine, "1", "done");
    assert_eq!(names_in(&dir, "runtime/logs"), ["task-plan.1-wait.log"]);
}
