//! Full run-command scenarios with a private, thread-local identity clock.
//! Compiled in the CLI's test binary to keep the seam out of the public CLI.
//! §FS-rhei-run.2.7 §FS-rhei-cost-accounting.3.7

use super::*;

const FIRST: u64 = 1_791_839_823_004_821_827;
const COLLISION: u64 = 1_791_843_000_017_266_136;
const CONTROL: u64 = 1_791_843_423_128_278_616;
const MACHINE: &str = "name: run-id\nversion: 1\nstates:\n  draft:\n    initial: true\n    target: missing:openai:luna\n  review:\n    gating: true\n  completed:\n    final: true\ntransitions:\n  - from: draft\n    to: review\n  - from: review\n    to: completed\n";
const TASK: &str = "### Task 1: Draft\n**State:** draft\n";

fn fixture(project: bool) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".agent-grounds/rhei")).unwrap();
    let settings = serde_json::json!({
        "agents": { "missing": { "family": "codex", "command": [root.join("no-such-agent")], "timeout": "60s" } },
        "models": { "luna": { "provider": "openai", "model": "gpt-5.6-luna" } }
    });
    fs::write(root.join(".agent-grounds/rhei/settings.json"), settings.to_string()).unwrap();
    fs::write(root.join("states.yaml"), MACHINE).unwrap();
    let input = if project {
        fs::write(root.join("index.panta.md"), "# Panta: Run IDs\n").unwrap();
        for name in ["alpha", "beta"] {
            fs::create_dir_all(root.join(name).join("tasks")).unwrap();
            fs::write(root.join(name).join("index.rhei.md"), "# Rhei: Member\n").unwrap();
            fs::write(root.join(name).join("tasks/work.md"), TASK).unwrap();
        }
        root.to_path_buf()
    } else {
        let plan = root.join("plan.rhei.md");
        fs::write(&plan, format!("# Rhei: Run IDs\n\n## Tasks\n\n{TASK}")).unwrap();
        plan
    };
    (dir, input)
}

fn run_at(input: &Path, root: &Path, nanos: u64, parallel: usize) -> String {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "run_id_allocation_tests::run_id_fixture_child", "--nocapture"]);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("RHEI_") || key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .env("HOME", root.join(".home"))
        .env("USERPROFILE", root.join(".home"))
        .env("XDG_STATE_HOME", root.join(".state"))
        .env("XDG_CONFIG_HOME", root.join(".config"))
        .env("GIT_CEILING_DIRECTORIES", root.parent().unwrap())
        .env("RHEI_499_TEST_INPUT", input)
        .env("RHEI_499_TEST_ROOT", root)
        .env("RHEI_499_TEST_NANOS", nanos.to_string())
        .env("RHEI_499_TEST_PARALLEL", parallel.to_string());
    let log = root.join("fixture-child.log");
    command
        .stdout(fs::File::create(&log).unwrap())
        .stderr(fs::File::options().append(true).open(&log).unwrap());
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(
                status.success(),
                "fixture child failed: {}",
                fs::read_to_string(&log).unwrap()
            );
            break;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("identity allocation did not terminate with a frozen clock");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    fs::read_to_string(root.join("fixture-result.txt")).unwrap()
}

// Subprocess entry only: clean budget authority and process-global run state.
#[test]
fn run_id_fixture_child() {
    let Some(input) = std::env::var_os("RHEI_499_TEST_INPUT") else { return };
    let root = PathBuf::from(std::env::var_os("RHEI_499_TEST_ROOT").unwrap());
    let nanos = std::env::var("RHEI_499_TEST_NANOS").unwrap().parse().unwrap();
    let parallel = std::env::var("RHEI_499_TEST_PARALLEL").unwrap().parse().unwrap();
    let mut opts = default_run_options();
    opts.standalone.no_tui = true;
    opts.standalone.no_callbacks = true;
    opts.standalone.parallel = parallel;
    RUN_IDENTITY_TEST_WALL
        .with(|wall| wall.set(Some(std::time::UNIX_EPOCH + Duration::from_nanos(nanos))));
    let result = run_command(Path::new(&input), Some(&root.join("states.yaml")), opts);
    RUN_IDENTITY_TEST_WALL.with(|wall| wall.set(None));
    let outcome = match result {
        Ok(()) => "Ok(())".to_string(),
        Err(error) => error.to_string(),
    };
    fs::write(root.join("fixture-result.txt"), outcome).unwrap();
}

