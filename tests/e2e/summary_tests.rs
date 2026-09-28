//! `rhei summary`: the compact, publishable Markdown account of a run.
//! §FS-rhei-summary

use std::fs;

use super::*;

/// Two terminal states with tasks in them and one task still moving, so the
/// tally has to name both terminals and the in-progress remainder.
const SUMMARY_PLAN: &str = r#"# Rhei: Ticket Fix
**States:** integration-test

## Tasks

### Task 1: Supervise the fix
**State:** completed

### Task 2: Implement the fix
**State:** completed

### Task 3: Review the fix
**State:** cancelled

### Task 4: Ship it
**State:** pending
"#;

const MEASURED_TOKENS: &str = r#"{
  "total": { "value": 45000, "source": "agent-usage-capture" },
  "input": {
    "total": { "value": 41200, "source": "agent-usage-capture" },
    "cached_read": { "value": 30000, "source": "agent-usage-capture" },
    "cache_write": { "status": "unsupported" }
  },
  "output": {
    "total": { "value": 3800, "source": "agent-usage-capture" },
    "cached_read": { "status": "unsupported" },
    "cache_write": { "status": "unsupported" }
  }
}"#;

const UNMEASURED_TOKENS: &str = r#"{
  "total": { "status": "unknown" },
  "input": {
    "total": { "status": "unknown" },
    "cached_read": { "status": "unsupported" },
    "cache_write": { "status": "unsupported" }
  },
  "output": {
    "total": { "status": "unknown" },
    "cached_read": { "status": "unsupported" },
    "cache_write": { "status": "unsupported" }
  }
}"#;

struct Invocation<'a> {
    id: &'a str,
    task_id: &'a str,
    state: &'a str,
    visit: u64,
    model: &'a str,
    started_at: &'a str,
    ended_at: &'a str,
    measured: bool,
}

/// Write one `rhei.accounting.invocation.v1` record, the durable material the
/// summary is rendered from. §FS-rhei-summary.1
fn write_invocation(root: &Path, invocation: &Invocation<'_>) {
    let dir = root.join("runtime/accounting/invocations");
    fs::create_dir_all(&dir).expect("accounting directory should be created");
    let (extraction_status, tokens) = if invocation.measured {
        ("measured", MEASURED_TOKENS)
    } else {
        ("no-usage-emitted", UNMEASURED_TOKENS)
    };
    let record = format!(
        r#"{{
  "schema": "rhei.accounting.invocation.v1",
  "invocation_id": "{id}",
  "task_id": "{task_id}",
  "state": "{state}",
  "visit": {visit},
  "agent": "claude-code",
  "provider": "anthropic",
  "model": "{model}",
  "started_at": "{started_at}",
  "ended_at": "{ended_at}",
  "extraction_status": "{extraction_status}",
  "scope": "aggregate-agent-process",
  "tokens": {tokens},
  "pricing": {{ "status": "unpriced", "currency": "USD" }}
}}"#,
        id = invocation.id,
        task_id = invocation.task_id,
        state = invocation.state,
        visit = invocation.visit,
        model = invocation.model,
        started_at = invocation.started_at,
        ended_at = invocation.ended_at,
    );
    fs::write(dir.join(format!("{}.json", invocation.id)), record)
        .expect("invocation record should be written");
}

/// The supervisor is visited twice and the implementer once, written to disk
/// out of order so only `started_at` can produce the printed order.
fn write_three_invocations(root: &Path) {
    write_invocation(
        root,
        &Invocation {
            id: "zzz-implement",
            task_id: "plan.2",
            state: "pending",
            visit: 1,
            model: "claude-sonnet-5",
            started_at: "2026-05-20T10:03:00Z",
            ended_at: "2026-05-20T10:21:04Z",
            measured: true,
        },
    );
    write_invocation(
        root,
        &Invocation {
            id: "mmm-supervise-2",
            task_id: "plan.1",
            state: "draft",
            visit: 2,
            model: "claude-fable-5",
            started_at: "2026-05-20T10:22:00Z",
            ended_at: "2026-05-20T10:22:45Z",
            measured: true,
        },
    );
    write_invocation(
        root,
        &Invocation {
            id: "aaa-supervise-1",
            task_id: "plan.1",
            state: "draft",
            visit: 1,
            model: "claude-fable-5",
            started_at: "2026-05-20T10:00:00Z",
            ended_at: "2026-05-20T10:02:32Z",
            measured: false,
        },
    );
}

