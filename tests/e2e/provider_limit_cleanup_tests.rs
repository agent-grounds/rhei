//! Command-level cleanup of state-qualified provider waits.
//! §FS-rhei-run.3.3 §FS-rhei-reset.2

use super::provider_limit_support::*;
use super::*;

/// R1-08: moving out of the reporting state clears its wait and preserves
/// unrelated metadata. This starts from an actual classified refusal.
/// §FS-rhei-run.3.3
#[test]
fn provider_limit_transition_out_clears_only_wait_metadata() {
    let fixture = ProviderFixture::new(
        "provider-transition-cleanup",
        SINGLE_TASK,
        SIMPLE_MACHINE,
        &format!("print({LIMIT_SIGNAL:?}, file=sys.stderr, flush=True)\nraise SystemExit(1)\n"),
    );
    edit_metadata(&fixture.root, |m| {
        m["metadata"] = serde_json::json!({"note": "keep", "tasks": {"1": {"custom": "keep"}}});
    });
    let mut run = fixture.start(&[]);
    fixture.parked(&mut run);
    run.stop();
    let result = run_cli(
        "transition",
        &fixture.root,
        &fixture.machine,
        &[
            "--task",
            "workspace.1",
            "--from",
            "working",
            "--to",
            "completed",
            "--result",
            "Controlled transition after provider wait",
            "--no-callbacks",
        ],
    );
    assert_success(&result);
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "completed");
    let m = metadata(&fixture.root);
    assert!(m["metadata"]["tasks"]["1"]["providerLimits"].is_null());
    assert_eq!(m["metadata"]["note"], "keep");
    assert_eq!(m["metadata"]["tasks"]["1"]["custom"], "keep");
}

/// R1-08: a narrowed reset removes its own runtime fields and artifacts while
/// keeping the other rhei's wait and shared run-owned output byte-for-byte.
/// §FS-rhei-reset.2 §FS-rhei-reset.2.1 §FS-rhei-run.3.3
#[test]
fn provider_limit_scoped_reset_preserves_other_rhei_and_shared_runtime() {
    let dir = unique_temp_dir("provider-scoped-reset");
    let project = dir.join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("index.panta.md"), "# Panta: Provider cleanup\n").unwrap();
    let machine = dir.join("states.yaml");
    fs::write(&machine, SIMPLE_MACHINE).unwrap();
    let fields = serde_json::json!({"metadata": {
        "note": "project data",
        "tasks": {"1": {
            "providerLimits": {"working": limit_record(epoch_now() + 3600)},
            "stateVisits": {"working": 2},
            "supervision": {"released": []},
            "custom": "keep task data"
        }}
    }});
    let plan = format!(
        "# Rhei: Parked\n---\n{}---\n\n## Tasks\n\n### Task 1: Parked task\n**State:** working\n",
        yaml_metadata(&fields)
    );
    for name in ["alpha", "beta"] {
        fs::write(project.join(format!("{name}.rhei.md")), &plan).unwrap();
        fs::create_dir_all(project.join("runtime/results")).unwrap();
        fs::create_dir_all(project.join("runtime/logs")).unwrap();
        fs::write(project.join(format!("runtime/results/{name}.1.md")), "retained result").unwrap();
        fs::write(project.join(format!("runtime/logs/task-{name}.1-working.log")), "retained log")
            .unwrap();
    }
    fs::write(project.join("runtime/run-report.md"), "shared run report").unwrap();
    fs::write(
        project.join("runtime/state-transitions.log"),
        "alpha.1 working@working\nbeta.1 working@working\n",
    )
    .unwrap();
    let result = run_cli("reset", &project, &machine, &["--rhei", "alpha", "--yes"]);
    assert_success(&result);
    let alpha = fs::read_to_string(project.join("alpha.rhei.md")).unwrap();
    let alpha = serde_json::to_value(rhei_core::parse(&alpha).unwrap().metadata).unwrap();
    assert!(alpha["metadata"]["tasks"]["1"]["providerLimits"].is_null());
    assert!(alpha["metadata"]["tasks"]["1"]["stateVisits"].is_null());
    assert!(alpha["metadata"]["tasks"]["1"]["supervision"].is_null());
    assert_eq!(alpha["metadata"]["tasks"]["1"]["custom"], "keep task data");
    assert_eq!(alpha["metadata"]["note"], "project data");
    assert_eq!(fs::read_to_string(project.join("beta.rhei.md")).unwrap(), plan);
    assert!(!project.join("runtime/results/alpha.1.md").exists());
    assert!(!project.join("runtime/logs/task-alpha.1-working.log").exists());
    assert_eq!(
        fs::read_to_string(project.join("runtime/results/beta.1.md")).unwrap(),
        "retained result"
    );
    assert_eq!(
        fs::read_to_string(project.join("runtime/logs/task-beta.1-working.log")).unwrap(),
        "retained log"
    );
    assert_eq!(
        fs::read_to_string(project.join("runtime/run-report.md")).unwrap(),
        "shared run report"
    );
    assert_eq!(
        fs::read_to_string(project.join("runtime/state-transitions.log")).unwrap(),
        "beta.1 working@working\n"
    );
}
