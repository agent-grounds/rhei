//! Task-level read boundaries, from authored metadata through validation,
//! prompt composition, manual work, and an enforcing custom profile.
//! §FS-rhei-plan-language.3.13 §FS-rhei-agents.3

use std::fs;
use std::path::{Path, PathBuf};

use super::new_tests::{new_run, project_with_rhei};
use super::*;

const SECRET: &str = "SIBLING-SECRET-207";

const EXCLUSION_MACHINE: &str = r#"name: exclusions-e2e
version: 1
states:
  work:
    initial: true
    concurrent: true
    agent: mock
    agent_timeout: 10s
  completed:
    final: true
transitions:
  - from: work
    to: completed
"#;

fn write_settings(workspace: &Path, command: &str, deny_read: bool) {
    let settings_dir = workspace.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("settings directory");
    let adapter = if deny_read { r#", "deny_read": { "path_flag": "--deny-read" }"# } else { "" };
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "mock": {{ "command": {command}, "stdin_prompt": true, "timeout": "10s"{adapter} }}
  }}
}}"#
        ),
    )
    .expect("settings file");
}

fn exclusion_workspace(prefix: &str) -> (TestDir, PathBuf, PathBuf) {
    let index = "# Rhei: Blind review\n";
    let tasks = [
        (
            "01-source.md",
            r#"### Task source: Source statement
**State:** completed
**Provides:** statement
"#,
        ),
        (
            "02-control.md",
            r#"### Task control: Unexcluded control
**State:** work
**Consumes:** source:statement
"#,
        ),
        (
            "03-blind.md",
            r#"### Task blind: Blind participant
**State:** work
**Excludes:** source:statement, artifact=runtime/results/workspace.source.md
"#,
        ),
    ];
    let (dir, workspace, machine) = create_workspace(prefix, index, &tasks);
    fs::write(&machine, EXCLUSION_MACHINE).expect("machine");
    let agent = write_python_agent(
        &dir,
        "capture-agent.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
local = env('RHEI_TASK_ID_LOCAL')
write(root / 'runtime' / 'prompts' / (local + '.md'), agent_prompt())
result('## Result\n\nCaptured exclusions prompt.\n')
"#,
    );
    write_settings(&workspace, &fixture_command(&agent), false);

    fs::create_dir_all(workspace.join("runtime/exports/workspace.source"))
        .expect("export directory");
    fs::create_dir_all(workspace.join("runtime/results")).expect("results directory");
    fs::write(
        workspace.join("runtime/exports/workspace.source/statement.md"),
        format!("## Statement\n\n{SECRET}\n"),
    )
    .expect("pre-created sibling export");
    fs::write(
        workspace.join("runtime/results/workspace.source.md"),
        format!("## Result\n\n{SECRET}\n"),
    )
    .expect("pre-created sibling result");
    (dir, workspace, machine)
}

fn assert_blind_run(prefix: &str, parallel: &str) {
    let (_dir, workspace, machine) = exclusion_workspace(prefix);
    let run = run_cli(
        "run",
        &workspace,
        &machine,
        &["--no-tui", "--no-callbacks", "--parallel", parallel],
    );
    assert_success(&run);

    let control =
        fs::read_to_string(workspace.join("runtime/prompts/control.md")).expect("control prompt");
    assert!(control.contains(SECRET), "control proves the source is normally composed:\n{control}");

    let blind =
        fs::read_to_string(workspace.join("runtime/prompts/blind.md")).expect("blind prompt");
    assert!(!blind.contains(SECRET), "excluded bytes leaked into the prompt:\n{blind}");
    for visible in [
        "# Task workspace.blind: Blind participant",
        "Task workspace.source: Source statement",
        "completed",
        "runtime/results/workspace.source.md",
        "Every rhei in this project and its execution root",
    ] {
        assert!(blind.contains(visible), "navigation {visible:?} was lost:\n{blind}");
    }
    assert!(blind.contains("## Exclusions"), "got:\n{blind}");
    assert!(
        blind.contains("composition only; paths remain readable outside Rhei-composed context"),
        "got:\n{blind}"
    );
}

