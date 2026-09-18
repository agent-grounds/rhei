use std::fs;

use super::session_continuation_support::*;
use super::*;

/// A supported self-loop continues visit 1 into visit 2, records the `_state`
/// parent, writes no redundant named snapshot, and composes ordinary memory
/// even for the warm spawn. §FS-rhei-snapshots.4.7
#[test]
fn self_loop_continuation_preloads_the_previous_auto_snapshot() {
    let transitions = simple_transitions(2);
    let fixture =
        continuation_fixture("session-continue-supported", 2, 1, "supported", "", &transitions);
    fs::create_dir_all(fixture.dir.join("runtime/supervise")).expect("brief directory");
    fs::write(
        fixture.dir.join("runtime/supervise/plan.1.md"),
        "Keep the repeated state grounded.\n",
    )
    .expect("write brief");

    let run = run_continuation(&fixture, &[]);
    assert_success(&run);
    let log = continuation_log(&fixture);
    assert!(
        log.contains("visit=1 attempt=1 target=fake-acme-model-a resume=")
            && log.contains(
                "visit=2 attempt=1 target=fake-acme-model-a \
                 resume=plan.1-fake-acme-model-a-v1-a1"
            ),
        "visit 2 must receive visit 1's session identity; got:\n{log}"
    );

    let parent = &manifest(&fixture, 2, "fake-acme-model-a", "_state")["parent_ref"];
    assert_eq!(parent["snapshot_name"], "_state");
    assert_eq!(parent["emitting_state"], "loop");
    assert_eq!(parent["visit"], 1);
    assert_eq!(parent["target_slug"], "fake-acme-model-a");
    assert_eq!(parent["generation"], 1);

    let task_cache = fixture.dir.join(".rhei/cache/snapshots/plan.1");
    let names = fs::read_dir(&task_cache)
        .expect("task snapshot cache")
        .map(|entry| entry.expect("snapshot name").file_name())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["_state"], "continuation alone writes only auto snapshots");

    let prompt =
        fs::read_to_string(fixture.dir.join("runtime/prompts/visit-2-fake-acme-model-a-1.md"))
            .expect("visit 2 prompt");
    assert!(prompt.contains("## Previous Visits"), "warm prompt lost visit memory:\n{prompt}");
    assert!(prompt.contains("## Supervisor Brief"), "warm prompt lost its brief:\n{prompt}");
}

/// Selection is exact rather than merely "some prior visit": visit 3 must
/// continue visit 2. §FS-rhei-snapshots.4.7
#[test]
fn continuation_selects_exactly_visit_n_minus_one() {
    let transitions = simple_transitions(3);
    let fixture =
        continuation_fixture("session-continue-three-visits", 3, 1, "supported", "", &transitions);
    let run = run_continuation(&fixture, &[]);
    assert_success(&run);
    let log = continuation_log(&fixture);
    assert!(
        log.contains(
            "visit=2 attempt=1 target=fake-acme-model-a resume=plan.1-fake-acme-model-a-v1-a1"
        ),
        "visit 2 did not continue visit 1:\n{log}"
    );
    assert!(
        log.contains(
            "visit=3 attempt=1 target=fake-acme-model-a resume=plan.1-fake-acme-model-a-v2-a1"
        ),
        "visit 3 must continue visit 2, not an older visit:\n{log}"
    );
}

/// A profile that can emit but cannot preload still executes every visit,
/// explains the cold fallback, and records no parent. §FS-rhei-snapshots.4.7
#[test]
fn unsupported_preload_runs_every_visit_cold_with_a_reason() {
    let transitions = simple_transitions(2);
    let fixture =
        continuation_fixture("session-continue-unsupported", 2, 1, "emit-only", "", &transitions);
    let run = run_continuation(&fixture, &[]);
    assert_success(&run);
    let log = continuation_log(&fixture);
    assert_eq!(log.lines().count(), 2, "both visits execute:\n{log}");
    assert!(log.lines().all(|line| line.contains("resume= parent=")), "both run cold:\n{log}");
    let output = format!("{}{}", run.stdout, run.stderr);
    assert!(
        output.contains("no supported snapshot preload strategy")
            && output.contains("running cold"),
        "cold fallback must say why:\n{output}"
    );
    assert!(
        manifest(&fixture, 2, "fake-acme-model-a", "_state")["parent_ref"].is_null(),
        "cold continuation must not synthesize lineage"
    );
}

