//! Startup and inspection must agree about identity across the selected roots.
//! §FS-rhei-panta.6.5 §FS-rhei-cost-accounting.11

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::accounting_attempt_identity_tests::{write_attempt_prices, write_record};
use super::*;

const WORK: &str = "### Task 1: Advance once\n**State:** work\n";
const PASSIVE_ARGS: &[&str] = &["--no-tui", "--no-callbacks", "--no-agent", "--no-program"];

fn review_record(name: &str) -> serde_json::Value {
    serde_json::from_str(
        &fs::read_to_string(fixture_path("accounting-attempt-cross-root").join(name))
            .expect("read retained review record"),
    )
    .expect("parse retained review record")
}

fn add_member(project: &Path, member: &str) -> PathBuf {
    let root = project.join(member);
    fs::create_dir_all(root.join("tasks")).expect("create member");
    fs::write(root.join("index.rhei.md"), format!("# Rhei: {member}\n"))
        .expect("write member index");
    fs::write(root.join("tasks/01-work.md"), WORK).expect("write fresh working task");
    root
}

/// Copies the review's records and machine into a fresh project. The reviewed
/// task had already advanced, so every scenario starts it back in work.
fn cross_root_workspace() -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir("attempt-cross-root");
    let project = dir.join("project");
    let alpha = add_member(&project, "alpha");
    fs::write(project.join("index.panta.md"), "# Panta: Cross-root Identity Review\n")
        .expect("write project");
    let machine = dir.join("states.yaml");
    fs::copy(fixture_path("accounting-attempt-cross-root/states.yaml"), &machine)
        .expect("copy review machine");
    write_record(&project, "first.json", &review_record("first.json"));
    write_record(&alpha, "second.json", &review_record("second.json"));
    (dir, project, machine)
}

/// Include directory entries as well as bytes so newly written accounting
/// artifacts cannot hide behind an unchanged invocation count.
fn tree_snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(root: &Path, path: &Path, entries: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        let value = if path.is_dir() { None } else { Some(fs::read(path).expect("read artifact")) };
        entries.insert(path.strip_prefix(root).expect("relative path").to_path_buf(), value);
        if path.is_dir() {
            for child in fs::read_dir(path).expect("read directory") {
                visit(root, &child.expect("directory entry").path(), entries);
            }
        }
    }
    let mut entries = BTreeMap::new();
    if root.exists() {
        visit(root, root, &mut entries);
    }
    entries
}

fn cost(project: &Path, machine: &Path, extra: &[&str]) -> serde_json::Value {
    let mut args = vec!["--json"];
    args.extend_from_slice(extra);
    let result = run_cli("cost", project, machine, &args);
    assert_success(&result);
    serde_json::from_str(&result.stdout).expect("parse cost payload")
}

