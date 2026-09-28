#[test]
fn workspace_transition_updates_index_metadata_for_counted_loops() {
    let dir = unique_temp_dir("workspace-counted-loop");
    fs::create_dir_all(dir.join("tasks")).expect("create tasks dir");
    write_fixture_file(
        &dir,
        "index.rhei.md",
        r#"# Rhei: Workspace Counted Loop

## Overview
Metadata lives here.
"#,
    );
    write_fixture_file(
        &dir.join("tasks"),
        "01-review.md",
        r#"### Task 1: Review task
**State:** pending
"#,
    );
    let machine_path = write_fixture_file(&dir, "states.yaml", COUNTED_LOOP_STATE_MACHINE);

    let result = run_transition(&dir, &machine_path, "1", "pending", "agent-review");
    assert!(
        result.status.success(),
        "workspace transition should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let index_raw = fs::read_to_string(dir.join("index.rhei.md")).expect("read index");
    let index = parse_workspace_index(&index_raw).expect("parse index");
    assert_eq!(
        visit_count_from_metadata(index.metadata.as_ref(), &TaskId::number(1), "agent-review"),
        Some(1)
    );

    let task_raw = fs::read_to_string(dir.join("tasks/01-review.md")).expect("read task file");
    let tasks = rhei_core::parser::parse_workspace_tasks(&task_raw).expect("parse task file");
    assert_eq!(tasks[0].state, "agent-review");
}

#[test]
fn workspace_run_advances_tasks_to_completion() {
    let (_dir, ws, machine_path) = create_workspace(
        "ws-run",
        "# Rhei: Run Test\n",
        &[
            ("a.md", "### Task 1: Alpha\n**State:** pending\n"),
            ("b.md", "### Task 2: Beta\n**State:** pending\n**Prior:** Task 1\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = rhei_command()
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("run")
        .arg(&ws)
        .arg("--no-callbacks")
        .output()
        .expect("run command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "run should succeed\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );

    // Both tasks should reach completed.
    let loaded = workspace::load_workspace(&ws).expect("reload workspace");
    for task in &loaded.rhei.tasks {
        assert_eq!(task.state.as_str(), "completed", "Task {} should be completed", task.id);
    }

    assert!(stdout.contains("Run complete"));
}

#[test]
fn workspace_reset_restores_initial_states_and_removes_runtime() {
    let (_dir, ws, machine_path) = create_workspace(
        "ws-reset",
        "# Rhei: Reset Test\n",
        &[
            (
                "a.md",
                "### Task 1: Alpha\n**State:** completed\n\n#### Task 1.1: Detail\n**State:** in-progress\n",
            ),
            ("b.md", "### Task 2: Beta\n**State:** in-progress\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let runtime_dir = ws.join("runtime/logs");
    fs::create_dir_all(&runtime_dir).expect("create runtime dir");
    fs::write(runtime_dir.join("team.log"), "generated").expect("write runtime log");
    // The ledger is what records where each task started. §FS-rhei-reset.2.2
    fs::write(
        ws.join("runtime").join("state-transitions.log"),
        "workspace.1 pending@completed\nworkspace.1.1 pending@in-progress\n\
         workspace.2 pending@in-progress\n",
    )
    .expect("write ledger");

    let result = run_reset_command(&ws, &machine_path);

    assert!(
        result.status.success(),
        "workspace reset should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result
            .stdout
            .contains("Reset 2 task(s) (and 1 descendant task(s)) to their authored states."),
        "unexpected stdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Removed runtime output."),
        "expected runtime cleanup message, got:\n{}",
        result.stdout
    );

    let loaded = workspace::load_workspace(&ws).expect("reload workspace");
    assert_eq!(loaded.rhei.tasks[0].state.as_str(), "pending");
    assert_eq!(loaded.rhei.tasks[0].children[0].state.as_str(), "pending");
    assert_eq!(loaded.rhei.tasks[1].state.as_str(), "pending");
    assert!(!ws.join("runtime").exists(), "runtime directory should be removed");
}

#[test]
fn assignee_field_round_trips_through_parse_and_json() {
    let input = "# Rhei: Roundtrip\n\n\
## Tasks\n\n\
### Task 1: Alpha\n\
**State:** in-progress\n\
**Prior:** Task 2\n\
**Assignee:** alice\n\n\
### Task 2: Beta\n\
**State:** pending\n";

    let rhei = parse(input).expect("parse ok");

    let task1 = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("task 1");
    assert_eq!(task1.assignee.as_deref(), Some("alice"));
    let task2 = rhei.tasks.iter().find(|t| t.id == TaskId::number(2)).expect("task 2");
    assert_eq!(task2.assignee, None);

    let json = to_json_value(&rhei).expect("every frontmatter value has a JSON image");
    let tasks = json["tasks"].as_array().expect("tasks array");
    let t1 = tasks.iter().find(|t| t["id"]["path"].as_str() == Some("1")).expect("task 1 json");
    assert_eq!(t1["assignee"].as_str(), Some("alice"));
    let t2 = tasks.iter().find(|t| t["id"]["path"].as_str() == Some("2")).expect("task 2 json");
    assert!(t2.as_object().unwrap().get("assignee").is_none());

    let md = to_github_markdown(&rhei);
    assert!(md.contains("- Assignee: alice"), "expected assignee in markdown output:\n{md}");
}

#[test]
fn workspace_index_with_tasks_section_is_rejected() {
    let index = "# Rhei: Bad Index\n\n## Tasks\n\n### Task 1: Oops\n**State:** pending\n";
    let err = rhei_core::parser::parse_workspace_index(index)
        .expect_err("should reject Tasks section in index");
    assert!(
        err.message.contains("must not contain a '## Tasks' section"),
        "error: {}",
        err.message
    );
}