/// A pre-existing sibling artifact stays blind in both scheduler paths, while
/// the unexcluded control demonstrates that its payload would otherwise be read.
/// §FS-rhei-agents.3 §FS-rhei-agents.5.2.1 §FS-rhei-agents.5.2.2
#[test]
fn serial_and_parallel_runs_filter_excluded_payload_but_keep_navigation() {
    assert_blind_run("exclusions-serial", "1");
    assert_blind_run("exclusions-parallel", "2");
}

/// Manual workers receive the same filtering, but never a filesystem-denial
/// claim because `rhei next` does not spawn them. §FS-rhei-memory.5
#[test]
fn next_filters_excluded_history_and_reports_composition_only() {
    let (_dir, workspace, machine) = exclusion_workspace("exclusions-next");
    let next = run_cli("next", &workspace, &machine, &["--task", "blind", "--peek"]);
    assert_success(&next);
    assert!(!next.stdout.contains(SECRET), "got:\n{}", next.stdout);
    assert!(
        next.stdout.contains("Task workspace.source: Source statement"),
        "got:\n{}",
        next.stdout
    );
    assert!(next.stdout.contains("runtime/results/workspace.source.md"), "got:\n{}", next.stdout);
    assert!(
        next.stdout
            .contains("composition only; paths remain readable outside Rhei-composed context"),
        "got:\n{}",
        next.stdout
    );
}

/// Export resolution is graph-based: the producer and declaration must exist,
/// while its runtime file may be written later. §FS-rhei-plan-language.3.13
#[test]
fn validate_accepts_unwritten_declared_export_and_root_relative_paths() {
    let plan = r#"# Rhei: Future export

## Tasks

### Task source: Future producer
**State:** pending
**Provides:** statement

### Task blind: Blind consumer
**State:** pending
**Excludes:** source:statement, checkout=notes/future file.md, artifact=runtime/reviews/
"#;
    let (_dir, plan_path, machine) = setup_single_file("exclusions-future", plan);
    let validate = run_cli("validate", &plan_path, &machine, &[]);
    assert_success(&validate);
}

fn assert_exclusion_error(prefix: &str, exclusion: &str, extra: &str, expected: &str) {
    let plan = format!(
        "# Rhei: Invalid exclusion\n\n## Tasks\n\n\
         ### Task source: Producer\n**State:** pending\n**Provides:** statement\n\n\
         ### Task blind: Consumer\n**State:** pending\n{extra}\
         **Excludes:** {exclusion}\n"
    );
    let (_dir, plan_path, machine) = setup_single_file(prefix, &plan);
    let validate = run_cli("validate", &plan_path, &machine, &[]);
    assert!(!validate.status.success(), "validation should reject {exclusion}");
    assert!(validate.stderr.contains(expected), "expected {expected:?}, got:\n{}", validate.stderr);
}

/// Malformed paths, unresolved graph references, duplicates, and positive /
/// negative data-flow contradictions are distinct diagnostics. §FS-rhei-validate.4
#[test]
fn validate_rejects_malformed_unresolved_duplicate_and_consumed_exclusions() {
    assert_exclusion_error("exclude-dotdot", "checkout=../secret", "", "must not contain '..'");
    assert_exclusion_error(
        "exclude-missing-task",
        "missing:statement",
        "",
        "unknown task 'missing'",
    );
    assert_exclusion_error(
        "exclude-missing-name",
        "source:missing",
        "",
        "does not provide 'missing'",
    );
    assert_exclusion_error(
        "exclude-duplicate",
        "artifact=runtime/private.md, artifact=runtime/private.md",
        "",
        "duplicate exclusion",
    );
    assert_exclusion_error(
        "exclude-consumed",
        "source:statement",
        "**Consumes:** source:statement\n",
        "both consumes and excludes",
    );
    assert_exclusion_error(
        "exclude-consumed-ancestor",
        "artifact=runtime/exports/plan.source/",
        "**Consumes:** source:statement\n",
        "contains consumed export",
    );
}

