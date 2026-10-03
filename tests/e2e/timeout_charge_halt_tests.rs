//! A halt that follows a timeout transition the engine could not fire says so,
//! instead of calling the stranded task blocked, gated, or assigned.
//!
//! `agent-grounds/rhei#401` reached this halt through a budget index another
//! process held open on Windows. This scenario reaches it the portable way: the
//! budget index is a directory, so the timeout edge's charge - the project's
//! first, which establishes the account - cannot read it on any platform, and
//! no wait would ever make it readable.
// §FS-rhei-run.3 §FS-rhei-budgets.5.3

use std::fs;

use super::*;

/// The fixture of the integration test that failed on windows-latest: a program
/// that outlives `program_timeout: 1s`, and a `timeout: 1s` edge to a final
/// state.
fn timeout_machine(command: &str) -> String {
    format!(
        r#"name: timeout-charge-halt
version: 1
states:
  build:
    description: Build artifact
    program:
      command: {command}
    program_timeout: 1s
  timed-out:
    description: Timed out
    final: true
transitions:
  - from: build
    to: timed-out
    timeout: 1s
"#
    )
}

const PLAN: &str = "# Rhei: Timeout\n\n## Tasks\n\n### Task 1: Build artifact\n**State:** build\n";

/// The timeout edge fails to charge, so the run halts with the task in `build`
/// and exits 1 as before; but its help names the failed timeout transition and
/// the warning above, and not `rhei list`, which cannot show why.
// §FS-rhei-run.3
#[test]
fn a_halt_after_a_timeout_transition_failed_to_fire_says_so() {
    let dir = unique_temp_dir("timeout-charge-halt");
    let script = write_python_agent(&dir, "build.py", "time.sleep(5)\n");
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine =
        write_fixture_file(&dir, "states.yaml", &timeout_machine(&fixture_command(&script)));
    let home = dir.join(".home");
    // A directory where the index file belongs: unreadable as a file everywhere.
    fs::create_dir_all(home.join("state/rhei/budget-authority/roots.json"))
        .expect("block the budget index");

    let output = rhei_command(&home)
        .current_dir(&*dir)
        .arg("--state-machine")
        .arg(&machine)
        .arg("run")
        .arg(&plan)
        .arg("--no-tui")
        .output()
        .expect("rhei run should execute");
    let run = CliRun::from(&output);

    assert_eq!(run.status.code(), Some(1), "the halt keeps its exit status\n{}", run.stderr);
    assert!(
        fs::read_to_string(&plan).expect("read plan").contains("**State:** build"),
        "the task is left in its working state"
    );
    let all = format!("{}\n{}", run.stdout, run.stderr);
    assert!(
        all.contains("warning: failed to fire timeout transition for Task plan.1"),
        "the warning names the task and the cause:\n{all}"
    );
    let help = run
        .stderr
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("help:"))
        .unwrap_or_else(|| panic!("the halt carries a help line:\n{}", run.stderr));
    assert!(
        !help.contains("blocked, gated, or assigned"),
        "the task is none of those, so the help must not say so: {help}"
    );
    assert!(!help.contains("rhei list"), "rhei list cannot show why: {help}");
    assert!(
        help.contains("timeout transition") && help.contains("warning"),
        "the help says a timeout transition could not be fired and points at the warning: {help}"
    );
}

/// A timeout transition that fails once and fires on a later pass leaves no
/// trace in the help: the run halts on another task, stranded on an unmet
/// completion condition, so the help is the ordinary one, not the failed
/// timeout's. The help is for a task still in the state its timeout could not
/// leave, judged at the halt.
// §FS-rhei-run.3
#[test]
fn a_halt_after_a_failed_timeout_that_later_fired_gives_the_ordinary_help() {
    let dir = unique_temp_dir("timeout-fired-later-halt");
    let build = write_python_agent(&dir, "build.py", "time.sleep(3)\n");
    let quick = write_python_agent(
        &dir,
        "quick.py",
        "pathlib.Path('runtime/results').mkdir(parents=True, exist_ok=True)\n\
         pathlib.Path('runtime/results/plan.2.md').write_text('ok\\n')\n",
    );
    // Exits 0 without the result the final state requires, so it stays put.
    let stuck = write_python_agent(&dir, "stuck.py", "");
    // Rejects the timeout edge's first firing and accepts every later one.
    let fail_once = write_python_agent(
        &dir,
        "fail_once.py",
        "marker = pathlib.Path(__file__).with_name('fail_once.marker')\n\
         if not marker.exists():\n    marker.touch()\n    sys.exit(1)\n\
         sys.stdout.write('{\"success\": true}')\n",
    );
    let callback = serde_json::to_string(&format!("cli:{}", fixture_command_line(&fail_once)))
        .expect("callback should serialize");
    let machine = format!(
        r#"name: timeout-fired-later-halt
version: 1
states:
  build:
    description: Build artifact
    program:
      command: {build}
    program_timeout: 1s
  quick:
    description: Quick
    program:
      command: {quick}
  stuck:
    description: Stuck
    program:
      command: {stuck}
  timed-out:
    description: Timed out
    final: true
transitions:
  - from: build
    to: timed-out
    timeout: 1s
    on_leave: {callback}
  - from: quick
    to: timed-out
    exit_code: 0
  - from: stuck
    to: timed-out
    exit_code: 0
"#,
        build = fixture_command(&build),
        quick = fixture_command(&quick),
        stuck = fixture_command(&stuck),
    );
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Timeout\n\n## Tasks\n\n### Task 1: Build artifact\n**State:** build\n\n\
         ### Task 2: Quick\n**State:** quick\n\n### Task 3: Stuck\n**State:** stuck\n",
    );
    let machine = write_fixture_file(&dir, "states.yaml", &machine);

    let output = rhei_command(dir.join(".home"))
        .current_dir(&*dir)
        .arg("--state-machine")
        .arg(&machine)
        .arg("run")
        .arg(&plan)
        .arg("--no-tui")
        .output()
        .expect("rhei run should execute");
    let run = CliRun::from(&output);
    let all = format!("{}\n{}", run.stdout, run.stderr);

    assert_eq!(run.status.code(), Some(1), "the run halts on the stuck task\n{all}");
    assert!(
        all.contains("warning: failed to fire timeout transition for Task plan.1"),
        "the first firing is rejected:\n{all}"
    );
    let text = fs::read_to_string(&plan).expect("read plan");
    let state_of = |task: &str| {
        text.split("### Task ")
            .find(|section| section.starts_with(task))
            .and_then(|section| section.lines().find_map(|l| l.strip_prefix("**State:** ")))
            .map(str::to_string)
    };
    assert_eq!(state_of("1:").as_deref(), Some("timed-out"), "the timeout fired later:\n{all}");
    assert_eq!(state_of("3:").as_deref(), Some("stuck"), "the halt is the stuck task's:\n{all}");
    let help = run
        .stderr
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("help:"))
        .unwrap_or_else(|| panic!("the halt carries a help line:\n{}", run.stderr));
    assert!(
        help.contains("blocked, gated, or assigned"),
        "nothing is left of the failed timeout, so the help is the ordinary one: {help}"
    );
    assert!(!help.contains("timeout transition"), "the timeout already fired: {help}");
}
