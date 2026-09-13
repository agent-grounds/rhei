// §AR-source-file-size.3: directory workspaces — how one is loaded, validated,
// and written back. Projects, diagnostics, `panta`, and `rhei init` are
// siblings; shared fixtures live in `common.rs`.
/// A path broken across lines cannot be copied, clicked, or grepped. The
/// slash-and-hyphen-heavy fixture keeps the selected spelling longer than the
/// renderer's wrap width; the assertion must accept the shorter of its relative
/// and absolute spellings, intact on one physical line. §FS-rhei-validate.4.2
#[test]
fn diagnostics_never_break_a_file_path_across_lines() {
    let root = unique_temp_dir("diag-long-path");
    let deep = root
        .join("a-directory-with-hyphens")
        .join("and-another-long-segment")
        .join("plus-one-more-to-overflow-the-wrap-column");
    fs::create_dir_all(&deep).expect("create nested dirs");
    let plan_path = write_fixture_file(&deep, "broken.rhei.md", "# Not A Rhei Heading\n");

    let output = rhei_command()
        .arg("validate")
        .arg(&plan_path)
        .output()
        .expect("validate command should run");
    assert!(!output.status.success(), "the malformed plan must fail validation");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let invocation_dir = std::env::current_dir().expect("read test invocation directory");
    let rendered_path = stderr
        .lines()
        .skip_while(|line| line.trim() != "│ in")
        .nth(1)
        .map(str::trim)
        .and_then(|line| line.strip_prefix("│ "))
        .expect("parse diagnostic should name its file after `in`");
    eprintln!("invocation directory: {}", invocation_dir.display());
    eprintln!("fixture root: {}", root.display());
    eprintln!("absolute path ({} chars): {}", plan_path.as_os_str().len(), plan_path.display());
    eprintln!("selected diagnostic path ({} chars): {rendered_path}", rendered_path.len());
    eprintln!("validation exit: {:?}", output.status.code());
    eprintln!("parse error present: {}", stderr.contains("PARSE ERROR"));
    let wanted = plan_path.display().to_string();
    assert!(
        stderr.lines().any(|line| line.contains(&wanted)),
        "the path must appear intact on one line so it stays copy-pasteable.\n\
         wanted: {wanted}\ngot:\n{stderr}"
    );
}

