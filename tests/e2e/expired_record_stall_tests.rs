//! An expired provider-limit record beside a stalled ticket whose visit still
//! has attempts to spend (agent-grounds/rhei#728).
//!
//! A run-out record reads as "eligible now", and a deadline sleep releases every
//! stalled ticket. So a record observed before the ticket's visit last spawned
//! it, which that spawn already ran past, woke the run every second and
//! re-spawned the stall until its six attempts were gone, where the same plan
//! without the record halted after one. Such a record is no deadline: the run
//! halts on the stall as it does without it.
//!
//! The guard keeps the other half: a ticket a session limit parked, whose record
//! its own spawn wrote, still sleeps to the window and spawns again after it.
// §FS-rhei-run.5.1.1 §FS-rhei-run.3.6

use super::provider_limit_support::*;
use super::until_idle_support::*;
use super::*;
use std::time::Duration;

/// Far above how long the halt takes; the regression spawns six times.
const HALT_PATIENCE: Duration = Duration::from_secs(30);

/// Work with six attempts, and work that needs a brief.
const MACHINE: &str = r#"name: expired-record-stall
version: 1
states:
  work:
    initial: true
    description: Agent work with attempts to spare
    target: codex:openai:alpha
    agent_timeout: 30m
    attempts: 6
  needs:
    description: Agent work that needs a brief
    target: codex:openai:alpha
    agent_timeout: 30m
    inputs:
      - name: brief
        path: runtime/brief.md
  done:
    final: true
    description: Done
transitions:
  - from: work
    to: done
  - from: needs
    to: done
"#;

/// A plan whose agent ends every attempt with `exit` and no result, and logs
/// each spawn beside itself. `records` is the `metadata.tasks` mapping.
fn stall_plan(prefix: &str, records: &str, tasks: &str, exit: i32) -> IdlePlan {
    let front = if records.is_empty() {
        String::new()
    } else {
        format!("---\nmetadata:\n  tasks:\n{records}---\n\n")
    };
    let plan = IdlePlan::new(prefix, &format!("{front}## Tasks\n\n{tasks}"));
    fs::write(&plan.machine, MACHINE).expect("write the machine");
    let body = format!(
        "append(str(pathlib.Path(__file__).with_name('spawns.log')), 'spawn\\n')\nsys.exit({exit})\n"
    );
    let agent = write_python_agent(plan.root(), "agent.py", &body);
    let profile = serde_json::json!({
        "command": serde_json::from_str::<serde_json::Value>(&fixture_command(&agent))
            .expect("fixture command"),
        "stdin_prompt": true,
        "timeout": "12s"
    });
    fs::write(
        plan.root().join(".agent-grounds/rhei/settings.json"),
        serde_json::json!({"agents": {"codex": profile}}).to_string(),
    )
    .expect("write agent settings");
    plan
}

/// A record on ticket `task` in `state`, observed and run out in 2000.
fn expired_record(task: &str, state: &str) -> String {
    format!("    {task}:\n      {}\n", provider_limit(state, ELAPSED_POLL))
}

fn spawns(plan: &IdlePlan) -> usize {
    fs::read_to_string(plan.root().join("spawns.log")).unwrap_or_default().lines().count()
}

fn run_within(plan: &IdlePlan, args: &[&str]) -> CliRun {
    let mut command = rhei_command(&plan.home);
    command.arg("--state-machine").arg(&plan.machine).arg("run").arg(&plan.plan).args(args);
    bounded_within(command, &format!("rhei run {}", args.join(" ")), plan.root(), HALT_PATIENCE)
}

/// The plan with the record and the control without it halt the same way: non-zero,
/// after one spawn, and without sleeping on the record.
#[track_caller]
fn assert_halts_like_its_control(
    (control, plan): (&IdlePlan, &IdlePlan),
    args: &[&str],
    what: &str,
) {
    let result = run_within(control, args);
    assert_ne!(result.status.code(), Some(0), "{what}: the control halts for attention");
    assert_eq!(spawns(control), 1, "{what}: the control halts after its one stalled attempt");
    let result = run_within(plan, args);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert_ne!(result.status.code(), Some(0), "{what}: the run halts; got:\n{combined}");
    assert_eq!(spawns(plan), 1, "{what}: one spawn, as the control; got:\n{combined}");
    assert!(
        !combined.contains("No ready tasks; sleeping"),
        "{what}: a spent record is no deadline; got:\n{combined}"
    );
}

/// The stalled ticket's own expired record, with five attempts left.
#[test]
fn a_stalled_tickets_own_expired_record_does_not_release_it() {
    let tasks = "### Task 1: Fails every attempt\n**State:** work\n";
    let control = stall_plan("expired-own-control", "", tasks, 1);
    let plan = stall_plan("expired-own-record", &expired_record("1", "work"), tasks, 1);
    let args = ["--no-tui", "--continue-on-error"];
    assert_halts_like_its_control((&control, &plan), &args, "the ticket's own record");
}

/// The expired record sits on another ticket on the same execution identity,
/// one missing its input; the stalled ticket exits `0` without its result.
#[test]
fn another_tickets_expired_record_does_not_release_a_stall() {
    let tasks = "### Task 1: Stalls on a missing output\n**State:** work\n\n\
                 ### Task 2: Needs a brief\n**State:** needs\n";
    let control = stall_plan("expired-other-control", "", tasks, 0);
    let plan = stall_plan("expired-other-record", &expired_record("2", "needs"), tasks, 0);
    assert_halts_like_its_control((&control, &plan), &["--no-tui"], "a record on another ticket");
}

/// The guard: a session limit parks a ticket with attempts to spare, the window
/// is moved to a few seconds away, and the same run sleeps to it and spawns the
/// ticket again, which then finishes. Its record was observed after its spawn
/// started, so neither the live window nor its expiry is read as spent.
// §FS-rhei-run.3.3 §FS-rhei-agents.5.2.1
#[test]
fn a_parked_ticket_sleeps_to_its_window_and_spawns_again() {
    let fixture = ProviderFixture::new(
        "expired-record-parked-guard",
        SINGLE_TASK,
        &SIMPLE_MACHINE.replace("attempts: 1", "attempts: 3"),
        &format!(
            r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root/'starts', env('RHEI_ATTEMPT') + '\n')
if not (root/'refused').exists():
    write(root/'refused', 'refused\n')
    print({LIMIT_SIGNAL:?}, file=sys.stderr, flush=True)
    raise SystemExit(1)
result('## Result\n\nResumed after the window.\n')
"#
        ),
    );
    let mut run = fixture.start(&[]);
    fixture.parked(&mut run);
    edit_metadata(&fixture.root, |m| {
        m["metadata"]["tasks"]["1"]["providerLimits"]["working"]["nextAttemptAt"] =
            utc_at(epoch_now() + 3).into();
    });
    fixture.finish_success(&mut run);
    let starts = fs::read_to_string(fixture.root.join("starts")).expect("starts");
    assert_eq!(starts.lines().count(), 2, "parked once, spawned again after the window");
}