/// The union retains its first record for read-only inspection; startup must
/// name both paths and refuse before writing a book, task, result, or journal.
// §FS-rhei-cost-accounting.11 §FS-rhei-panta.6.5
#[test]
fn attempt_identity_cross_root_conflict_refuses_before_mutation() {
    let (dir, project, machine) = cross_root_workspace();
    let alpha = project.join("alpha");
    let task = alpha.join("tasks/01-work.md");
    let task_before = fs::read(&task).expect("read working task");
    let project_before = tree_snapshot(&project.join("runtime/accounting"));
    let alpha_before = tree_snapshot(&alpha.join("runtime/accounting"));
    let paths = [
        project.join("runtime/accounting/invocations/first.json"),
        alpha.join("runtime/accounting/invocations/second.json"),
    ];

    let payload = cost(&project, &machine, &[]);
    assert_eq!(payload["selection"]["invocation_count"], 1);
    assert_eq!(payload["summary"]["total"]["value"], 100);
    assert_eq!(payload["summary"]["cost_micro"], 1_000);
    let errors = payload["errors"].as_array().expect("inspection errors");
    assert_eq!(errors.len(), 1);
    for path in &paths {
        assert!(errors[0].as_str().expect("identity error").contains(path.to_str().expect("path")));
    }
    assert_eq!(tree_snapshot(&project.join("runtime/accounting")), project_before);
    assert_eq!(tree_snapshot(&alpha.join("runtime/accounting")), alpha_before);
    assert_eq!(fs::read(&task).expect("task after inspection"), task_before);

    let prices = write_attempt_prices(&dir);
    // A conflicting currency as well proves identity validation runs first.
    let mut book: serde_json::Value = serde_json::from_slice(&fs::read(&prices).unwrap()).unwrap();
    book["currency"] = "EUR".into();
    fs::write(&prices, serde_json::to_vec_pretty(&book).unwrap()).unwrap();
    let prices_arg = prices.to_string_lossy();
    for extra in [Vec::new(), vec!["--prices", prices_arg.as_ref()]] {
        let mut args = PASSIVE_ARGS.to_vec();
        args.extend(extra);
        let run = run_cli("run", &project, &machine, &args);
        assert!(!run.status.success(), "conflict must refuse startup");
        assert!(run.stderr.contains("accounting identity"), "{}", run.stderr);
        assert!(!run.stderr.contains("currency"), "{}", run.stderr);
        for path in &paths {
            assert!(run.stderr.contains(path.to_str().expect("path")), "{}", run.stderr);
        }
        assert_eq!(fs::read(&task).expect("task after refusal"), task_before);
        assert_eq!(tree_snapshot(&project.join("runtime/accounting")), project_before);
        assert_eq!(tree_snapshot(&alpha.join("runtime/accounting")), alpha_before);
        for root in [&project, &alpha] {
            for artifact in ["run.json", "events.jsonl", "results", "state-transitions.log"] {
                assert!(!root.join("runtime").join(artifact).exists(), "created {artifact}");
            }
        }
    }
}

/// Cross-root exact copies still count once; different runs and disjoint
/// intervals still retain every legacy attempt without rewriting its bytes.
// §FS-rhei-cost-accounting.3.7 §FS-rhei-panta.6.5
#[test]
fn attempt_identity_cross_root_copies_and_legacy_attempts_are_admitted() {
    for evidence in ["copy", "run", "interval", "unattributed"] {
        let (_dir, project, machine) = cross_root_workspace();
        let alpha = project.join("alpha");
        let mut first = review_record("first.json");
        let mut second = review_record("second.json");
        match evidence {
            "copy" => second = first.clone(),
            "run" => second["run_id"] = "another-run".into(),
            "interval" | "unattributed" => {
                second["started_at"] = "2026-09-01T10:00:02Z".into();
                second["ended_at"] = "2026-09-01T10:00:03Z".into();
                if evidence == "unattributed" {
                    first.as_object_mut().unwrap().remove("run_id");
                    second.as_object_mut().unwrap().remove("run_id");
                }
            }
            _ => unreachable!(),
        }
        write_record(&project, "first.json", &first);
        write_record(&alpha, "second.json", &second);
        let before = [
            tree_snapshot(&project.join("runtime/accounting")),
            tree_snapshot(&alpha.join("runtime/accounting")),
        ];
        for _ in 0..2 {
            assert_success(&run_cli("run", &project, &machine, PASSIVE_ARGS));
        }
        let task = fs::read_to_string(alpha.join("tasks/01-work.md")).expect("read task");
        assert!(task.contains("**State:** completed"));
        let payload = cost(&project, &machine, &[]);
        let (count, total) = if evidence == "copy" { (1, 100) } else { (2, 300) };
        assert_eq!(payload["selection"]["invocation_count"], count, "{evidence}");
        assert_eq!(payload["summary"]["total"]["value"], total, "{evidence}");
        assert_eq!(payload["summary"]["cost_micro"], total * 10, "{evidence}");
        assert_eq!(payload["errors"], serde_json::json!([]));
        assert_eq!(tree_snapshot(&project.join("runtime/accounting")), before[0]);
        assert_eq!(tree_snapshot(&alpha.join("runtime/accounting")), before[1]);
    }
}

