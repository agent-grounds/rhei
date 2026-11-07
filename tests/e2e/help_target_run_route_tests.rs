//! What `rhei run` prints about a budget or a snapshot names the target the run was
//! given, so the line runs as printed from where the run was typed: a spent lifetime
//! allowance, a snapshot missing, misnamed or matched more than once, and a corrupt
//! one, which no command repairs. §FS-rhei-errors.1.2.1
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use super::help_target_paste_tests::{assert_on_one_line, write};
use super::help_target_tests::{pasted, Case};
use super::snapshot_tests::{write_fake_snapshot_agent, write_fake_snapshot_settings};
use super::*;

/// One agent state, so each ticket a run takes up spends one invocation.
const WORK_MACHINE: &str = r#"name: work
version: 1
states:
  work:
    initial: true
    description: Do the work
    agent: mock
    agent_timeout: 30s
  done:
    description: Done
    final: true
transitions:
  - { from: work, to: done, description: Work done }
"#;

const TWO_TICKETS: &str = r#"# Rhei: Two

## Tasks

### Task 1: One
**State:** work

### Task 2: Two
**State:** work
"#;

/// The account's settings name an agent that does its work and exits.
fn mock_agent(case: &Case) {
    let agent = write(
        &case.root,
        "agent.sh",
        "cat > /dev/null\nmkdir -p \"$RHEI_ROOT/runtime\"\n\
         [ -n \"${RHEI_RESULT_PATH:-}\" ] && { mkdir -p \"$(dirname \"$RHEI_RESULT_PATH\")\"; \
         echo done > \"$RHEI_RESULT_PATH\"; }\nexit 0\n",
    );
    let settings = serde_json::json!({
        "defaults": { "agent": "mock", "agent_timeout": "30s" },
        "agents": { "mock": {
            "command": ["bash", agent.display().to_string()],
            "stdin_prompt": true,
            "timeout": "30s",
        } },
    });
    write(&case.home, ".config/rhei/settings.json", &settings.to_string());
}

/// A run that spends a lifetime allowance says how to raise it for the project it was
/// given, which from the parent a bare `rhei budget adjust` cannot find.
/// §FS-rhei-errors.1.2.1 §FS-rhei-budgets.10
#[test]
fn help_target_a_spent_allowance_adjusts_the_target_the_run_was_given() {
    let case = Case::new("help-target-allowance");
    let typed = "my project/plan.rhei.md";
    write(&case.root.join("my project"), "plan.rhei.md", TWO_TICKETS);
    write(&case.root.join("my project"), "states.yaml", WORK_MACHINE);
    mock_agent(&case);
    let init =
        case.rhei(&case.root, &["budget", "init", typed, "--invocations", "1", "--reason", "x"]);
    assert!(init.status.success(), "the account is established:\n{}", raw_stderr(&init));

    let out =
        case.rhei(&case.root, &["run", typed, "--no-tui", "--parallel", "1", "--no-callbacks"]);

    let printed = format!("{}{}", stdout(&out), raw_stderr(&out));
    assert!(printed.contains("has spent its invocation allowance"), "one ticket halts:\n{printed}");
    let command =
        format!("rhei budget adjust {} --invocations <N> --reason <TEXT>", shell_quote(typed));
    assert_on_one_line(&printed, &format!("to raise it: run `{command}`"));
    let filled = command.replace("<N>", "5").replace("<TEXT>", "more");
    let again = pasted(&filled, &case.root, &case.home);
    assert!(again.status.success(), "`{filled}` should run:\n{}", again.stderr);
    assert!(again.stdout.contains("lifetime invocation allowance: 5"), "{}", again.stdout);
}

/// The target every snapshot case runs, typed from the parent.
const TARGET: &str = "proj/plan.rhei.md";

/// `source` emits `implementation` and `consume` must inherit it.
const EMIT_MACHINE: &str = r#"name: snapshot-emit
version: 1
states:
  source:
    initial: true
    description: Produce the named session
    target: fake:acme:model-a
    snapshot:
      emit: { name: implementation, on: always }
  consume:
    description: Continue the predecessor
    target: fake:acme:model-a
    snapshot:
      inherit: { name: implementation, from: prior, required: true }
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: completed
  - from: consume
    to: completed
"#;

/// One source and its consumer, held while the source runs.
const ONE_SOURCE: &str = r#"# Rhei: Emit

## Tasks

### Task source: Source
**State:** source

### Task consume: Consumer
**State:** consume
**Prior:** Task source
**Assignee:** someone-else
"#;

/// Two sources of the same name, and a consumer of both, held while they run.
const TWO_SOURCES: &str = r#"# Rhei: Two

## Tasks

### Task one: Source one
**State:** source

### Task two: Source two
**State:** source

### Task consume: Consumer
**State:** consume
**Prior:** Task one, Task two
**Assignee:** someone-else
"#;

/// The snapshot agent's project at `proj/`, under `plan`.
fn snapshot_project(case: &Case, plan: &str) -> PathBuf {
    let project = case.root.join("proj");
    write(&project, "plan.rhei.md", plan);
    write(&project, "states.yaml", EMIT_MACHINE);
    let agent = write_fake_snapshot_agent(&project);
    write_fake_snapshot_settings(&project, &agent);
    project
}

fn run_from_parent(case: &Case, extra: &[&str]) -> Output {
    let args = [&["run", TARGET, "--no-tui", "--parallel", "1", "--no-callbacks"][..], extra];
    case.rhei(&case.root, &args.concat())
}

