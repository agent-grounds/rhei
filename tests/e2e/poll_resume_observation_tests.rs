//! Controlled observations of the resumed-poll regression's timing oracle.
//! §FS-rhei-run.3.5

use super::*;

fn unix_ns() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("Unix clock").as_nanos()
}

/// Port of triage's observer-only delay: keep the old plan, then let the real
/// exit-75 retry finish before reading its independent counter.
/// The plan is captured while stopped, before a verified pre-deadline restart.
/// That later clock check bounds the capture under the same nondecreasing-clock
/// assumption as invocation evidence; no callback clock sample is capture proof.
/// §FS-rhei-run.3.5
#[test]
fn a_lawful_retry_during_snapshot_observation_is_accepted() {
    resumed_poll_scenario(|workspace, deadline, captured_plan| {
        let mixed = workspace.snapshot_from_plan_with_gap(captured_plan, || {
            eprintln!("BOUNDARY observer_resumed_ns={} deadline={deadline}", unix_ns());
            wait_for("real attempt 2 and its exit-75 self-loop during observer delay", || {
                let after = workspace.snapshot();
                after.attempts == 2
                    && after.visits == Some(3)
                    && after.deadline.is_some_and(|next| next > deadline)
            });
            eprintln!("BOUNDARY snapshot_read_ns={}", unix_ns());
            // Publication is independent of the plan and counter. Print the
            // completed record for diagnosis, without repairing the oracle.
            let record =
                workspace.project.join("runtime/spawns/task-triage-tool-reports.1-triage.json");
            wait_for("completed second spawn record", || {
                fs::read_to_string(&record)
                    .ok()
                    .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
                    .is_some_and(|value| value["attempt"] == 2 && value["code"] == 75)
            });
            eprintln!("BOUNDARY completed_spawn={}", fs::read_to_string(record).unwrap());
        });
        assert_eq!(
            mixed.attempts, 2,
            "the delayed counter must include the real retry: {mixed:#?}"
        );
        assert_eq!(mixed.state, "triage", "the captured plan must remain polling: {mixed:#?}");
        assert_eq!(mixed.visits, Some(2), "keep the captured first-attempt plan: {mixed:#?}");
        assert_eq!(
            mixed.deadline,
            Some(deadline),
            "keep the original captured deadline: {mixed:#?}"
        );
        eprintln!("BOUNDARY mixed_snapshot={mixed:#?}");
        mixed
    });
}

/// Synthetic early-invocation evidence with a delayed observation exercises
/// the same oracle as the detached-run scenario. No early process is launched.
/// §FS-rhei-run.3.5
#[test]
fn an_early_retry_is_rejected_even_when_observed_after_the_deadline() {
    let workspace = PollWorkspace::new();
    let records = workspace.project.join("runtime/spawns");
    fs::create_dir_all(&records).expect("create synthetic record directory");
    let log = workspace.project.join("runtime/logs/task-triage-tool-reports.1-triage-attempt2.log");
    let worker = format!(
        "{} {}",
        super::super::python_command(),
        workspace._root.join("poll-program.py").display()
    );
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    fs::write(&log, format!("=== rhei program log v1 ===\nprogram: {worker}\ntask: triage-tool-reports.1\nstate: triage\n\n=== exit ===\ncode: 75\n===\n"))
        .unwrap();
    let record = serde_json::json!({
        "task": "triage-tool-reports.1", "state": "triage", "moves": 0,
        "attempt": 2, "charged": 2, "attempt_charged": true,
        "kind": "program", "worker": worker, "log": log,
        "started": "2020-01-01T00:00:09Z", "ended": "2020-01-01T00:00:11Z",
        "duration": "2s", "code": 75, "ending": "exited"
    });
    fs::write(records.join("task-triage-tool-reports.1-triage.json"), record.to_string()).unwrap();
    // Synthetic creation at :09, deadline :10, interpreter entry and
    // observation at :11. A late observer cannot erase the early evidence.
    let deadline = 1_577_836_810;
    let observed = PollSnapshot {
        attempts: 2,
        state: "triage".into(),
        deadline: Some(deadline + 5),
        visits: Some(3),
        run_log: String::new(),
        ledger: String::new(),
    };
    let rejected = std::panic::catch_unwind(|| {
        assert_retry_not_early(&workspace, deadline, deadline + 1, &observed);
    });
    let panic = rejected.expect_err(
        "the shared oracle accepted synthetic pre-deadline attempt 2 after a delayed observation",
    );
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        message.contains("early spawn"),
        "the negative control must reject invocation timing, not unrelated evidence: {message}"
    );
}
