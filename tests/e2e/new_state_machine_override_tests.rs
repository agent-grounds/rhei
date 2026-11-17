//! The global machine override governs ticket creation and both validation passes.
//! §FS-rhei-new.3.2 §FS-rhei-new.5.2

use super::new_tests::{assert_failure, flattened_output, new_run};
use super::*;

const OVERRIDE: &str = "sm-elsewhere.yaml";
const TASKS: &str = "tasks/01-country.md";

fn machine(name: &str, initial: &str) -> String {
    format!(
        "name: {name}
version: 1
states:
  {initial}:
    initial: true
    description: Not started
  land-check:
    description: Checking
  completed:
    final: true
    description: Done
  cancelled:
    final: true
    description: Stopped
transitions:
  - from: {initial}
    to: land-check
  - from: land-check
    to: completed
  - from: {initial}
    to: cancelled
  - from: land-check
    to: cancelled
"
    )
}

/// Intake's Directory Workspace: only the flag can find its machine.
fn land(initial: &str) -> (TestDir, PathBuf) {
    let root = unique_temp_dir("rhei-741-new-override");
    let dir = root.join("land");
    fs::create_dir_all(dir.join("tasks")).expect("workspace tasks");
    write_fixture_file(&dir, "index.rhei.md", "# Rhei: Land\n");
    write_fixture_file(
        &dir,
        TASKS,
        "### Task country: Country\n**State:** land-check\n\nThe existing parent.\n",
    );
    write_fixture_file(&dir, OVERRIDE, &machine("land-override", initial));
    (root, dir)
}

fn validate_with_override(dir: &Path) {
    assert_success(&new_run(&["--state-machine", OVERRIDE, "validate", "."], dir));
}

fn create_with_override(dir: &Path, extra: &[&str]) -> CliRun {
    let mut args = vec![
        "--state-machine",
        OVERRIDE,
        "new",
        "Child",
        "--under",
        "land.country",
        "--id",
        "child",
    ];
    args.extend_from_slice(extra);
    new_run(&args, dir)
}

fn assert_ticket_state(dir: &Path, expected: &str) {
    let text = fs::read_to_string(dir.join(TASKS)).expect("authored tasks");
    let child = text.split("#### Task country.child: Child\n").nth(1).expect("new child ticket");
    assert_eq!(child.lines().next(), Some(format!("**State:** {expected}").as_str()), "{text}");
}

fn assert_no_inherited_warning(result: &CliRun) {
    assert!(
        !flattened_output(result).contains("already failing validation"),
        "clean override-governed workspace was blamed for inherited errors:\n{}",
        result.stderr
    );
}

/// §FS-rhei-new.3.2: guard and lookup control from the intake reproducer.
#[test]
fn issue_741_override_fixture_requires_flag_and_automatic_lookup_still_works() {
    let (_root, dir) = land("pending");
    validate_with_override(&dir);
    assert_failure(&new_run(&["validate", "."], &dir), "invalid state 'land-check'");

    fs::rename(dir.join(OVERRIDE), dir.join("states.yaml")).expect("enable automatic lookup");
    let result = new_run(
        &["new", "Child", "--under", "land.country", "--id", "child", "--state", "land-check"],
        &dir,
    );
    assert_success(&result);
    assert_ticket_state(&dir, "land-check");
    assert_no_inherited_warning(&result);
    assert_success(&new_run(&["validate", "."], &dir));
}

/// §FS-rhei-new.3.2 §FS-rhei-new.5.2: the explicit state is legal only under the flag.
#[test]
#[ignore = "red until #741: new accepts the override-only explicit state; implement removes this"]
fn issue_741_new_accepts_override_only_explicit_state() {
    let (_root, dir) = land("pending");
    validate_with_override(&dir);

    let result = create_with_override(&dir, &["--state", "land-check"]);
    assert_success(&result);
    assert_ticket_state(&dir, "land-check");
    assert_no_inherited_warning(&result);
    validate_with_override(&dir);
}

