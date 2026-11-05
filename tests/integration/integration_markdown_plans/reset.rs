#[test]
fn reset_restores_single_file_plan_to_authored_states() {
    let machine = r#"name: reset-test
version: 1
states:
  draft:
    description: Start here
    initial: true
  pending:
    description: Ready
  in-progress:
    description: Active
  completed:
    description: Done
    final: true
transitions:
  - from: draft
    to: pending
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
"#;

    let plan = r#"# Rhei: Resettable

## Tasks

### Task 1: Alpha
**State:** completed
**Assignee:** codex

#### Task 1.1: Detail
**State:** completed
**Assignee:** claude-code

### Task 2: Beta
**State:** pending
"#;

    let dir = unique_temp_dir("reset-single-file");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    // Every task moved out of `draft`, and the ledger is what records it. The child
    // finished before its parent: reset refuses a plan validation refuses. §FS-rhei-reset.2.2
    let runtime = dir.join("runtime");
    fs::create_dir_all(&runtime).expect("create runtime dir");
    fs::write(
        runtime.join("state-transitions.log"),
        "plan.1 draft@pending\nplan.1.1 draft@pending\nplan.2 draft@pending\n\
         plan.1.1 pending@completed\nplan.1 pending@completed\n",
    )
    .expect("write ledger");

    let result = run_reset_command(&plan_path, &machine_path);

    assert!(
        result.status.success(),
        "reset should succeed\nstdout:\n{}\nstderr:\n{}",
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
    // The summary names each move rather than only counting them.
    // §FS-rhei-reset.4
    assert!(
        result.stdout.contains("Task plan.1: completed → draft"),
        "the summary should name each move:\n{}",
        result.stdout
    );

    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse reset plan");
    assert_eq!(rhei.tasks[0].state.as_str(), "draft");
    assert_eq!(rhei.tasks[0].children[0].state.as_str(), "draft");
    assert_eq!(rhei.tasks[1].state.as_str(), "draft");
    assert_eq!(rhei.tasks[0].assignee, None);
    assert_eq!(rhei.tasks[0].children[0].assignee, None);
    assert_eq!(rhei.tasks[1].assignee, None);
    assert!(!updated.contains("**Assignee:**"));
}

// ── Directory Workspace tests ────────────────────────────────────────────────

/// §FS-rhei-reset.1.2: without a terminal there is nobody to answer, so reset
/// refuses rather than reading consent into an unattended caller's silence.
#[test]
fn reset_refuses_without_yes_when_stdin_is_not_a_terminal() {
    let machine = r#"name: reset-noninteractive
version: 1
states:
  pending:
    description: Ready
    initial: true
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: completed
"#;

    let plan = r#"# Rhei: Unattended

## Tasks

### Task 1: Alpha
**State:** completed
"#;

    let dir = unique_temp_dir("reset-noninteractive");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_reset_command_with_args(&plan_path, &machine_path, &[]);

    assert!(
        !result.status.success(),
        "reset without -y on a pipe should fail\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("stdin is not a terminal"),
        "error should explain why it stopped:\n{}",
        result.stderr
    );
    assert!(
        result.stderr.contains("`-y`"),
        "error should name the flag that confirms:\n{}",
        result.stderr
    );

    let untouched = fs::read_to_string(&plan_path).expect("read plan");
    assert!(
        untouched.contains("**State:** completed"),
        "a refused reset must not rewrite state:\n{untouched}"
    );
}

/// §FS-rhei-reset.1.2: the damage preview precedes every destructive reset,
/// including the `--yes` path that never stops to ask.
#[test]
fn reset_prints_the_damage_preview_even_with_yes() {
    let machine = r#"name: reset-preview
version: 1
states:
  pending:
    description: Ready
    initial: true
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: completed
"#;

    let plan = r#"# Rhei: Previewed

## Tasks

### Task 1: Alpha
**State:** completed
"#;

    let dir = unique_temp_dir("reset-preview");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    fs::create_dir_all(dir.join("runtime/results")).expect("seed runtime tree");
    fs::write(dir.join("runtime/results/1.md"), "## Result\n\ndone\n").expect("seed result");

    let result = run_reset_command_with_args(&plan_path, &machine_path, &["-y"]);

    assert!(
        result.status.success(),
        "reset -y should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("Would reset"),
        "preview must print before the destructive step:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Would delete"),
        "preview must name the runtime tree it deletes:\n{}",
        result.stdout
    );
}

/// A pre-authored chain — one supervisor, children authored in the states they
/// will run in — survives a reset. Every child used to come back as a
/// supervisor, and the next run dispatched them on the supervisor's target
/// with no brief.
// §FS-rhei-reset.2.2 §FS-rhei-supervision.7
#[test]
fn reset_keeps_a_pre_authored_chain_in_its_authored_states() {
    let machine = r#"name: supervised-reset
version: 1
states:
  supervising:
    description: Supervisor
    initial: true
    execute_on: child-terminal
    target: claude-code:anthropic:claude-opus-4-7
  implement:
    description: Build it
  review:
    description: Read it
  completed:
    description: Done
    final: true
transitions:
  - from: supervising
    to: supervising
  - from: implement
    to: completed
  - from: review
    to: completed
  - from: supervising
    to: completed
"#;

    let plan = r#"# Rhei: Supervised

## Tasks

### Task 1: Deliver
**State:** supervising

#### Task 1.1: Implement
**State:** implement

#### Task 1.2: Review
**State:** review
"#;

    let dir = unique_temp_dir("reset-pre-authored");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    // The supervisor took one visit and self-looped; no child ever moved.
    let runtime = dir.join("runtime");
    fs::create_dir_all(&runtime).expect("create runtime dir");
    fs::write(
        runtime.join("state-transitions.log"),
        "plan.1 supervising@supervising\n",
    )
    .expect("write ledger");

    let result = run_reset_command(&plan_path, &machine_path);
    assert!(
        result.status.success(),
        "reset should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse reset plan");
    assert_eq!(rhei.tasks[0].state.as_str(), "supervising");
    assert_eq!(
        rhei.tasks[0].children[0].state.as_str(),
        "implement",
        "a child that never moved keeps its authored state:\n{updated}"
    );
    assert_eq!(
        rhei.tasks[0].children[1].state.as_str(),
        "review",
        "a child that never moved keeps its authored state:\n{updated}"
    );
}
