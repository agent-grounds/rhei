// §FS-rhei-retry.2 §FS-rhei-transition-cmd.2
use super::supervision_retry_support::*;
use super::*;

fn refuses_retry_without_writes(state: &str, expected: &str, worker: bool, charged: Option<u64>) {
    let (dir, plan, _) = fixture();
    // A one-ticket plan isolates the refusal from supervision readiness.
    fs::write(
        &plan,
        format!("# Rhei: Refusal\n\n## Tasks\n\n### Task 1: Retry\n**State:** {state}\n"),
    )
    .unwrap();
    if let Some(charged) = charged {
        fs::create_dir_all(dir.join("runtime/spawns")).unwrap();
        let value = serde_json::json!({
            "task": "plan.1", "state": state, "moves": 0, "attempt": charged,
            "charged": charged, "attempt_charged": true, "kind": "agent", "worker": "mock",
            "log": "runtime/logs/old.log", "started": "2026-10-13T10:00:00Z",
            "ended": "2026-10-13T10:00:01Z", "duration": "1s", "code": 1, "ending": "exited"
        });
        fs::write(dir.join(format!("runtime/spawns/task-plan.1-{state}.json")), value.to_string())
            .unwrap();
    }
    let before = artifacts(&dir);
    let output = cli(&dir, "retry", "plan.1", &[], worker);
    assert!(!output.status.success());
    assert_eq!(artifacts(&dir), before, "refused grant must write nothing");
    assert!(
        text(&output).contains(expected),
        "refusal must explain {expected:?}: {}",
        text(&output)
    );
}

#[test]
#[ignore = "red until #489 adds rhei retry and its no-write refusals; implement removes this"]
fn retry_refuses_remaining_attempts_without_writes() {
    refuses_retry_without_writes("work", "attempts left", false, Some(1));
}

#[test]
#[ignore = "red until #489 adds rhei retry and its no-write refusals; implement removes this"]
fn retry_refuses_poll_state_without_writes() {
    refuses_retry_without_writes("waiting", "poll", false, Some(2));
}

#[test]
#[ignore = "red until #489 adds rhei retry and its no-write refusals; implement removes this"]
fn retry_refuses_terminal_state_without_writes() {
    refuses_retry_without_writes("completed", "terminal", false, Some(2));
}

#[test]
#[ignore = "red until #489 adds rhei retry and its no-write refusals; implement removes this"]
fn retry_refuses_missing_current_record_without_writes() {
    refuses_retry_without_writes("work", "spawn record", false, None);
}

#[test]
#[ignore = "red until #489 adds rhei retry and its no-write refusals; implement removes this"]
fn retry_refuses_inside_worker_without_writes() {
    refuses_retry_without_writes("work", "worker", true, Some(2));
}

fn refuses_release_flag(from: &str, to: &str, extra: &[&str]) {
    let (dir, _, _) = fixture();
    let before = artifacts(&dir);
    let mut args = vec!["--from", from, "--to", to, "--release-subtree"];
    args.extend_from_slice(extra);
    let task = if from == "work" { "plan.1.1" } else { "plan.1" };
    let output = cli(&dir, "transition", task, &args, false);
    assert!(!output.status.success());
    assert_eq!(artifacts(&dir), before);
    assert!(
        !text(&output).contains("unexpected argument"),
        "flag must be recognized: {}",
        text(&output)
    );
    assert!(text(&output).contains("--release-subtree"), "{}", text(&output));
}

#[test]
#[ignore = "red until #489 adds transition --release-subtree and its refusals; implement removes this"]
fn release_flag_refuses_non_supervising_edge_without_writes() {
    refuses_release_flag("work", "completed", &["--no-callbacks", "--result", "Done"]);
}

#[test]
#[ignore = "red until #489 adds transition --release-subtree and its refusals; implement removes this"]
fn release_flag_refuses_supervising_non_self_loop_without_writes() {
    refuses_release_flag("supervising", "cancelled", &["--no-callbacks", "--result", "Dropped"]);
}

#[test]
#[ignore = "red until #489 adds transition --release-subtree and its refusals; implement removes this"]
fn release_flag_refuses_force_without_writes() {
    refuses_release_flag(
        "supervising",
        "supervising",
        &["--force", "--reason", "operator release"],
    );
}
