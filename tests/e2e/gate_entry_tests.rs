//! Black-box contract for the edges around a gate: what entering one checks,
//! what `rhei transition` may do to leave one, and what the machine refuses
//! to load. §FS-rhei-gate-brief.2 §FS-rhei-transition-cmd.3.5

use super::gate_support::*;
use super::*;

/// The display and the request are inputs nobody listed: a missing one
/// refuses the edge, and every problem is named at once. §FS-rhei-gate-brief.2
#[test]
fn entering_a_gate_without_its_files_is_refused() {
    let fx = gate_fixture("gate-entry-missing");
    let result = fx.transition("verdict.1", "drafting", "review");
    assert_eq!(
        result.status.code(),
        Some(1),
        "stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(&result, "Task verdict.1 cannot enter state review.");
    assert_stderr_contains(
        &result,
        "the display file 'runtime/drafts/verdict.1.md' does not exist",
    );
    assert_stderr_contains(
        &result,
        "the request file 'runtime/gates/verdict.1.request.json' does not exist",
    );
    assert_stderr_contains(&result, "rhei gate schema request");
    assert_eq!(fx.state_of("verdict.1"), "drafting");
}

/// A request that does not hold refuses the edge with every problem, each
/// naming the file. §FS-rhei-gate-brief.2 §FS-rhei-gate-brief.3
#[test]
fn entering_a_gate_with_an_invalid_request_lists_every_problem() {
    let fx = gate_fixture("gate-entry-invalid");
    fx.prepare_review(
        "verdict.1",
        r#"{
          "version": 2,
          "title": "Rule",
          "display": "runtime/other.md",
          "colour": "red",
          "questions": [
            { "id": "approve", "kind": "scale", "prompt": "Ship it?" },
            { "id": "reason", "kind": "text", "prompt": "Why?" }
          ]
        }"#,
    );
    let result = fx.transition("verdict.1", "drafting", "review");
    assert_eq!(result.status.code(), Some(1));
    for expected in [
        "`version` is 2",
        "`display` is set by",
        "unknown field `colour`",
        "'scale'",
        "'reason' is reserved",
    ] {
        assert_stderr_contains(&result, expected);
    }
    assert_stderr_contains(&result, "runtime/gates/verdict.1.request.json: ");
    assert_eq!(fx.state_of("verdict.1"), "drafting");
}

/// A route reads a question the request must ask; one it does not ask
/// refuses the edge. §FS-rhei-gate-brief.5
#[test]
fn entering_a_gate_whose_routes_read_an_unasked_question_is_refused() {
    let fx = gate_fixture("gate-entry-routes");
    fx.prepare_review(
        "verdict.1",
        r#"{ "version": 1, "title": "Rule", "questions": [ { "id": "ship", "kind": "boolean", "prompt": "Ship?" } ] }"#,
    );
    let result = fx.transition("verdict.1", "drafting", "review");
    assert_eq!(result.status.code(), Some(1));
    assert_stderr_contains(&result, "`routes[0].when.approve` names no question the gate asks");
    assert_eq!(fx.state_of("verdict.1"), "drafting");
}

/// At a gate that asks authored required questions, `rhei transition` is
/// refused and names the command that answers. §FS-rhei-transition-cmd.3.5
#[test]
fn transition_out_of_a_gate_with_authored_questions_is_refused() {
    let fx = gate_fixture("gate-transition-refused");
    fx.prepare_review("verdict.1", GOOD_REQUEST);
    fx.enter("verdict.1", "review");

    let result = fx.rhei(&["transition", "verdict.1", "--from", "review", "--to", "drafting"]);
    assert_eq!(
        result.status.code(),
        Some(1),
        "stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(
        &result,
        "Task verdict.1 waits at gate 'review', which asks 1 required question.",
    );
    // Each command names the plan as every suggested command does, spelled per platform. §FS-rhei-errors.1.2.1
    assert_stderr_contains(&result, "read the brief with: rhei gate show ");
    assert_stderr_contains(&result, "answer it with: rhei gate answer ");
    assert_stderr_contains(
        &result,
        "verdict.rhei.md --task verdict.1 --from review --answers <file|->",
    );
    assert_eq!(fx.state_of("verdict.1"), "review");

    fx.enter("verdict.2", "vetting");
    let vetting = fx.rhei(&["transition", "verdict.2", "--from", "vetting", "--to", "drafting"]);
    assert_eq!(vetting.status.code(), Some(1));
    assert_stderr_contains(&vetting, "which asks 1 required question.");
}

/// A gate with no block, or whose brief degraded, transitions as it always
/// did. §FS-rhei-gate-brief.8
#[test]
fn transition_out_of_a_gate_without_authored_questions_still_works() {
    let fx = gate_fixture("gate-transition-allowed");
    fx.enter("verdict.3", "parked");
    assert_success(&fx.transition("verdict.3", "parked", "drafting"));
    assert_eq!(fx.state_of("verdict.3"), "drafting");

    fx.prepare_review("verdict.1", GOOD_REQUEST);
    fx.enter("verdict.1", "review");
    fs::remove_file(fx.root().join("runtime/gates/verdict.1.request.json"))
        .expect("remove the request");
    assert_success(&fx.transition("verdict.1", "review", "drafting"));
    assert_eq!(fx.state_of("verdict.1"), "drafting");
}

/// A `gate:` block the machine cannot honour is refused when it loads, every
/// problem at once. §FS-rhei-gate-brief.1.2
#[test]
fn a_machine_with_a_broken_gate_block_does_not_load() {
    let fx = gate_fixture("gate-validate");
    let broken = GATE_MACHINE
        .replace(
            "    description: The agent drafts the verdict\n",
            "    description: The agent drafts the verdict\n    gate:\n      title: Not a gate\n",
        )
        .replace("        - { to: drafting }\n", "        - { to: shipping }\n")
        .replace(
            "      title: \"Rule on {task_id}\"\n",
            "      title: \"Rule on {task_id}\"\n      questions:\n        - { id: approve, kind: boolean, prompt: \"Ship?\" }\n",
        );
    fx.write("states.yaml", &broken);

    let result = fx.rhei(&["validate", "verdict.rhei.md"]);
    assert!(!result.status.success(), "stdout:\n{}\nstderr:\n{}", result.stdout, result.stderr);
    assert_stderr_contains(&result, "state 'drafting' declares `gate:` but is not `gating: true`");
    assert_stderr_contains(
        &result,
        "`routes[1].to` 'shipping' is not a state this state has a declared edge to",
    );
    assert_stderr_contains(&result, "declares both `request` and `questions`");

    let show = fx.rhei(&["gate", "list"]);
    assert!(
        !show.status.success(),
        "a machine that fails to load is refused before anything is read"
    );
}
