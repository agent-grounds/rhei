//! Lock invariants around live project membership. Admission adds roots to the
//! existing project-wide exclusion; it never creates a second execution loop.

use std::fs;
use std::process::Stdio;
use std::time::{Duration, Instant};

use super::*;

fn wait_until(label: &str, predicate: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if predicate() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {label}");
}

/// A direct member run queues behind the live project run and names that wait.
/// The marker is a deterministic barrier proving the project worker is active
/// before the direct run starts; releasing it lets both processes terminate.
// §FS-rhei-run.2.6 §FS-rhei-panta.6.2
#[test]
fn issue_205_direct_member_run_remains_excluded_by_the_project_run() {
    let project = unique_temp_dir("live-member-direct-exclusion");
    let seed = project.join("seed");
    fs::create_dir_all(seed.join("tasks")).expect("create seed workspace");
    write_fixture_file(&project, "index.panta.md", "# Panta: Lock Exclusion\n");
    write_fixture_file(&seed, "index.rhei.md", "# Rhei: Seed\n**States:** lock-machine\n");
    write_fixture_file(
        &seed,
        "tasks/01-hold.md",
        "### Task 1: Hold project run\n**State:** holding\n",
    );
    let blocker = write_python_agent(
        &seed,
        "blocker.py",
        r#"write('started', 'started\n')
deadline = time.monotonic() + 12.0
while time.monotonic() < deadline and not pathlib.Path('release').exists():
    time.sleep(0.02)
if not pathlib.Path('release').exists():
    sys.exit(41)
result('project lock holder finished\n')
"#,
    );
    write_fixture_file(
        &seed,
        "states.yaml",
        &format!(
            r#"name: lock-machine
version: 1
states:
  holding:
    initial: true
    description: Hold the live run
    program:
      command: {}
    program_timeout: 15s
  done:
    final: true
    description: Done
transitions:
  - from: holding
    to: done
    exit_code: 0
"#,
            fixture_command(&blocker)
        ),
    );

    let home = project.join(".home");
    let outer_console = project.join("outer.out");
    let outer_file = fs::File::create(&outer_console).expect("outer console");
    let mut outer = rhei_command(&home)
        .current_dir(&*project)
        .args(["run", ".", "--no-tui", "--parallel", "1"])
        .stdin(Stdio::null())
        .stdout(outer_file.try_clone().expect("clone outer console"))
        .stderr(outer_file)
        .spawn()
        .expect("project run should start");
    wait_until("the project worker barrier", || seed.join("started").is_file());

    let direct_console = project.join("direct.out");
    let direct_file = fs::File::create(&direct_console).expect("direct console");
    let mut direct = rhei_command(&home)
        .current_dir(&*project)
        .arg("run")
        .arg(&seed)
        .arg("--no-tui")
        .stdin(Stdio::null())
        .stdout(direct_file.try_clone().expect("clone direct console"))
        .stderr(direct_file)
        .spawn()
        .expect("direct member run should start");
    wait_until("the direct run to announce lock contention", || {
        fs::read_to_string(&direct_console).is_ok_and(|text| text.contains("Waiting for run"))
    });
    assert!(direct.try_wait().expect("probe direct run").is_none());

    write_fixture_file(&seed, "release", "release\n");
    let outer_status = outer.wait().expect("project run exits");
    let direct_status = direct.wait().expect("direct run exits after acquiring the lock");
    assert!(
        outer_status.success(),
        "project run failed:\n{}",
        fs::read_to_string(&outer_console).unwrap_or_default()
    );
    assert!(
        direct_status.success(),
        "direct run failed after its queue advanced:\n{}",
        fs::read_to_string(&direct_console).unwrap_or_default()
    );
}
