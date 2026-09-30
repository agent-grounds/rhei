//! Recognition keys on the resolved provider, in a closed set, and never on
//! the agent registry id. §FS-rhei-agents.2.3 §FS-rhei-run.3.3

use super::provider_limit_support::*;
use super::*;

/// Always refuse with exactly the reset-bearing signal line, so the only
/// variable between the two cases below is the target's provider.
const REFUSE_BODY: &str = "print(SIGNAL, file=sys.stderr, flush=True)\nraise SystemExit(1)\n";

/// The line the incident actually captured: the weekly period word and a reset
/// with no minutes, from the same provider and the same exit code as the
/// control. §FS-rhei-agents.2.3
const WEEKLY_BARE_SIGNAL: &str = "You've hit your weekly limit · resets 2pm (Europe/Zurich)";

fn refusing_fixture_printing(
    name: &str,
    target: &str,
    attempts: u32,
    signal: &str,
) -> ProviderFixture {
    let machine = SIMPLE_MACHINE
        .replace("target: codex:openai:alpha", &format!("target: {target}"))
        .replace("attempts: 1", &format!("attempts: {attempts}"));
    ProviderFixture::new(
        name,
        SINGLE_TASK,
        &machine,
        &REFUSE_BODY.replace("SIGNAL", &format!("{signal:?}")),
    )
}

fn refusing_fixture(name: &str, target: &str, attempts: u32) -> ProviderFixture {
    refusing_fixture_printing(name, target, attempts, LIMIT_SIGNAL)
}

/// The incident's own shape: a registry entry the user named — neither
/// `codex` nor `claude-code` — resolving provider `anthropic`. Its reset-bearing
/// refusal parks, is recorded under the pair it ran as, costs the visit no
/// attempt, and leaves the run alive on a budget it has not spent.
/// §FS-rhei-agents.2.3 §FS-rhei-agents.3.2.3 §FS-rhei-run.3.3
#[test]
fn a_user_named_entry_on_anthropic_parks_and_keeps_its_attempt_budget() {
    // `attempts: 2` is the budget the report was filed against: today both
    // refusals are charged and the run halts on the second one.
    let fixture = refusing_fixture("provider-anthropic-parks", "mock:anthropic:alpha", 2);
    let mut run = fixture.start(&[]);
    fixture.parked(&mut run);

    let record =
        metadata(&fixture.root)["metadata"]["tasks"]["1"]["providerLimits"]["working"].clone();
    assert_eq!(record["identity"]["agent"], "mock", "{record:#}");
    assert_eq!(record["identity"]["provider"], "anthropic", "{record:#}");
    assert_eq!(record["signal"], LIMIT_SIGNAL, "{record:#}");
    let next_attempt = record["nextAttemptAt"].as_str().expect("a recorded deadline").to_string();
    // The safe boundary is the minute after the reported 10:20pm, and
    // Europe/Zurich is a whole-minute offset, so the UTC instant keeps it.
    assert!(next_attempt.ends_with(":21:00Z"), "{next_attempt}");
    assert!(next_attempt > utc_at(epoch_now()), "{next_attempt} must still be ahead");

    let events = fixture.events();
    let limited = events.iter().filter(|e| e["outcome"] == "provider_limited").collect::<Vec<_>>();
    assert_eq!(limited.len(), 1, "{}", fixture.output());
    assert_eq!(limited[0]["provider"], "anthropic");
    assert_eq!(limited[0]["task"], "workspace.1");
    assert_eq!(limited[0]["next_attempt_at"], next_attempt.as_str());

    let records = fixture.records();
    assert_eq!(records.len(), 1, "one retained spawn record per refusal: {records:#?}");
    assert_eq!(records[0]["ending"], "provider_limited", "{:#}", records[0]);
    assert_eq!(records[0]["attempt_charged"], false, "{:#}", records[0]);
    // The visit's running total: none of its two attempts is spent, so the
    // next run resumes rather than halting on a spent budget.
    assert_eq!(records[0]["charged"], 0, "{:#}", records[0]);
    assert_eq!(records[0]["code"], 1, "{:#}", records[0]);

    assert_stays_parked(&fixture, &mut run);
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
}

