fn write_firing_probe(dir: &Path, name: &str) -> PathBuf {
    write_python_agent(
        dir,
        name,
        r#"import json

phase = sys.argv[1] if len(sys.argv) > 1 else 'direct'
raw = sys.stdin.read()
context = json.loads(raw) if raw.strip() else {}
plan_value = pathlib.Path(env('RHEI_PLAN_PATH')) if env('RHEI_PLAN_PATH') else None
root = plan_value.parent if plan_value else pathlib.Path.cwd()
try:
    plan_text = plan_value.read_text(encoding='utf-8') if plan_value and plan_value.exists() else ''
except OSError:
    # Windows holds a mandatory exclusive lock on the plan file for the whole
    # on_leave subprocess call, since the atomic state write that releases it
    # has not happened yet; on_enter reads it after that write, unlocked.
    plan_text = ''
match = re.search(r'^\*\*State:\*\*\s+([^\n]+)', plan_text, re.MULTILINE)
ledger_path = root / 'runtime' / 'state-transitions.log'
journal_path = root / 'runtime' / 'transitions.log'
ledger = ledger_path.read_text(encoding='utf-8').splitlines() if ledger_path.exists() else []
journal = journal_path.read_text(encoding='utf-8').splitlines() if journal_path.exists() else []
transition = context.get('transition', {})
observation = {
    'phase': phase,
    'state': match.group(1).strip() if match else None,
    'context': context,
    'env_firing_id': os.environ.get('RHEI_TRANSITION_FIRING_ID'),
    'env_ledger_status': os.environ.get('RHEI_TRANSITION_LEDGER_STATUS'),
    'ledger': ledger,
    'journal': journal,
}
append(root / 'runtime' / 'callback-observations.jsonl', json.dumps(observation) + '\n')
if phase == 'enter':
    published = list(ledger)
    if transition.get('ledgerStatus') == 'pending':
        published.append('{} {}@{}'.format(
            context['task']['id'], transition['from'], transition['to']))
    write(root / 'runtime' / 'published-history.txt', '\n'.join(published) + ('\n' if published else ''))
if phase == 'redirect':
    print(json.dumps({'success': True, 'nextState': 'review'}))
else:
    print(json.dumps({'success': True}))
"#,
    )
}

fn callback_command(script: &Path, phase: &str) -> String {
    serde_json::to_string(&format!("cli:{} {phase}", fixture_command_line(script)))
        .expect("callback command should serialize as YAML")
}

fn read_callback_observations(dir: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(dir.join("runtime/callback-observations.jsonl"))
        .expect("callbacks should write observations")
        .lines()
        .map(|line| serde_json::from_str(line).expect("observation should be JSON"))
        .collect()
}

/// The scaffold separately proves the pre-existing state/write/ledger order;
/// this baseline is expected to pass before firing identity is implemented.
/// §FS-rhei-transition-cmd.3
#[test]
fn transition_firing_existing_order_baseline() {
    let dir = unique_temp_dir("callback-firing-order-baseline");
    let probe = write_firing_probe(&dir, "callback.py");
    let machine = format!(
        r#"name: callback-order-baseline
version: 1
states:
  pending:
    initial: true
  active: {{}}
  completed:
    final: true
transitions:
  - from: pending
    to: active
    on_leave: {leave}
    on_enter: {enter}
  - from: active
    to: completed
"#,
        leave = callback_command(&probe, "leave"),
        enter = callback_command(&probe, "enter"),
    );
    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Order\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n",
    );
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);
    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "active", &[]);
    assert!(result.status.success(), "order fixture should transition: {}", result.stderr);

    let observations = read_callback_observations(&dir);
    // Windows cannot read the plan file from on_leave: rhei still holds its
    // exclusive lock there, unlike on_enter, which runs after the write that
    // releases it.
    if cfg!(windows) {
        assert!(observations[0]["state"].is_null(), "leave cannot read the locked plan file");
    } else {
        assert_eq!(observations[0]["state"], "pending");
    }
    assert_eq!(observations[1]["state"], "active");
    assert!(observations.iter().all(|item| item["ledger"].as_array().unwrap().is_empty()));
    assert_eq!(
        fs::read_to_string(dir.join("runtime/state-transitions.log")).unwrap(),
        "plan.1 pending@active\n"
    );
}

