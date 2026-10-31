//! The endings a provider-unavailable `pi` session is not, each charged as it
//! is today. These guards pass before the fix and must keep passing after it.
// §FS-rhei-agents.2.5

use super::provider_limit_support::*;
use super::provider_unavailable_pi_support::*;
use super::*;

/// pi's own verdict decides: an error pi would not retry, a retry that
/// recovered, and a retry that stopped short of `maxAttempts` all stay charged,
/// and so does the signal written as prose rather than as pi's events.
// §FS-rhei-agents.2.5
#[test]
fn pi_endings_that_are_not_its_retries_giving_up_stay_charged() {
    let overflow = "context_length_exceeded: the request exceeds the model's context window";
    let bad_request = "400 Bad Request: invalid input";
    let cases = [
        ("not-retried", session(&[failed_turn(overflow)])),
        (
            "recovered",
            session(&[
                failed_turn(SIGNAL),
                retry_start(1, SIGNAL),
                assistant("stop", None),
                retry_end(true, 1, None),
            ]),
        ),
        (
            "gave-up-early",
            session(&[
                failed_turn("503 Service Unavailable"),
                retry_start(1, "503 Service Unavailable"),
                failed_turn(bad_request),
                retry_end(false, 1, Some(bad_request)),
            ]),
        ),
        ("prose", format!("Error: {SIGNAL}\npi gave up after 3 retries\n")),
    ];
    for (name, stream) in cases {
        let fixture = Outage::pi(stream).fixture(&format!("pi-charged-{name}"));
        let pass = run_once(&fixture, name);
        assert_charged_as_today(&fixture, &pass, "exited");
        assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
    }
}

/// The same stream from a profile of another family is that family's output,
/// not pi's verdict. §FS-rhei-agents.2.5
#[test]
fn the_same_stream_from_another_family_stays_charged() {
    let outage = Outage { family: "claude-code", ..Outage::pi(gave_up()) };
    let fixture = outage.fixture("pi-charged-other-family");
    let pass = run_once(&fixture, "other-family");
    assert_charged_as_today(&fixture, &pass, "exited");
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
}

/// A target that resolves no provider has nothing to key the wait on, so the
/// reporter's own spelling stays charged. §FS-rhei-agents.2.5
#[test]
fn a_target_without_a_resolved_provider_stays_charged() {
    let outage = Outage { target: TARGET_WITHOUT_PROVIDER, ..Outage::pi(gave_up()) };
    let fixture = outage.fixture("pi-charged-no-provider");
    let pass = run_once(&fixture, "no-provider");
    let argv = fs::read_to_string(fixture.root.join("runtime/argv.txt")).unwrap();
    assert!(argv.contains("--mode json"), "rhei runs pi in json mode: {argv}");
    assert_charged_as_today(&fixture, &pass, "exited");
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
}

/// A session the engine timed out ended by the engine's hand, whatever its
/// stream said before that. §FS-rhei-agents.2.5
#[test]
fn a_timed_out_pi_session_stays_charged() {
    let outage = Outage { agent_timeout: Some("2s"), linger: 20, ..Outage::pi(gave_up()) };
    let fixture = outage.fixture("pi-charged-timed-out");
    let pass = run_once(&fixture, "timed-out");
    assert_charged_as_today(&fixture, &pass, "timed out");
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
}

/// A session that wrote its outputs moves on exactly as it does today, whatever
/// pi said about its provider. §FS-rhei-agents.2.5
#[test]
fn a_pi_session_with_its_outputs_on_disk_moves_as_before() {
    let outage = Outage { writes_outputs: true, ..Outage::pi(gave_up()) };
    let fixture = outage.fixture("pi-moves-with-outputs");
    let pass = run_once(&fixture, "outputs");
    let record = only_record(&fixture, &pass);
    assert_eq!(record["ending"], "exited", "{record:#}; {}", pass.said());
    assert_eq!(record["code"], 0, "{record:#}");
    assert_eq!(record["unavailable"].as_u64().unwrap_or(0), 0, "{record:#}");
    assert!(!markdown_text(&fixture.root).contains("providerLimits:"), "{}", pass.said());
    pass.assert_never_says("the provider was unavailable");
    assert_eq!(pass.code, Some(0), "{}", pass.said());
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "completed");
}
