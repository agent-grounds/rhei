//! A Claude Code session that gives up on an unreachable provider waits instead
//! of spending the visit's attempts (agent-grounds/rhei#745).
//!
//! Without the recognition the session's exit `1` is an ordinary failed
//! attempt: charged, routed as an error, and without `--continue-on-error` the
//! run aborts.
// §FS-rhei-provider-unavailability.2 §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3 §FS-rhei-run.3.3

use super::provider_limit_support::*;
use super::provider_unavailable_claude_code_support::*;
use super::provider_unavailable_pi_support::{backoff, Pass};
use super::*;
use serde_json::{json, Value};

/// The text of `stream`'s `is_api_error_message` event: the signal.
fn give_up_text(stream: &str) -> String {
    let event: Value = stream
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .find(is_give_up)
        .expect("a give-up event");
    event["message"]["content"][0]["text"].as_str().unwrap().to_string()
}

/// What a recognized, free first ending leaves for every ticket of `mode`:
/// an uncharged record with the real exit code, a one-minute wait on `cld` on
/// `anthropic`, both lines, and the ticket in its state. §FS-rhei-agents.8.4
fn assert_waits_free(fixture: &ProviderFixture, pass: &Pass, mode: Mode, signal: &str) {
    for (local, task) in mode.tasks() {
        let record = record_of(fixture, pass, task);
        assert_eq!(
            record["ending"],
            "provider_unavailable",
            "{mode:?} {record:#}; {}",
            pass.said()
        );
        assert_eq!(record["code"], 1, "{mode:?} {record:#}");
        assert_eq!(record["attempt_charged"], false, "{mode:?} {record:#}");
        assert_eq!(record["charged"], 0, "{mode:?} {record:#}");
        assert_eq!(record["unavailable"], 1, "{mode:?} {record:#}");
        assert_eq!(record["signal"], signal, "{mode:?} {record:#}");
        let wait = task_wait(fixture, local);
        assert_eq!(wait["kind"], "unavailable", "{mode:?} {wait:#}; {}", pass.said());
        assert_eq!(wait["identity"], json!({"agent": "cld", "provider": "anthropic"}));
        assert_eq!(wait["signal"], signal, "{mode:?} {wait:#}");
        assert!((backoff(&wait) - 60).abs() <= 1, "{mode:?} one minute: {wait:#}");
        let deadline = wait["nextAttemptAt"].as_str().unwrap();
        let free = "no attempt charged (free 1 of 6 on this visit)";
        pass.assert_says(&unavailable_warning(task, signal, free));
        pass.assert_says(&format!("waiting: cld on anthropic starts again after {deadline}"));
    }
    pass.assert_never_says("agent exited with code 1");
    pass.assert_never_says("required outputs are missing");
    assert!(!pass.events.iter().any(|event| event["event"] == "task_outputs_missing"));
    assert_eq!(pass.code, Some(3), "{mode:?}: until-idle reports the wait; {}", pass.said());
    assert_eq!(pass.stop()["idle_blocker"], "provider_unavailable", "{mode:?}");
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
}

/// The incident: the connection is refused through every retry, Claude Code
/// exits `1`. The visit keeps its attempts and waits a minute, a run before the
/// deadline starts nothing, and the run after it finishes the ticket once the
/// provider answers again. On a priced model the day settles at the measured zero.
// §FS-rhei-provider-unavailability.2 §FS-rhei-agents.3.2.1 §FS-rhei-agents.8.4 §FS-rhei-run.3.3
// §FS-rhei-run-json.2.1 §FS-rhei-budgets.6.2
#[test]
#[ignore = "red until #745: a claude-code give-up is recognized and waits; implement removes this"]
fn a_claude_code_session_that_gives_up_on_its_provider_waits_without_spending_an_attempt() {
    let fixture = Outage::claude(REFUSED).fixture("claude-unavailable-waits");
    let first = run_once(&fixture, "first");
    assert_waits_free(&fixture, &first, MODES[0], SIGNAL);
    let record = record_of(&fixture, &first, TASK);
    // The footer keeps the plain exit. §FS-rhei-agents.8.4
    let log = fs::read_to_string(record["log"].as_str().unwrap()).unwrap();
    let footer = log.rsplit("=== exit ===").next().unwrap();
    assert!(footer.contains("code: 1") && !footer.contains("provider"), "{footer}");
    let released = first.event("slot_released");
    assert_eq!(released["outcome"], "provider_unavailable", "{released:#}");
    assert_eq!(released["provider"], "anthropic", "{released:#}");
    assert!(!first.events.iter().any(|event| event["outcome"] == "failed"), "{}", first.said());
    let journal = fs::read_to_string(fixture.root.join("runtime/transitions.log")).unwrap();
    assert!(journal.contains("outcome=provider_unavailable"), "{journal}");
    let spend = budget_show(&fixture);
    assert!(spend.contains("project spend: $0.00 consumed"), "measured zero: {spend}");

    let early = run_once(&fixture, "before-the-deadline");
    assert_eq!(early.code, Some(3), "{}", early.said());
    assert_eq!(starts(&fixture), 1, "nothing starts before the deadline; {}", early.said());

    expire_provider_deadlines(&fixture.root);
    fs::write(fixture.root.join(PROVIDER_BACK), "").unwrap();
    let back = run_once(&fixture, "provider-back");
    back.assert_says(&format!(
        "Re-spawning Task {TASK} in state 'working': attempt 1 of 2; \
         the previous attempt found its provider unavailable ({SIGNAL})"
    ));
    assert_eq!(back.code, Some(0), "{}", back.said());
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "completed");
    assert!(!markdown_text(&fixture.root).contains("providerLimits:"), "{}", back.said());
}

