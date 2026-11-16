//! An expired provider-limit record on a ticket a `--strict-inputs` run refused
//! (agent-grounds/rhei#728).
//!
//! The run takes no refused ticket again, so like a missing input the refusal
//! is a blocker no clock releases: the record the ticket still carries, whose
//! wait ended long ago, must not become a deadline of "now" that keeps the
//! continuous run sleeping 1 s and rescanning a ticket it will never start.
// §FS-rhei-run.5.1.1 §FS-rhei-seal.8.4

use super::until_idle_support::*;
use super::*;
use std::time::Duration;

/// Far above how long the halt takes; the regression is a run that never ends.
const HALT_PATIENCE: Duration = Duration::from_secs(20);

/// One agent state with no inputs, so only the seal can refuse it.
const MACHINE: &str = r#"name: expired-wait-strict-refusal
version: 1
states:
  needs:
    initial: true
    description: Agent work a changed machine refuses
    target: codex:openai:alpha
    agent_timeout: 30m
  done:
    final: true
    description: Done
transitions:
  - from: needs
    to: done
"#;

fn rhei_within(plan: &IdlePlan, args: &[&str]) -> CliRun {
    let mut command = rhei_command(&plan.home);
    command.arg("--state-machine").arg(&plan.machine).args(args);
    bounded_within(command, &format!("rhei {}", args.join(" ")), plan.root(), HALT_PATIENCE)
}

/// The ticket is refused at its first spawn because its machine changed after
/// the seal, and it carries a record whose wait ended in 2000. The run halts
/// with the refusal instead of sleeping for the record. §FS-rhei-run.5.1.1
#[test]
fn an_expired_provider_record_does_not_keep_a_strictly_refused_ticket_waiting() {
    let plan = IdlePlan::new(
        "expired-wait-strict-refusal",
        &format!(
            "---\nmetadata:\n  tasks:\n    1:\n      {}\n---\n\n## Tasks\n\n\
             ### Task 1: Refused by its seal\n**State:** needs\n",
            provider_limit("needs", ELAPSED_POLL)
        ),
    );
    fs::write(&plan.machine, MACHINE).expect("write the machine");
    let plan_arg = plan.plan.display().to_string();
    assert_success(&rhei_within(&plan, &["seal", &plan_arg]));
    fs::write(&plan.machine, format!("{MACHINE}# changed after the seal\n"))
        .expect("change the machine");

    let result = rhei_within(&plan, &["run", &plan_arg, "--no-tui", "--strict-inputs"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert_exit(&result, 1, "a refusal is work for a person");
    assert!(combined.contains("was not started"), "the seal refuses the ticket; got:\n{combined}");
    assert!(
        !combined.contains("No ready tasks; sleeping"),
        "no deadline can release a refused ticket, so the run must not sleep; got:\n{combined}"
    );
    assert!(plan.records().is_empty(), "nothing is spawned for a refused ticket");
}
