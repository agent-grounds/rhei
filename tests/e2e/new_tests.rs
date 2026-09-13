//! `rhei new` — creating a rhei under Panta, and creating tickets inside one.
//! §FS-rhei-new

use std::fs;

use super::*;

pub fn new_run(args: &[&str], cwd: &std::path::Path) -> CliRun {
    let output = rhei_command(cwd.join(".home"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("rhei command should run");
    CliRun::from(&output)
}

/// Run `rhei` with something on standard input. `--description-file -` is the
/// only flag that reads it, and `Command::output` would otherwise hand it an
/// immediate EOF. §FS-rhei-new.1.1
fn new_run_with_stdin(args: &[&str], cwd: &std::path::Path, stdin: &str) -> CliRun {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = rhei_command(cwd.join(".home"))
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rhei command should start");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(stdin.as_bytes())
        .expect("description should be written to stdin");
    let output = child.wait_with_output().expect("rhei command should run");
    CliRun::from(&output)
}

/// A project directory with `index.panta.md` and nothing else.
pub fn empty_project(prefix: &str) -> TestDir {
    let dir = unique_temp_dir(prefix);
    write_fixture_file(&dir, "index.panta.md", "# Panta: Test\n");
    dir
}

pub fn assert_failure(result: &CliRun, needle: &str) {
    assert!(!result.status.success(), "command should fail\nstdout:\n{}", result.stdout);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(combined.contains(needle), "expected {needle:?} in output, got:\n{combined}");
}

/// Everything the command said, with miette's line wrapping and its box-drawing
/// gutter flattened away. A needle longer than a few words otherwise straddles
/// a wrap and never matches.
pub fn flattened_output(result: &CliRun) -> String {
    format!("{}{}", result.stdout, result.stderr)
        .replace('\u{2502}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Creating a rhei — §FS-rhei-new.2
// ---------------------------------------------------------------------------

#[test]
fn creates_a_single_file_rhei_from_a_title() {
    let dir = empty_project("new-rhei-single");
    let result = new_run(&["new", "Authentication"], &dir);
    assert_success(&result);

    let plan = fs::read_to_string(dir.join("authentication.rhei.md")).expect("rhei file");
    assert_eq!(plan, "# Rhei: Authentication\n\n## Tasks\n");
    assert!(result.stdout.contains("Created rhei \"Authentication\" as `authentication`"));
    // The one command that follows. §FS-rhei-new.5.4
    assert!(result.stdout.contains("--under authentication"));
}

/// §FS-rhei-plan-language.1.1: the empty rhei `new` writes has to validate.
#[test]
fn a_freshly_created_rhei_validates() {
    let dir = empty_project("new-rhei-validates");
    assert_success(&new_run(&["new", "Authentication"], &dir));
    let result = new_run(&["validate"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("Validation succeeded"));
}

/// §FS-rhei-new.4: lowercase, non-id characters collapsed to `-`.
#[test]
fn derives_the_id_from_the_title() {
    let dir = empty_project("new-rhei-slug");
    assert_success(&new_run(&["new", "Billing & Dunning"], &dir));
    assert!(dir.join("billing-dunning.rhei.md").is_file());
}

#[test]
fn explicit_id_replaces_derivation() {
    let dir = empty_project("new-rhei-id");
    assert_success(&new_run(&["new", "Authentication", "--id", "auth"], &dir));
    assert!(dir.join("auth.rhei.md").is_file());
}

/// §FS-rhei-new.2.1: `--dir` writes the Directory Workspace shape, whose index
/// must not carry a `## Tasks` section.
#[test]
fn dir_creates_a_workspace_rhei() {
    let dir = empty_project("new-rhei-dir");
    assert_success(&new_run(&["new", "Billing", "--dir"], &dir));

    let index = fs::read_to_string(dir.join("billing/index.rhei.md")).expect("index");
    assert_eq!(index, "# Rhei: Billing\n");
    assert!(dir.join("billing/tasks").is_dir());
}

/// §FS-rhei-new.2: header fields land in plan-language order — heading,
/// `**States:**`, frontmatter, description, `## Tasks`.
#[test]
fn writes_header_fields_in_plan_language_order() {
    let dir = empty_project("new-rhei-header");
    write_fixture_file(
        &dir,
        "states.yaml",
        "name: custom\nversion: 1\nstates:\n  todo:\n    initial: true\n    description: Todo\n  done:\n    final: true\n    description: Done\ntransitions:\n  - from: todo\n    to: done\n",
    );
    let result = new_run(
        &[
            "new",
            "Billing",
            "--states",
            "custom",
            "--max-levels",
            "3",
            "--node-kinds",
            "task,bug",
            "--description",
            "Everything invoice-related.",
        ],
        &dir,
    );
    assert_success(&result);

    let plan = fs::read_to_string(dir.join("billing.rhei.md")).expect("rhei file");
    assert_eq!(
        plan,
        "# Rhei: Billing\n**States:** custom\n\n---\nstructure:\n  maxLevels: 3\n  \
         nodeKinds: [task, bug]\n---\n\nEverything invoice-related.\n\n## Tasks\n"
    );
}

#[test]
fn refuses_the_reserved_basin_id() {
    let dir = empty_project("new-rhei-basin");
    assert_failure(&new_run(&["new", "Basin", "--id", "basin"], &dir), "reserved rhei id");
}

#[test]
fn refuses_a_colliding_rhei_id() {
    let dir = empty_project("new-rhei-collision");
    assert_success(&new_run(&["new", "Auth"], &dir));
    assert_failure(&new_run(&["new", "Auth"], &dir), "already exists");
}

#[test]
fn refuses_an_illegal_explicit_id() {
    let dir = empty_project("new-rhei-illegal");
    assert_failure(&new_run(&["new", "Auth", "--id", "9lives"], &dir), "not a valid rhei id");
}

/// §FS-rhei-new.2.1: a rhei is a member of a project; a lone plan has nowhere
/// to put a second one.
#[test]
fn refuses_to_create_a_rhei_outside_a_project() {
    let dir = unique_temp_dir("new-rhei-lone");
    write_fixture_file(
        &dir,
        "solo.rhei.md",
        "# Rhei: Solo\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n",
    );
    assert_failure(&new_run(&["new", "Another"], &dir), "is not a Panta project");
}

// ---------------------------------------------------------------------------
// Creating a ticket — §FS-rhei-new.3
// ---------------------------------------------------------------------------

pub fn project_with_rhei(prefix: &str) -> TestDir {
    let dir = empty_project(prefix);
    assert_success(&new_run(&["new", "Authentication", "--id", "auth"], &dir));
    dir
}

#[test]
fn creates_a_top_level_ticket_numbered_from_its_siblings() {
    let dir = project_with_rhei("new-ticket-top");
    assert_success(&new_run(&["new", "First", "--under", "auth"], &dir));
    let result = new_run(&["new", "Second", "--under", "auth"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("Created ticket auth.2"));

    let plan = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");
    assert!(plan.contains("### Task 1: First\n**State:** pending\n"));
    assert!(plan.contains("### Task 2: Second\n**State:** pending\n"));
}

/// §FS-rhei-new.1.3: every plan-language metadata field has a flag.
#[test]
fn writes_every_metadata_field_in_grammar_order() {
    let dir = project_with_rhei("new-ticket-fields");
    assert_success(&new_run(&["new", "First", "--under", "auth", "--provides", "api"], &dir));
    let result = new_run(
        &[
            "new",
            "Second",
            "--under",
            "auth",
            "--prior",
            "1",
            "--provides",
            "client",
            "--consumes",
            "1:api",
            "--assignee",
            "vj",
            "--description",
            "Body text.",
        ],
        &dir,
    );
    assert_success(&result);

    let plan = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");
    assert!(
        plan.contains(
            "### Task 2: Second\n**State:** pending\n**Prior:** 1\n**Provides:** client\n\
             **Consumes:** 1:api\n**Assignee:** vj\n\nBody text.\n"
        ),
        "metadata order wrong:\n{plan}"
    );
}

/// §FS-rhei-new.3.1: a subtask goes after its parent's subtree, not at the end
/// of the file.
#[test]
fn inserts_a_subtask_after_its_parents_subtree() {
    let dir = project_with_rhei("new-ticket-subtask");
    assert_success(&new_run(&["new", "First", "--under", "auth"], &dir));
    assert_success(&new_run(&["new", "Second", "--under", "auth"], &dir));
    let result = new_run(&["new", "Nested", "--under", "auth.1"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("Created ticket auth.1.1"));

    let plan = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");
    let nested = plan.find("#### Task 1.1: Nested").expect("subtask present");
    let second = plan.find("### Task 2: Second").expect("sibling present");
    assert!(nested < second, "subtask must precede the next top-level ticket:\n{plan}");
}

#[test]
fn workspace_tickets_get_their_own_task_file_and_subtasks_join_it() {
    let dir = empty_project("new-ticket-workspace");
    assert_success(&new_run(&["new", "Billing", "--dir"], &dir));
    assert_success(&new_run(&["new", "Dunning emails", "--under", "billing"], &dir));
    assert_success(&new_run(&["new", "Retry schedule", "--under", "billing.1"], &dir));

    let task_file = dir.join("billing/tasks/001-dunning-emails.md");
    let contents = fs::read_to_string(&task_file).expect("task file");
    assert!(contents.starts_with("### Task 1: Dunning emails\n"));
    // A task file owns a subtree. §FS-rhei-new.3.1
    assert!(contents.contains("#### Task 1.1: Retry schedule\n"));
    let authored_task_files = fs::read_dir(dir.join("billing/tasks"))
        .expect("tasks dir")
        .map(|entry| entry.expect("task directory entry"))
        .filter(|entry| entry.path().extension().is_some_and(|extension| extension == "md"))
        .count();
    assert_eq!(authored_task_files, 1, "a subtask must not become a second authored file");
}

/// §FS-rhei-panta.2: the basin is created on demand and has no authored index.
#[test]
fn captures_into_the_basin_creating_it_on_demand() {
    let dir = project_with_rhei("new-ticket-basin");
    let result = new_run(&["new", "Fix the footer typo", "--under", "basin"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("Created ticket basin.1"));
    assert!(dir.join("basin/001-fix-the-footer-typo.md").is_file());
    assert!(!dir.join("basin/index.rhei.md").exists(), "the basin manifest is synthetic");
    assert_success(&new_run(&["validate"], &dir));
}

/// A lone plan for a worker that was handed one file and no project.
fn lone_plan(prefix: &str) -> TestDir {
    let dir = unique_temp_dir(prefix);
    write_fixture_file(
        &dir,
        "solo.rhei.md",
        "# Rhei: Solo\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n",
    );
    dir
}

/// §FS-rhei-new.3.5: outside a project there is no unfiled inbox to file into,
/// so the capture is refused rather than growing a `basin/` beside the plan.
#[test]
fn refuses_a_basin_capture_outside_a_project() {
    let dir = lone_plan("new-ticket-basin-lone");
    let result = new_run(&["new", "Fix the footer typo", "--under", "basin"], &dir);
    assert_failure(&result, "the basin exists only inside a Panta project");

    let said = flattened_output(&result);
    assert!(said.contains("solo.rhei.md"), "the refusal names the plan it resolved, got:\n{said}");
    assert!(said.contains("rhei init"), "and points at the one remedy, got:\n{said}");
    assert!(!dir.join("basin").exists(), "and creates nothing on the way to refusing");
}

/// §FS-rhei-new.3.5: `basin` is the one `--under` value a lone plan cannot
/// answer for — every other one resolves inside the plan itself.
#[test]
fn a_lone_plan_still_takes_a_ticket_under_its_own_ids() {
    let dir = lone_plan("new-ticket-lone-under");
    assert_success(&new_run(&["new", "A sibling", "--under", "solo"], &dir));
    assert_success(&new_run(&["new", "A subtask", "--under", "solo.1"], &dir));

    let plan = fs::read_to_string(dir.join("solo.rhei.md")).expect("rhei file");
    assert!(plan.contains("#### Task 1.1: A subtask\n"), "got:\n{plan}");
    assert!(plan.contains("### Task 2: A sibling\n"), "got:\n{plan}");
}

/// §FS-rhei-new.3.2: the state comes from the *owning rhei's* machine, not the
/// project default.
#[test]
fn starts_in_the_owning_rheis_initial_state() {
    let dir = empty_project("new-ticket-state");
    write_fixture_file(
        &dir,
        "states.yaml",
        "name: custom\nversion: 1\nstates:\n  todo:\n    initial: true\n    description: Todo\n  done:\n    final: true\n    description: Done\ntransitions:\n  - from: todo\n    to: done\n",
    );
    assert_success(&new_run(&["new", "Reporting", "--states", "custom"], &dir));
    let result = new_run(&["new", "First report", "--under", "reporting"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("[todo]"), "got: {}", result.stdout);
}

#[test]
fn explicit_state_is_checked_against_that_machine() {
    let dir = project_with_rhei("new-ticket-badstate");
    assert_failure(
        &new_run(&["new", "First", "--under", "auth", "--state", "nope"], &dir),
        "has no state 'nope'",
    );
}

#[test]
fn refuses_an_undeclared_node_kind() {
    let dir = project_with_rhei("new-ticket-kind");
    assert_failure(
        &new_run(&["new", "First", "--under", "auth", "--kind", "spike"], &dir),
        "does not declare the node kind 'spike'",
    );
}

/// §FS-rhei-new.3.3: depth is refused before anything is written.
#[test]
fn refuses_a_subtask_deeper_than_max_levels() {
    let dir = project_with_rhei("new-ticket-depth");
    assert_success(&new_run(&["new", "First", "--under", "auth"], &dir));
    assert_success(&new_run(&["new", "Nested", "--under", "auth.1"], &dir));
    assert_failure(&new_run(&["new", "Deeper", "--under", "auth.1.1"], &dir), "allows 2");
}

#[test]
fn refuses_an_unknown_parent() {
    let dir = project_with_rhei("new-ticket-parent");
    assert_failure(&new_run(&["new", "First", "--under", "nope"], &dir), "names no rhei or ticket");
}

/// §FS-rhei-new.1.1: the body comes from a file, or from standard input.
#[test]
fn description_file_supplies_the_body() {
    let dir = project_with_rhei("new-ticket-description-file");
    write_fixture_file(&dir, "body.md", "Read from a file.\n");
    assert_success(&new_run(
        &["new", "First", "--under", "auth", "--description-file", "body.md"],
        &dir,
    ));

    let piped = new_run_with_stdin(
        &["new", "Second", "--under", "auth", "--description-file", "-"],
        &dir,
        "Read from standard input.\n",
    );
    assert_success(&piped);

    let plan = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");
    assert!(plan.contains("\nRead from a file.\n"), "got:\n{plan}");
    assert!(plan.contains("\nRead from standard input.\n"), "got:\n{plan}");
}

const OVERRIDE_MACHINE: &str = r#"name: custom
version: 1
models: [fast]
states:
  todo:
    initial: true
    description: Todo
  doing:
    description: Doing
  done:
    final: true
    description: Done
transitions:
  - from: todo
    to: doing
  - from: doing
    to: done
"#;

const BUGS_RHEI: &str = r#"# Rhei: Bugs
**States:** custom

---
structure:
  nodeKinds: [task, bug]
---

## Tasks
"#;

/// §FS-rhei-new.1.3: the ticket flags have success paths, not only refusals.
/// `--model` and `--target` need a machine that declares the model and settings
/// that know the agent, which is exactly the state a real project is in.
#[test]
fn kind_state_model_and_target_write_the_fields_they_name() {
    let dir = empty_project("new-ticket-overrides");
    write_fixture_file(&dir, "states.yaml", OVERRIDE_MACHINE);
    write_fixture_file(&dir, "bugs.rhei.md", BUGS_RHEI);

    let bug = new_run(
        &["new", "Crash on save", "--under", "bugs", "--kind", "bug", "--state", "doing"],
        &dir,
    );
    assert_success(&bug);
    assert!(bug.stdout.contains("[doing]"), "got: {}", bug.stdout);
    assert_success(&new_run(&["new", "Fast one", "--under", "bugs", "--model", "fast"], &dir));
    assert_success(&new_run(
        &["new", "Targeted", "--under", "bugs", "--target", "claude-code:sonnet"],
        &dir,
    ));

    let plan = fs::read_to_string(dir.join("bugs.rhei.md")).expect("rhei file");
    assert!(plan.contains("### Bug 1: Crash on save\n**State:** doing\n"), "got:\n{plan}");
    assert!(
        plan.contains("### Task 2: Fast one\n**State:** todo\n**Model:** fast\n"),
        "got:\n{plan}"
    );
    assert!(
        plan.contains("### Task 3: Targeted\n**State:** todo\n**Target:** claude-code:sonnet\n"),
        "got:\n{plan}"
    );
    assert_success(&new_run(&["validate"], &dir));
}

/// §FS-rhei-new.2.1: a member rhei widens to the project it belongs to — the
/// same resolution every other command does, announced the same way.
#[test]
fn a_member_rhei_widens_to_its_project() {
    let dir = empty_project("new-widen");
    assert_success(&new_run(&["new", "Authentication", "--id", "auth"], &dir));
    assert_success(&new_run(&["new", "Billing", "--dir"], &dir));
    let member = dir.join("billing");

    // Standing inside one member, `--under` still names any rhei in the
    // project — which a lone plan could not resolve at all.
    let inside = new_run(&["new", "Rotate keys", "--under", "auth"], &member);
    assert_success(&inside);
    assert!(inside.stdout.contains("belongs to the project at"), "got: {}", inside.stdout);
    assert!(fs::read_to_string(dir.join("auth.rhei.md"))
        .expect("rhei file")
        .contains("Rotate keys"));

    // A new rhei created from inside a member lands beside the manifest.
    assert_success(&new_run(&["new", "Reporting"], &member));
    assert!(dir.join("reporting.rhei.md").is_file());

    // `--project <member>` resolves the same project, so the basin exists.
    let captured = new_run(&["new", "Typo", "--under", "basin", "--project", "auth.rhei.md"], &dir);
    assert_success(&captured);
    assert!(dir.join("basin/001-typo.md").is_file());
}

/// §FS-rhei-new.3: a lone plan is its own rhei, so tickets can be added to it
/// even outside a project.
#[test]
fn adds_a_ticket_to_a_lone_plan() {
    let dir = unique_temp_dir("new-ticket-lone");
    write_fixture_file(
        &dir,
        "solo.rhei.md",
        "# Rhei: Solo\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n",
    );
    let result = new_run(&["new", "Two", "--under", "solo"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("Created ticket solo.2"));
}
