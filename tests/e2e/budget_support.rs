//! The fixture world the four budget scenarios share.
//!
//! Every one of them needs the same three things: a machine-global settings
//! file separate from the project's, a portable fake agent that records each
//! spawn, and a `rhei` invocation whose clock the test can set. None of that is
//! specific to one scenario, and a second copy of it would drift.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;

/// How many spawns the fake agent allows before it refuses to continue.
///
/// Nothing bounds a ping-pong ticket today, so without a guard these scenarios
/// would not fail — they would **hang**, which pins nothing and blocks the
/// suite. The guard makes the unbounded case a loud non-zero exit with a spawn
/// count nobody asked for. It sits far above every bound these tests set, so a
/// correct engine never reaches it.
pub(super) const FIXTURE_SPAWN_GUARD: usize = 25;

/// A two-state ping-pong: `work` sends the ticket to `review`, `review` sends
/// it back, and only a spent bound ever ends it.
///
/// Each state also declares an edge to `cancelled`, because a state with no
/// path to a final state is a machine the validator rejects. They are declared
/// **after** the loop edges and carry no condition, so the first match is
/// always the loop and the escape is never selected.
pub(super) const PING_PONG_MACHINE: &str = r#"name: budget-ping-pong
version: 1
states:
  work:
    initial: true
    description: Do a round of work
    agent: mock
    agent_timeout: 30s
    outputs:
      - name: work
        path: runtime/work.md
  review:
    description: Send it back for another round
    agent: mock
    agent_timeout: 30s
    outputs:
      - name: review
        path: runtime/review.md
  cancelled:
    description: Stop
    final: true
transitions:
  - { from: work, to: review, description: Round done }
  - { from: review, to: work, description: Another round }
  - { from: work, to: cancelled, description: Stop }
  - { from: review, to: cancelled, description: Stop }
"#;

/// A plan that finishes: one agent state and one move into a terminal state.
/// Nothing here declares a bound, which is the whole point of the
/// declaration-free scenario.
pub(super) const FINISHING_MACHINE: &str = r#"name: budget-finishing
version: 1
states:
  work:
    initial: true
    description: Do the work once
    agent: mock
    agent_timeout: 30s
    outputs:
      - name: work
        path: runtime/work.md
  completed:
    description: Done
    final: true
transitions:
  - { from: work, to: completed, description: Work done }
"#;

pub(super) const PLAN: &str = r#"# Rhei: Bounded work

## Tasks

### Task 1: Work
**State:** work
"#;

/// A fake agent that records the spawn and writes both loop artifacts.
///
/// It writes **no** result: the loop never reaches a terminal state, and a
/// ticket that was stopped rather than finished must have no account of itself
/// on disk for the halt scenario to mean anything.
///
/// Past the guard it exits non-zero without declaring an `exit_code:` route,
/// which aborts the run. Deleting its artifacts instead would not do: an old
/// `work.md` still satisfies the completion condition, so the loop would
/// continue.
pub(super) const RECORDING_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
log = root / 'runtime' / 'spawn-count.log'
seen = len(log.read_text(encoding='utf-8').splitlines()) if log.exists() else 0
append(log, '{}\n'.format(env('RHEI_STATE')))
if seen >= 25:
    raise SystemExit(17)
write(root / 'runtime' / 'work.md', 'work\n')
write(root / 'runtime' / 'review.md', 'review\n')
"#;

/// The same agent for a plan that does reach a terminal state, so it owes the
/// ticket's own account. §FS-rhei-states.3.3
pub(super) const FINISHING_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'runtime' / 'spawn-count.log', '{}\n'.format(env('RHEI_STATE')))
write(root / 'runtime' / 'work.md', 'work\n')
result('done\n')
"#;

/// Where this test's machine-global settings live.
///
/// `rhei_command` pins `HOME` under the isolated home, so the machine's file is
/// the one the product reads at `~/.config/rhei/settings.json`. Writing it here
/// rather than into the project is what makes the ceiling a *machine* value: a
/// project file resolves one tier lower and could never clamp anything.
// §FS-rhei-agents.1.1.1
pub(super) fn write_machine_settings(root: &Path, body: &str) {
    let dir = home_for(root).join(".config/rhei");
    fs::create_dir_all(&dir).expect("create machine settings directory");
    fs::write(dir.join("settings.json"), body).expect("write machine settings");
}

