// What `rhei reset` does when the ledger cannot account for a task: no ledger
// at the execution root, a state the machine has since dropped, or a plan that
// simply never ran. The restoring path is next door in `reset.rs`; these are
// the cases where reset deliberately changes nothing and says why.

// §AR-source-file-size.3 §FS-rhei-reset.2.2

/// With no ledger anywhere, nothing records where a task came from. Reset
/// changes no state and names what it left, rather than moving tasks to a
/// state they may never have held.
// §FS-rhei-reset.2.2
#[test]
fn reset_leaves_states_alone_when_no_ledger_records_a_move() {
    let machine = r#"name: reset-no-ledger
version: 1
states:
  draft:
    description: Start here
    initial: true
  in-progress:
    description: Active
  completed:
    description: Done
    final: true
transitions:
  - from: draft
    to: in-progress
  - from: in-progress
    to: completed
"#;

    // Task 1 carries a result link: the trace that says a run touched it, so
    // reset cannot write it off as authored where it stands. The link is the
    // one validation accepts, since reset refuses a plan it refuses. §FS-rhei-reset.2
    let plan = r#"# Rhei: Unrecorded

## Tasks

### Task 1: Alpha
**State:** completed

> **Result:** [plan.1](runtime/results/plan.1.md)

### Task 2: Beta
**State:** draft
"#;

    let dir = unique_temp_dir("reset-no-ledger");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_reset_command(&plan_path, &machine_path);
    assert!(
        result.status.success(),
        "reset should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("Nothing records where these"),
        "reset should say why it moved nothing:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Task plan.1: completed"),
        "reset should name the task it left outside its initial state:\n{}",
        result.stdout
    );

    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse reset plan");
    assert_eq!(rhei.tasks[0].state.as_str(), "completed");
    assert_eq!(rhei.tasks[1].state.as_str(), "draft");
}

/// A counted-visit suffix is runtime state, not part of the authored state, so
/// it is cleared even for a task whose state name reset leaves alone. It used
/// to survive, while `stateVisits` was wiped — leaving the visit budget
/// recorded only in the suffix, so the reset workspace was already out of
/// visits and its supervisor could not run.
// §FS-rhei-reset.2 §FS-rhei-reset.2.2
#[test]
fn reset_clears_a_counted_visit_suffix_with_no_ledger() {
    let machine = r#"name: visits
version: 1
states:
  supervising:
    description: Supervisor
    initial: true
    execute_on: child-terminal
    target: claude-code:anthropic:claude-opus-4-7
    visits: 3
  completed:
    description: Done
    final: true
transitions:
  - from: supervising
    to: completed
    condition: openDescendants < 1
  - from: supervising
    to: supervising
"#;

    let plan = "# Rhei: Visits\n\n## Tasks\n\n### Task 1: Supervisor\n**State:** supervising-3\n";

    let dir = unique_temp_dir("reset-visit-suffix");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_reset_command(&plan_path, &machine_path);
    assert!(
        result.status.success(),
        "reset should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read plan");
    assert!(
        updated.contains("**State:** supervising\n"),
        "the suffix should be gone:\n{updated}"
    );
    assert!(!updated.contains("supervising-3"), "the suffix should be gone:\n{updated}");
}

/// The ledger can name a state the machine no longer declares — renamed since
/// the run. Writing it back would leave a plan that fails `rhei validate`,
/// with the ledger that explained it deleted moments later, so reset keeps the
/// task where it stands and says which state it could not restore.
// §FS-rhei-reset.2.2
#[test]
fn reset_keeps_a_state_the_machine_no_longer_declares() {
    let machine = r#"name: renamed
version: 1
states:
  todo:
    description: Start here
    initial: true
  review:
    description: Look
  completed:
    description: Done
    final: true
transitions:
  - from: todo
    to: review
  - from: review
    to: completed
"#;

    let plan = "# Rhei: Renamed\n\n## Tasks\n\n### Task 1: Alpha\n**State:** review\n";

    let dir = unique_temp_dir("reset-renamed-state");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    // The run happened while the initial state was still called `draft`.
    let runtime = dir.join("runtime");
    fs::create_dir_all(&runtime).expect("create runtime dir");
    fs::write(runtime.join("state-transitions.log"), "plan.1 draft@review\n")
        .expect("write ledger");

    let result = run_reset_command(&plan_path, &machine_path);
    assert!(
        result.status.success(),
        "reset should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("no longer declares"),
        "reset should say which state it could not restore:\n{}",
        result.stdout
    );

    let updated = fs::read_to_string(&plan_path).expect("read plan");
    assert!(
        updated.contains("**State:** review"),
        "an undeclared state must not be written back:\n{updated}"
    );
    assert!(!updated.contains("draft"), "an undeclared state must not be written back:\n{updated}");
}

/// The tasks reset could not account for are named against their *resolved
/// profile's* initial state, not the machine's one legacy `initial: true`
/// state — level 2 here resolves to `simple`, whose initial is `pending`.
// §FS-rhei-reset.2.2 §FS-rhei-states.9.2
#[test]
fn reset_reports_stranded_tasks_against_their_resolved_profile() {
    let machine = r#"name: profiled
schema_version: 3
version: 1
states:
  draft:
    description: Root start
  pending:
    description: Child start
  in-progress:
    description: Active
  completed:
    description: Done
    final: true
transitions:
  - from: draft
    to: in-progress
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
profiles:
  reviewed:
    initial: draft
    allowed: [draft, in-progress, completed]
  simple:
    initial: pending
    allowed: [pending, in-progress, completed]
node_policy:
  root: reviewed
  default: reviewed
  overrides:
    - match: { level: 2 }
      profile: simple
"#;

    // Task 1 is at its profile's initial (`draft`) and must not be listed;
    // Task 1.1 is not at its own profile's initial (`pending`) and must be.
    // Both carry an assignee, the trace that says a run touched them.
    let plan = "# Rhei: Profiled\n\n## Tasks\n\n### Task 1: Alpha\n**State:** draft\n\
                **Assignee:** codex\n\n#### Task 1.1: Detail\n**State:** in-progress\n\
                **Assignee:** codex\n";

    let dir = unique_temp_dir("reset-stranded-profile");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_reset_command(&plan_path, &machine_path);
    assert!(
        result.status.success(),
        "reset should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("Task plan.1.1: in-progress"),
        "a task outside its own profile's initial state is named:\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains("Task plan.1: draft"),
        "a task already at its profile's initial state is not named:\n{}",
        result.stdout
    );
}

/// The flagship shape must reset quietly. A pre-authored chain has every child
/// outside the machine's `initial: true` state by construction, so listing
/// them all on an ordinary reset of a plan that never ran would cry wolf
/// eleven times and bury the one task that is genuinely stale.
// §FS-rhei-reset.2.2 §FS-rhei-supervision.7
#[test]
fn reset_does_not_flag_a_pre_authored_chain_that_never_ran() {
    let machine = r#"name: quiet-chain
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

    let plan = r#"# Rhei: Quiet

## Tasks

### Task 1: Deliver
**State:** supervising

#### Task 1.1: Implement
**State:** implement

#### Task 1.2: Review
**State:** review
"#;

    let dir = unique_temp_dir("reset-quiet-chain");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_reset_command(&plan_path, &machine_path);
    assert!(
        result.status.success(),
        "reset should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        !result.stdout.contains("Nothing records where"),
        "a plan that never ran has nothing to account for:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("No task had moved from its authored state."),
        "unexpected stdout:\n{}",
        result.stdout
    );
}
