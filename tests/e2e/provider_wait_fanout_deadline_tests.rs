//! A fan-out ticket whose arms wait on two provider deadlines is due when the
//! first of its held arms frees, not the last (agent-grounds/rhei#729).
//!
//! The run folded one ticket's per-arm deadlines with the latest of them, so a
//! fan-out held on one identity until NEAR and on another until FAR reported
//! FAR: `--until-idle` named it as the next attempt, and the continuous run,
//! sequential or in the worker pool, announced a sleep to it, while the first
//! arm could already spawn at NEAR. The single-target control beside it shows
//! the fold across tickets was already the earliest. The last case guards the
//! other half of the rule: an arm already finished is not still to be spawned,
//! so its identity's earlier record does not make the ticket due.
// §FS-rhei-run.5.1 §FS-rhei-run.3.3

use super::provider_limit_support::*;
use super::provider_wait_quiet_support::*;
use super::until_idle_support::{assert_exit, assert_idle_line, bounded, stop_payload, EXIT_IDLE};
use super::*;

/// `fix` is the ticket's fan-out, one arm per identity: `codex/openai` and
/// `mock/mock`. `finishing` is the same fan-out owing one output per arm, so an
/// arm whose output is written is satisfied while its sibling is still to be
/// spawned. `solo` holds the `codex/openai` identity on one target alone, and
/// `other` the `mock/mock` one. §FS-rhei-run.3.3
const MACHINE: &str = r#"name: fan-out-deadlines
version: 1
states:
  fix:
    initial: true
    all_targets:
      - codex:openai:alpha
      - mock:mock:beta
    attempts: 1
  finishing:
    all_targets:
      - codex:openai:alpha
      - mock:mock:beta
    attempts: 1
    outputs:
      - name: arm
        path: runtime/fanout/{model.name}.txt
  solo:
    target: codex:openai:alpha
    attempts: 1
  other:
    target: mock:mock:beta
    attempts: 1
  review:
    gating: true
  completed:
    final: true
transitions:
  - from: fix
    to: completed
  - from: finishing
    to: review
  - from: review
    to: completed
  - from: solo
    to: completed
  - from: other
    to: completed
"#;

/// A stand-in that records every start, by arm, and refuses it: nothing in
/// this file may start, because every arm still to be spawned is held.
const LOGS_EVERY_START: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'starts', env('RHEI_MODEL_NAME', '?') + '\n')
raise SystemExit(1)
"#;

/// The most a slow runner may take between seeding the deadlines and the run
/// announcing its sleep. An hour separates NEAR from FAR, so this cannot blur
/// the two.
const SLACK_SECS: u64 = 600;

/// A provider-limit record holding `agent`/`provider` until `deadline`.
fn held(agent: &str, provider: &str, deadline: u64) -> serde_json::Value {
    let mut record = limit_record(deadline);
    record["identity"] = serde_json::json!({"agent": agent, "provider": provider});
    record
}

/// Two hours apart from now: NEAR frees `codex/openai`, FAR frees `mock/mock`.
struct Deadlines {
    near: u64,
    far: u64,
}

impl Deadlines {
    fn from_now() -> Self {
        let now = epoch_now();
        Self { near: now + 3600, far: now + 7200 }
    }
}

/// Task 1 in `task_one_state`, Task 2 in `other`, and any `extra` tickets,
/// with `metadata.tasks` set to `tasks`.
fn fixture(
    name: &str,
    task_one_state: &str,
    extra: &[(&str, &str)],
    tasks: serde_json::Value,
) -> ProviderFixture {
    let first = format!("### Task 1: Fix it\n**State:** {task_one_state}\n");
    let mut files =
        vec![("01.md", first.as_str()), ("02.md", "### Task 2: Other\n**State:** other\n")];
    files.extend_from_slice(extra);
    let fixture = ProviderFixture::new(name, &files, MACHINE, LOGS_EVERY_START);
    edit_metadata(&fixture.root, |m| m["metadata"] = serde_json::json!({ "tasks": tasks }));
    fixture
}

/// The issue's fixture: Task 1 fans out over both identities and carries the
/// `codex/openai` record until NEAR; Task 2, on `mock/mock`, carries that
/// identity's record until FAR, which also holds Task 1's second arm.
fn fan_out_held_to_two_deadlines(name: &str, at: &Deadlines) -> ProviderFixture {
    fixture(
        name,
        "fix",
        &[],
        serde_json::json!({
            "1": {"providerLimits": {"fix": held("codex", "openai", at.near)}},
            "2": {"providerLimits": {"other": held("mock", "mock", at.far)}},
        }),
    )
}

/// `rhei run --until-idle` over `fixture`, with `extra` arguments.
fn run_until_idle(fixture: &ProviderFixture, extra: &[&str]) -> CliRun {
    let mut command = rhei_command(fixture.root.join(".home"));
    command.arg("--state-machine").arg(&fixture.machine).arg("run").arg(&fixture.root);
    command.args(["--until-idle", "--no-dashboard"]).args(extra);
    bounded(command, "rhei run --until-idle", &fixture.dir)
}

/// The seconds of the continuous run's sleep line, in its text mode with
/// `extra` arguments. The run is stopped once the line is out.
fn announced_sleep(fixture: &ProviderFixture, name: &str, extra: &[&str]) -> (u64, String) {
    let mut run = start_text_run(fixture, name, extra);
    let stdout = fixture.dir.join(format!("{name}.stdout"));
    wait_for("the run to say it is sleeping", &mut run, || {
        stdout_from_file(&stdout).contains(SLEEP_LINE)
    });
    run.stop();
    let printed = stdout_from_file(&stdout);
    let seconds = printed
        .split_once("No ready tasks; sleeping ")
        .and_then(|(_, rest)| rest.split_once("s until"))
        .and_then(|(secs, _)| secs.parse().ok())
        .unwrap_or_else(|| panic!("the sleep line names no seconds; it printed:\n{printed}"));
    (seconds, printed)
}