/// The closed set is a contract, not an implementation detail: the same script
/// and the same line under a provider outside it stay an ordinary failure, and
/// the attempt stays charged. §FS-rhei-agents.2.3 §FS-rhei-agents.5.2.1
#[test]
fn an_unrecognized_provider_refusal_stays_an_ordinary_failure() {
    let fixture = refusing_fixture("provider-unrecognized-fails", "mock:acme:alpha", 1);
    let mut run = fixture.start(&[]);
    assert!(!fixture.finish(&mut run).success(), "{}", fixture.output());

    assert!(
        !markdown_text(&fixture.root).contains("providerLimits:"),
        "an unrecognized provider must persist no wait"
    );
    assert!(
        !fixture.events().iter().any(|e| e["outcome"] == "provider_limited"),
        "{}",
        fixture.output()
    );

    let records = fixture.records();
    assert_eq!(records.len(), 1, "{records:#?}");
    assert_eq!(records[0]["ending"], "exited", "{:#}", records[0]);
    assert_eq!(records[0]["attempt_charged"], true, "{:#}", records[0]);
    assert_eq!(records[0]["charged"], 1, "{:#}", records[0]);

    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
}

/// The ticket's own `weekly-bare` case, end to end: the same user-named
/// `anthropic` entry and the same exit code as the test above, differing only
/// in the sentence the agent prints. A weekly limit and a reset with no minutes
/// are the same provider refusal, so they reach the same parked ending and
/// leave the visit's two attempts untouched.
/// §FS-rhei-agents.2.3 §FS-rhei-agents.3.2.3 §FS-rhei-run.3.3
#[test]
fn a_weekly_limit_with_a_bare_hour_reset_parks_and_keeps_its_attempt_budget() {
    // `attempts: 2` is the budget the report was filed against: today both
    // refusals are charged and the run halts on the second one.
    let fixture = refusing_fixture_printing(
        "provider-weekly-parks",
        "mock:anthropic:alpha",
        2,
        WEEKLY_BARE_SIGNAL,
    );
    let mut run = fixture.start(&[]);
    fixture.parked(&mut run);

    let record =
        metadata(&fixture.root)["metadata"]["tasks"]["1"]["providerLimits"]["working"].clone();
    assert_eq!(record["identity"]["agent"], "mock", "{record:#}");
    assert_eq!(record["identity"]["provider"], "anthropic", "{record:#}");
    // The sentence is stored verbatim; no period is derived from it.
    assert_eq!(record["signal"], WEEKLY_BARE_SIGNAL, "{record:#}");
    let next_attempt = record["nextAttemptAt"].as_str().expect("a recorded deadline").to_string();
    // A bare hour is that hour at `00`, so the safe boundary is the minute
    // after it, and Europe/Zurich is a whole-minute offset from UTC.
    assert!(next_attempt.ends_with(":01:00Z"), "{next_attempt}");
    assert!(next_attempt > utc_at(epoch_now()), "{next_attempt} must still be ahead");

    let events = fixture.events();
    let limited = events.iter().filter(|e| e["outcome"] == "provider_limited").collect::<Vec<_>>();
    assert_eq!(limited.len(), 1, "{}", fixture.output());
    assert_eq!(limited[0]["provider"], "anthropic");
    assert_eq!(limited[0]["task"], "workspace.1");
    assert_eq!(limited[0]["next_attempt_at"], next_attempt.as_str());

    let records = fixture.records();
    assert_eq!(records.len(), 1, "one retained spawn record per refusal: {records:#?}");
    assert_eq!(records[0]["ending"], "provider_limited", "{:#}", records[0]);
    assert_eq!(records[0]["attempt_charged"], false, "{:#}", records[0]);
    // The visit's running total: none of its two attempts is spent, so the
    // next run resumes rather than halting on a spent budget.
    assert_eq!(records[0]["charged"], 0, "{:#}", records[0]);
    assert_eq!(records[0]["code"], 1, "{:#}", records[0]);

    assert_stays_parked(&fixture, &mut run);
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
}
