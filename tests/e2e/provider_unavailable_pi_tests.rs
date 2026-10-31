//! A `pi` session whose provider stops answering waits instead of spending the
//! visit's attempts (agent-grounds/rhei#513).
//!
//! Without the recognition the session's exit `0` with its outputs missing is
//! an ordinary attempt, charged, and the second one halts the visit.
// §FS-rhei-agents.2.5 §FS-rhei-agents.3.2.3 §FS-rhei-run.3.3

use super::provider_limit_support::*;
use super::provider_unavailable_pi_support::*;
use super::*;
use serde_json::json;

/// The incident: the transport fails through every retry pi allows, pi exits
/// `0` and the outputs are missing. The visit keeps its attempts and waits a
/// minute, a run before the deadline starts nothing, and the run after it
/// finishes the ticket once the provider answers again.
// §FS-rhei-agents.2.5 §FS-rhei-agents.3.2.1 §FS-rhei-agents.8.4 §FS-rhei-run.3.3
// §FS-rhei-run-json.2.1 §FS-rhei-run-tui.1.7
#[test]
fn a_pi_session_that_loses_its_provider_waits_without_spending_an_attempt() {
    let fixture = Outage::pi(gave_up()).fixture("pi-unavailable-waits");
    let first = run_once(&fixture, "first");
    let argv = fs::read_to_string(fixture.root.join("runtime/argv.txt")).unwrap();
    assert!(argv.contains("--mode json"), "rhei runs pi in json mode: {argv}");

    let record = only_record(&fixture, &first);
    assert_eq!(record["ending"], "provider_unavailable", "{record:#}; {}", first.said());
    assert_eq!(record["attempt_charged"], false, "{record:#}");
    assert_eq!(record["charged"], 0, "{record:#}");
    assert_eq!(record["unavailable"], 1, "{record:#}");
    assert_eq!(record["code"], 0, "{record:#}");
    assert_eq!(record["signal"], SIGNAL, "{record:#}");
    // The footer is written before the output check decides. §FS-rhei-agents.8.2
    let log = fs::read_to_string(record["log"].as_str().unwrap()).unwrap();
    let footer = log.rsplit("=== exit ===").next().unwrap();
    assert!(footer.contains("code: 0") && !footer.contains("provider"), "{footer}");

    let wait = wait(&fixture);
    assert_eq!(wait["kind"], "unavailable", "{wait:#}; {}", first.said());
    assert_eq!(wait["identity"], json!({"agent": "pi-local", "provider": "openai-codex"}));
    assert_eq!(wait["signal"], SIGNAL, "{wait:#}");
    assert_backoff(&wait, 1);
    let deadline = wait["nextAttemptAt"].as_str().unwrap().to_string();

    let warning = unavailable_warning("no attempt charged (free 1 of 6 on this visit)");
    first.assert_says(&warning);
    let waiting = format!("waiting: pi-local on openai-codex starts again after {deadline}");
    first.assert_says(&waiting);
    first.assert_never_says("required outputs are missing");
    first.assert_never_says("attempts spent on this visit");
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");

    let released = first.event("slot_released");
    assert_eq!(released["outcome"], "provider_unavailable", "{released:#}");
    assert_eq!(released["provider"], "openai-codex", "{released:#}");
    assert_eq!(released["next_attempt_at"], deadline.as_str(), "{released:#}");
    assert!(!first.events.iter().any(|event| event["event"] == "task_outputs_missing"));
    let journal = fs::read_to_string(fixture.root.join("runtime/transitions.log")).unwrap();
    assert!(journal.contains("outcome=provider_unavailable"), "{journal}");
    assert_eq!(first.code, Some(3), "until-idle reports the wait; {}", first.said());
    let stop = first.stop();
    assert_eq!(stop["reason"], "idle", "{stop:#}");
    assert_eq!(stop["idle_blocker"], "provider_unavailable", "{stop:#}");
    assert_eq!(stop["next_attempt_at"], deadline.as_str(), "{stop:#}");

    let early = run_once(&fixture, "before-the-deadline");
    assert_eq!(early.code, Some(3), "{}", early.said());
    assert_eq!(starts(&fixture), 1, "nothing starts before the deadline; {}", early.said());
    assert_eq!(fixture.records(), vec![record]);

    expire_provider_deadlines(&fixture.root);
    fs::write(fixture.root.join(PROVIDER_BACK), "").unwrap();
    let back = run_once(&fixture, "provider-back");
    let respawn = format!(
        "Re-spawning Task {TASK} in state 'working': attempt 1 of 2; \
         the previous attempt found its provider unavailable ({SIGNAL})"
    );
    back.assert_says(&respawn);
    assert_eq!(back.code, Some(0), "{}", back.said());
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "completed");
    assert!(!markdown_text(&fixture.root).contains("providerLimits:"), "{}", back.said());
}