/// Nothing started: every arm still to be spawned was held.
#[track_caller]
fn assert_nothing_started(fixture: &ProviderFixture) {
    let starts = fixture.root.join("starts");
    assert!(
        !starts.exists(),
        "no arm may start while it is held; started: {}",
        fs::read_to_string(&starts).unwrap_or_default()
    );
}

/// `--until-idle` reports NEAR, on the console and in the stream: the anthropic
/// arm of the issue, here `codex/openai`, can spawn then. §FS-rhei-run.5.1
#[test]
fn until_idle_reports_a_fan_out_due_when_its_first_held_arm_frees() {
    let at = Deadlines::from_now();
    let fixture = fan_out_held_to_two_deadlines("fan-out-deadline-idle", &at);

    let text = run_until_idle(&fixture, &[]);
    assert_exit(&text, EXIT_IDLE, "both tickets are waiting on a provider");
    assert!(
        !text.stdout.contains(&utc_at(at.far)),
        "the next attempt is the first arm free (NEAR {}), not the later arm (FAR {});\n{}",
        utc_at(at.near),
        utc_at(at.far),
        text.stdout
    );
    assert_idle_line(&text, "provider_limit", &utc_at(at.near));

    let json = run_until_idle(&fixture, &["--json"]);
    assert_exit(&json, EXIT_IDLE, "both tickets are waiting on a provider");
    assert_eq!(
        stop_payload(&json)["next_attempt_at"],
        serde_json::json!(utc_at(at.near)),
        "the stream's next attempt is the first arm free (NEAR), not FAR"
    );
    assert_nothing_started(&fixture);
}

/// The continuous run announces a sleep to NEAR, about an hour, not to FAR, in
/// the sequential loop and the `--parallel` worker pool alike. §FS-rhei-run.5.1
#[test]
fn the_continuous_run_sleeps_until_a_fan_outs_first_held_arm_frees() {
    for (name, extra) in [("sequential", &[][..]), ("pool", &["--parallel", "2"][..])] {
        let at = Deadlines::from_now();
        let fixture = fan_out_held_to_two_deadlines(&format!("fan-out-deadline-{name}"), &at);
        let (seconds, printed) = announced_sleep(&fixture, name, extra);
        assert!(
            seconds <= 3600 && seconds + SLACK_SECS > 3600,
            "{name}: the run sleeps until the first arm frees (about 3600s, NEAR), not the later \
             arm (about 7200s, FAR); it slept {seconds}s and printed:\n{printed}"
        );
        assert_nothing_started(&fixture);
    }
}

/// Control: Task 1 on the `codex/openai` target alone reports NEAR today. The
/// fold across tickets already takes the earliest; only the fold over one
/// ticket's arms did not. §FS-rhei-run.5.1
#[test]
fn a_single_target_ticket_beside_a_later_wait_reports_its_own_deadline() {
    let at = Deadlines::from_now();
    let tasks = serde_json::json!({
        "1": {"providerLimits": {"solo": held("codex", "openai", at.near)}},
        "2": {"providerLimits": {"other": held("mock", "mock", at.far)}},
    });
    let fixture = fixture("single-target-deadline", "solo", &[], tasks);

    let idle = run_until_idle(&fixture, &[]);
    assert_exit(&idle, EXIT_IDLE, "both tickets are waiting on a provider");
    assert_idle_line(&idle, "provider_limit", &utc_at(at.near));

    let (seconds, printed) = announced_sleep(&fixture, "single", &[]);
    assert!(
        seconds <= 3600 && seconds + SLACK_SECS > 3600,
        "the run sleeps until NEAR, about 3600s; it slept {seconds}s and printed:\n{printed}"
    );
    assert_nothing_started(&fixture);
}

/// Guard: Task 1's `codex/openai` arm has already written its output, so only
/// its `mock/mock` arm, held until FAR, is still to be spawned. A claimed Task 3
/// holds `codex/openai` until NEAR and, being claimed, makes nothing due then.
/// The ticket is due at FAR, so the run sleeps about two hours. This passes
/// today only because the latest of the arms is FAR here; it fails a fold that
/// takes the earliest over every arm, finished or not. §FS-rhei-run.5.1
#[test]
fn a_finished_arm_on_an_earlier_held_identity_does_not_make_its_fan_out_due() {
    let at = Deadlines::from_now();
    let tasks = serde_json::json!({
        "2": {"providerLimits": {"other": held("mock", "mock", at.far)}},
        "3": {"providerLimits": {"solo": held("codex", "openai", at.near)}},
    });
    let claimed = ("03.md", "### Task 3: Claimed\n**State:** solo\n**Assignee:** alice\n");
    let fixture = fixture("finished-arm-deadline", "finishing", &[claimed], tasks);
    let outputs = fixture.root.join("runtime/fanout");
    fs::create_dir_all(&outputs).expect("create the fan-out's output directory");
    fs::write(outputs.join("alpha.txt"), "done").expect("write the finished arm's output");

    let (seconds, printed) = announced_sleep(&fixture, "finished-arm", &[]);
    assert!(
        seconds + SLACK_SECS > 7200,
        "Task 1's only arm still to be spawned waits until FAR, about 7200s; its finished arm's \
         identity, held until NEAR, must not make it due; it slept {seconds}s and \
         printed:\n{printed}"
    );
    assert_nothing_started(&fixture);
}
