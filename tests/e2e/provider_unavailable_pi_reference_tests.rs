//! A `pi` target that names its provider only inside pi's own
//! `<provider>/<model>` reference waits out an outage as the respelling with a
//! provider segment does (agent-grounds/rhei#664).
//!
//! Before the fix the session is charged as an ordinary attempt and nothing
//! waits, because no provider keyed the wait.
// §FS-rhei-agents.2.5 §FS-rhei-run.3.3

use super::provider_limit_support::*;
use super::provider_unavailable_pi_support::*;
use super::*;
use serde_json::json;

/// The incident under the reporter's spelling, on a custom entry of the `pi`
/// family: no attempt charged, a wait keyed on the reference's provider, pi
/// handed the same `--model` as before, and no price-book complaint, because
/// the target's own provider is untouched. §FS-rhei-agents.2.5
// §FS-rhei-run.3.3 §FS-rhei-run-json.2.1
#[test]
#[ignore = "red until #664: the pi reference spelling is charged and writes no wait; implement removes this"]
fn a_pi_reference_target_waits_without_spending_an_attempt() {
    let outage = Outage { target: TARGET_WITHOUT_PROVIDER, ..Outage::pi(gave_up()) };
    let fixture = outage.fixture("pi-reference-waits");
    let first = run_once(&fixture, "first");

    let argv = fs::read_to_string(fixture.root.join("runtime/argv.txt")).unwrap();
    assert!(argv.contains("--model openai-codex/gpt-6.1-sol"), "pi's model reference: {argv}");

    let record = only_record(&fixture, &first);
    assert_eq!(record["ending"], "provider_unavailable", "{record:#}; {}", first.said());
    assert_eq!(record["attempt_charged"], false, "{record:#}");
    assert_eq!(record["charged"], 0, "{record:#}");
    assert_eq!(record["unavailable"], 1, "{record:#}");

    let wait = wait(&fixture);
    assert_eq!(wait["kind"], "unavailable", "{wait:#}; {}", first.said());
    assert_eq!(wait["identity"], json!({"agent": "pi-local", "provider": "openai-codex"}));
    assert_backoff(&wait, 1);
    let deadline = wait["nextAttemptAt"].as_str().unwrap();

    first.assert_says(&unavailable_warning("no attempt charged (free 1 of 6 on this visit)"));
    first.assert_says(&format!("waiting: pi-local on openai-codex starts again after {deadline}"));
    first.assert_never_says("required outputs are missing");
    first.assert_never_says("price book");
    let released = first.event("slot_released");
    assert_eq!(released["outcome"], "provider_unavailable", "{released:#}");
    assert_eq!(released["provider"], "openai-codex", "{released:#}");
    assert_eq!(first.code, Some(3), "until-idle reports the wait; {}", first.said());
    assert_eq!(first.stop()["idle_blocker"], "provider_unavailable", "{}", first.said());
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
}

/// The derived wait is the one the spawn gates read: a run before the deadline
/// starts nothing, and the run after it spawns the ticket once, finishes it and
/// clears the wait. Single worker first, then the pool, which reaches the gate
/// from its own call site. §FS-rhei-agents.2.5 §FS-rhei-run.3.3
#[test]
#[ignore = "red until #664: no wait is derived, so nothing holds the next spawn; implement removes this"]
fn a_wait_derived_from_a_pi_reference_holds_the_next_spawn() {
    let single = Outage { target: TARGET_WITHOUT_PROVIDER, ..Outage::pi(gave_up()) };
    let fixture = single.fixture("pi-reference-holds");
    let first = run_once(&fixture, "first");
    assert_eq!(wait(&fixture)["kind"], "unavailable", "a wait is written; {}", first.said());
    let early = run_once(&fixture, "before-the-deadline");
    assert_eq!(early.code, Some(3), "{}", early.said());
    assert_eq!(starts(&fixture), 1, "nothing starts before the deadline; {}", early.said());
    expire_provider_deadlines(&fixture.root);
    fs::write(fixture.root.join(PROVIDER_BACK), "").unwrap();
    let back = run_once(&fixture, "provider-back");
    assert_eq!(back.code, Some(0), "{}", back.said());
    assert_eq!(starts(&fixture), 2, "one spawn after the deadline; {}", back.said());
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "completed");
    assert!(!markdown_text(&fixture.root).contains("providerLimits:"), "{}", back.said());

    let pool = Outage {
        target: TARGET_WITHOUT_PROVIDER,
        tasks: TWO_TASKS,
        concurrent: true,
        ..Outage::pi(gave_up())
    };
    let fixture = pool.fixture("pi-reference-holds-pool");
    let first = run_pool(&fixture, "first");
    for local in ["1", "2"] {
        let wait = task_wait(&fixture, local);
        assert_eq!(wait["kind"], "unavailable", "ticket {local}: {wait:#}; {}", first.said());
    }
    let early = run_pool(&fixture, "before-the-deadline");
    assert_eq!(early.code, Some(3), "{}", early.said());
    assert_eq!(starts(&fixture), 2, "nothing starts before the deadline; {}", early.said());
    expire_provider_deadlines(&fixture.root);
    fs::write(fixture.root.join(PROVIDER_BACK), "").unwrap();
    let back = run_pool(&fixture, "provider-back");
    assert_eq!(back.code, Some(0), "{}", back.said());
    assert_eq!(starts(&fixture), 4, "one spawn each after the deadline; {}", back.said());
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "completed");
    assert!(!markdown_text(&fixture.root).contains("providerLimits:"), "{}", back.said());
}