/// The `defaults` block every scenario needs before it adds bounds of its own:
/// an agent that exists and a timeout that is finite.
pub(super) fn agent_defaults(agent: &Path, extra: &str) -> String {
    format!(
        r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "30s"{extra} }},
  "agents": {{
    "mock": {{ "command": {}, "stdin_prompt": true, "timeout": "30s" }}
  }}
}}"#,
        fixture_command(agent)
    )
}

pub(super) fn home_for(root: &Path) -> PathBuf {
    root.join(".home")
}

/// A workspace whose machine settings carry `extra` inside their `defaults`
/// block, with `agent_body` installed as the only agent.
pub(super) fn setup_with_agent(
    prefix: &str,
    machine: &str,
    agent_body: &str,
    extra: &str,
) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let agent = write_python_agent(&dir, "mock-agent.py", agent_body);
    write_machine_settings(&dir, &agent_defaults(&agent, extra));
    (dir, plan, machine_path)
}

/// [`setup_with_agent`] with the looping agent, which is what three of the four
/// scenarios want.
pub(super) fn setup(prefix: &str, machine: &str, extra: &str) -> (TestDir, PathBuf, PathBuf) {
    setup_with_agent(prefix, machine, RECORDING_AGENT, extra)
}

/// `rhei <subcommand>` against this workspace, with the budget clock set.
///
/// The ordinary `run_cli` helper cannot carry an environment, and the window
/// scenario is the one place a test has to say what day it is:
/// `RHEI_BUDGET_NOW` is the only seam through which the engine reads a clock.
// §FS-rhei-budgets.3.3.1
pub(super) fn run_at(
    subcommand: &str,
    plan: &Path,
    machine: &Path,
    now: Option<&str>,
    extra_args: &[&str],
) -> CliRun {
    let root = plan.parent().expect("plan has a parent");
    let mut cmd: Command = rhei_command(home_for(root));
    cmd.arg("--state-machine").arg(machine).arg(subcommand).arg(plan);
    for arg in extra_args {
        cmd.arg(arg);
    }
    if let Some(instant) = now {
        cmd.env("RHEI_BUDGET_NOW", instant);
    }
    let output = cmd.output().expect("rhei command should run");
    CliRun::from(&output)
}

/// `rhei run` with the TUI and callbacks out of the way.
pub(super) fn run_plan(plan: &Path, machine: &Path, now: Option<&str>) -> CliRun {
    run_at("run", plan, machine, now, &["--no-tui", "--no-callbacks"])
}

/// One line per spawn, in the order the agent was started.
pub(super) fn spawns(root: &Path) -> Vec<String> {
    fs::read_to_string(root.join("runtime/spawn-count.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Every applied move the engine recorded, which is what travel is counted
/// from. §FS-rhei-budgets.4.1
pub(super) fn ledger(root: &Path) -> Vec<String> {
    fs::read_to_string(root.join("runtime/state-transitions.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Assert the spawn count, and say plainly when the fixture guard was what
/// stopped the run — the signature of a bound that is not there at all.
pub(super) fn assert_spawn_count(root: &Path, expected: usize, why: &str) {
    let observed = spawns(root);
    let guarded = observed.len() > FIXTURE_SPAWN_GUARD;
    assert_eq!(
        observed.len(),
        expected,
        "{why}{}\nspawns={observed:?}\nledger={:?}",
        if guarded { " — the run was stopped by the fixture guard, not by a bound" } else { "" },
        ledger(root)
    );
}

/// Assert on the halt, wherever the run put it. A halt is owed on stderr and in
/// the summary, and a test that read only one of them would pass on a build
/// that dropped the other.
pub(super) fn assert_halt_mentions(result: &CliRun, expected: &str) {
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains(expected),
        "expected the halt to mention {expected:?}; got:\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}