/// Every source has run and emitted, and the consumer is released for the next run.
fn emitted(case: &Case, plan: &str) -> PathBuf {
    let project = snapshot_project(case, plan);
    let first = run_from_parent(case, &[]);
    let manifests = manifests_of(&project.join(".rhei/cache/snapshots"), "implementation");
    assert!(!manifests.is_empty(), "the sources emit:\n{}", raw_stderr(&first));
    let plan_path = project.join("plan.rhei.md");
    let held = fs::read_to_string(&plan_path).expect("read the plan");
    fs::write(&plan_path, held.replace("**Assignee:** someone-else\n", "")).expect("release");
    project
}

/// Every `manifest.json` beneath `dir` on a path through `name`.
fn manifests_of(dir: &Path, name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return found;
    };
    for path in entries.map(|entry| entry.expect("dir entry").path()) {
        if path.is_dir() {
            found.extend(manifests_of(&path, name));
        } else if path.ends_with("manifest.json")
            && path.components().any(|part| part.as_os_str() == name)
        {
            found.push(path);
        }
    }
    found
}

/// That `command` is on one line of `rendered`, and pasted from the parent lists `listed`.
#[track_caller]
fn assert_lists_from_parent(case: &Case, rendered: &str, command: &str, listed: &[&str]) {
    assert_on_one_line(rendered, command);
    let again = pasted(command, &case.root, &case.home);
    assert!(again.status.success(), "`{command}` should run as printed:\n{}", again.stderr);
    for task in listed {
        assert!(again.stdout.contains(task), "`{command}` should list {task}:\n{}", again.stdout);
    }
}

/// A required inherit with nothing cached lists the store of the plan it was given,
/// which a bare `rhei snapshot list` reads from `.`. §FS-rhei-errors.1.2.1
#[test]
fn help_target_a_missing_snapshot_lists_the_store_the_run_read() {
    let case = Case::new("help-target-snapshot-missing");
    let never_ran = ONE_SOURCE.replace("**State:** source", "**State:** completed");
    snapshot_project(&case, &never_ran.replace("**Assignee:** someone-else\n", ""));

    let out = run_from_parent(&case, &[]);

    let rendered = raw_stderr(&out);
    assert!(rendered.contains("missing-snapshot"), "the inherit refuses:\n{rendered}");
    let command = format!("rhei snapshot list --plan {TARGET}");
    assert_on_one_line(&rendered, &format!("List what is cached with: {command}, or run"));
    assert_lists_from_parent(&case, &rendered, &command, &[]);
}

/// A reference that matches nothing, and one of another name, each send the reader
/// to the store the run read. §FS-rhei-errors.1.2.1
#[test]
fn help_target_a_misnamed_snapshot_lists_the_store_the_run_read() {
    let case = Case::new("help-target-snapshot-misnamed");
    emitted(&case, ONE_SOURCE);
    let command = format!("rhei snapshot list --plan {TARGET}");

    for (reference, offered) in [
        ("plan.source:nope", format!("Copy one from: {command}")),
        ("plan.source:_state", format!("list them with: {command} — or relax")),
    ] {
        let out = run_from_parent(&case, &["--from-snapshot", reference]);

        let rendered = raw_stderr(&out);
        assert!(!out.status.success(), "`{reference}` is refused:\n{rendered}");
        assert_on_one_line(&rendered, &offered);
        assert_lists_from_parent(&case, &rendered, &command, &["plan.source"]);
    }
}

/// More than one source matches, and no `gc` policy tells distinct snapshots apart,
/// so the help names the key that does and what matched. §FS-rhei-errors.1.2.1
#[test]
fn help_target_an_ambiguous_snapshot_lists_what_matched() {
    let case = Case::new("help-target-snapshot-ambiguous");
    emitted(&case, TWO_SOURCES);

    let out = run_from_parent(&case, &[]);

    let rendered = raw_stderr(&out);
    assert!(rendered.contains("ambiguous-lineage"), "two sources match:\n{rendered}");
    assert!(!rendered.contains("rhei snapshot gc"), "no gc policy separates them:\n{rendered}");
    let command = format!("rhei snapshot list --plan {TARGET}");
    assert_on_one_line(&rendered, &format!("see what matches with: {command}"));
    assert_lists_from_parent(&case, &rendered, &command, &["plan.one", "plan.two"]);
}

/// `gc` skips the corrupt generation and leaves it for a person, so the help names the
/// directory to delete rather than a command that will not.
/// §FS-rhei-errors.1.2 §FS-rhei-snapshot-operations.1.6
#[test]
fn help_target_a_corrupt_snapshot_names_no_command_that_leaves_it_in_place() {
    let case = Case::new("help-target-snapshot-corrupt");
    let project = emitted(&case, ONE_SOURCE);
    for manifest in manifests_of(&project.join(".rhei/cache/snapshots"), "implementation") {
        fs::write(manifest, "{not json").expect("corrupt the manifest");
    }

    let out = case
        .rhei(&case.root, &["snapshot", "show", "plan.source:implementation", "--plan", TARGET]);

    let rendered = raw_stderr(&out);
    assert!(rendered.contains("failed to parse snapshot manifest"), "it is corrupt:\n{rendered}");
    assert!(!rendered.contains("rhei snapshot gc"), "gc leaves it in place:\n{rendered}");
    assert!(rendered.contains("generation directory"), "name what to delete:\n{rendered}");
}
