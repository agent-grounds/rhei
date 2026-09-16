//! Fanout and live-pool provider scheduling regressions. §FS-rhei-run.3.3

use super::provider_limit_support::*;
use super::*;

fn fanout_machine(second: &str) -> String {
    SIMPLE_MACHINE.replace(
        "    target: codex:openai:alpha",
        &format!("    all_targets:\n      - codex:openai:alpha\n      - {second}"),
    ).replace("    attempts: 1", "    attempts: 1\n    outputs:\n      - name: completed-invocation\n        path: runtime/fanout/{model.name}.txt")
}

/// R1-02: beta finishes only after alpha's future wait is persisted. Its
/// successful exit must not clear that wait or launch alpha early. §FS-rhei-run.3.3
#[test]
fn provider_limit_survives_successful_inflight_fanout_sibling() {
    let fixture = ProviderFixture::new(
        "provider-fanout-retention",
        SINGLE_TASK,
        &fanout_machine("codex:openai:beta"),
        &format!(
            r#"root = pathlib.Path(env('RHEI_ROOT'))
model = env('RHEI_MODEL_NAME')
append(root/(model + '-starts'), env('RHEI_ATTEMPT') + '\n')
if model == 'alpha' and env('RHEI_ATTEMPT') == '1':
    await_marker(root/'beta-starts')
    print({LIMIT_SIGNAL:?}, file=sys.stderr, flush=True)
    raise SystemExit(1)
if model == 'beta':
    await_marker(root/'release-beta')
write(root/'runtime/fanout'/(model + '.txt'), 'done')
result('## Result\n\nDone.\n')
"#
        ),
    );
    let mut run = fixture.start(&[]);
    fixture.parked(&mut run);
    let original = metadata(&fixture.root)["metadata"]["tasks"]["1"]["providerLimits"].clone();
    fs::write(fixture.root.join("release-beta"), "go").unwrap();
    wait_for("beta's successful slot release", || {
        fixture.events().iter().any(|e| e["event"] == "slot_released" && e["exit_code"] == 0)
    });
    assert_stays_parked(&fixture, &mut run);
    assert_eq!(metadata(&fixture.root)["metadata"]["tasks"]["1"]["providerLimits"], original);
    assert_eq!(fs::read_to_string(fixture.root.join("alpha-starts")).unwrap(), "1\n");
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
    expire_provider_deadlines(&fixture.root);
    fixture.finish_success(&mut run);
    assert_eq!(fs::read_to_string(fixture.root.join("alpha-starts")).unwrap(), "1\n2\n");
    assert_eq!(fs::read_to_string(fixture.root.join("beta-starts")).unwrap(), "1\n");
    assert!(!markdown_text(&fixture.root).contains("providerLimits:"));
}

/// R1-03: a durable wait suppresses only its resolved identity, including
/// after another run completed the unrelated part of the fanout. §FS-rhei-run.3.3
#[test]
fn provider_limit_restart_runs_unrelated_fanout_identity() {
    let fixture = ProviderFixture::new(
        "provider-mixed-restart",
        SINGLE_TASK,
        &fanout_machine("mock:mock:beta"),
        r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root/(env('RHEI_MODEL_NAME') + '-starts'), env('RHEI_ATTEMPT') + '\n')
write(root/'runtime/fanout'/(env('RHEI_MODEL_NAME') + '.txt'), 'done')
result('## Result\n\nDone.\n')
"#,
    );
    edit_metadata(&fixture.root, |m| {
        m["metadata"] = serde_json::json!({"tasks": {"1": {
            "providerLimits": {"working": limit_record(epoch_now() + 3600)}
        }}});
    });
    let mut first = fixture.start(&[]);
    wait_for("unrelated beta to complete before expiry", || {
        fixture.events().iter().any(|e| e["event"] == "slot_released" && e["exit_code"] == 0)
    });
    assert_stays_parked(&fixture, &mut first);
    assert!(!fixture.root.join("alpha-starts").exists());
    first.stop();
    let mut restarted = fixture.start(&[]);
    assert_stays_parked(&fixture, &mut restarted);
    assert!(!fixture.root.join("alpha-starts").exists());
    assert_eq!(fs::read_to_string(fixture.root.join("beta-starts")).unwrap(), "1\n");
    expire_provider_deadlines(&fixture.root);
    fixture.finish_success(&mut restarted);
    assert_eq!(fs::read_to_string(fixture.root.join("alpha-starts")).unwrap(), "1\n");
    assert_eq!(fs::read_to_string(fixture.root.join("beta-starts")).unwrap(), "1\n");
    assert!(!markdown_text(&fixture.root).contains("providerLimits:"));
}

fn concurrent_machine() -> String {
    SIMPLE_MACHINE.replace("    initial: true", "    initial: true\n    concurrent: true")
        .replace("  completed:", "  unrelated:\n    concurrent: true\n    target: mock:mock:beta\n    attempts: 1\n  completed:")
        + "  - from: unrelated\n    to: completed\n"
}

