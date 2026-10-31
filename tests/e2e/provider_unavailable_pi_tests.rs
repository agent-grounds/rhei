//! A `pi` session whose provider stops answering waits instead of spending the
//! visit's attempts (agent-grounds/rhei#513).
//!
//! Every test here is red until the recognition lands: today the session's
//! exit `0` with its outputs missing is an ordinary attempt, charged, and the
//! second one halts the visit.
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
#[ignore = "red until #513: a pi session that gave up on its provider is charged as an attempt; implement removes this"]
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
#[ignore = "red until #513: a second unavailable ending is charged and spends the visit; implement removes this"]
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
#[ignore = "red until #513: the spawn record keeps no count of unavailable endings to cap; implement removes this"]
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
}

/// A session the provider answered before it went away had begun the work, so
/// it is charged; it is still the visit's first unavailable ending and still
/// waits its minute.
// §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3 §FS-rhei-run.3.3
#[test]
#[ignore = "red until #513: a pi session that lost its provider mid-work is not recorded as unavailable; implement removes this"]
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