/// Every transient class Claude Code retries to its count - no response, `429`,
/// `5xx` - is recognized, through the single worker and the pool, with and
/// without `--continue-on-error`: the run never aborts on it. §FS-rhei-agents.5.2.1
// §FS-rhei-provider-unavailability.2 §FS-rhei-agents.5.2.2 §FS-rhei-run.3
#[test]
#[ignore = "red until #745: every transient give-up waits in every mode; implement removes this"]
fn every_transient_give_up_waits_through_the_single_worker_and_the_pool() {
    for mode in MODES {
        let fixture = mode.outage(Outage::claude(REFUSED)).fixture(&mode.name("refused"));
        let pass = run_in(&fixture, "first", mode);
        assert_waits_free(&fixture, &pass, mode, SIGNAL);
    }
    for (name, stream) in
        [("hang", HANG), ("429", STATUS_429), ("500", STATUS_500), ("529", STATUS_529)]
    {
        let fixture = Outage::claude(stream).fixture(&format!("claude-unavailable-{name}"));
        let pass = run_once(&fixture, name);
        assert_waits_free(&fixture, &pass, MODES[0], &give_up_text(stream));
    }
}

/// The wait is keyed on the login, so a ticket on the same `cld` and
/// `anthropic` that has not started yet is held rather than spending its own
/// session to find out the same thing. §FS-rhei-run.3.3
#[test]
#[ignore = "red until #745: a claude-code give-up holds the next ticket on its login; implement removes this"]
fn a_give_up_holds_the_next_ticket_on_the_same_login() {
    let outage = Outage { tasks: TWO_TASKS, ..Outage::claude(REFUSED) };
    let fixture = outage.fixture("claude-unavailable-holds");
    let first = run_once(&fixture, "first");
    assert_eq!(starts(&fixture), 1, "the sibling is held; {}", first.said());
    assert_eq!(task_wait(&fixture, "1")["kind"], "unavailable", "{}", first.said());
    assert!(fixture.records().iter().all(|record| record["task"] == TASK), "{}", first.said());
    assert_eq!(first.code, Some(3), "{}", first.said());
    let early = run_once(&fixture, "before-the-deadline");
    assert_eq!(starts(&fixture), 1, "nothing starts before the deadline; {}", early.said());
}

/// A session the model answered before the provider went away had begun the
/// work, so it is charged; the give-up message itself is `<synthetic>` and is
/// never an answer. It is still recorded as unavailable and still waits.
// §FS-rhei-agents.3.2.3 §FS-rhei-provider-unavailability.2
#[test]
#[ignore = "red until #745: an answered claude-code give-up is charged and waits; implement removes this"]
fn a_session_the_model_answered_is_charged_and_still_waits() {
    for mode in MODES {
        let outage = mode.outage(Outage::claude(&answered_first(REFUSED)));
        let fixture = outage.fixture(&mode.name("answered"));
        let pass = run_in(&fixture, "first", mode);
        for (local, task) in mode.tasks() {
            let charge = "attempt charged, because the provider answered this session";
            pass.assert_says(&unavailable_warning(task, SIGNAL, charge));
            let record = record_of(&fixture, &pass, task);
            assert_eq!(record["ending"], "provider_unavailable", "{mode:?} {record:#}");
            assert_eq!(record["attempt_charged"], true, "{mode:?} {record:#}");
            assert_eq!(record["charged"], 1, "{mode:?} {record:#}");
            assert_eq!(record["unavailable"], 1, "{mode:?} {record:#}");
            assert_eq!(task_wait(&fixture, local)["kind"], "unavailable", "{mode:?}");
        }
        assert_eq!(pass.code, Some(3), "{mode:?} {}", pass.said());
        assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
    }
}