#[test]
fn summary_leads_with_the_workflow_the_invocations_and_the_task_tally() {
    // §FS-rhei-summary.2.1: the lead line names the machine, the invocation
    // count, the distinct models, and the tally with the in-progress remainder.
    let (dir, plan_path, machine_path) = setup_single_file("summary-lead", SUMMARY_PLAN);
    write_three_invocations(&dir);

    let result = run_cli("summary", &plan_path, &machine_path, &[]);
    assert_success(&result);

    let lead = result.stdout.lines().next().expect("a lead line");
    assert_eq!(
        lead,
        "`integration-test` workflow: 3 agent invocations across 2 models; \
         2 tasks completed, 1 cancelled, 1 in progress.",
        "got:\n{}",
        result.stdout
    );
}

#[test]
fn summary_numbers_the_steps_by_started_at_with_visits_and_measured_tokens() {
    // §FS-rhei-summary.2.2: one numbered entry per record in `started_at`
    // order; `(visit N)` only where a task has more than one record, and token
    // counts only where the record measured them.
    let (dir, plan_path, machine_path) = setup_single_file("summary-steps", SUMMARY_PLAN);
    write_three_invocations(&dir);

    let result = run_cli("summary", &plan_path, &machine_path, &[]);
    assert_success(&result);

    let steps: Vec<&str> =
        result.stdout.lines().filter(|line| line.starts_with(char::is_numeric)).collect();
    // These records carry no `token_convention`, and their `agent` is
    // `claude-code`, so their 30,000 cached reads are part of the 41,200 input
    // tokens they name rather than a figure beside it.

    // §FS-rhei-cost-accounting.5.2
    assert_eq!(
        steps,
        vec![
            "1. `plan.1` draft (visit 1) — claude-code, anthropic/claude-fable-5 — 2m32s",
            "2. `plan.2` pending — claude-code, anthropic/claude-sonnet-5 — 18m04s \
             — 71.2k in / 3.8k out",
            "3. `plan.1` draft (visit 2) — claude-code, anthropic/claude-fable-5 — 45.0s \
             — 71.2k in / 3.8k out",
        ],
        "got:\n{}",
        result.stdout
    );
}

#[test]
fn summary_prints_the_aggregate_accounting_table() {
    // §FS-rhei-summary.2.3: the aggregate strip in the per-run report's shape,
    // with no cost row because nothing in this run was priced.
    let (dir, plan_path, machine_path) = setup_single_file("summary-accounting", SUMMARY_PLAN);
    write_three_invocations(&dir);

    let result = run_cli("summary", &plan_path, &machine_path, &[]);
    assert_success(&result);

    for expected in ["| Accounting | Value |", "| coverage | Partial |"] {
        assert!(result.stdout.contains(expected), "expected {expected:?}; got:\n{}", result.stdout);
    }
    let expected_rows = [
        "| total tokens | 150.0k |",
        "| input tokens (incl. cache) | 142.4k |",
        "| input cache read | 60.0k |",
        "| input cache write | - |",
        "| output tokens (incl. cache) | 7.6k |",
        "| output cache read | - |",
        "| output cache write | - |",
    ]
    .join("\n");
    assert!(
        result.stdout.contains(&expected_rows),
        "expected ordered accounting rows:\n{expected_rows}\ngot:\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains("| cost |"),
        "unpriced run shows no cost; got:\n{}",
        result.stdout
    );
}

#[test]
fn summary_carries_no_local_path() {
    // §FS-rhei-summary.4: the output must be publishable verbatim, so no
    // workspace directory, log file, or home-relative path may appear in it.
    let (dir, plan_path, machine_path) = setup_single_file("summary-paths", SUMMARY_PLAN);
    write_three_invocations(&dir);

    let result = run_cli("summary", &plan_path, &machine_path, &[]);
    assert_success(&result);

    let root = dir.to_string_lossy().into_owned();
    assert!(!result.stdout.contains(&root), "workspace path leaked; got:\n{}", result.stdout);
    assert!(!result.stdout.contains("runtime/"), "runtime path leaked; got:\n{}", result.stdout);
    assert!(!result.stdout.contains(".rhei.md"), "plan path leaked; got:\n{}", result.stdout);
}