/// §FS-rhei-plan-language.1.2: one freshly created workspace directory used to
/// fail the whole project's load with an error that named no file at all.
#[test]
fn empty_workspace_rhei_does_not_break_the_project_and_is_warned_about() {
    let project = create_panta_project(
        "panta-empty-rhei",
        "# Panta: Product Suite\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            ("growth/index.rhei.md", "# Rhei: Growth\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded =
        workspace::load_panta_project(&project).expect("empty rhei must not fail the load");
    assert_eq!(loaded.rhei_ids, vec!["auth", "growth"]);
    assert_eq!(loaded.rhei.tasks.len(), 1, "the sibling rhei's tickets must still load");

    let output = rhei_command()
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    assert!(
        output.status.success(),
        "an empty rhei is valid\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("rhei 'growth' holds no tickets"),
        "validate must name the empty rhei so a mistyped `tasks/` is not silent, got:\n{combined}"
    );
}

/// The duplicate-id diagnostic used to be masked by the empty-workspace error,
/// so a collision surfaced as an unattributed "workspace contains no tasks".
#[test]
fn duplicate_rhei_id_is_reported_even_when_one_side_is_empty() {
    let project = create_panta_project(
        "panta-dup-empty",
        "# Panta: Product Suite\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            ("auth/index.rhei.md", "# Rhei: Auth Dir\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let error = workspace::load_panta_project(&project).expect_err("duplicate ids must fail");
    assert!(
        error.message.contains("duplicate rhei id 'auth'"),
        "expected the collision to surface, got:\n{}",
        error.message
    );
}

#[test]
fn workspace_loads_and_validates_correctly() {
    let (_dir, ws, machine_path) = create_workspace(
        "ws-valid",
        "# Rhei: Workspace Test\n\n## Context\nSome context here.\n",
        &[
            ("alpha.md", "### Task 1: Alpha\n**State:** pending\n\nAlpha description.\n"),
            (
                "beta.md",
                "### Task 2: Beta\n**State:** completed\n**Prior:** Task 1\n\nBeta description.\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    // is_workspace detection
    assert!(workspace::is_workspace(&ws));

    // load_workspace produces merged plan
    let loaded = workspace::load_workspace(&ws).expect("load workspace");
    assert_eq!(loaded.rhei.title, "Workspace Test");
    assert_eq!(loaded.rhei.tasks.len(), 2);
    assert_eq!(loaded.task_sources.len(), 2);
    assert!(loaded.task_sources.contains_key("1"));
    assert!(loaded.task_sources.contains_key("2"));

    // CLI validate succeeds
    let output = rhei_command()
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("validate")
        .arg(&ws)
        .output()
        .expect("validate command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "validate should succeed\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Validation succeeded"));
}

#[test]
fn workspace_validate_accumulates_parse_errors_across_task_files() {
    let (_dir, ws, machine_path) = create_workspace(
        "ws-parse-errors",
        "# Rhei: Workspace Parse Errors\n",
        &[
            (
                "a.md",
                "### Task 1: Missing state\n\n### Task 2: Valid fallback\n**State:** pending\n",
            ),
            (
                "b.md",
                "### Task 3: Bad state field\n**State** pending\n\n### Task 4: Valid fallback\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = rhei_command()
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("validate")
        .arg(&ws)
        .output()
        .expect("validate command should run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "validate should fail\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        stderr
    );
    assert!(stderr.contains("PARSE ERROR"), "expected parse header, got:\n{stderr}");
    assert!(stderr.contains("2 problems"), "expected problem count, got:\n{stderr}");
    assert!(stderr.contains("a.md"), "expected first task file, got:\n{stderr}");
    assert!(stderr.contains("b.md"), "expected second task file, got:\n{stderr}");
    assert!(stderr.contains("line 1"), "expected first line hint, got:\n{stderr}");
    assert!(stderr.contains("line 2"), "expected second line hint, got:\n{stderr}");
    assert!(
        stderr.contains("missing mandatory **State:**")
            && stderr.contains("Malformed metadata field"),
        "expected both parse errors, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("VALIDATION ERROR"),
        "parse failures should not fall through to validation output, got:\n{stderr}"
    );
}

#[test]
fn validate_and_list_accept_workspace_index_file_path() {
    let (_dir, ws, machine_path) = create_workspace(
        "ws-index-path",
        "# Rhei: Workspace Index Path\n\n## Context\nIndex addressed directly.\n",
        &[("alpha.md", "### Task 1: Alpha\n**State:** pending\n\nDescription.\n")],
        WORKSPACE_STATE_MACHINE,
    );

    let index_path = ws.join("index.rhei.md");

    // workspace_dir resolves both directory and index file paths.
    assert!(workspace::is_workspace(&ws));
    assert!(workspace::workspace_dir(&ws).is_some());
    assert!(workspace::workspace_dir(&index_path).is_some());

    // CLI validate succeeds against the index file path.
    let output = rhei_command()
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("validate")
        .arg(&index_path)
        .output()
        .expect("validate command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "validate should succeed for index.rhei.md\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Validation succeeded"));

    // CLI list also succeeds.
    let output = rhei_command()
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("list")
        .arg(&index_path)
        .output()
        .expect("list command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "list should succeed for index.rhei.md\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Task workspace.1: Alpha"));
}

#[test]
fn workspace_discovers_task_files_recursively_and_skips_hidden_paths() {
    let (_dir, ws, _machine_path) = create_workspace(
        "ws-recursive",
        "# Rhei: Workspace Recursive\n\n## Context\nSome context here.\n",
        &[
            ("alpha.md", "### Task 1: Alpha\n**State:** pending\n"),
            ("group/beta.md", "### Task 2: Beta\n**State:** pending\n"),
            (".ignored.md", "### Task bad: Hidden\n**State:** not-a-state\n"),
            ("group/.ignored/gamma.md", "### Task bad2: Hidden dir\n**State:** pending\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded = workspace::load_workspace(&ws).expect("load workspace");
    assert_eq!(loaded.rhei.tasks.len(), 2);
    assert_eq!(loaded.rhei.tasks[0].id.to_string(), "1");
    assert_eq!(loaded.rhei.tasks[1].id.to_string(), "2");
    assert!(loaded.task_sources["2"].ends_with("group/beta.md"));
    assert!(!loaded.task_sources.contains_key("bad"));
    assert!(!loaded.task_sources.contains_key("bad2"));
}

#[test]
fn validate_auto_discovers_workspace_root_state_machine_from_states_declaration() {
    let dir = unique_temp_dir("ws-auto-states");
    let ws = dir.join("workspace");
    let tasks_dir = ws.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(
        ws.join("index.rhei.md"),
        "# Rhei: Workspace Auto States\n**States:** workspace-test-machine\n",
    )
    .expect("write index");
    fs::write(tasks_dir.join("alpha.md"), "### Task 1: Alpha\n**State:** pending\n")
        .expect("write task file");
    write_fixture_file(&ws, "states.yaml", WORKSPACE_STATE_MACHINE);

    let output = rhei_command()
        .arg("validate")
        .arg(&ws)
        .output()
        .expect("validate command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "validate should succeed\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Validation succeeded"));
}

#[test]
fn validate_reports_mismatched_auto_discovered_state_machine_name() {
    let dir = unique_temp_dir("auto-states-mismatch");
    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Auto States Mismatch\n**States:** custom-review\n\n## Tasks\n\n### Task 1: Review docs\n**State:** draft\n",
    );
    write_fixture_file(
        &dir,
        "states.yaml",
        "name: wrong-machine\nversion: 1\nstates:\n  draft:\n    initial: true\n    description: Start\n  completed:\n    final: true\n    description: Done\ntransitions:\n  - from: draft\n    to: completed\n",
    );

    let output = rhei_command()
        .arg("validate")
        .arg(&plan_path)
        .output()
        .expect("validate command should run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "validate should fail when auto-discovered machine name mismatches\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        stderr
    );
    assert!(
        stderr.contains("plan declares state machine 'custom-review'"),
        "expected mismatch diagnostic, got:\n{}",
        stderr
    );
    assert!(
        stderr.contains("declares 'wrong-machine'"),
        "expected discovered machine name in diagnostic, got:\n{}",
        stderr
    );
}

#[test]
fn workspace_render_json_includes_all_tasks() {
    let (_dir, ws, machine_path) = create_workspace(
        "ws-render",
        "# Rhei: Render Test\n",
        &[
            ("a.md", "### Task 1: First\n**State:** pending\n"),
            ("b.md", "### Task 2: Second\n**State:** completed\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = rhei_command()
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("render")
        .arg(&ws)
        .arg("--format")
        .arg("json")
        .arg("--pretty")
        .output()
        .expect("render command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "render should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("\"title\": \"Render Test\""));
    assert!(stdout.contains("\"First\""));
    assert!(stdout.contains("\"Second\""));
}

#[test]
fn workspace_duplicate_task_id_across_files_is_reported() {
    let (_dir, ws, _machine_path) = create_workspace(
        "ws-dup",
        "# Rhei: Dup Test\n",
        &[
            ("a.md", "### Task 1: First\n**State:** pending\n"),
            ("b.md", "### Task 1: Duplicate\n**State:** pending\n"),
        ],
        fixtures::TEST_STATE_MACHINE,
    );

    let err = workspace::load_workspace(&ws).expect_err("should fail on duplicate");
    assert!(
        err.message.contains("duplicate task ID '1'"),
        "error should mention duplicate: {}",
        err.message
    );
}

#[test]
fn workspace_missing_index_is_not_detected_as_workspace() {
    let dir = unique_temp_dir("ws-no-index");
    let ws = dir.join("workspace");
    fs::create_dir_all(ws.join("tasks")).expect("create dirs");

    assert!(!workspace::is_workspace(&ws));
}

#[test]
/// §FS-rhei-plan-language.1.2: an empty workspace is a valid, empty rhei. It
/// loads, and validate warns so a mistyped `tasks/` is not mistaken for a
/// deliberately empty one.
fn workspace_empty_tasks_directory_loads_and_is_warned_about() {
    let (_dir, ws, _machine_path) =
        create_workspace("ws-empty", "# Rhei: Empty Test\n", &[], fixtures::TEST_STATE_MACHINE);

    let loaded = workspace::load_workspace(&ws).expect("an empty workspace is valid");
    assert!(loaded.rhei.tasks.is_empty());

    let output = rhei_command()
        .arg("validate")
        .arg(&ws)
        .output()
        .expect("validate command should run");
    assert!(
        output.status.success(),
        "an empty workspace must validate\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("holds no tickets"),
        "validate must name the emptiness rather than report a bare green, got:\n{combined}"
    );
}

#[test]
fn workspace_transition_updates_correct_task_file() {
    let (_dir, ws, machine_path) = create_workspace(
        "ws-transition",
        "# Rhei: Transition Test\n",
        &[
            ("a.md", "### Task 1: Alpha\n**State:** pending\n"),
            ("b.md", "### Task 2: Beta\n**State:** pending\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = rhei_command()
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("transition")
        .arg(&ws)
        .arg("--task")
        .arg("1")
        .arg("--from")
        .arg("pending")
        .arg("--to")
        .arg("in-progress")
        .arg("--no-callbacks")
        .output()
        .expect("transition command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "transition should succeed\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify Task 1's file was updated.
    let a_content = fs::read_to_string(ws.join("tasks/a.md")).expect("read a.md");
    assert!(
        a_content.contains("**State:** in-progress"),
        "a.md should have updated state: {}",
        a_content
    );

    // Verify Task 2's file was NOT modified.
    let b_content = fs::read_to_string(ws.join("tasks/b.md")).expect("read b.md");
    assert!(b_content.contains("**State:** pending"), "b.md should be untouched: {}", b_content);
}