/// A run callback can publish the committed prefix plus its pending firing,
/// including the final transition that is not in either ledger yet.
/// §FS-rhei-transitions.1.2 §FS-rhei-run-tui.1.7
#[test]
fn transition_firing_run_callbacks_publish_complete_history() {
    let dir = unique_temp_dir("callback-firing-run");
    let probe = write_firing_probe(&dir, "callback.py");
    let worker = write_python_agent(
        &dir,
        "worker.py",
        r#"write(pathlib.Path(env('RHEI_ROOT')) / 'runtime' / 'program-transition-env.json',
    __import__('json').dumps({
        'firing_id': os.environ.get('RHEI_TRANSITION_FIRING_ID'),
        'ledger_status': os.environ.get('RHEI_TRANSITION_LEDGER_STATUS'),
    }))
result('## Result\n\nThe program completed.\n')
"#,
    );
    let machine = format!(
        r#"name: callback-firing-run
version: 1
states:
  working:
    initial: true
    program:
      command: {worker}
  completed:
    final: true
transitions:
  - from: working
    to: completed
    exit_code: 0
    on_leave: {leave}
    on_enter: {enter}
"#,
        worker = fixture_command(&worker),
        leave = callback_command(&probe, "leave"),
        enter = callback_command(&probe, "enter"),
    );
    let plan = "# Rhei: Firing Run\n\n## Tasks\n\n### Task 1: Work\n**State:** working\n";
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let result = run_run_command(&plan_path, &machine_path, &[]);
    assert!(
        result.status.success(),
        "run fixture should complete before contract assertions\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let observations = read_callback_observations(&dir);
    assert_eq!(observations.len(), 2, "leave and enter should each be observed");
    let leave = &observations[0];
    let enter = &observations[1];
    let leave_id = leave["context"]["transition"]["firingId"]
        .as_str()
        .expect("on_leave must receive a nonempty transition.firingId");
    let enter_id = enter["context"]["transition"]["firingId"]
        .as_str()
        .expect("on_enter must receive transition.firingId");
    assert!(!leave_id.is_empty(), "firingId must be opaque but nonempty");
    assert_eq!(leave_id, enter_id, "one firing must keep one ID across callbacks");
    for observation in [&leave, &enter] {
        assert_eq!(observation["context"]["transition"]["ledgerStatus"], "pending");
        assert_eq!(observation["env_firing_id"], leave_id, "JSON and environment IDs differ");
        assert_eq!(observation["env_ledger_status"], "pending");
        assert!(
            !observation["ledger"].as_array().unwrap().iter().any(|line| {
                line.as_str().is_some_and(|line| line == "plan.1 working@completed")
            }),
            "the current central row must be absent during callbacks"
        );
        assert!(
            !observation["journal"].as_array().unwrap().iter().any(|line| {
                line.as_str().is_some_and(|line| line.contains("end@working"))
            }),
            "the invocation release must be absent during callbacks"
        );
    }
    // See the note above transition_firing_existing_order_baseline's
    // equivalent assertion: on_leave cannot read the still-locked plan file
    // on Windows.
    if cfg!(windows) {
        assert!(leave["state"].is_null(), "leave cannot read the locked plan file");
    } else {
        assert_eq!(leave["state"], "working");
    }
    assert_eq!(enter["state"], "completed");
    assert_eq!(leave["context"]["task"]["metadata"]["state"], "working");
    assert_eq!(enter["context"]["task"]["metadata"]["state"], "completed");

    let ledger = fs::read_to_string(dir.join("runtime/state-transitions.log")).expect("ledger");
    assert_eq!(ledger, "plan.1 working@completed\n", "ledger format must remain unchanged");
    let journal = fs::read_to_string(dir.join("runtime/transitions.log")).expect("journal");
    assert!(journal.lines().any(|line| line.contains("end@working")));
    let published =
        fs::read_to_string(dir.join("runtime/published-history.txt")).expect("published history");
    assert_eq!(
        published, "plan.1 working@completed\n",
        "on_enter must be able to publish history including its final pending transition"
    );
    let program_env: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("runtime/program-transition-env.json")).unwrap(),
    )
    .unwrap();
    assert!(program_env["firing_id"].is_null());
    assert!(program_env["ledger_status"].is_null());
}