/// A charged ending that spends the visit's last attempt halts on the
/// non-zero exit exactly as pi's does on exit `0`: the halt line and
/// `task_outputs_missing` name the artifact left unwritten, and no wait is
/// written. Reached by the seventh ending of a visit, and by an answered one.
// §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3 §FS-rhei-run-json.2.1
#[test]
#[ignore = "red until #745: a charged claude-code give-up on the last attempt halts naming its outputs; implement removes this"]
fn a_charged_give_up_on_the_last_attempt_halts_naming_the_missing_outputs() {
    let seventh = "attempt charged, because this visit's 6 free endings are spent";
    let answered = "attempt charged, because the provider answered this session";
    let cases = [
        ("seventh", REFUSED.to_string(), 6, 7, seventh),
        ("answered", answered_first(REFUSED), 1, 2, answered),
    ];
    for (case, stream, seeded, unavailable, charge) in cases {
        for mode in MODES {
            let fixture = mode.outage(Outage::claude(&stream)).fixture(&mode.name(case));
            seed_ending(&fixture, seeded, 1);
            let pass = run_in(&fixture, "last", mode);
            pass.assert_says(&unavailable_warning(TASK, SIGNAL, charge));
            let finding = format!("finding (runtime/exports/{TASK}/finding.md)");
            pass.assert_says(&format!(
                "halting Task {TASK} in state 'working': 2 attempts spent on this visit \
                 and the completion condition is still unmet: {finding}"
            ));
            pass.assert_says(&format!("run: rhei retry {TASK}"));
            let record = record_of(&fixture, &pass, TASK);
            assert_eq!(record["ending"], "provider_unavailable", "{mode:?} {record:#}");
            assert_eq!(record["code"], 1, "{mode:?} {record:#}");
            assert_eq!(record["attempt_charged"], true, "{mode:?} {record:#}");
            assert_eq!(record["charged"], 2, "{mode:?} {record:#}");
            assert_eq!(record["unavailable"], unavailable, "{mode:?} {record:#}");
            assert_eq!(task_wait(&fixture, "1"), Value::Null, "{mode:?}: no wait; {}", pass.said());
            let missing = pass.event("task_outputs_missing");
            assert_eq!(missing["task"], TASK, "{missing:#}");
            assert_eq!(missing["entries"][0], finding.as_str(), "{missing:#}");
            let at = |name: &str| {
                pass.events.iter().position(|e| e["event"] == name && e["task"] == TASK)
            };
            assert!(at("task_outputs_missing") < at("slot_released"), "{}", pass.said());
            let released = pass
                .events
                .iter()
                .find(|e| e["event"] == "slot_released" && e["task"] == TASK)
                .unwrap();
            assert_eq!(released["outcome"], "provider_unavailable", "{released:#}");
            assert!(released.get("next_attempt_at").is_none(), "{released:#}");
            assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
        }
    }
}

/// A free ending frees an attempt, not spend: on an unpriced model, or with no
/// `result` event to measure, the session still settles a full reserve.
// §FS-rhei-budgets.6.2 §FS-rhei-agents.3.2.3
#[test]
#[ignore = "red until #745: an unmeasured free claude-code give-up settles the reserve; implement removes this"]
fn a_free_give_up_that_nothing_prices_still_settles_the_reserve() {
    let unpriced = Outage { target: UNPRICED_TARGET, ..Outage::claude(REFUSED) };
    let no_result = Outage::claude(&without(REFUSED, is_result));
    for (name, outage) in [("unpriced", unpriced), ("no-result", no_result)] {
        let fixture = outage.fixture(&format!("claude-unavailable-{name}"));
        let pass = run_once(&fixture, name);
        let record = record_of(&fixture, &pass, TASK);
        assert_eq!(record["ending"], "provider_unavailable", "{name}: {record:#}; {}", pass.said());
        assert_eq!(record["attempt_charged"], false, "{name}: {record:#}");
        let spend = budget_show(&fixture);
        assert!(spend.contains("project spend: $20.00 consumed"), "{name}: the reserve: {spend}");
    }
}