/// R1-04: a real future deadline expires while an unrelated subprocess remains
/// alive; the pool must resume the refused invocation in the free slot.
/// §FS-rhei-run.3.3 §FS-rhei-run.5.1
#[test]
fn provider_limit_deadline_refills_while_unrelated_worker_runs() {
    let fixture = ProviderFixture::new(
        "provider-inflight-deadline",
        &[
            ("01.md", "### Task 1: Limited\n**State:** working\n"),
            ("02.md", "### Task 2: Unrelated\n**State:** unrelated\n"),
        ],
        &concurrent_machine(),
        &format!(
            r#"root = pathlib.Path(env('RHEI_ROOT'))
model = env('RHEI_MODEL_NAME')
append(root/(model + '-starts'), env('RHEI_ATTEMPT') + '\n')
if model == 'alpha' and env('RHEI_ATTEMPT') == '1':
    await_marker(root/'beta-starts')
    print({LIMIT_SIGNAL:?}, file=sys.stderr, flush=True)
    raise SystemExit(1)
if model == 'beta':
    await_marker(root/'release-beta')
result('## Result\n\nDone.\n')
write(root/(model + '-finished'), 'done')
"#
        ),
    );
    let mut run = fixture.start(&["--parallel", "2"]);
    fixture.parked(&mut run);
    edit_metadata(&fixture.root, |m| {
        m["metadata"]["tasks"]["1"]["providerLimits"]["working"]["nextAttemptAt"] =
            utc_at(epoch_now() + 3).into();
    });
    wait_for("alpha to resume while beta is alive", || {
        fixture.root.join("alpha-finished").exists()
    });
    assert!(!fixture.root.join("beta-finished").exists());
    assert!(run.child().try_wait().unwrap().is_none());
    assert_eq!(fs::read_to_string(fixture.root.join("alpha-starts")).unwrap(), "1\n2\n");
    fs::write(fixture.root.join("release-beta"), "go").unwrap();
    fixture.finish_success(&mut run);
}

/// R1-06: two flag settings exercise queued suppression, unrelated refill,
/// running sibling continuity, capacity, JSON fields and eventual resumption.
/// §FS-rhei-run.3.3 §FS-rhei-run-json.2.1
#[test]
fn provider_limit_scheduling_preserves_work_with_both_error_flags() {
    for continue_on_error in [false, true] {
        let fixture = ProviderFixture::new(
            "provider-queue-flags",
            &[
                ("01.md", "### Task 1: Limited\n**State:** working\n"),
                ("02.md", "### Task 2: In flight\n**State:** working\n"),
                ("03.md", "### Task 3: Queued\n**State:** working\n"),
                ("04.md", "### Task 4: Unrelated\n**State:** unrelated\n"),
            ],
            &concurrent_machine(),
            &format!(
                r#"root = pathlib.Path(env('RHEI_ROOT'))
local = env('RHEI_TASK_ID_LOCAL')
append(root/(local + '-starts'), env('RHEI_ATTEMPT') + '\n')
if local == '1':
    if env('RHEI_ATTEMPT') == '1':
        await_marker(root/'2-starts')
        print({LIMIT_SIGNAL:?}, file=sys.stderr, flush=True)
        raise SystemExit(1)
    await_marker(root/'release-resumed')
if local == '2':
    await_marker(root/'release-sibling')
result('## Result\n\nDone.\n')
write(root/(local + '-finished'), 'done')
"#
            ),
        );
        let mut args = vec!["--parallel", "2"];
        if continue_on_error {
            args.push("--continue-on-error");
        }
        let mut run = fixture.start(&args);
        fixture.parked(&mut run);
        wait_for("unrelated task to refill the slot", || {
            fixture
                .events()
                .iter()
                .any(|e| e["event"] == "slot_released" && e["task"] == "workspace.4")
        });
        assert!(!fixture.root.join("2-finished").exists());
        assert!(!fixture.root.join("3-starts").exists());
        let events = fixture.events();
        let limited =
            events.iter().filter(|e| e["outcome"] == "provider_limited").collect::<Vec<_>>();
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0]["provider"], "openai");
        assert_eq!(limited[0]["task"], "workspace.1");
        assert_eq!(limited[0]["from"], "working");
        assert_eq!(
            limited[0]["next_attempt_at"],
            metadata(&fixture.root)["metadata"]["tasks"]["1"]["providerLimits"]["working"]
                ["nextAttemptAt"]
        );
        assert!(!events.iter().any(|e| e["event"] == "run_finished"));
        expire_provider_deadlines(&fixture.root);
        wait_for("resumed task to claim the free slot", || {
            fs::read_to_string(fixture.root.join("1-starts")).unwrap().lines().count() == 2
        });
        assert!(!fixture.root.join("3-starts").exists(), "two live tasks consume capacity");
        assert!(!fixture.root.join("2-finished").exists());
        fs::write(fixture.root.join("release-resumed"), "go").unwrap();
        wait_for("queued identity to resume", || fixture.root.join("3-finished").exists());
        fs::write(fixture.root.join("release-sibling"), "go").unwrap();
        fixture.finish_success(&mut run);
        assert!(!markdown_text(&fixture.root).contains("providerLimits:"));
        assert_eq!(fixture.events().iter().filter(|e| e["event"] == "run_finished").count(), 1);
    }
}
