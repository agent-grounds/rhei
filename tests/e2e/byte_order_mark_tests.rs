//! A file a user authors loads the same whether or not their editor saved it
//! with a UTF-8 byte order mark (#630). §REQ-cross-platform.9
//!
//! Each test runs one command twice, in two directories that differ only in the
//! three leading bytes `EF BB BF` of the file under test, and asks for the same
//! outcome from both: the state machine of §FS-rhei-states.1, the template
//! manifest and the `--values` file of §FS-rhei-templates, and the plan and task
//! files of §FS-rhei-language-reference.1.

use std::path::Path;

use super::*;

const MARK: &[u8] = b"\xef\xbb\xbf";

/// The machine from #630.
const MACHINE: &str = "name: m\nversion: 1\nstates:\n  o:\n    initial: true\n  d:\n    final: true\ntransitions:\n  - {from: o, to: d}\n";

/// The template both template tests instantiate: two inputs, so a values file
/// can carry the two keys a mark turns into "more than one document".
const MANIFEST: &str = "name: probe\nversion: 1.0.0\ndescription: Probe.\ninputs:\n  - name: title\n    description: The plan title.\n    type: string\n    required: true\n  - name: note\n    description: The task title.\n    type: string\n    default: none\n";
const TEMPLATE_PLAN: &str =
    "# Rhei: {{ title }}\n\n## Tasks\n\n### Task 1: {{ note }}\n**State:** pending\n";
const VALUES: &str = "title: Marked\nnote: kept\n";

const PLAN: &str = "# Rhei: Example\n\n---\nmetadata:\n  tasks:\n    1:\n      stateVisits:\n        review: 2\n---\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n";
const TASK_FILE: &str = "---\nmetadata:\n  tasks:\n    a:\n      priority: high\n---\n\n### Task a: Alpha\n**State:** pending\n";

fn write_bytes(dir: &Path, name: &str, text: &str, marked: bool) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture dir");
    let bytes = if marked { [MARK, text.as_bytes()].concat() } else { text.as_bytes().to_vec() };
    std::fs::write(path, bytes).expect("fixture file should be written");
}

fn run_in(dir: &Path, args: &[&str]) -> CliRun {
    let home = dir.join("home");
    CliRun::from(
        &rhei_command(&home).current_dir(dir).args(args).output().expect("rhei command should run"),
    )
}

/// Run `args` over a fixture written plain and one written with the mark, and
/// require the marked run to succeed with the plain run's stdout.
fn same_outcome(prefix: &str, write: impl Fn(&Path, bool), args: &[&str]) -> (TestDir, TestDir) {
    let plain = unique_temp_dir(&format!("{prefix}-plain"));
    let marked = unique_temp_dir(&format!("{prefix}-marked"));
    write(&plain, false);
    write(&marked, true);
    let expected = run_in(&plain, args);
    assert!(
        expected.status.success(),
        "the fixture loads without the mark\nstdout:\n{}\nstderr:\n{}",
        expected.stdout,
        expected.stderr
    );
    let got = run_in(&marked, args);
    assert!(
        got.status.success(),
        "`rhei {}` refused a file only because it starts with a byte order mark\nstderr:\n{}",
        args.join(" "),
        got.stderr
    );
    assert_eq!(got.stdout, expected.stdout, "the marked file reads as the plain one");
    (plain, marked)
}

/// The reported case: `rhei states --state-machine` on the machine from #630.
/// §FS-rhei-states.1 §FS-rhei-errors.1.2.2
#[test]
fn a_state_machine_saved_with_a_byte_order_mark_loads() {
    same_outcome(
        "bom-states",
        |dir, marked| write_bytes(dir, "states.yaml", MACHINE, marked),
        &["states", "--state-machine", "states.yaml"],
    );
}

/// Exactly one mark is set aside: a second is content, so the machine is still
/// refused. Green today, and kept green by the fix. §REQ-cross-platform.9
#[test]
fn a_second_byte_order_mark_is_content() {
    let dir = unique_temp_dir("bom-states-twice");
    write_bytes(&dir, "states.yaml", &format!("\u{feff}{MACHINE}"), true);
    let run = run_in(&dir, &["states", "--state-machine", "states.yaml"]);
    assert_eq!(run.status.code(), Some(1), "stdout:\n{}\nstderr:\n{}", run.stdout, run.stderr);
}

/// `rhei instantiate --list-inputs` on a template whose `template.yaml` has the mark.
/// §FS-rhei-templates
#[test]
fn a_template_manifest_saved_with_a_byte_order_mark_loads() {
    same_outcome(
        "bom-manifest",
        |dir, marked| {
            write_bytes(dir, "probe/template.yaml", MANIFEST, marked);
            write_bytes(dir, "probe/plan.rhei.md", TEMPLATE_PLAN, false);
        },
        &["instantiate", "./probe", "--list-inputs"],
    );
}

/// `rhei instantiate --values` with a values file that has the mark renders the
/// same plan. §FS-rhei-templates
#[test]
fn a_values_file_saved_with_a_byte_order_mark_loads() {
    let (plain, marked) = same_outcome(
        "bom-values",
        |dir, marked| {
            write_bytes(dir, "probe/template.yaml", MANIFEST, false);
            write_bytes(dir, "probe/plan.rhei.md", TEMPLATE_PLAN, false);
            write_bytes(dir, "values.yaml", VALUES, marked);
        },
        &["instantiate", "./probe", "--values", "values.yaml", "--output", "out"],
    );
    let read = |dir: &Path| std::fs::read_to_string(dir.join("out/plan.rhei.md")).expect("plan");
    assert_eq!(read(&marked), read(&plain));
    assert!(read(&marked).contains("# Rhei: Marked"), "the values reached the plan");
}

/// `rhei render` on a single-file plan that has the mark. §FS-rhei-language-reference.1
#[test]
fn a_plan_saved_with_a_byte_order_mark_loads() {
    same_outcome(
        "bom-plan",
        |dir, marked| write_bytes(dir, "plan.rhei.md", PLAN, marked),
        &["render", "plan.rhei.md", "--format", "json"],
    );
}

/// `rhei render` on a workspace whose task file has the mark keeps that file's
/// frontmatter. §FS-rhei-language-reference.1 §FS-rhei-plan-language.1.4
#[test]
fn a_task_file_saved_with_a_byte_order_mark_keeps_its_frontmatter() {
    let (_plain, marked) = same_outcome(
        "bom-task-file",
        |dir, marked| {
            write_bytes(dir, "ws/index.rhei.md", "# Rhei: W\n", false);
            write_bytes(dir, "ws/tasks/01-a.md", TASK_FILE, marked);
        },
        &["render", "ws", "--format", "json"],
    );
    let run = run_in(&marked, &["render", "ws", "--format", "json"]);
    assert!(run.stdout.contains("\"priority\":\"high\""), "stdout:\n{}", run.stdout);
}
