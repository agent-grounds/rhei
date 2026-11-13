//! A stay the ledger never recorded an arrival for — a `**State:**` line edited
//! by hand between two runs — still takes its own entry number, so the next stay
//! in that state does not reuse its log name (agent-grounds/rhei#667, a gap in
//! the entry numbers of agent-grounds/rhei#309).

// §FS-rhei-agents.8.1 §FS-rhei-authoring.7

use std::fs;

use super::entry_log_support::*;
use super::*;

/// The machine the ticket was reported against: `gate` is an uncounted agent
/// state the ticket reaches from the gating `park`, and `check` sends it back
/// into `gate` once over a real edge before it finishes.
const ARRIVAL_MACHINE: &str = r#"name: missing-arrival
version: 1
states:
  prepare:
    initial: true
    description: Prepare
    program:
      command: PREPARE
    program_timeout: 20s
  park:
    description: Wait for a person
    gating: true
  gate:
    description: Select the gates
    agent: mock
    agent_timeout: 20s
  check:
    description: Check the selection
    program:
      command: CHECK
    program_timeout: 20s
  done:
    description: Done
    final: true
transitions:
  - { from: prepare, to: park, description: Prepared }
  - { from: park, to: gate, description: Go }
  - { from: gate, to: check, description: Selected }
  - { from: check, to: gate, exit_code: 10, description: Select again }
  - { from: check, to: done, exit_code: 0, description: Checked }
"#;

/// Says which spawn of `gate` it is.
const GATE: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
counter = root / 'runtime' / 'gate-spawns.txt'
n = int(counter.read_text().strip()) + 1 if counter.exists() else 1
write(counter, str(n))
sys.stdout.write('GATE-{}\n'.format(n))
"#;

/// Sends the ticket back into `gate` on its first run, finishes it on the next.
const CHECK: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
bounced = root / 'runtime' / 'bounced.txt'
if not bounced.exists():
    write(bounced, '1')
    raise SystemExit(10)
result('checked\n')
"#;

/// Runs the ticket from `prepare` to the `park` gate, so it can be moved into
/// `gate` between two runs.
fn parked_fixture(name: &str) -> (TestDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = unique_temp_dir(name);
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        &ONE_TICKET.replace("**State:** measure", "**State:** prepare"),
    );
    let prepare = write_python_agent(&dir, "prepare.py", "sys.stdout.write('PREPARED\\n')\n");
    let check = write_python_agent(&dir, "check.py", CHECK);
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &ARRIVAL_MACHINE
            .replace("PREPARE", &fixture_command(&prepare))
            .replace("CHECK", &fixture_command(&check)),
    );
    let agent = write_python_agent(&dir, "gate.py", GATE);
    settings(&dir, &agent);
    assert_success(&run_cli("run", &plan, &machine, &RUN));
    assert_task_state(&plan, &machine, "1", "park");
    (dir, plan, machine)
}

/// Both stays in `gate` keep their own log, and the ticket reaches `done`.
fn assert_two_gate_entries(
    dir: &std::path::Path,
    plan: &std::path::Path,
    machine: &std::path::Path,
) {
    let run = run_cli("run", plan, machine, &RUN);
    let combined = format!("{}{}", run.stdout, run.stderr);
    assert!(!combined.contains("refusing to spawn"), "nothing to refuse:\n{combined}");
    assert_success(&run);
    assert_task_state(plan, machine, "1", "done");
    assert_eq!(
        names_in(dir, "runtime/logs"),
        [
            "task-plan.1-check-2.log",
            "task-plan.1-check.log",
            "task-plan.1-gate-2.log",
            "task-plan.1-gate.log",
            "task-plan.1-prepare.log",
        ],
    );
    assert!(read(dir, "runtime/logs/task-plan.1-gate.log").contains("GATE-1"));
    assert!(read(dir, "runtime/logs/task-plan.1-gate-2.log").contains("GATE-2"));
}

/// A ticket moved into an uncounted state by editing its `**State:**` line, as
/// the authoring guide allows, leaves no arrival in the ledger. Its departure
/// from that state reveals the stay, so the next stay over a real edge is entry
/// 2 rather than a second entry 1 refused for a log its own record names.
// §FS-rhei-agents.8.1 §FS-rhei-authoring.7
#[test]
fn a_hand_edited_arrival_still_gives_the_next_stay_its_own_entry() {
    let (dir, plan, machine) = parked_fixture("entry-log-hand-edited-arrival");
    let authored = fs::read_to_string(&plan).expect("plan");
    fs::write(&plan, authored.replace("**State:** park", "**State:** gate"))
        .expect("edit the state by hand");
    assert_two_gate_entries(&dir, &plan, &machine);
    assert_eq!(
        read(&dir, "runtime/state-transitions.log"),
        "plan.1 prepare@park\nplan.1 gate@check\nplan.1 check@gate\nplan.1 gate@check\nplan.1 check@done\n",
        "the ledger has no arrival for the hand-edited stay"
    );
}

/// The control: the same move made with `rhei transition` records the arrival,
/// and the second stay is entry 2 already today.
// §FS-rhei-agents.8.1
#[test]
fn a_recorded_arrival_gives_the_next_stay_its_own_entry() {
    let (dir, plan, machine) = parked_fixture("entry-log-recorded-arrival");
    assert_success(&run_transition(&plan, &machine, "1", "park", "gate"));
    assert_two_gate_entries(&dir, &plan, &machine);
}
