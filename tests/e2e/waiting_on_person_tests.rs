// A poll that waits on a person, read back from every surface an operator or
// an external scheduler looks at. The ticket this pins reported one machine
// through `rhei states --json` and `rhei list`, so the transcript here is that
// one: a self-resuming approval wait beside a CI watch, and the reading each
// surface gives them.

// §AR-source-file-size.3 §FS-rhei-states.2.5

use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

/// Two polls with the same shape and different waits: one on the author, one
/// on CI. Everything below turns on telling them apart.
// §FS-rhei-states.2.5
const APPROVAL_MACHINE: &str = r#"name: approvals
version: 1
states:
  plan-approval:
    description: Wait for the author to answer on the issue.
    program: "/bin/true"
    initial: true
    poll:
      interval: 10m
      max_attempts: 60
      waiting_on: author
  ci-watch:
    description: Watch CI for the branch.
    program: "/bin/true"
    poll:
      interval: 2m
      max_attempts: 30
  completed:
    description: Done
    final: true
transitions:
  - from: plan-approval
    to: plan-approval
  - from: plan-approval
    to: ci-watch
  - from: ci-watch
    to: ci-watch
  - from: ci-watch
    to: completed
"#;

const APPROVAL_PLAN: &str = r#"# Rhei: Approvals

## Tasks

### Task 1: Get the plan approved
**State:** plan-approval

### Task 2: Watch CI
**State:** ci-watch
"#;

fn approval_fixture(prefix: &str) -> (TestDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", APPROVAL_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", APPROVAL_MACHINE);
    (dir, plan_path, machine_path)
}

fn states_json(machine_path: &std::path::Path) -> serde_json::Value {
    let mut cmd = rhei_command(isolated_home_for(machine_path));
    cmd.arg("states").arg("--state-machine").arg(machine_path).arg("--json");
    let output = cmd.output().expect("rhei states should run");
    assert!(output.status.success(), "rhei states failed: {output:?}");
    serde_json::from_slice(&output.stdout).expect("parse states JSON")
}

/// The ticket's own transcript: a poll reported only as a cadence could not say
/// it was waiting on a person, so the label now rides beside `interval` and
/// `max_attempts` — and only where it was authored. §FS-rhei-states-cmd.5
#[test]
fn states_json_reports_the_person_a_poll_waits_on() {
    let (_dir, _plan_path, machine_path) = approval_fixture("waiting-on-states-json");
    let json = states_json(&machine_path);
    let state = |name: &str| {
        json["states"]
            .as_array()
            .expect("states array")
            .iter()
            .find(|s| s["name"] == name)
            .expect("state present")
            .clone()
    };

    assert_eq!(state("plan-approval")["poll"]["waiting_on"], "author");
    assert_eq!(
        state("ci-watch")["poll"],
        serde_json::json!({ "interval": "2m", "max_attempts": 30 }),
        "a machine-backoff poll keeps exactly the JSON it had"
    );
}

/// The listing an operator checks first names whose turn it is, so an approval
/// wait no longer reads like a build being watched. §FS-rhei-list.4.1
#[test]
fn list_marks_a_ticket_waiting_on_a_person() {
    let (_dir, plan_path, machine_path) = approval_fixture("waiting-on-list-text");
    let listed = run_cli("list", &plan_path, &machine_path, &[]);
    assert_success(&listed);

    assert!(
        listed
            .stdout
            .contains("Task plan.1: Get the plan approved [plan-approval] (waiting on author)"),
        "person-waiting row missing in:\n{}",
        listed.stdout
    );
    assert!(
        listed.stdout.contains("Task plan.2: Watch CI [ci-watch]\n"),
        "the CI watch row changed in:\n{}",
        listed.stdout
    );
}

/// The same answer in the machine-readable form a scheduler reads, additive:
/// present only on the ticket it means something for. §FS-rhei-list.4.2
#[test]
fn list_json_carries_waiting_on_only_for_the_person_wait() {
    let (_dir, plan_path, machine_path) = approval_fixture("waiting-on-list-json");
    let listed = run_cli("list", &plan_path, &machine_path, &["--json"]);
    assert_success(&listed);

    let tasks: Vec<serde_json::Value> =
        serde_json::from_str(&listed.stdout).expect("parse list JSON");
    let task = |id: &str| {
        tasks.iter().find(|t| t["id"] == id).unwrap_or_else(|| panic!("{id} listed")).clone()
    };

    assert_eq!(task("plan.1")["waiting_on"], "author");
    assert_eq!(
        task("plan.2").get("waiting_on"),
        None,
        "a ticket that is not person-waiting carries no such field"
    );
}