/// Current sources and required state inputs cannot be made optional by an
/// exclusion. §FS-rhei-plan-language.3.13 §FS-rhei-validate.4
#[test]
fn validate_rejects_current_sources_and_required_inputs() {
    for (prefix, exclusion, expected) in [
        ("exclude-task-source", "artifact=plan.rhei.md", "current task source"),
        ("exclude-machine-source", "artifact=states.yaml", "active state-machine source"),
    ] {
        assert_exclusion_error(prefix, exclusion, "", expected);
    }

    let machine = r#"name: required-input
version: 1
states:
  pending:
    initial: true
    inputs:
      - { name: brief, path: runtime/brief.md }
  completed: { final: true }
transitions: [{ from: pending, to: completed }]
"#;
    let plan = r#"# Rhei: Required input

## Tasks

### Task 1: Worker
**State:** pending
**Excludes:** artifact=runtime/brief.md
"#;
    let (dir, plan_path, machine_path) = setup_single_file("exclude-required", plan);
    fs::write(&machine_path, machine).expect("required-input machine");
    fs::create_dir_all(dir.join("runtime")).expect("runtime directory");
    fs::write(dir.join("runtime/brief.md"), "required\n").expect("required input");
    let validate = run_cli("validate", &plan_path, &machine_path, &[]);
    assert!(!validate.status.success());
    assert!(validate.stderr.contains("required input 'brief'"), "got:\n{}", validate.stderr);
}

/// `rhei new` authors the field in grammar order, and both renderers expose it
/// according to their metadata contracts. §FS-rhei-new.1.3 §FS-rhei-render.3
#[test]
fn new_and_render_preserve_exclusions_and_no_metadata_hides_them() {
    let dir = project_with_rhei("new-exclusions");
    let created = new_run(
        &[
            "new",
            "Blind review",
            "--under",
            "auth",
            "--excludes",
            "checkout=notes/private.md",
            "--excludes",
            "artifact=runtime/reviews/",
            "--assignee",
            "manual",
        ],
        &dir,
    );
    assert_success(&created);
    let plan_path = dir.join("auth.rhei.md");
    let plan = fs::read_to_string(&plan_path).expect("created plan");
    assert!(
        plan.contains(
            "**Excludes:** checkout=notes/private.md, artifact=runtime/reviews/\n**Assignee:** manual"
        ),
        "got:\n{plan}"
    );

    let json = new_run(&["render", plan_path.to_str().unwrap(), "--format", "json"], &dir);
    assert_success(&json);
    let rendered: serde_json::Value = serde_json::from_str(&json.stdout).expect("rendered JSON");
    assert_eq!(
        rendered["tasks"][0]["excludes"],
        serde_json::json!([
            { "kind": "checkout", "path": "notes/private.md" },
            { "kind": "artifact", "path": "runtime/reviews/", "recursive": true }
        ])
    );

    let github = new_run(&["render", plan_path.to_str().unwrap(), "--format", "github"], &dir);
    assert_success(&github);
    assert!(github.stdout.contains("**Excludes:** checkout=notes/private.md"));
    let hidden = new_run(
        &["render", plan_path.to_str().unwrap(), "--format", "github", "--no-metadata"],
        &dir,
    );
    assert_success(&hidden);
    assert!(!hidden.stdout.contains("**Excludes:**"));
}