/// A member selection never reads the project or an unselected member's
/// identities, even when those omitted roots would contradict its history.
// §FS-rhei-panta.6.5 §FS-rhei-cost-accounting.11
#[test]
fn attempt_identity_cross_root_member_scope_excludes_other_roots() {
    let (_dir, project, machine) = cross_root_workspace();
    let beta = add_member(&project, "beta");
    write_record(&beta, "first.json", &review_record("first.json"));
    write_record(&beta, "second.json", &review_record("second.json"));
    let beta_before = tree_snapshot(&beta);
    let project_before = tree_snapshot(&project.join("runtime/accounting"));
    let mut args = PASSIVE_ARGS.to_vec();
    args.extend(["--rhei", "alpha"]);
    assert_success(&run_cli("run", &project, &machine, &args));
    let payload = cost(&project, &machine, &["--rhei", "alpha"]);
    assert_eq!(payload["selection"]["invocation_count"], 1);
    assert_eq!(payload["summary"]["total"]["value"], 200);
    assert_eq!(payload["errors"], serde_json::json!([]));
    assert!(fs::read_to_string(project.join("alpha/tasks/01-work.md"))
        .unwrap()
        .contains("**State:** completed"));
    // Run locks are created for all loaded members; authored work and
    // accounting in the unselected member must still remain unchanged.
    for relative in [
        "tasks/01-work.md",
        "runtime/accounting/invocations/first.json",
        "runtime/accounting/invocations/second.json",
    ] {
        assert_eq!(
            Some(&Some(fs::read(beta.join(relative)).unwrap())),
            beta_before.get(Path::new(relative))
        );
    }
    assert_eq!(tree_snapshot(&project.join("runtime/accounting")), project_before);
}

/// Single-file rheis share the project root. Filtering must precede identity
/// comparison, so valid but conflicting records of another rhei do not veto it.
// §FS-rhei-panta.6.5 §FS-rhei-cost-accounting.11
#[test]
fn attempt_identity_cross_root_shared_root_filters_unselected_tasks() {
    let (_dir, project, machine) = cross_root_workspace();
    for member in ["ledger", "spool"] {
        fs::write(
            project.join(format!("{member}.rhei.md")),
            format!("# Rhei: {member}\n\n## Tasks\n\n{WORK}"),
        )
        .unwrap();
    }
    for name in ["first.json", "second.json"] {
        let mut record = review_record(name);
        record["task_id"] = "spool.1".into();
        record["invocation_id"] = "spool.1::work::codex::visit-1".into();
        write_record(&project, &format!("spool-{name}"), &record);
    }
    let spool_before = fs::read(project.join("spool.rhei.md")).unwrap();
    let alpha_before = fs::read(project.join("alpha/tasks/01-work.md")).unwrap();
    let accounting_before = tree_snapshot(&project.join("runtime/accounting"));
    let mut args = PASSIVE_ARGS.to_vec();
    args.extend(["--rhei", "ledger"]);
    assert_success(&run_cli("run", &project, &machine, &args));
    let payload = cost(&project, &machine, &["--rhei", "ledger"]);
    assert_eq!(payload["selection"]["invocation_count"], 0);
    assert_eq!(payload["errors"], serde_json::json!([]));
    assert!(fs::read_to_string(project.join("ledger.rhei.md"))
        .unwrap()
        .contains("**State:** completed"));
    assert_eq!(fs::read(project.join("spool.rhei.md")).unwrap(), spool_before);
    assert_eq!(fs::read(project.join("alpha/tasks/01-work.md")).unwrap(), alpha_before);
    assert_eq!(tree_snapshot(&project.join("runtime/accounting")), accounting_before);
}
