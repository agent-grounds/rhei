//! Explicit recovery for plans that predate the direct export-prior rule.

use std::fs;
use std::path::{Path, PathBuf};

use super::terminal_result_tests::write_mock_agent_settings;
use super::*;

const MACHINE: &str = r#"name: migrate
version: 1
states:
  pending:
    initial: true
    description: Consume the handoff
    agent: mock
    agent_timeout: 10s
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: completed
"#;

const AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
write(root / 'runtime' / ('spawned-' + task + '.txt'), agent_prompt())
result('consumed migrated export\n')
"#;

const OLD_SHAPE_PLAN: &str = r#"# Rhei: Export-prior recovery
**States:** migrate

## Tasks

### Task 1: Producer
**State:** completed
**Provides:** contract

### Task 2: Completed middle
**State:** completed
**Prior:** Task 1

### Task 3: Open consumer
**State:** pending
**Prior:** Task 2
**Consumes:** 1:contract
"#;

fn recovery_case(prefix: &str) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(&dir, "plan.rhei.md", OLD_SHAPE_PLAN);
    write_fixture_file(&dir, "states.yaml", MACHINE);
    let agent = write_python_agent(&dir, "consumer.py", AGENT);
    write_mock_agent_settings(&dir, &agent);
    let export = dir.join("runtime/exports/plan.1/contract.md");
    fs::create_dir_all(export.parent().expect("export parent")).expect("export directory");
    fs::write(export, "approved contract\n").expect("producer export");
    (dir, plan)
}

fn migrate(target: &Path, dry_run: bool) -> CliRun {
    let mut command = rhei_command(target.parent().expect("target parent").join(".home"));
    command.arg("migrate").arg("export-priors");
    if dry_run {
        command.arg("--dry-run");
    }
    command.arg(target);
    let output = command.output().expect("rhei migrate should run");
    CliRun::from(&output)
}

fn assert_unchanged(path: &Path, before: &[u8], context: &str) {
    assert_eq!(fs::read(path).expect("authored file after command"), before, "{context}");
}

/// Strict validation and execution stay read-only, but make the one-command
/// compatibility path copyable and identify both ends of the missing edge.
// §FS-rhei-migrate.5 §FS-rhei-validate.4.3 §FS-rhei-run.3
#[test]
fn validate_and_run_offer_migration_without_editing_the_old_shape() {
    let (dir, plan) = recovery_case("export-prior-guidance");
    let before = fs::read(&plan).expect("plan before refusal");
    let help = format!(
        "help: rhei migrate export-priors {}",
        shell_quote(plan.to_str().expect("UTF-8 fixture path"))
    );

    let validated = run_cli_without_machine("validate", &plan, &[]);
    assert!(!validated.status.success(), "the unversioned old shape remains invalid");
    assert_stderr_contains(&validated, "Task plan.3 consumes export 'contract' from Task plan.1");
    assert_stderr_contains(&validated, &help);
    assert_unchanged(&plan, &before, "validation must not migrate");

    let run = run_cli_without_machine("run", &plan, &["--no-tui", "--no-callbacks"]);
    assert!(!run.status.success(), "run must stop at strict validation");
    assert_stderr_contains(&run, "Task plan.3 consumes export 'contract' from Task plan.1");
    assert_stderr_contains(&run, &help);
    assert_unchanged(&plan, &before, "run must not migrate");
    assert!(!dir.join("runtime/spawned-plan.3.txt").exists(), "consumer must not spawn");
}

/// Each post-diagnostic stage is entered directly so a missing help line in
/// the preceding test cannot conceal a broken preview, rewrite, or resumed run.
// §FS-rhei-migrate.2 §FS-rhei-migrate.3 §FS-rhei-migrate.5
#[test]
fn dry_run_then_migration_releases_the_open_consumer_without_manual_edits() {
    let (dir, plan) = recovery_case("export-prior-recovery");
    let before = fs::read(&plan).expect("plan before preview");

    let preview = migrate(&plan, true);
    assert_success(&preview);
    assert_eq!(
        preview.stdout,
        format!(
            "Would add Task 1 to Task plan.3 **Prior:** in {}\n\
             Added 1 direct Prior edge(s) in 1 file(s)\n",
            plan.display()
        )
    );
    assert_unchanged(&plan, &before, "dry-run must not replace authored bytes");

    let migrated = migrate(&plan, false);
    assert_success(&migrated);
    assert_eq!(
        migrated.stdout,
        format!(
            "Added Task 1 to Task plan.3 **Prior:** in {}\n\
             Added 1 direct Prior edge(s) in 1 file(s)\n",
            plan.display()
        )
    );
    let after = fs::read_to_string(&plan).expect("migrated plan");
    assert!(after.contains("**Prior:** Task 2, Task 1\n**Consumes:** 1:contract"), "{after}");

    let validated = run_cli_without_machine("validate", &plan, &[]);
    assert_success(&validated);
    assert!(validated.stdout.contains("Validation succeeded"), "{}", validated.stdout);
    assert!(validated.stdout.contains("warning: **Consumes:**"), "{}", validated.stdout);

    let run = run_cli_without_machine("run", &plan, &["--no-tui", "--no-callbacks"]);
    assert_success(&run);
    assert_task_state(&plan, &dir.join("states.yaml"), "3", "completed");
    let prompt = fs::read_to_string(dir.join("runtime/spawned-plan.3.txt"))
        .expect("consumer spawn evidence");
    assert!(prompt.contains("approved contract"), "{prompt}");
}