/// Two endings in a row are one count for the visit: the second is free too,
/// is counted `2`, and waits twice as long.
// §FS-rhei-agents.3.2.3 §FS-rhei-agents.8.4 §FS-rhei-run.3.3
#[test]
fn the_count_of_unavailable_endings_carries_across_spawns_and_doubles_the_wait() {
    let fixture = Outage::pi(gave_up()).fixture("pi-unavailable-count");
    run_once(&fixture, "first");
    expire_provider_deadlines(&fixture.root);
    let second = run_once(&fixture, "second");

    let respawn = format!(
        "Re-spawning Task {TASK} in state 'working': attempt 1 of 2; \
         the previous attempt found its provider unavailable ({SIGNAL})"
    );
    second.assert_says(&respawn);
    let warning = unavailable_warning("no attempt charged (free 2 of 6 on this visit)");
    second.assert_says(&warning);

    let record = only_record(&fixture, &second);
    assert_eq!(record["ending"], "provider_unavailable", "{record:#}");
    assert_eq!(record["attempt_charged"], false, "{record:#}");
    assert_eq!(record["charged"], 0, "{record:#}");
    assert_eq!(record["unavailable"], 2, "{record:#}");
    assert_eq!(record["attempt"], 2, "{record:#}");
    assert_backoff(&wait(&fixture), 2);
    assert_eq!(second.code, Some(3), "{}", second.said());
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
}

/// The seventh ending of a visit is charged, and on a visit with one attempt
/// left it is the last: the exhaustion halt names `rhei retry` and no wait is
/// written, because no spawn follows it.
// §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3 §FS-rhei-run-json.2.1
#[test]
fn the_seventh_unavailable_ending_is_charged_and_halts_a_spent_visit() {
    let fixture = Outage::pi(gave_up()).fixture("pi-unavailable-cap");
    seed_sixth_ending(&fixture);
    let seventh = run_once(&fixture, "seventh");

    let respawn = format!(
        "Re-spawning Task {TASK} in state 'working': attempt 2 of 2; \
         the previous attempt found its provider unavailable ({SIGNAL})"
    );
    seventh.assert_says(&respawn);
    let warning =
        unavailable_warning("attempt charged, because this visit's 6 free endings are spent");
    seventh.assert_says(&warning);
    let halt = format!("halting Task {TASK} in state 'working': 2 attempts spent on this visit");
    seventh.assert_says(&halt);
    seventh.assert_says(&format!("run: rhei retry {TASK}"));
    seventh.assert_never_says("waiting: pi-local");

    let record = only_record(&fixture, &seventh);
    assert_eq!(record["ending"], "provider_unavailable", "{record:#}");
    assert_eq!(record["attempt_charged"], true, "{record:#}");
    assert_eq!(record["charged"], 2, "{record:#}");
    assert_eq!(record["unavailable"], 7, "{record:#}");
    assert!(!markdown_text(&fixture.root).contains("providerLimits:"), "{}", seventh.said());
    let released = seventh.event("slot_released");
    assert_eq!(released["outcome"], "provider_unavailable", "{released:#}");
    assert!(released.get("next_attempt_at").is_none(), "{released:#}");
    assert!(released.get("provider").is_none(), "{released:#}");
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");

    // A spent visit names what it left unwritten, as any exhausted visit does.
    // §FS-rhei-run-json.2.1 §FS-rhei-run-report.3.1 §FS-rhei-run-report.4
    let finding = format!("finding (runtime/exports/{TASK}/finding.md)");
    let missing = seventh.event("task_outputs_missing");
    assert_eq!(missing["task"], TASK, "{missing:#}");
    assert_eq!(missing["state"], "working", "{missing:#}");
    assert_eq!(missing["entries"][0], finding.as_str(), "{missing:#}");
    let at = |name: &str| seventh.events.iter().position(|event| event["event"] == name);
    assert!(at("task_outputs_missing") < at("slot_released"), "{}", seventh.said());
    let report = fs::read_to_string(fixture.root.join("runtime/run-report.md")).unwrap();
    let row = report_row(&report, "Attention", TASK);
    assert!(
        row.starts_with(&format!("| {TASK} | working | worker exited 0 without {finding}")),
        "{report}"
    );
    assert!(row.contains("| write the file(s) above and rerun, or record"), "{report}");
    assert!(!report.contains("stalled in non-terminal state"), "{report}");
    let ledger = report_row(&report, "Transition Ledger", TASK);
    assert!(ledger.ends_with("| provider unavailable |"), "{report}");
}