/// §FS-rhei-new.3.2 §FS-rhei-new.5.2: the override has no pending state.
#[test]
#[ignore = "red until #741: new starts in the override's queued state; implement removes this"]
fn issue_741_new_uses_override_initial_state() {
    let (_root, dir) = land("queued");
    validate_with_override(&dir);

    let result = create_with_override(&dir, &[]);
    assert_success(&result);
    assert_ticket_state(&dir, "queued");
    assert_no_inherited_warning(&result);
    validate_with_override(&dir);
}

/// §FS-rhei-new.5.2: pending is legal under both machines; only the existing
/// parent's land-check state distinguishes the before/after validation passes.
#[test]
#[ignore = "red until #741: both validation passes use the override; implement removes this"]
fn issue_741_new_uses_override_for_before_and_after_validation() {
    let (_root, dir) = land("pending");
    validate_with_override(&dir);

    let result = create_with_override(&dir, &["--state", "pending"]);
    assert_success(&result);
    assert_ticket_state(&dir, "pending");
    assert_no_inherited_warning(&result);
    validate_with_override(&dir);

    // An inherited error must be recognized under the override before writing,
    // or a corrected post-write pass would misclassify it as introduced. §FS-rhei-new.5.2
    let (_dirty_root, dirty) = land("pending");
    write_fixture_file(
        &dirty,
        "tasks/02-old.md",
        "### Task old: Existing invalid ticket\n**State:** queued\n",
    );
    let validate = ["--state-machine", OVERRIDE, "validate", "."];
    assert_failure(&new_run(&validate, &dirty), "invalid state 'queued'");

    let inherited = create_with_override(&dirty, &["--state", "pending"]);
    assert_success(&inherited);
    assert_ticket_state(&dirty, "pending");
    let said = flattened_output(&inherited);
    assert!(said.contains("already failing validation before this create (1 error)"), "{said}");
    assert_failure(&new_run(&validate, &dirty), "invalid state 'queued'");
}

/// §FS-rhei-new.3.2: an override is not merely the project default; a member's
/// own states.yaml must not shadow it during either create decision.
#[test]
#[ignore = "red until #741: override beats Panta member lookup; implement removes this"]
fn issue_741_new_override_beats_panta_member_machine() {
    let dir = unique_temp_dir("rhei-741-new-panta-override");
    fs::create_dir_all(dir.join("land/tasks")).expect("member workspace tasks");
    write_fixture_file(&dir, "index.panta.md", "# Panta: Test\n");
    write_fixture_file(&dir, "states.yaml", &machine("project", "project-ready"));
    write_fixture_file(&dir, OVERRIDE, &machine("override", "queued"));
    write_fixture_file(&dir, "land/index.rhei.md", "# Rhei: Land\n");
    write_fixture_file(&dir, "land/states.yaml", &machine("member", "member-ready"));
    write_fixture_file(
        &dir,
        "land/tasks/01-country.md",
        "### Task country: Country\n**State:** land-check\n",
    );
    validate_with_override(&dir);

    let result = create_with_override(&dir, &[]);
    assert_success(&result);
    assert_ticket_state(&dir.join("land"), "queued");
    assert_no_inherited_warning(&result);
    validate_with_override(&dir);
}

/// §FS-rhei-new.3.2: pending belongs to the built-in machine, but is invalid
/// under this override. Refusal preserves all authored fixture bytes.
#[test]
#[ignore = "red until #741: new refuses states absent from override; implement removes this"]
fn issue_741_new_refuses_state_outside_override_without_authored_changes() {
    let (_root, dir) = land("queued");
    validate_with_override(&dir);
    let files = ["index.rhei.md", TASKS, OVERRIDE];
    let before: Vec<_> =
        files.iter().map(|file| fs::read(dir.join(file)).expect("fixture")).collect();

    let result = create_with_override(&dir, &["--state", "pending"]);
    assert_failure(&result, "has no state 'pending'");
    assert!(flattened_output(&result).contains("land-override"), "{}", result.stderr);
    for (file, expected) in files.iter().zip(before) {
        assert_eq!(fs::read(dir.join(file)).expect("authored content"), expected, "{file}");
    }
    let task_files: Vec<_> = fs::read_dir(dir.join("tasks"))
        .expect("task directory")
        .map(|entry| entry.expect("task entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    assert_eq!(task_files, [dir.join(TASKS)], "no extra authored task file");
    validate_with_override(&dir);
}
