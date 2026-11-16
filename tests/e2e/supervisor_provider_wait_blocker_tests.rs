//! Priors and nested holds keep their precedence over an owed visit's own blockers.
//! §FS-rhei-run-report.3.1 §FS-rhei-supervision.3.2 §FS-rhei-run.5.1

use super::provider_limit_support::*;
use super::supervisor_provider_wait_support::{fixture, seed_wait, MACHINE};
use super::*;

fn gate_machine() -> String {
    MACHINE
        .replace("  work:\n", "  review:\n    gating: true\n  work:\n")
        .replace("transitions:\n", "transitions:\n  - { from: review, to: completed }\n")
}

fn assert_idle(
    fixture: &ProviderFixture,
    parallel: bool,
    blocker: &str,
    next_attempt_at: Option<&str>,
    states: &[(&str, &str)],
) {
    let mut args = vec!["--until-idle"];
    if parallel {
        args.extend(["--parallel", "2"]);
    }
    let mut run = fixture.start(&args);
    let status = run.wait_for_exit("the supervisor's classified idle wait");
    let output = fixture.output();
    let events = fixture.events();
    let started = events
        .iter()
        .find(|event| event["event"] == "run_started")
        .unwrap_or_else(|| panic!("missing start event: {output}"));
    // Two task files prevent a requested parallel run from falling back to sequential.
    assert_eq!(started["parallel"], if parallel { 2 } else { 1 }, "{output}");
    assert_eq!(status.code(), Some(3), "{output}");
    let finished = events
        .iter()
        .find(|event| event["event"] == "run_finished")
        .unwrap_or_else(|| panic!("missing finish event: {output}"));
    assert_eq!(
        finished["summary"]["stop"],
        serde_json::json!({
            "reason": "idle", "exit_code": 3, "idle_blocker": blocker,
            "next_attempt_at": next_attempt_at,
        }),
        "{output}"
    );
    assert!(!events.iter().any(|event| event["event"] == "task_spawned"), "{output}");
    assert_eq!(fs::read_to_string(fixture.root.join("starts")).unwrap_or_default(), "");
    for (id, state) in states {
        assert_task_state(&fixture.root, &fixture.machine, id, state);
    }
}

// A gated prior answers before missing input, and neither input variant admits a retry.
// §FS-rhei-run-report.3.1 §FS-rhei-run.5.1
fn gated_prior(parallel: bool) {
    for input_exists in [false, true] {
        let fixture = fixture();
        let machine = gate_machine().replace("  work:\n", "  work:\n    gating: true\n").replace(
            "    visits: 6\n",
            "    visits: 6\n    inputs:\n      - name: brief\n        path: runtime/brief.md\n",
        );
        fs::write(&fixture.machine, machine).unwrap();
        let supervisor_path = fixture.root.join("tasks/01-supervisor.md");
        let supervisor = fs::read_to_string(&supervisor_path).unwrap();
        fs::write(
            supervisor_path,
            supervisor
                .replace("**State:** supervising", "**State:** supervising\n**Prior:** Task 2"),
        )
        .unwrap();
        fs::write(
            fixture.root.join("tasks/02-prior.md"),
            "### Task 2: Human decision\n**State:** review\n",
        )
        .unwrap();
        let deadline = seed_wait(&fixture);
        if input_exists {
            fs::create_dir_all(fixture.root.join("runtime")).unwrap();
            fs::write(fixture.root.join("runtime/brief.md"), "required input\n").unwrap();
        }
        assert_idle(
            &fixture,
            parallel,
            "gate",
            None,
            &[("1", "supervising"), ("1.1", "work"), ("2", "review")],
        );
        let metadata = metadata(&fixture.root);
        assert_eq!(
            metadata["metadata"]["tasks"]["1"]["providerLimits"]["supervising"]["nextAttemptAt"],
            deadline
        );
        assert_ne!(metadata["metadata"]["tasks"]["1"]["supervision"]["phase"], "released");
    }
}

#[test]
fn supervisor_provider_wait_gated_prior_input_precedence_sequential() {
    gated_prior(false);
}

#[test]
fn supervisor_provider_wait_gated_prior_input_precedence_parallel() {
    gated_prior(true);
}

// A held gate inherits its outer holder's provider wait, as does the gate's held child.
// §FS-rhei-run-report.3.1 §FS-rhei-supervision.3.2
fn nested_holder(parallel: bool) {
    for with_leaf in [false, true] {
        let fixture = fixture();
        fs::write(&fixture.machine, gate_machine()).unwrap();
        let mut supervisor = String::from(
            "### Task 1: Outer supervisor\n**State:** supervising\n\n\
             #### Task 1.1: Gate holding its subtree\n**State:** review\n",
        );
        let mut states = vec![("1", "supervising"), ("1.1", "review"), ("2", "completed")];
        if with_leaf {
            supervisor.push_str("\n##### Task 1.1.1: Work\n**State:** work\n");
            states.push(("1.1.1", "work"));
        }
        fs::write(fixture.root.join("tasks/01-supervisor.md"), supervisor).unwrap();
        fs::write(
            fixture.root.join("tasks/02-completed.md"),
            "### Task 2: Already complete\n**State:** completed\n",
        )
        .unwrap();
        let deadline = seed_wait(&fixture);
        edit_metadata(&fixture.root, |m| {
            m["structure"] = serde_json::json!({"maxLevels": 3});
            m["metadata"]["tasks"]["1.1"] = serde_json::json!({"supervision": {"phase": "held"}});
        });
        assert_idle(&fixture, parallel, "provider_limit", Some(&deadline), &states);
        let metadata = metadata(&fixture.root);
        assert_eq!(metadata["metadata"]["tasks"]["1.1"]["supervision"]["phase"], "held");
        assert_ne!(metadata["metadata"]["tasks"]["1"]["supervision"]["phase"], "released");
    }
}

#[test]
fn supervisor_provider_wait_nested_gate_holder_sequential() {
    nested_holder(false);
}

#[test]
fn supervisor_provider_wait_nested_gate_holder_parallel() {
    nested_holder(true);
}

// Without an outer holder, the retained gate itself holds the subtree. §FS-rhei-supervision.3.1
fn ordinary_gate_holder(parallel: bool) {
    let fixture = fixture();
    fs::write(&fixture.machine, gate_machine()).unwrap();
    fs::write(
        fixture.root.join("tasks/01-supervisor.md"),
        "### Task 1: Gate holding its subtree\n**State:** review\n\n\
         #### Task 1.1: Held work\n**State:** work\n",
    )
    .unwrap();
    fs::write(
        fixture.root.join("tasks/02-completed.md"),
        "### Task 2: Already complete\n**State:** completed\n",
    )
    .unwrap();
    edit_metadata(&fixture.root, |m| {
        m["metadata"] = serde_json::json!({"tasks": {"1": {"supervision": {"phase": "held"}}}});
    });
    assert_idle(
        &fixture,
        parallel,
        "gate",
        None,
        &[("1", "review"), ("1.1", "work"), ("2", "completed")],
    );
    assert_eq!(metadata(&fixture.root)["metadata"]["tasks"]["1"]["supervision"]["phase"], "held");
}

#[test]
fn supervisor_provider_wait_ordinary_gate_holder_sequential() {
    ordinary_gate_holder(false);
}

#[test]
fn supervisor_provider_wait_ordinary_gate_holder_parallel() {
    ordinary_gate_holder(true);
}
