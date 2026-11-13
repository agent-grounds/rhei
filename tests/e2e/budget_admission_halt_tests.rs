//! Budget refusals must reach the final no-progress diagnosis in both schedulers.
//! §FS-rhei-run.3.4

use super::budget_admission_halt_support::*;
use super::budget_delegated_support::assert_remedy_names_machine_file;

const MISSING: &str = "no such reservation";

#[test]
fn issue671_serial_ancestor_refusal_explains_final_halt() {
    let fixture = Fixture::new(1, 10);
    let out = fixture.run(true);
    fixture.assert_stranded(&out, MISSING, true);
    assert_refused_admission_help(&out, MISSING, 1);
    assert_ancestor_remedy(&out);
}

#[test]
fn issue671_pool_ancestor_refusal_covers_refill_and_initial_fill() {
    let fixture = Fixture::new(2, 10);
    // prepare finishes in the pool; the just-ready write state reaches the
    // shared admission helper through refill, within that same first pass.
    let refill = fixture.run(true);
    fixture.assert_stranded(&refill, MISSING, true);
    let events = fixture.events();
    assert!(
        events.iter().take_while(|event| event["event"] != "pass_ended").any(|event| {
            event["event"] == "message"
                && event["text"].as_str().is_some_and(|s| s.contains(MISSING))
        }),
        "refusal must occur during first-pass refill, before any initial agent fill:\n{}",
        text(&refill)
    );
    // With no prepare left, write now reaches admission through initial fill.
    // Run both before asserting the help so the red result covers both paths.
    let initial = fixture.run(true);
    fixture.assert_stranded(&initial, MISSING, false);
    assert!(initial.stdout.contains("Initial states: write=2"), "{}", text(&initial));
    assert!(!initial.stdout.contains("Spawning program"), "{}", text(&initial));
    for out in [&refill, &initial] {
        assert_refused_admission_help(out, MISSING, 2);
        assert_ancestor_remedy(out);
    }
}

#[test]
fn issue671_pool_exhausted_admission_keeps_limiter_diagnosis() {
    // The program's one applied edge spends the whole travel allowance; the
    // first neural admission is refused before a worker can start.
    let fixture = Fixture::new(2, 1);
    let out = fixture.run(false);
    fixture.assert_stranded(&out, "ticket travel", true);
    let all = text(&out);
    for fact in [
        "1 (machine)",
        "consumed:    1  outstanding: 0  remaining: 0",
        "per ticket identity",
        "transition_limit",
        "settings.json",
    ] {
        assert!(all.contains(fact), "the limiter explanation must survive ({fact}):\n{all}");
    }
    assert_remedy_names_machine_file(&out, &fixture.root, "transition_limit");
    let events = fixture.events();
    let halted: Vec<_> = events.iter().filter(|event| event["event"] == "budget_halt").collect();
    assert!(!halted.is_empty(), "exhaustion must retain budget_halt events");
    for event in halted {
        assert!(matches!(event["task"].as_str(), Some("admission.1" | "admission.2")));
        assert_eq!(event["state"], "write");
        assert_eq!(event["dimension"], "ticket travel");
    }
    assert_refused_admission_help(&out, "ticket travel", 2);
}