/// A custom adapter is proven by real filesystem reads: the wrapper removes
/// each resolved target from its process view before the agent body opens it.
/// Allowed reads still succeed. §FS-rhei-agents.1.1.2 §FS-rhei-agents.3
#[test]
fn deny_read_adapter_blocks_file_directory_descendant_and_export_reads() {
    let index = "# Rhei: Adapter\n";
    let tasks = [
        (
            "01-source.md",
            "### Task source: Producer\n**State:** completed\n**Provides:** statement\n",
        ),
        (
            "02-blind.md",
            "### Task blind: Consumer\n**State:** work\n**Excludes:** checkout=private.txt, checkout=private-dir/, source:statement\n",
        ),
    ];
    let (dir, workspace, machine) = create_workspace("deny-read-adapter", index, &tasks);
    let git = std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(&workspace)
        .status()
        .expect("git init should run");
    assert!(git.success(), "fixture checkout should initialize");
    fs::write(&machine, EXCLUSION_MACHINE).expect("machine");
    fs::write(workspace.join("allowed.txt"), "allowed\n").expect("allowed file");
    fs::write(workspace.join("private.txt"), "private\n").expect("private file");
    fs::write(workspace.join("private.txt.copy"), "allowed exact-name neighbor\n")
        .expect("exact-name neighbor");
    fs::create_dir_all(workspace.join("private-dir")).expect("private directory");
    fs::write(workspace.join("private-dir/child.txt"), "private child\n").expect("private child");
    fs::create_dir_all(workspace.join("private-dir-copy")).expect("directory-name neighbor");
    fs::write(workspace.join("private-dir-copy/child.txt"), "allowed directory-name neighbor\n")
        .expect("directory-name neighbor child");
    fs::create_dir_all(workspace.join("runtime/exports/workspace.source")).expect("exports");
    fs::write(workspace.join("runtime/exports/workspace.source/statement.md"), SECRET)
        .expect("export");

    let allowed = serde_json::to_string(&workspace.join("allowed.txt")).unwrap();
    let denied_file = serde_json::to_string(&workspace.join("private.txt")).unwrap();
    let exact_neighbor = serde_json::to_string(&workspace.join("private.txt.copy")).unwrap();
    let denied_child = serde_json::to_string(&workspace.join("private-dir/child.txt")).unwrap();
    let directory_neighbor =
        serde_json::to_string(&workspace.join("private-dir-copy/child.txt")).unwrap();
    let denied_export =
        serde_json::to_string(&workspace.join("runtime/exports/workspace.source/statement.md"))
            .unwrap();
    let body = format!(
        r#"args = sys.argv[1:]
separator = args.index('--') if '--' in args else len(args)
if '--deny-read' in args[separator:]:
    sys.exit(12)
denied = []
while '--deny-read' in args:
    at = args.index('--deny-read')
    target = pathlib.Path(args[at + 1])
    denied.append(target)
    del args[at:at + 2]
for number, target in enumerate(denied):
    if target.exists() or target.is_symlink():
        target.replace(target.with_name(target.name + '.denied-' + str(number)))
observed = []
for label, raw, should_read in [
    ('allowed', {allowed}, True),
    ('file', {denied_file}, False),
    ('exact-neighbor', {exact_neighbor}, True),
    ('directory-child', {denied_child}, False),
    ('directory-neighbor', {directory_neighbor}, True),
    ('export', {denied_export}, False),
]:
    try:
        pathlib.Path(raw).read_text(encoding='utf-8')
        observed.append(label + '=read')
    except OSError:
        observed.append(label + '=denied')
write(pathlib.Path(env('RHEI_ROOT')) / 'runtime' / 'adapter-observed.txt', '\n'.join(observed) + '\n')
result('## Result\n\nAdapter exercised.\n')
"#
    );
    let agent = write_python_agent(&dir, "deny-wrapper.py", &body);
    write_settings(&workspace, &fixture_command(&agent), true);

    let run = run_cli("run", &workspace, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&run);
    assert_eq!(
        fs::read_to_string(workspace.join("runtime/adapter-observed.txt")).unwrap(),
        "allowed=read\nfile=denied\nexact-neighbor=read\ndirectory-child=denied\n\
         directory-neighbor=read\nexport=denied\n"
    );
}

/// Compatibility control: plans that do not opt in keep their existing prompt
/// and run behavior. §FS-rhei-plan-language.3.13
#[test]
fn a_plan_without_exclusions_is_unchanged() {
    let plan = "# Rhei: Control\n\n## Tasks\n\n### Task 1: Work\n**State:** draft\n";
    let (_dir, plan_path, machine) = setup_single_file("exclusions-control", plan);
    let validate = run_cli("validate", &plan_path, &machine, &[]);
    assert_success(&validate);
    let rendered = run_cli("render", &plan_path, &machine, &["--format", "json"]);
    assert_success(&rendered);
    assert!(!rendered.stdout.contains("Exclusions"));
}
