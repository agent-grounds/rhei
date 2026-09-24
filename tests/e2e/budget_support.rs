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

/// A fake agent that records the spawn and writes the artifact of the state it
/// is in — its own, and only its own.
///
/// Writing the sibling's too would hand the ticket its first arrival at that
/// state for free: with no spawn record for the visit and no prior visit to the
/// state, `InvocationCompletion::work_is_eligible_for_visit` reads the artifact
/// already on disk as this visit's own work and walks the edge with no
/// subprocess. One free edge per run, so a bound of `n` would yield `n - 1`
/// spawns. From the second visit onwards `has_prior_state_visit()` closes that
/// door by itself, which is why clearing the sibling is not needed.
/// §FS-rhei-states.3.3
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
write(root / 'runtime' / '{}.md'.format(env('RHEI_STATE')), '{}\n'.format(env('RHEI_STATE')))
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

/// Assert a command left the plan as it was, but for the ticket's budget
/// identity.
///
/// The identity follows **spending**, never attempting: an admission that
/// appended a reservation against the project's account earns the ticket its
/// durable `budgetTicketId`, and one that spent nothing writes nothing at all.
/// So a visit that spawned leaves that one key behind even where it changed
/// nothing else, and the assertion that a hold rewrote nothing is about
/// everything else — the state, the visit count, the supervision block, the
/// body of the document.
///
/// Compared by frontmatter *value* with the key removed rather than
/// byte-for-byte, because writing any metadata re-emits the whole frontmatter
/// and normalizes its YAML style. §FS-rhei-budgets.6.1
pub(super) fn assert_plan_but_for_the_budget_identity(actual: &str, before: &str, why: &str) {
    assert_eq!(plan_body(actual), plan_body(before), "{why}");
    assert_eq!(
        plan_metadata_without_identity(actual),
        plan_metadata_without_identity(before),
        "{why}"
    );
}

/// The document with its frontmatter block excised, which no budget write ever
/// touches — and which reads the same whether or not the plan has one.
fn plan_body(plan: &str) -> String {
    match frontmatter_span(plan) {
        Some((open, _, end)) => format!("{}{}", &plan[..open], &plan[end..]),
        None => plan.to_string(),
    }
}

/// The frontmatter as a value, with every ticket's `budgetTicketId` removed.
fn plan_metadata_without_identity(plan: &str) -> serde_yaml::Value {
    let Some((_, start, end)) = frontmatter_span(plan) else { return serde_yaml::Value::Null };
    let body = &plan[start..end.saturating_sub(5)];
    let mut value: serde_yaml::Value = serde_yaml::from_str(body).expect("plan frontmatter");
    if let Some(tasks) = value
        .get_mut("metadata")
        .and_then(|metadata| metadata.get_mut("tasks"))
        .and_then(serde_yaml::Value::as_mapping_mut)
    {
        for (_, task) in tasks.iter_mut() {
            if let Some(task) = task.as_mapping_mut() {
                task.remove(serde_yaml::Value::from("budgetTicketId"));
            }
            // A ticket whose only metadata was the identity is a ticket the
            // plan never carried an entry for.
            if task.as_mapping().is_some_and(serde_yaml::Mapping::is_empty) {
                *task = serde_yaml::Value::Null;
            }
        }
        tasks.retain(|_, task| !task.is_null());
        if tasks.is_empty() {
            value.as_mapping_mut().expect("frontmatter mapping").remove("metadata");
        }
    }
    // A frontmatter block holding nothing else is the plan that had none: the
    // identity is what put it there.
    if value.as_mapping().is_some_and(serde_yaml::Mapping::is_empty) {
        return serde_yaml::Value::Null;
    }
    value
}

/// Where the opening `---` line begins, where the body begins, and where the
/// closing `---\n` line ends.
fn frontmatter_span(plan: &str) -> Option<(usize, usize, usize)> {
    let open = plan.find("\n---\n")?;
    let start = open + 5;
    let close = start + plan[start..].find("\n---\n")?;
    Some((open, start, close + 5))
}