/// Through the worker pool the incident reads as on the single worker: each
/// ticket keeps its attempts, says so, and waits its minute.
// §FS-rhei-agents.5.2.2 §FS-rhei-agents.3.2.1 §FS-rhei-run.3.3 §FS-rhei-run-json.2.1
#[test]
fn the_worker_pool_lets_a_pi_session_that_loses_its_provider_wait() {
    let outage = Outage { tasks: TWO_TASKS, concurrent: true, ..Outage::pi(gave_up()) };
    let fixture = outage.fixture("pi-unavailable-pool");
    let first = run_pool(&fixture, "first");

    // The pool fills both slots before it judges either exit. §FS-rhei-agents.5.2.2
    let at = |name: &str| first.events.iter().position(|event| event["event"] == name);
    let assigned = first.events.iter().filter(|event| event["event"] == "slot_assigned").count();
    assert_eq!(assigned, 2, "{}", first.said());
    let last_assigned = first.events.iter().rposition(|event| event["event"] == "slot_assigned");
    assert!(last_assigned < at("slot_released"), "both run at once; {}", first.said());

    let warning = unavailable_warning("no attempt charged (free 1 of 6 on this visit)");
    for (local, task) in [("1", TASK), ("2", "workspace.2")] {
        first.assert_says(&warning.replace(TASK, task));
        let record = fixture.records().into_iter().find(|record| record["task"] == task).unwrap();
        assert_eq!(record["ending"], "provider_unavailable", "{record:#}; {}", first.said());
        assert_eq!(record["attempt_charged"], false, "{record:#}");
        assert_eq!(record["charged"], 0, "{record:#}");
        assert_eq!(record["unavailable"], 1, "{record:#}");
        let wait = task_wait(&fixture, local);
        assert_eq!(wait["kind"], "unavailable", "{wait:#}; {}", first.said());
        let deadline = wait["nextAttemptAt"].as_str().unwrap();
        first.assert_says(&format!(
            "waiting: pi-local on openai-codex starts again after {deadline}"
        ));
        let released = first
            .events
            .iter()
            .find(|event| event["event"] == "slot_released" && event["task"] == task);
        let released = released.unwrap_or_else(|| panic!("{task} released; {}", first.said()));
        assert_eq!(released["outcome"], "provider_unavailable", "{released:#}");
        assert_eq!(released["provider"], "openai-codex", "{released:#}");
        assert_eq!(released["next_attempt_at"], deadline, "{released:#}");
    }
    first.assert_never_says("required outputs are missing");
    assert!(!first.events.iter().any(|event| event["event"] == "task_outputs_missing"));
    assert_eq!(first.code, Some(3), "until-idle reports the wait; {}", first.said());
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
}

/// A session the provider answered before it went away had begun the work, so
/// it is charged; it is still the visit's first unavailable ending and still
/// waits its minute.
// §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3 §FS-rhei-run.3.3
#[test]
fn a_session_the_provider_answered_is_charged_and_still_waits() {
    let stream = gave_up_after(&[assistant("toolUse", None)], SIGNAL);
    let fixture = Outage::pi(stream).fixture("pi-unavailable-answered");
    let first = run_once(&fixture, "first");

    let warning =
        unavailable_warning("attempt charged, because the provider answered this session");
    first.assert_says(&warning);
    first.assert_says("waiting: pi-local on openai-codex");
    let record = only_record(&fixture, &first);
    assert_eq!(record["ending"], "provider_unavailable", "{record:#}");
    assert_eq!(record["attempt_charged"], true, "{record:#}");
    assert_eq!(record["charged"], 1, "{record:#}");
    assert_eq!(record["unavailable"], 1, "{record:#}");
    let wait = wait(&fixture);
    assert_eq!(wait["kind"], "unavailable", "{wait:#}");
    assert_backoff(&wait, 1);
    assert_eq!(first.code, Some(3), "{}", first.said());
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
}

/// The record the visit's sixth unavailable ending leaves, one of the six
/// charged, so one attempt of two is left. Seeded rather than reached: six
/// runs and five expired waits would prove nothing the count test does not.
fn seed_sixth_ending(fixture: &ProviderFixture) {
    let spawns = fixture.root.join("runtime/spawns");
    fs::create_dir_all(&spawns).unwrap();
    let log = fixture.root.join("runtime/logs").join(format!("{}-attempt6.log", record_stem()));
    let record = json!({
        "task": TASK, "state": "working", "moves": 0, "attempt": 6, "charged": 1,
        "attempt_charged": false, "kind": "agent", "worker": "pi-local", "log": log,
        "started": "2026-10-29T12:00:00Z", "ended": "2026-10-29T12:00:00Z", "duration": "0s",
        "code": 0, "ending": "provider_unavailable", "signal": SIGNAL, "unavailable": 6
    });
    fs::write(spawns.join(format!("{}.json", record_stem())), format!("{record:#}\n")).unwrap();
}

/// The invocation's file name, as `rhei run` names its log minus the attempt.
fn record_stem() -> String {
    format!("task-{TASK}-working-pi-local-openai-codex-gpt-6.1-sol")
}