/// The reason the ticket was filed: a run whose only unfinished work is an
/// approval poll must read as parked, not as work in flight. The deadline is
/// put a day out so the scan refuses the ticket the way it does between
/// attempts, and the prediction is asked for rather than the sleep.
// §FS-rhei-run-report.3.1 §FS-rhei-run.4
#[test]
fn a_run_parked_on_a_person_names_the_wait_and_asks_for_nothing() {
    let deadline = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_secs()
        + 86_400;
    let plan = format!(
        r#"# Rhei: Approvals

---
metadata:
  tasks:
    1:
      pollNextAttemptAt:
        plan-approval: {deadline}
---

## Tasks

### Task 1: Get the plan approved
**State:** plan-approval
"#
    );

    let dir = unique_temp_dir("waiting-on-run-parked");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", &plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", APPROVAL_MACHINE);

    let predicted = run_cli("run", &plan_path, &machine_path, &["--dry-run"]);
    assert_success(&predicted);
    assert!(
        predicted.stdout.contains(
            "Task plan.1 (plan-approval): waiting on author \u{2014} nothing to do on \
             Task plan.1; the poll resumes itself when author answers"
        ),
        "the wait is not named in:\n{}",
        predicted.stdout
    );
}

/// One run must not give one ticket two reasons. The `--rhei` no-work line
/// reads the claim first, the way the halt line above it does: a claimed
/// ticket is already named there with `rhei release` as its remedy, so naming
/// its poll's person again promised an answer that could not move it.
// §FS-rhei-run.5.1 §FS-rhei-panta.6.1
#[test]
fn the_scoped_no_work_line_reads_the_claim_before_the_person() {
    let plan = r#"# Rhei: Claims

## Tasks

### Task 1: Watch CI
**State:** ci-watch
**Assignee:** builder

### Task 2: Get the plan approved
**State:** plan-approval
**Prior:** 1

### Task 3: Get the other plan approved
**State:** plan-approval
**Assignee:** bot
"#;
    let dir = unique_temp_dir("waiting-on-scoped-no-work");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", APPROVAL_MACHINE);

    let result = run_cli(
        "run",
        &plan_path,
        &machine_path,
        &["--rhei", "plan", "--no-callbacks", "--no-tui"],
    );

    assert!(
        result.stdout.contains("Task plan.3 (plan-approval): claimed by bot"),
        "the halt block stopped naming the claim in:\n{}",
        result.stdout
    );
    let no_work = result
        .stdout
        .lines()
        .find(|line| line.starts_with("No tasks could be advanced"))
        .unwrap_or_else(|| panic!("no no-advancement line in:\n{}", result.stdout));
    assert_eq!(
        no_work,
        "No tasks could be advanced in the --rhei scope (plan): \
         Task plan.2 waiting on Task plan.1 (ci-watch).",
        "a claimed ticket contributes no reason of its own, and the prior still names one"
    );
}

/// The same rule for a blocker no clock releases: a person-waiting poll missing
/// its required input is reported for the input in the halt block, so the
/// `--rhei` no-work line names no person for it either. Naming the author there
/// gave the one ticket two reasons, and the author's answer cannot write the file.
// §FS-rhei-run.5.1.1 §FS-rhei-run-report.3.1 §FS-rhei-panta.6.1
#[test]
fn the_scoped_no_work_line_names_no_person_for_a_poll_missing_its_input() {
    let machine = APPROVAL_MACHINE.replacen(
        "    initial: true\n",
        "    initial: true\n    inputs:\n      - name: brief\n        path: runtime/brief.md\n",
        1,
    );
    let plan = "# Rhei: Brief\n\n## Tasks\n\n### Task 1: Get the plan approved\n**State:** plan-approval\n";
    let dir = unique_temp_dir("waiting-on-scoped-missing-input");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let result = run_cli(
        "run",
        &plan_path,
        &machine_path,
        &["--rhei", "plan", "--no-callbacks", "--no-tui"],
    );

    assert!(
        result.stdout.contains("Task plan.1 (plan-approval): required input(s) not found: brief"),
        "the halt block names the input in:\n{}",
        result.stdout
    );
    let no_work = result
        .stdout
        .lines()
        .find(|line| line.starts_with("No tasks could be advanced"))
        .unwrap_or_else(|| panic!("no no-advancement line in:\n{}", result.stdout));
    assert_eq!(
        no_work, "No tasks could be advanced in the --rhei scope (plan).",
        "the input, not the author, holds the ticket"
    );
}