#[test]
fn summary_details_wraps_the_whole_output_in_one_collapsed_block() {
    // §FS-rhei-summary.3: the lead line becomes the `<summary>`, prefixed
    // `AI workflow: `, with a blank line after it so GitHub renders the rest.
    let (dir, plan_path, machine_path) = setup_single_file("summary-details", SUMMARY_PLAN);
    write_three_invocations(&dir);

    let result = run_cli("summary", &plan_path, &machine_path, &["--details"]);
    assert_success(&result);

    let lines: Vec<&str> = result.stdout.lines().collect();
    assert_eq!(lines[0], "<details>", "got:\n{}", result.stdout);
    assert!(
        lines[1].starts_with("<summary>AI workflow: `integration-test`, 3 agent invocations"),
        "got:\n{}",
        result.stdout
    );
    assert!(lines[1].ends_with("</summary>"), "got:\n{}", result.stdout);
    assert_eq!(lines[2], "", "blank line after the summary tag; got:\n{}", result.stdout);
    assert_eq!(lines[lines.len() - 1], "</details>", "got:\n{}", result.stdout);
    assert!(
        result.stdout.contains("1. `plan.1` draft (visit 1)"),
        "steps stay inside the block; got:\n{}",
        result.stdout
    );
}

#[test]
fn summary_replaces_the_table_when_no_record_measured_a_total() {
    // §FS-rhei-summary.2.3: an unmeasured run says so in one line rather than
    // rendering a table of dashes that reads like a zero.
    let (dir, plan_path, machine_path) = setup_single_file("summary-unmeasured", SUMMARY_PLAN);
    write_invocation(
        &dir,
        &Invocation {
            id: "only",
            task_id: "plan.1",
            state: "draft",
            visit: 1,
            model: "claude-fable-5",
            started_at: "2026-05-20T10:00:00Z",
            ended_at: "2026-05-20T10:02:32Z",
            measured: false,
        },
    );

    let result = run_cli("summary", &plan_path, &machine_path, &[]);
    assert_success(&result);

    assert!(
        result.stdout.contains("Token accounting was not measured for this run."),
        "got:\n{}",
        result.stdout
    );
    assert!(!result.stdout.contains("| Accounting |"), "got:\n{}", result.stdout);
    assert!(
        result.stdout.contains("1. `plan.1` draft — claude-code, anthropic/claude-fable-5 — 2m32s"),
        "the step still prints, without a token clause; got:\n{}",
        result.stdout
    );
}

#[test]
fn summary_of_a_workspace_with_no_accounting_still_prints_the_lead_line() {
    // §FS-rhei-summary.5: a freshly instantiated workspace is summarizable —
    // zero invocations, the tally from the plan, and exit 0.
    let (_dir, plan_path, machine_path) = setup_single_file("summary-empty", SUMMARY_PLAN);

    let result = run_cli("summary", &plan_path, &machine_path, &[]);
    assert_success(&result);

    assert_eq!(
        result.stdout,
        "`integration-test` workflow: 0 agent invocations across 0 models; \
         2 tasks completed, 1 cancelled, 1 in progress.\n\n\
         Token accounting was not measured for this run.\n",
        "got:\n{}",
        result.stdout
    );
}

#[test]
fn summary_uses_a_members_machine_when_the_project_has_no_default() {
    // §FS-rhei-summary.2.1 §DA-per-rhei-state-machines: a project-wide
    // summary must classify a member's tasks under that member's machine.
    let dir = unique_temp_dir("summary-member-machine");
    fs::write(dir.join("index.panta.md"), "# Panta: Member machine\n")
        .expect("panta manifest should be written");
    let member = dir.join("flow");
    fs::create_dir_all(member.join("tasks")).expect("member tasks directory should be created");
    fs::write(member.join("index.rhei.md"), "# Rhei: Flow\n**States:** grounded-ticket\n")
        .expect("member index should be written");
    fs::write(
        member.join("states.yaml"),
        r#"name: grounded-ticket
version: 1
states:
  pending:
    description: Task not yet started
    initial: true
  completed:
    description: Task finished successfully
    final: true
  cancelled:
    description: Task intentionally cancelled
    final: true
transitions:
  - from: pending
    to: completed
  - from: pending
    to: cancelled
"#,
    )
    .expect("member machine should be written");
    for (name, task) in [
        ("completed.md", "### Task 1: Completed\n**State:** completed\n"),
        ("cancelled.md", "### Task 2: Cancelled\n**State:** cancelled\n"),
        ("pending.md", "### Task 3: Pending\n**State:** pending\n"),
    ] {
        fs::write(member.join("tasks").join(name), task).expect("task should be written");
    }

    let output = rhei_command(dir.join(".home"))
        .arg("summary")
        .arg(&dir)
        .output()
        .expect("summary should run");
    let result = CliRun::from(&output);
    assert_success(&result);
    assert_eq!(
        result.stdout.lines().next(),
        Some("`grounded-ticket` workflow: 0 agent invocations across 0 models; 1 task completed, 1 cancelled, 1 in progress."),
        "got:\n{}",
        result.stdout
    );
}
