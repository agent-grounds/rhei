//! Historical collisions remain safe refusals with an accurate explanation.
//! §FS-rhei-cost-accounting.11 §FS-rhei-run.2.7

use super::accounting_attempt_identity_tests::write_record;
use super::*;

const PLAN: &str = "# Rhei: Collision\n\n## Tasks\n\n### Task 1: Work\n**State:** work\n";
const MACHINE: &str = "name: collision\nversion: 1\nstates:\n  work:\n    initial: true\n  completed:\n    final: true\ntransitions:\n  - from: work\n    to: completed\n";

fn record() -> serde_json::Value {
    let mut record: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture_path("accounting-attempt-cross-root/first.json")).unwrap(),
    )
    .unwrap();
    record["task_id"] = "plan.1".into();
    record["invocation_id"] = "plan.1::work::codex::visit-1::run-82a0ba::move-0::attempt-1".into();
    record["run_id"] = "82a0ba".into();
    record
}

fn report(root: &Path, stamp: &str, started: &str) {
    let reports = root.join("runtime/run-reports");
    fs::create_dir_all(&reports).unwrap();
    fs::write(reports.join(format!("{stamp}-82a0ba.md")), format!(
        "# Run Report: Collision\n\nRun: {started} / 82a0ba\nWorkspace: {}\nCommand: rhei run plan.rhei.md\nMode: agent · parallel 1\nDuration: 1s\nResult: failed\n",
        root.display()
    )).unwrap();
}

#[test]
fn run_id_499_historical_collision_explains_reuse_without_repair() {
    let (dir, plan, machine) = setup_single_file("run-id-499-conflict", PLAN);
    fs::write(&machine, MACHINE).unwrap();
    let mut first = record();
    first["started_at"] = "2026-10-12T21:17:03Z".into();
    first["ended_at"] = "2026-10-12T21:17:04Z".into();
    let mut second = first.clone();
    second["started_at"] = "2026-10-12T22:10:00Z".into();
    second["ended_at"] = "2026-10-12T22:10:01Z".into();
    write_record(&dir, "first.json", &first);
    write_record(&dir, "second.json", &second);
    report(&dir, "2026-10-12T21-17-03Z", "2026-10-12T21:17:03Z");
    report(&dir, "2026-10-12T22-10-00Z", "2026-10-12T22:10:00Z");
    let before = fs::read(&plan).unwrap();
    let paths = ["first.json", "second.json"]
        .map(|name| dir.join("runtime/accounting").join("invocations").join(name));
    let bytes = paths.each_ref().map(|path| fs::read(path).unwrap());
    let run = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert!(!run.status.success());
    assert!(run.stderr.contains("accounting identity conflict"), "{}", run.stderr);
    for (path, original) in paths.iter().zip(bytes) {
        assert!(run.stderr.contains(&path.to_string_lossy().to_string()), "{}", run.stderr);
        assert_eq!(fs::read(path).unwrap(), original, "must not repair history");
    }
    assert_eq!(fs::read(plan).unwrap(), before, "must refuse before advancing");
    for artifact in ["run.json", "events.jsonl", "spawns", "results", "state-transitions.log"] {
        assert!(!dir.join("runtime").join(artifact).exists(), "created {artifact}");
    }
    let diagnostic = run.stderr.to_lowercase();
    assert!(diagnostic.contains("repair") && diagnostic.contains("remove"), "{}", run.stderr);
    assert!(
        diagnostic.contains("run id") && diagnostic.contains("82a0ba")
            && diagnostic.contains("reus") && diagnostic.contains("run"),
        "must explain distinct runs reused run id 82a0ba, not just print it inside invocation id:\n{}",
        run.stderr
    );
}

