//! Alternate result sources for `rhei complete`.
//! §FS-rhei-complete.2.2 §FS-rhei-complete.3.2 §FS-rhei-complete.4

use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use super::*;

const MACHINE: &str = r#"name: complete-result-input
version: 1
states:
  pending:
    initial: true
    description: Ready
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: completed
"#;

const TASK: &str = "### Task 1: Record the result\n**State:** pending\n";

#[derive(Clone, Copy)]
enum PlanShape {
    SingleFile,
    Workspace,
}

struct CompletionCase {
    _root: TestDir,
    target: PathBuf,
    machine: PathBuf,
    plan_files: Vec<PathBuf>,
    runtime: PathBuf,
    result: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
struct DurableSnapshot {
    plans: Vec<Vec<u8>>,
    runtime: Vec<(PathBuf, Vec<u8>)>,
}

fn setup_case(prefix: &str, shape: PlanShape) -> CompletionCase {
    match shape {
        PlanShape::SingleFile => {
            let root = unique_temp_dir(prefix);
            let target = write_fixture_file(
                &root,
                "plan.rhei.md",
                &format!("# Rhei: Result Input\n\n## Tasks\n\n{TASK}"),
            );
            let machine = write_fixture_file(&root, "states.yaml", MACHINE);
            let runtime = root.join("runtime");
            let result = runtime.join("results/plan.1.md");
            CompletionCase {
                _root: root,
                target: target.clone(),
                machine,
                plan_files: vec![target],
                runtime,
                result,
            }
        }
        PlanShape::Workspace => {
            let (root, target, machine) =
                create_workspace(prefix, "# Rhei: Result Input\n", &[("01-result.md", TASK)]);
            let runtime = target.join("runtime");
            let result = runtime.join("results/workspace.1.md");
            CompletionCase {
                _root: root,
                plan_files: vec![target.join("index.rhei.md"), target.join("tasks/01-result.md")],
                target,
                machine,
                runtime,
                result,
            }
        }
    }
}

fn run_complete(
    case: &CompletionCase,
    cwd: &Path,
    result_args: &[&OsStr],
    stdin: Option<&[u8]>,
) -> CliRun {
    let mut command = rhei_command(cwd.join(".home"));
    command
        .current_dir(cwd)
        .arg("--state-machine")
        .arg(&case.machine)
        .arg("complete")
        .arg(&case.target)
        .args(["--task", "1"])
        .args(result_args)
        .arg("--no-callbacks");

    let output = if let Some(input) = stdin {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("rhei complete should start");
        child.stdin.take().expect("piped stdin").write_all(input).expect("write result stdin");
        child.wait_with_output().expect("rhei complete should run")
    } else {
        command.output().expect("rhei complete should run")
    };
    CliRun::from(&output)
}

fn expected_entry(message: &[u8]) -> Vec<u8> {
    [b"## Result\n\n".as_slice(), message, b"\n\n".as_slice()].concat()
}

fn snapshot_tree(path: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn collect(root: &Path, current: &Path, entries: &mut Vec<(PathBuf, Vec<u8>)>) {
        if !current.exists() {
            return;
        }
        let mut children = fs::read_dir(current)
            .expect("snapshot directory")
            .map(|entry| entry.expect("snapshot entry").path())
            .collect::<Vec<_>>();
        children.sort();
        for child in children {
            if child.is_dir() {
                collect(root, &child, entries);
            } else {
                entries.push((
                    child.strip_prefix(root).expect("snapshot relative path").into(),
                    fs::read(&child).expect("snapshot file"),
                ));
            }
        }
    }

    let mut entries = Vec::new();
    collect(path, path, &mut entries);
    entries
}

fn durable_snapshot(case: &CompletionCase) -> DurableSnapshot {
    DurableSnapshot {
        plans: case.plan_files.iter().map(|path| fs::read(path).expect("snapshot plan")).collect(),
        runtime: snapshot_tree(&case.runtime),
    }
}

fn assert_failed_without_mutation(case: &CompletionCase, before: &DurableSnapshot, run: &CliRun) {
    assert!(!run.status.success(), "invalid result source should fail\nstdout:\n{}", run.stdout);
    assert_eq!(&durable_snapshot(case), before, "failure mutated plan or runtime artifacts");
}

/// File and stdin input are transport choices only: both plan shapes retain
/// every message byte inside the normal result-entry framing.
// §FS-rhei-complete.2.2 §FS-rhei-complete.3.2 §FS-rhei-complete.4.2
// §FS-rhei-complete.4.3
#[test]
fn complete_result_input_preserves_file_and_stdin_messages_for_both_plan_shapes() {
    let variants = [
        (
            PlanShape::SingleFile,
            false,
            b"Used `origin/main`, 'single', and \"double\".\nSecond line.\n".as_slice(),
        ),
        (
            PlanShape::SingleFile,
            true,
            b"Used `origin/main`, 'single', and \"double\".\r\nSecond line.\r\n".as_slice(),
        ),
        (
            PlanShape::Workspace,
            false,
            b"Used `origin/main`, 'single', and \"double\".\r\nSecond line.\r\n".as_slice(),
        ),
        (
            PlanShape::Workspace,
            true,
            b"Used `origin/main`, 'single', and \"double\".\nSecond line.\n".as_slice(),
        ),
    ];

    for (index, (shape, from_stdin, message)) in variants.into_iter().enumerate() {
        let case = setup_case(&format!("complete-result-transport-{index}"), shape);
        let invoker = case._root.join("invoker");
        fs::create_dir(&invoker).expect("create invocation directory");
        let run = if from_stdin {
            run_complete(
                &case,
                &invoker,
                &[OsStr::new("--result-file"), OsStr::new("-")],
                Some(message),
            )
        } else {
            fs::write(invoker.join("result.md"), message).expect("write relative result source");
            run_complete(
                &case,
                &invoker,
                &[OsStr::new("--result-file"), OsStr::new("result.md")],
                None,
            )
        };

        assert_success(&run);
        assert_eq!(fs::read(&case.result).expect("result artifact"), expected_entry(message));
    }
}

/// The two options form one required group. The stdin sentinel belongs only to
/// the file option, while `./-` remains an ordinary path.
// §FS-rhei-complete.2.2
#[test]
fn complete_result_input_requires_one_source_and_distinguishes_both_dash_spellings() {
    for (label, args) in [
        ("missing", Vec::<&OsStr>::new()),
        (
            "conflicting",
            vec![
                OsStr::new("--result"),
                OsStr::new("inline"),
                OsStr::new("--result-file"),
                OsStr::new("result.md"),
            ],
        ),
    ] {
        let case = setup_case(&format!("complete-result-source-{label}"), PlanShape::SingleFile);
        let before = durable_snapshot(&case);
        let run = run_complete(&case, &case._root, &args, None);
        assert_failed_without_mutation(&case, &before, &run);
    }

    let inline = setup_case("complete-result-inline-dash", PlanShape::SingleFile);
    let run =
        run_complete(&inline, &inline._root, &[OsStr::new("--result"), OsStr::new("-")], None);
    assert_success(&run);
    assert_eq!(fs::read(&inline.result).expect("inline dash result"), expected_entry(b"-"));

    let file = setup_case("complete-result-file-dash", PlanShape::SingleFile);
    fs::write(file._root.join("-"), "from the dash file\n").expect("write dash file");
    let run =
        run_complete(&file, &file._root, &[OsStr::new("--result-file"), OsStr::new("./-")], None);
    assert_success(&run);
    assert_eq!(
        fs::read(&file.result).expect("dash file result"),
        expected_entry(b"from the dash file\n")
    );
}

/// Source paths belong to the caller's working directory, not the selected
/// plan, and absolute paths do not change meaning.
// §FS-rhei-complete.2.2
#[test]
fn complete_result_input_resolves_relative_and_absolute_file_paths() {
    for (label, absolute) in [("relative", false), ("absolute", true)] {
        let case = setup_case(&format!("complete-result-path-{label}"), PlanShape::Workspace);
        let invoker = case._root.join("elsewhere");
        fs::create_dir(&invoker).expect("create invocation directory");
        let source = invoker.join("outside.md");
        let message = format!("loaded from {label}\n");
        fs::write(&source, &message).expect("write result source");
        let argument = if absolute { source.as_os_str() } else { OsStr::new("outside.md") };

        let run = run_complete(&case, &invoker, &[OsStr::new("--result-file"), argument], None);
        assert_success(&run);
        assert_eq!(
            fs::read(&case.result).expect("result artifact"),
            expected_entry(message.as_bytes())
        );
    }
}

/// Every source error is decided before plan loading and leaves both authored
/// state and runtime artifacts untouched.
// §FS-rhei-complete.2.2 §FS-rhei-complete.4
#[test]
fn complete_result_input_rejects_bad_sources_before_plan_loading_or_mutation() {
    for label in ["missing", "unreadable", "invalid-utf8", "blank-file", "blank-stdin"] {
        let case = setup_case(&format!("complete-result-invalid-{label}"), PlanShape::SingleFile);
        let source = case._root.join(format!("{label}.input"));
        let (argument, stdin): (&OsStr, Option<&[u8]>) = match label {
            "missing" => (source.as_os_str(), None),
            "unreadable" => {
                fs::create_dir(&source).expect("create non-file input");
                (source.as_os_str(), None)
            }
            "invalid-utf8" => {
                fs::write(&source, [0xff, 0xfe, 0xfd]).expect("write invalid UTF-8");
                (source.as_os_str(), None)
            }
            "blank-file" => {
                fs::write(&source, b" \r\n\t").expect("write blank input");
                (source.as_os_str(), None)
            }
            "blank-stdin" => (OsStr::new("-"), Some(b" \n\t")),
            _ => unreachable!(),
        };
        let before = durable_snapshot(&case);
        let run = run_complete(&case, &case._root, &[OsStr::new("--result-file"), argument], stdin);
        assert_failed_without_mutation(&case, &before, &run);
    }

    let root = unique_temp_dir("complete-result-error-precedence");
    let case = CompletionCase {
        target: root.join("missing-plan.rhei.md"),
        machine: root.join("missing-states.yaml"),
        plan_files: Vec::new(),
        runtime: root.join("runtime"),
        result: root.join("runtime/results/missing-plan.1.md"),
        _root: root,
    };
    let run = run_complete(
        &case,
        &case._root,
        &[OsStr::new("--result-file"), OsStr::new("missing-result.md")],
        None,
    );
    assert!(!run.status.success());
    assert!(run.stderr.contains("missing-result.md"), "source error should win:\n{}", run.stderr);
    assert!(
        !run.stderr.contains("missing-plan.rhei.md"),
        "plan loading ran before source validation:\n{}",
        run.stderr
    );
}

/// A file source is still a caller-carried message; it appends to, rather than
/// replacing or importing, a worker-authored result artifact.
// §FS-rhei-complete.3.2 §FS-rhei-states.3.3
#[test]
fn complete_result_input_appends_to_a_worker_authored_result() {
    let case = setup_case("complete-result-existing-artifact", PlanShape::SingleFile);
    fs::create_dir_all(case.result.parent().expect("result parent")).expect("create results");
    fs::write(&case.result, b"worker-authored bytes\r\n").expect("write worker result");
    let source = case._root.join("caller.md");
    fs::write(&source, b"caller message\n").expect("write caller source");

    let run =
        run_complete(&case, &case._root, &[OsStr::new("--result-file"), source.as_os_str()], None);
    assert_success(&run);
    assert_eq!(
        fs::read(&case.result).expect("combined result"),
        [b"worker-authored bytes\r\n".as_slice(), expected_entry(b"caller message\n").as_slice()]
            .concat()
    );
}

/// Result files are filesystem inputs, so completion offers paths rather than
/// treating their value as free text. §FS-rhei-completions.7
#[test]
fn complete_result_input_completes_file_paths() {
    let home = unique_temp_dir("completions-result-file-home");
    let dir = unique_temp_dir("completions-result-file-project");
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        &format!("# Rhei: Result Input\n\n## Tasks\n\n{TASK}"),
    );
    write_fixture_file(&dir, "result-details.md", "Result text\n");

    let output = rhei_command(&home)
        .current_dir(&dir)
        .args([
            OsStr::new("--"),
            OsStr::new("rhei"),
            OsStr::new("complete"),
            plan.as_os_str(),
            OsStr::new("--task"),
            OsStr::new("1"),
            OsStr::new("--result-file"),
            OsStr::new("res"),
        ])
        .env("COMPLETE", "fish")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME")
        .output()
        .expect("dynamic completion should run");
    let result = CliRun::from(&output);

    assert!(
        result.status.success(),
        "result-file completion should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("result-details.md"),
        "result-file completion should offer filesystem paths:\n{}",
        result.stdout
    );
}