/// Repeating a timestamp-free self-loop must yield a new identity even though
/// the second callback already sees an identical committed row.
/// §FS-rhei-transitions.1.2
#[test]
fn transition_firing_repeated_self_loops_have_distinct_ids() {
    let dir = unique_temp_dir("callback-firing-repeat");
    let probe = write_firing_probe(&dir, "callback.py");
    let machine = format!(
        r#"name: callback-firing-repeat
version: 1
states:
  waiting:
    initial: true
  completed:
    final: true
transitions:
  - from: waiting
    to: waiting
    on_enter: {enter}
  - from: waiting
    to: completed
"#,
        enter = callback_command(&probe, "enter"),
    );
    let plan = "# Rhei: Repeat\n\n## Tasks\n\n### Task 1: Wait\n**State:** waiting\n";
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    for _ in 0..2 {
        let result =
            run_transition_with_flags(&plan_path, &machine_path, "1", "waiting", "waiting", &[]);
        assert!(result.status.success(), "self-loop fixture should run: {}", result.stderr);
    }

    let observations = read_callback_observations(&dir);
    let first_id = observations[0]["context"]["transition"]["firingId"]
        .as_str()
        .expect("first firing ID");
    let second_id = observations[1]["context"]["transition"]["firingId"]
        .as_str()
        .expect("second firing ID");
    assert_ne!(first_id, second_id, "separate real firings need distinct IDs");
    assert_eq!(observations[0]["ledger"].as_array().unwrap().len(), 0);
    assert_eq!(
        observations[1]["ledger"].as_array().unwrap(),
        &[serde_json::Value::String("plan.1 waiting@waiting".into())],
        "the prior identical row is committed while the current one remains pending"
    );
}

/// Captured input replays its identity; a direct callback invocation receives
/// no identity manufactured by the callback process.
/// §FS-rhei-transitions.1.2
#[test]
fn transition_firing_replay_retains_id_and_direct_invocation_has_none() {
    let dir = unique_temp_dir("callback-firing-replay");
    let probe = write_firing_probe(&dir, "callback.py");
    let machine = format!(
        "name: replay\nversion: 1\nstates:\n  pending:\n    initial: true\n  active: {{}}\n  completed:\n    final: true\ntransitions:\n  - from: pending\n    to: active\n    on_enter: {}\n  - from: active\n    to: completed\n",
        callback_command(&probe, "enter")
    );
    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Replay\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n",
    );
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);
    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "active", &[]);
    assert!(result.status.success(), "capture fixture should transition: {}", result.stderr);
    let captured = read_callback_observations(&dir)[0]["context"].clone();
    let captured_id = captured["transition"]["firingId"].as_str().expect("captured firing ID");

    let mut replay = Command::new(python_command());
    replay
        .arg(&probe)
        .arg("replay")
        .current_dir(&dir)
        .env_remove("RHEI_TRANSITION_FIRING_ID")
        .env_remove("RHEI_TRANSITION_LEDGER_STATUS")
        .stdin(std::process::Stdio::piped());
    let mut child = replay.spawn().expect("replay callback");
    use std::io::Write as _;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&captured).unwrap().as_bytes())
        .unwrap();
    assert!(child.wait().expect("replay status").success());

    let direct = Command::new(python_command())
        .arg(&probe)
        .arg("direct")
        .current_dir(&dir)
        .env_remove("RHEI_TRANSITION_FIRING_ID")
        .env_remove("RHEI_TRANSITION_LEDGER_STATUS")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("direct callback");
    assert!(direct.status.success());
    let observations = read_callback_observations(&dir);
    assert_eq!(observations[1]["context"]["transition"]["firingId"], captured_id);
    assert!(observations[1]["env_firing_id"].is_null(), "replay does not mint an env ID");
    assert!(observations[2]["context"].as_object().unwrap().is_empty());
    assert!(observations[2]["env_firing_id"].is_null());
}