/// Member-only accounting scope still reads the owning project's run evidence.
/// §FS-rhei-cost-accounting.11
#[test]
fn run_id_499_scoped_historical_collision_uses_project_reports_without_repair() {
    let dir = unique_temp_dir("run-id-499-scoped-conflict");
    let manifest = dir.join("index.panta.md");
    fs::write(&manifest, "# Panta: Collision\n").unwrap();
    let member = dir.join("alpha");
    fs::create_dir_all(member.join("tasks")).unwrap();
    let index = member.join("index.rhei.md");
    fs::write(&index, "# Rhei: Member\n").unwrap();
    let task = member.join("tasks/work.md");
    fs::write(&task, "### Task 1: Work\n**State:** work\n").unwrap();
    let machine = dir.join("states.yaml");
    fs::write(&machine, MACHINE).unwrap();

    let mut first = record();
    first["task_id"] = "alpha.1".into();
    first["invocation_id"] = "alpha.1::work::codex::visit-1::run-82a0ba::move-0::attempt-1".into();
    first["started_at"] = "2026-10-12T21:17:03Z".into();
    first["ended_at"] = "2026-10-12T21:17:04Z".into();
    let mut second = first.clone();
    second["started_at"] = "2026-10-12T22:10:00Z".into();
    second["ended_at"] = "2026-10-12T22:10:01Z".into();
    write_record(&member, "first.json", &first);
    write_record(&member, "second.json", &second);
    report(&dir, "2026-10-12T21-17-03Z", "2026-10-12T21:17:03Z");
    report(&dir, "2026-10-12T22-10-00Z", "2026-10-12T22:10:00Z");
    let paths = ["first.json", "second.json"]
        .map(|name| member.join("runtime/accounting").join("invocations").join(name));
    let retained = [
        manifest,
        index,
        task,
        machine.clone(),
        paths[0].clone(),
        paths[1].clone(),
        dir.join("runtime/run-reports/2026-10-12T21-17-03Z-82a0ba.md"),
        dir.join("runtime/run-reports/2026-10-12T22-10-00Z-82a0ba.md"),
    ];
    let bytes = retained.each_ref().map(|path| fs::read(path).unwrap());
    let run = run_cli("run", &dir, &machine, &["--no-tui", "--no-callbacks", "--rhei", "alpha"]);
    assert_eq!(run.status.code(), Some(1), "{}", run.stderr);
    assert!(run.stderr.contains("accounting identity conflict"), "{}", run.stderr);
    for path in paths {
        assert!(run.stderr.contains(&path.to_string_lossy().to_string()), "{}", run.stderr);
    }
    assert!(run.stderr.contains("distinct runs reused run id '82a0ba'"), "{}", run.stderr);
    assert!(run.stderr.contains("repair or remove"), "{}", run.stderr);
    for (path, original) in retained.iter().zip(bytes) {
        assert_eq!(fs::read(path).unwrap(), original, "mutated {}", path.display());
    }
    for root in [&*dir, member.as_path()] {
        for artifact in ["run.json", "events.jsonl", "spawns", "results", "state-transitions.log"] {
            assert!(!root.join("runtime").join(artifact).exists(), "created {artifact}");
        }
    }
}

/// Unequal contents from one run do not prove that two runs reused an id.
/// §FS-rhei-cost-accounting.11
#[test]
fn run_id_499_same_run_unequal_copy_is_not_diagnosed_as_reuse() {
    let (dir, plan, machine) = setup_single_file("run-id-499-copy", PLAN);
    fs::write(&machine, MACHINE).unwrap();
    let first = record();
    let mut second = first.clone();
    second["duration_ms"] = 2000.into();
    write_record(&dir, "first.json", &first);
    write_record(&dir, "second.json", &second);
    report(&dir, "2026-09-01T10-00-00Z", "2026-09-01T10:00:00Z");
    let run = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert!(!run.status.success());
    assert!(run.stderr.contains("accounting identity conflict"), "{}", run.stderr);
    assert!(!run.stderr.to_lowercase().contains("reus"), "unproven reuse: {}", run.stderr);
}