fn records(root: &Path) -> Vec<serde_json::Value> {
    let mut held = Vec::new();
    for root in [root.to_path_buf(), root.join("alpha"), root.join("beta")] {
        if let Ok(entries) = fs::read_dir(root.join("runtime/accounting/invocations")) {
            held.extend(entries.map(|entry| {
                serde_json::from_slice::<serde_json::Value>(
                    &fs::read(entry.unwrap().path()).unwrap(),
                )
                .unwrap()
            }));
        }
    }
    assert!(!held.is_empty(), "fixture must reach the accounting store at {}", root.display());
    held
}

fn current_id(root: &Path) -> String {
    let descriptor: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("runtime/run.json")).unwrap()).unwrap();
    descriptor["id"].as_str().unwrap().to_string()
}

fn missing_agent(error: &str) {
    assert!(
        error.contains("failed to spawn agent"),
        "expected ordinary missing-agent error: {error}"
    );
    assert!(!error.contains("accounting identity conflict"), "workspace wedged: {error}");
}

fn identity_consistent(root: &Path, id: &str) {
    assert_eq!(id.len(), 6);
    assert!(id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    let reports = fs::read_dir(root.join("runtime/run-reports")).unwrap();
    assert!(reports
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().ends_with(&format!("-{id}.md"))));
    let held = records(root);
    assert!(held.iter().any(|record| record["run_id"] == id), "no accounting for {id}: {held:?}");
    for record in held.iter().filter(|record| record["run_id"] == id) {
        assert!(record["invocation_id"].as_str().unwrap().contains(&format!("::run-{id}::")));
    }
}

#[test]
#[ignore = "red until #499: colliding starts must not wedge later runs; implement removes this"]
fn run_id_499_colliding_starts_preserve_missing_agent_control() {
    for second in [CONTROL, COLLISION] {
        let (dir, input) = fixture(false);
        let root = dir.path();
        missing_agent(&run_at(&input, root, FIRST, 1));
        let first_id = current_id(root);
        missing_agent(&run_at(&input, root, second, 1));
        let second_id = current_id(root);
        let third = run_at(&input, root, second + 1_800_000_000_000, 1);
        let held = records(root);
        let ids =
            held.iter().map(|r| r["invocation_id"].as_str().unwrap()).collect::<BTreeSet<_>>();
        assert!(
            first_id != second_id && ids.len() == 3 && third.contains("failed to spawn agent")
                && !third.contains("accounting identity conflict"),
            "starts {FIRST}/{second}: ids {first_id}/{second_id}; invocations {ids:?}; third: {third}"
        );
        identity_consistent(root, &first_id);
        identity_consistent(root, &second_id);
        identity_consistent(root, &current_id(root));
        assert!(fs::read_to_string(input).unwrap().contains("**State:** draft"));
    }
}

fn retained_only(kind: &str, project: bool, parallel: usize) {
    let (dir, input) = fixture(project);
    let root = dir.path();
    missing_agent(&run_at(&input, root, FIRST, parallel));
    let occupied = current_id(root);
    identity_consistent(root, &occupied);
    // Leave precisely one kind of durable evidence; the clock remains frozen.
    fs::remove_file(root.join("runtime/run.json")).unwrap();
    fs::remove_dir_all(root.join(".state")).ok();
    fs::remove_file(root.join("runtime/run-report.md")).unwrap();
    fs::remove_file(root.join("runtime/events.jsonl")).unwrap();
    if kind == "report" {
        for root in [root.to_path_buf(), root.join("alpha"), root.join("beta")] {
            let accounting = root.join("runtime/accounting");
            if accounting.exists() {
                fs::remove_dir_all(accounting).unwrap();
            }
        }
    } else {
        fs::remove_dir_all(root.join("runtime/run-reports")).unwrap();
    }
    if project && kind == "report" {
        // Evidence at a different execution root must reserve the id too.
        let name = "run-reports";
        let member_runtime = root.join("alpha/runtime");
        fs::create_dir_all(&member_runtime).unwrap();
        fs::rename(root.join("runtime").join(name), member_runtime.join(name)).unwrap();
    }
    missing_agent(&run_at(&input, root, FIRST, parallel));
    let selected = current_id(root);
    assert_ne!(
        selected, occupied,
        "{kind}-only history must reserve {occupied} with a frozen clock"
    );
    identity_consistent(root, &selected);
    // Another identical input must get beyond both occupied candidates.
    missing_agent(&run_at(&input, root, FIRST, parallel));
    let third = current_id(root);
    assert!(third != occupied && third != selected, "retry reused {third}");
    identity_consistent(root, &third);
}

#[test]
#[ignore = "red until #499: report-only history must reject a frozen candidate; implement removes this"]
fn run_id_499_report_only_frozen_candidate() {
    retained_only("report", false, 1);
}

