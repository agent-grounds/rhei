//! A plan may quote the plan format, and one that does is still a plan.
//!
//! The language has one definition of a fenced code block — a run of three or
//! more backticks or tildes, closed only by a bare run of the same character at
//! least as long — and the structural scan that decides which `##` lines are
//! chapters reads it. §FS-rhei-plan-language.2.1
//!
//! These are black-box contracts because the cost of getting it wrong is paid at
//! the command: a plan that quotes a plan is refused, and one refused plan takes
//! the whole task store down with it. §FS-rhei-validate.4

use std::fs;
use std::path::Path;

use super::*;

/// The plan from agent-grounds/rhei#336: a `~~~markdown` fence in a content
/// section quoting the shape of a plan, then the plan's own tasks chapter.
const TILDE_QUOTED_PLAN: &str = r#"# Rhei: A plan that quotes a plan

## Notes

~~~markdown
## Tasks

### Task demo-1: an illustration
~~~

## Tasks

### Task real-1: the only real task
**State:** pending

Body.
"#;

/// A four-backtick fence holding a three-backtick run that is never closed —
/// the shape a pasted forge comment takes when the paste is truncated inside a
/// transcript. This is the plan that cost a queue.
const TRUNCATED_TRANSCRIPT_PLAN: &str = r#"# Rhei: A plan that quotes a transcript

## The conversation

````markdown
# Proposal

```console
$ rhei validate plan.rhei.md
## Terms

Truncated mid-transcript.
````

## Tasks

### Task real-1: the only real task
**State:** pending

Body.
"#;

/// The same plan with the inner run closed.
const CLOSED_TRANSCRIPT_PLAN: &str = r#"# Rhei: A plan that quotes a transcript

## The conversation

````markdown
# Proposal

```console
$ rhei validate plan.rhei.md
## Terms
```

Closed mid-transcript.
````

## Tasks

### Task real-1: the only real task
**State:** pending

Body.
"#;

const HEALTHY_PLAN: &str = r#"# Rhei: Healthy

## Tasks

### Task fine-1: nothing wrong
**State:** pending

Body.
"#;

fn run_in(cwd: &Path, args: &[&str]) -> CliRun {
    let output = rhei_command(cwd.join(".home"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("rhei command should run");
    CliRun::from(&output)
}

fn assert_validated(result: &CliRun, what: &str) {
    assert_eq!(result.status.code(), Some(0), "{what} must validate; stderr:\n{}", result.stderr);
}

/// §FS-rhei-plan-language.2.1: a tilde fence is a fence, so the `## Tasks` it
/// quotes is not a chapter and the plan's own one is still the final chapter.
#[test]
fn a_plan_may_quote_a_tasks_chapter_in_a_tilde_fence() {
    let dir = unique_temp_dir("fence-tilde");
    write_fixture_file(&dir, "quoting.rhei.md", TILDE_QUOTED_PLAN);

    let validated = run_in(&dir, &["validate", "quoting.rhei.md"]);
    assert_validated(&validated, "a plan quoting a plan in a tilde fence");

    let listed = run_in(&dir, &["list", "quoting.rhei.md"]);
    assert!(
        listed.stdout.contains("the only real task"),
        "the authored task must load; stdout:\n{}",
        listed.stdout
    );
    assert!(
        !listed.stdout.contains("an illustration"),
        "the quoted heading must not author a task; stdout:\n{}",
        listed.stdout
    );
}

/// §FS-rhei-plan-language.2.1: a three-backtick run does not close a
/// four-backtick fence, so a truncated paste cannot turn a quotation into
/// structure.
#[test]
fn a_plan_may_quote_a_transcript_whose_inner_fence_is_unclosed() {
    let dir = unique_temp_dir("fence-truncated");
    write_fixture_file(&dir, "pasted.rhei.md", TRUNCATED_TRANSCRIPT_PLAN);

    let validated = run_in(&dir, &["validate", "pasted.rhei.md"]);
    assert_validated(&validated, "a plan quoting a truncated transcript");

    let listed = run_in(&dir, &["list", "pasted.rhei.md"]);
    assert!(
        listed.stdout.contains("the only real task"),
        "the authored task must load; stdout:\n{}",
        listed.stdout
    );
}

/// §FS-rhei-plan-language.2.1: the control, and it passes before the fix. The
/// same plan with its inner run closed lands a line-by-line toggle back where it
/// started, so whether the plan was accepted at all turned on how many inner
/// fence lines the paste happened to carry — which is not a property of the
/// format. A fixture that closes its inner block pins nothing.
#[test]
fn a_plan_whose_quoted_transcript_closes_itself_already_validated() {
    let dir = unique_temp_dir("fence-closed");
    write_fixture_file(&dir, "pasted.rhei.md", CLOSED_TRANSCRIPT_PLAN);

    let validated = run_in(&dir, &["validate", "pasted.rhei.md"]);
    assert_validated(&validated, "a plan quoting a closed transcript");
}

/// §FS-rhei-plan-language.2.1, §FS-rhei-validate.4: the blast radius. One plan
/// the fence reader cannot read fails the whole project load, so a store's
/// well-formed plans go down with it — which is what made this a defect worth
/// stopping a queue for rather than one author's problem.
#[test]
fn one_quoted_chapter_does_not_refuse_the_whole_store() {
    let dir = unique_temp_dir("fence-store");
    let store = dir.join("store");
    fs::create_dir_all(&store).expect("create store");
    write_fixture_file(&store, "index.panta.md", "# Panta: Fence store\n");
    write_fixture_file(&store, "quoting.rhei.md", TILDE_QUOTED_PLAN);
    write_fixture_file(&store, "healthy.rhei.md", HEALTHY_PLAN);

    let validated = run_in(&dir, &["validate", "store"]);
    assert_validated(&validated, "a store holding a plan that quotes a plan");

    let listed = run_in(&dir, &["list", "store"]);
    assert!(
        listed.stdout.contains("quoting.real-1"),
        "the quoting plan's task must load; stdout:\n{}",
        listed.stdout
    );
    assert!(
        listed.stdout.contains("healthy.fine-1"),
        "the plan beside it must not go down with it; stdout:\n{}",
        listed.stdout
    );
}

/// §FS-rhei-plan-language.2.1: the workspace index reads a fence by the same
/// rule, so the manifest of a workspace may document the format too.
#[test]
fn a_workspace_index_may_quote_a_tasks_chapter() {
    let dir = unique_temp_dir("fence-workspace");
    let workspace = dir.join("workspace");
    fs::create_dir_all(workspace.join("tasks")).expect("create workspace");
    write_fixture_file(
        &workspace,
        "index.rhei.md",
        r#"# Rhei: A workspace that quotes a plan

## Notes

~~~markdown
## Tasks

### Task demo-1: an illustration
~~~

More notes.
"#,
    );
    write_fixture_file(
        &workspace.join("tasks"),
        "one.md",
        "### Task real-1: the only real task\n**State:** pending\n\nBody.\n",
    );

    let validated = run_in(&dir, &["validate", "workspace"]);
    assert_validated(&validated, "a workspace index quoting a plan");

    let listed = run_in(&dir, &["list", "workspace"]);
    assert!(
        listed.stdout.contains("the only real task"),
        "the workspace's own task must load; stdout:\n{}",
        listed.stdout
    );
}