/// Insertion and append preserve surrounding text, producer kind spelling,
/// existing reference spelling, first-Consumes order, and edge deduplication.
// §FS-rhei-migrate.2 §FS-rhei-migrate.2.1
#[test]
fn rewrite_is_minimal_ordered_kind_preserving_and_idempotent() {
    let plan_text = r#"---
structure:
  nodeKinds: [task, review]
---
# Rhei: Rewrite shape
**States:** migrate

Authored preface.

## Tasks

### Review 1: First producer
**State:** completed
**Provides:** alpha, second

### Task 2: Existing prior
**State:** completed

### Task 3: Second producer
**State:** completed
**Provides:** beta

### Task 4: Append consumer
**State:** pending
**Prior:** 2
**Consumes:** 1:alpha, 1:second, 3:beta
Keep this prose byte-for-byte.

### Task 5: Insert consumer
**State:** pending
**Consumes:** 3:beta
"#;
    let dir = unique_temp_dir("export-prior-rewrite");
    let plan = write_fixture_file(&dir, "plan.rhei.md", plan_text);
    write_fixture_file(&dir, "states.yaml", MACHINE);

    let migrated = migrate(&plan, false);
    assert_success(&migrated);
    assert_eq!(migrated.stdout.matches("Added Review 1 to Task plan.4").count(), 1);
    let once = fs::read(&plan).expect("first migration");
    let text = String::from_utf8(once.clone()).expect("UTF-8 plan");
    assert!(
        text.contains(
            "**Prior:** 2, Review 1, Task 3\n**Consumes:** 1:alpha, 1:second, 3:beta\n\
             Keep this prose byte-for-byte."
        ),
        "{text}"
    );
    assert!(text.contains("### Task 5: Insert consumer\n**State:** pending\n**Prior:** Task 3\n**Consumes:** 3:beta"), "{text}");

    let repeated = migrate(&plan, false);
    assert_success(&repeated);
    assert_eq!(repeated.stdout, "No export Prior migration needed\n");
    assert_unchanged(&plan, &once, "repeat migration must be byte-for-byte no-op");
}

/// Directory Workspace consumers are rewritten in their owning task files;
/// the workspace index and producer files are not collateral rewrite targets.
// §FS-rhei-migrate.1 §FS-rhei-migrate.2
#[test]
fn directory_workspace_rewrites_only_the_consumer_owning_file() {
    let (dir, workspace, _) = create_workspace(
        "export-prior-directory",
        "# Rhei: Directory migration\n**States:** migrate\n",
        &[
            (
                "01-producer.md",
                "### Task 1: Producer\n**State:** completed\n**Provides:** report\n",
            ),
            (
                "02-middle.md",
                "### Task 2: Middle\n**State:** completed\n**Prior:** Task 1\n",
            ),
            (
                "03-consumer.md",
                "### Task 3: Consumer\n**State:** pending\n**Prior:** Task 2\n**Consumes:** 1:report\n",
            ),
        ],
    );
    fs::write(workspace.join("states.yaml"), MACHINE).expect("workspace machine");
    let index = workspace.join("index.rhei.md");
    let producer = workspace.join("tasks/01-producer.md");
    let consumer = workspace.join("tasks/03-consumer.md");
    let index_before = fs::read(&index).expect("index before");
    let producer_before = fs::read(&producer).expect("producer before");

    let migrated = migrate(&workspace, false);
    assert_success(&migrated);
    assert!(fs::read_to_string(&consumer)
        .expect("consumer after")
        .contains("**Prior:** Task 2, Task 1\n**Consumes:** 1:report"));
    assert_unchanged(&index, &index_before, "workspace index is unrelated authored data");
    assert_unchanged(&producer, &producer_before, "producer file must not be rewritten");
    drop(dir);
}

/// A member target widens to Panta: it repairs both the member's qualified
/// cross-rhei edge and another member's local edge in their owning files.
// §FS-rhei-migrate.1 §FS-rhei-panta.6
#[test]
fn panta_member_target_widens_and_preserves_local_vs_qualified_references() {
    let dir = unique_temp_dir("export-prior-panta");
    let project = dir.join("project");
    fs::create_dir_all(&project).expect("project");
    write_fixture_file(
        &project,
        "index.panta.md",
        "# Panta: Migration scope\n**States:** migrate\n",
    );
    write_fixture_file(&project, "states.yaml", MACHINE);
    let producer = write_fixture_file(
        &project,
        "producer.rhei.md",
        r#"# Rhei: Producer

## Tasks

### Task 1: Publish
**State:** completed
**Provides:** context

### Task 2: Middle
**State:** completed
**Prior:** Task 1

### Task 3: Local consumer
**State:** pending
**Prior:** Task 2
**Consumes:** 1:context
"#,
    );
    let consumer = write_fixture_file(
        &project,
        "consumer.rhei.md",
        r#"# Rhei: Consumer

## Tasks

### Task 1: Cross-rhei middle
**State:** completed
**Prior:** Task producer.1

### Task 2: Cross-rhei consumer
**State:** pending
**Prior:** Task 1
**Consumes:** producer.1:context
"#,
    );

    let migrated = migrate(&consumer, false);
    assert_success(&migrated);
    assert!(migrated.stdout.contains("Added 2 direct Prior edge(s) in 2 file(s)"));
    assert!(fs::read_to_string(&producer)
        .expect("producer member")
        .contains("**Prior:** Task 2, Task 1\n**Consumes:** 1:context"));
    assert!(fs::read_to_string(&consumer)
        .expect("consumer member")
        .contains("**Prior:** Task 1, Task producer.1\n**Consumes:** producer.1:context"));
}