#[test]
#[ignore = "red until #499: accounting-only history must reject a frozen candidate; implement removes this"]
fn run_id_499_accounting_only_frozen_candidate() {
    retained_only("accounting", false, 1);
}

#[test]
#[ignore = "red until #499: sequential project runs must check other roots; implement removes this"]
fn run_id_499_project_sequential_rejects_other_root_candidate() {
    retained_only("report", true, 1);
}

#[test]
#[ignore = "red until #499: parallel project runs must check other roots; implement removes this"]
fn run_id_499_project_parallel_rejects_other_root_candidate() {
    retained_only("report", true, 2);
}

#[test]
#[ignore = "red until #499: sequential runs must check other accounting roots; implement removes this"]
fn run_id_499_project_sequential_rejects_other_accounting_root() {
    retained_only("accounting", true, 1);
}

#[test]
#[ignore = "red until #499: parallel runs must check other accounting roots; implement removes this"]
fn run_id_499_project_parallel_rejects_other_accounting_root() {
    retained_only("accounting", true, 2);
}

#[test]
#[ignore = "red until #499: retained descriptor alone must reserve an id; implement removes this"]
fn run_id_499_descriptor_only_frozen_candidate() {
    let (dir, input) = fixture(false);
    let root = dir.path();
    missing_agent(&run_at(&input, root, FIRST, 1));
    let occupied = current_id(root);
    fs::remove_file(root.join("runtime/run-report.md")).unwrap();
    fs::remove_file(root.join("runtime/events.jsonl")).unwrap();
    fs::remove_dir_all(root.join("runtime/run-reports")).unwrap();
    fs::remove_dir_all(root.join("runtime/accounting")).unwrap();
    fs::remove_dir_all(root.join(".state")).ok();
    missing_agent(&run_at(&input, root, FIRST, 1));
    assert_ne!(current_id(root), occupied, "descriptor-only history must reserve {occupied}");
}

#[test]
#[ignore = "red until #499: shared roots must reserve ids from other tasks; implement removes this"]
fn run_id_499_shared_root_reserves_other_task_history() {
    let (dir, input) = fixture(false);
    let root = dir.path();
    missing_agent(&run_at(&input, root, FIRST, 1));
    let occupied = current_id(root);
    fs::rename(&input, root.join("alpha.rhei.md")).unwrap();
    fs::write(root.join("beta.rhei.md"), format!("# Rhei: Beta\n\n## Tasks\n\n{TASK}")).unwrap();
    fs::write(root.join("index.panta.md"), "# Panta: Shared Root\n").unwrap();
    fs::remove_file(root.join("runtime/run.json")).unwrap();
    fs::remove_file(root.join("runtime/run-report.md")).unwrap();
    fs::remove_file(root.join("runtime/events.jsonl")).unwrap();
    fs::remove_dir_all(root.join("runtime/run-reports")).unwrap();
    fs::remove_dir_all(root.join(".state")).ok();
    // The retained record names plan.1, outside both current rheis; the id
    // remains reserved even though cost selection filters that task out.
    missing_agent(&run_at(root, root, FIRST, 1));
    let selected = current_id(root);
    assert_ne!(
        selected, occupied,
        "shared-root accounting of another task must reserve {occupied}"
    );
    identity_consistent(root, &selected);
}

fn descriptor_failure(parallel: usize) {
    let (dir, input) = fixture(true);
    let root = dir.path();
    missing_agent(&run_at(&input, root, FIRST, parallel));
    let occupied = current_id(root);
    fs::remove_file(root.join("runtime/run.json")).unwrap();
    fs::create_dir(root.join("runtime/run.json")).unwrap();
    let error = run_at(&input, root, FIRST, parallel);
    missing_agent(&error);
    let held = records(root);
    let selected = held.iter().filter_map(|r| r["run_id"].as_str()).find(|id| *id != occupied);
    assert!(selected.is_some(), "descriptor failure reused occupied id {occupied}: {held:?}");
    identity_consistent(root, selected.unwrap());
    assert!(root.join("runtime/run.json").is_dir());
    let log = fs::read_to_string(root.join("fixture-child.log")).unwrap();
    assert!(log.contains("could not publish the run descriptor"), "{log}");
}

#[test]
#[ignore = "red until #499: sequential identity must survive descriptor failure; implement removes this"]
fn run_id_499_sequential_descriptor_failure_keeps_selected_identity() {
    descriptor_failure(1);
}

#[test]
#[ignore = "red until #499: parallel identity must survive descriptor failure; implement removes this"]
fn run_id_499_parallel_descriptor_failure_keeps_selected_identity() {
    descriptor_failure(2);
}
