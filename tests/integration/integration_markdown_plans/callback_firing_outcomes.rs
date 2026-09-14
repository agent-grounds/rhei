/// Model fan-out and a redirected enter callback share one firing identity,
/// while the effective target is reflected in the enter context and ledger.
/// §FS-rhei-transitions.1.2
#[test]
fn transition_firing_model_fanout_and_redirect_share_identity() {
    let dir = unique_temp_dir("callback-firing-fanout");
    let probe = write_firing_probe(&dir, "callback.py");
    let machine = format!(
        r#"name: callback-firing-fanout
version: 1
models: [first, second]
states:
  pending:
    initial: true
    all_models: [first, second]
  active: {{}}
  review: {{}}
  completed:
    final: true
transitions:
  - from: pending
    to: active
    on_leave: {redirect}
  - from: pending
    to: review
    on_enter: {enter}
  - from: active
    to: completed
  - from: review
    to: completed
"#,
        redirect = callback_command(&probe, "redirect"),
        enter = callback_command(&probe, "enter"),
    );
    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Fanout\n\n## Tasks\n\n### Task 1: Review\n**State:** pending\n",
    );
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);
    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "active", &[]);
    assert!(result.status.success(), "redirect fixture should succeed: {}", result.stderr);

    let observations = read_callback_observations(&dir);
    assert_eq!(observations.len(), 3, "two leave models and one enter callback");
    let ids: Vec<_> = observations
        .iter()
        .map(|value| value["context"]["transition"]["firingId"].as_str().expect("firing ID"))
        .collect();
    assert!(ids.iter().all(|id| *id == ids[0]), "fan-out and redirect must share one ID");
    assert_eq!(observations[2]["context"]["transition"]["to"], "review");
    assert_eq!(
        fs::read_to_string(dir.join("runtime/state-transitions.log")).unwrap(),
        "plan.1 pending@review\n"
    );
}

/// Rejected leave and failed enter attempts may spend identities, but neither
/// may append the central ledger row and enter failure still rolls back.
/// §FS-rhei-transitions.1.2
#[test]
fn transition_firing_failed_attempts_have_identity_without_ledger_rows() {
    for (case, phase, callback_slot) in
        [("reject", "reject", "on_leave"), ("rollback", "fail", "on_enter")]
    {
        let dir = unique_temp_dir(&format!("callback-firing-{case}"));
        let probe = write_python_agent(
            &dir,
            "outcome.py",
            r#"import json
context = json.loads(sys.stdin.read())
root = pathlib.Path(env('RHEI_PLAN_PATH')).parent
append(root / 'runtime' / 'outcome-contexts.jsonl', json.dumps({
    'context': context,
    'env_id': os.environ.get('RHEI_TRANSITION_FIRING_ID'),
    'env_status': os.environ.get('RHEI_TRANSITION_LEDGER_STATUS'),
}) + '\n')
if sys.argv[1] == 'reject':
    print(json.dumps({'success': False, 'error': 'expected rejection'}))
else:
    print('expected enter failure', file=sys.stderr)
    sys.exit(1)
"#,
        );
        let callback = callback_command(&probe, phase);
        let machine = format!(
            "name: outcome\nversion: 1\nstates:\n  pending:\n    initial: true\n  active: {{}}\n  completed:\n    final: true\ntransitions:\n  - from: pending\n    to: active\n    {callback_slot}: {callback}\n  - from: active\n    to: completed\n"
        );
        let original = "# Rhei: Outcome\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n";
        let plan_path = write_fixture_file(&dir, "plan.rhei.md", original);
        let machine_path = write_fixture_file(&dir, "states.yaml", &machine);
        let result =
            run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "active", &[]);
        assert!(!result.status.success(), "{case} fixture must fail the transition");
        let raw = fs::read_to_string(dir.join("runtime/outcome-contexts.jsonl")).unwrap();
        let context: serde_json::Value = serde_json::from_str(raw.trim()).unwrap();
        let id = context["context"]["transition"]["firingId"]
            .as_str()
            .expect("failed attempt still receives a firing ID");
        assert!(!id.is_empty());
        assert_eq!(context["context"]["transition"]["ledgerStatus"], "pending");
        assert_eq!(context["env_id"], id);
        assert_eq!(context["env_status"], "pending");
        assert_eq!(fs::read_to_string(&plan_path).unwrap(), original, "state must be preserved");
        assert!(!dir.join("runtime/state-transitions.log").exists());
    }
}

/// Concurrent commands for distinct tasks receive independent identities and
/// retain exactly one existing-format central row per successful transition.
/// §FS-rhei-transitions.1.2
#[test]
fn transition_firing_concurrent_tasks_have_distinct_ids() {
    let dir = unique_temp_dir("callback-firing-concurrent");
    let probe = write_firing_probe(&dir, "callback.py");
    let machine = format!(
        "name: concurrent\nversion: 1\nstates:\n  pending:\n    initial: true\n  active: {{}}\n  completed:\n    final: true\ntransitions:\n  - from: pending\n    to: active\n    on_enter: {}\n  - from: active\n    to: completed\n",
        callback_command(&probe, "enter")
    );
    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Concurrent\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n\n### Task 2: Two\n**State:** pending\n",
    );
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let spawn = |task: &'static str| {
        let plan_path = plan_path.clone();
        let machine_path = machine_path.clone();
        std::thread::spawn(move || {
            run_transition_with_flags(
                &plan_path,
                &machine_path,
                task,
                "pending",
                "active",
                &[],
            )
        })
    };
    let one_handle = spawn("1");
    let two_handle = spawn("2");
    let one = one_handle.join().unwrap();
    let two = two_handle.join().unwrap();
    assert!(one.status.success(), "task 1: {}", one.stderr);
    assert!(two.status.success(), "task 2: {}", two.stderr);

    let observations = read_callback_observations(&dir);
    assert_eq!(observations.len(), 2);
    let first = observations[0]["context"]["transition"]["firingId"]
        .as_str()
        .expect("first identity");
    let second = observations[1]["context"]["transition"]["firingId"]
        .as_str()
        .expect("second identity");
    assert_ne!(first, second);
    let ledger = fs::read_to_string(dir.join("runtime/state-transitions.log")).unwrap();
    assert_eq!(ledger.lines().count(), 2);
    assert_eq!(ledger.matches("plan.1 pending@active").count(), 1);
    assert_eq!(ledger.matches("plan.2 pending@active").count(), 1);
}