/// With no session layout at all, continuation remains a degradable request:
/// validation and both visits succeed cold rather than producing the strict
/// named-emit error. §FS-rhei-snapshots.4.7 §FS-rhei-snapshots.11
#[test]
fn unsupported_session_layout_validates_and_executes_cold() {
    let transitions = simple_transitions(2);
    let fixture =
        continuation_fixture("session-continue-no-layout", 2, 1, "none", "", &transitions);
    let validated = run_cli("validate", &fixture.plan, &fixture.machine, &[]);
    assert_success(&validated);
    let run = run_continuation(&fixture, &[]);
    assert_success(&run);
    let log = continuation_log(&fixture);
    assert_eq!(log.lines().count(), 2, "both visits execute:\n{log}");
    assert!(log.lines().all(|line| line.contains("resume= parent=")), "both run cold:\n{log}");
    let output = format!("{}{}", run.stdout, run.stderr);
    assert!(
        output.contains("no supported snapshot preload strategy")
            && output.contains("running cold"),
        "unsupported profile must produce a reasoned cold fallback:\n{output}"
    );
}

/// A retry is another attempt of visit N, not visit N+1. Both attempts of
/// visit 2 therefore select visit 1's current generation. §FS-rhei-snapshots.4.7
#[test]
fn retries_keep_selecting_the_preceding_visit() {
    let transitions = simple_transitions(2);
    let fixture =
        continuation_fixture("session-continue-retry", 2, 2, "supported", "", &transitions);
    fs::create_dir_all(fixture.dir.join("runtime")).expect("runtime");
    fs::write(fixture.dir.join("runtime/agent-mode.txt"), "fail-first-attempt")
        .expect("agent mode");
    let first = run_continuation(&fixture, &[]);
    assert!(!first.status.success(), "visit 1 attempt 1 intentionally fails");
    assert_success(&run_transition(&fixture.plan, &fixture.machine, "1", "loop", "loop"));
    let second = run_continuation(&fixture, &[]);
    assert!(!second.status.success(), "visit 2 attempt 1 intentionally fails");
    let third = run_continuation(&fixture, &[]);
    assert_success(&third);
    let log = continuation_log(&fixture);
    let visit_two = log.lines().filter(|line| line.contains("visit=2 ")).collect::<Vec<_>>();
    assert_eq!(visit_two.len(), 2, "visit 2 should have one retry:\n{log}");
    assert!(
        visit_two.iter().all(|line| line.contains("resume=plan.1-fake-acme-model-a-v1-a1")),
        "both attempts of visit 2 must select visit 1's current generation:\n{log}"
    );
}

/// Auto snapshots use `on: always`, so an ordinary failed visit remains the
/// exact source for the next visit. §FS-rhei-snapshots.3.1 §FS-rhei-snapshots.4.7
#[test]
fn an_ordinary_failed_visit_is_eligible_for_continuation() {
    let fixture = continuation_fixture(
        "session-continue-failure",
        2,
        1,
        "supported",
        "",
        "  - from: loop\n    to: completed\n    condition: visitCount >= 2\n  - from: loop\n    to: loop\n",
    );
    fs::create_dir_all(fixture.dir.join("runtime")).expect("runtime");
    fs::write(fixture.dir.join("runtime/agent-mode.txt"), "fail-first-visit").expect("agent mode");
    let first = run_continuation(&fixture, &[]);
    assert!(!first.status.success(), "visit 1 intentionally exits unsuccessfully");
    assert_success(&run_transition(&fixture.plan, &fixture.machine, "1", "loop", "loop"));
    fs::write(fixture.dir.join("runtime/agent-mode.txt"), "").expect("clear agent mode");
    let run = run_continuation(&fixture, &[]);
    assert_success(&run);
    let log = continuation_log(&fixture);
    assert!(
        log.contains(
            "visit=2 attempt=1 target=fake-acme-model-a resume=plan.1-fake-acme-model-a-v1-a1"
        ),
        "visit 2 must preload the failed visit 1 transcript:\n{log}"
    );
    assert_eq!(manifest(&fixture, 1, "fake-acme-model-a", "_state")["completion"], "failure");
}
