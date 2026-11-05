//! A detached strict run's refusal is read from `runtime/run.log`, later and from
//! anywhere, so its remedy names the target absolute like its machine, in one
//! spelling, and seals what the run refused from another directory.
//! §FS-rhei-errors.1.2.1 §FS-rhei-seal.8.4
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};

use super::help_target_paste_tests::{write, OPEN_PLAN};
use super::help_target_tests::{discovered, pasted, Case, HELD_MACHINE};
use super::*;

/// The machine as sealed; [`changed`] edits it afterwards.
fn sealed_machine() -> String {
    HELD_MACHINE.replace("Ready to work, were it not held.", "As sealed.")
}

fn changed(machine: &Path) {
    let text = fs::read_to_string(machine).expect("read the machine");
    fs::write(machine, text.replace("As sealed.", "Changed.")).expect("change the machine");
}

/// The detached run's log once it has refused and halted, with the run gone.
fn refused_and_halted(case: &Case, execution_root: &Path, target: &str) -> String {
    let log = execution_root.join("runtime/run.log");
    super::headless_support::wait_until(
        "the detached run to refuse and halt",
        std::time::Duration::from_secs(30),
        || {
            let text = raw_stderr_from_file(&log);
            text.contains("accept the change with") && text.contains("rhei run halted")
        },
    );
    let _ = case.rhei(&case.root, &["stop", "--wait", target]);
    raw_stderr_from_file(&log)
}

/// The remedy the log prints, between its backticks.
fn logged_remedy(log: &str) -> String {
    let (_, rest) = log.split_once("accept the change with `").expect("a remedy in the log");
    rest.split_once('`').expect("the remedy's closing backtick").0.to_owned()
}

/// `path` the way a detached run spells it: absolute, through the working
/// directory the OS reports.
fn absolute(path: &Path) -> String {
    shell_quote(&discovered(path).display().to_string())
}

/// What a strict run prints once nothing is refused: the program starts.
#[track_caller]
fn assert_starts(case: &Case, args: &[&str], ticket: &str) {
    let out = case.rhei(&case.root, args);
    let stdout = CliRun::from(&out).stdout;
    assert!(
        stdout.contains(&format!("Spawning program for Task {ticket}")),
        "the next strict run should start; got:\n{stdout}\n{}",
        raw_stderr(&out)
    );
    assert!(!raw_stderr(&out).contains("was not started"), "the seal did not cure it");
}

fn elsewhere(case: &Case) -> PathBuf {
    let dir = case.root.join("elsewhere");
    fs::create_dir_all(&dir).expect("create another directory");
    dir
}

/// A single plan under a machine it was given. §FS-rhei-errors.1.2.1
#[test]
fn help_target_a_detached_strict_refusal_seals_from_anywhere() {
    let case = Case::new("help-target-detached-strict");
    let machine = write(&case.root.join("machines"), "custom.yaml", &sealed_machine());
    let sub = case.root.join("sub");
    write(&sub, "plan.rhei.md", OPEN_PLAN);
    let given = ["--state-machine", "machines/custom.yaml"];
    let sealed = case.rhei(&case.root, &[&["seal", "sub/plan.rhei.md"][..], &given].concat());
    assert!(sealed.status.success(), "the seal works:\n{}", raw_stderr(&sealed));
    changed(&machine);

    let strict = ["--strict-inputs", "--no-callbacks"];
    let launch = [&["run", "--headless", "sub/plan.rhei.md"][..], &given, &strict].concat();
    let launched = case.rhei(&case.root, &launch);
    assert!(launched.status.success(), "the launch works:\n{}", raw_stderr(&launched));
    let log = refused_and_halted(&case, &sub, "sub/plan.rhei.md");

    let expected = format!(
        "rhei seal {} --state-machine {}",
        absolute(&sub.join("plan.rhei.md")),
        absolute(&machine)
    );
    assert_eq!(logged_remedy(&log), expected, "one spelling, absolute:\n{log}");
    assert!(!log.contains("rhei seal sub/plan.rhei.md"), "a second spelling:\n{log}");
    let again = pasted(&expected, &elsewhere(&case), &case.home);
    assert!(again.status.success(), "`{expected}` should run from anywhere:\n{}", again.stderr);

    let run = [&["run", "sub/plan.rhei.md"][..], &given, &strict, &["--no-tui"]].concat();
    assert_starts(&case, &run, "plan.1");
}

/// A project's remedy names the rhei it refused, under the project. §FS-rhei-errors.1.2.1
#[test]
fn help_target_a_detached_strict_refusal_in_a_project_seals_its_rhei_from_anywhere() {
    let case = Case::new("help-target-detached-strict-project");
    let project = case.root.join("project");
    write(&project, "index.panta.md", "# Panta: Project\n");
    let machine = write(&project, "states.yaml", &sealed_machine());
    write(&project, "auth.rhei.md", OPEN_PLAN);
    let sealed = case.rhei(&case.root, &["seal", "project"]);
    assert!(sealed.status.success(), "the seal works:\n{}", raw_stderr(&sealed));
    changed(&machine);

    let launch = ["run", "--headless", "project", "--strict-inputs", "--no-callbacks"];
    let launched = case.rhei(&case.root, &launch);
    assert!(launched.status.success(), "the launch works:\n{}", raw_stderr(&launched));
    let log = refused_and_halted(&case, &project, "project");

    let expected = format!("rhei seal {} --rhei auth", absolute(&project));
    assert_eq!(logged_remedy(&log), expected, "one spelling, absolute:\n{log}");
    let again = pasted(&expected, &elsewhere(&case), &case.home);
    assert!(again.status.success(), "`{expected}` should run from anywhere:\n{}", again.stderr);

    let run = ["run", "project", "--strict-inputs", "--no-callbacks", "--no-tui"];
    assert_starts(&case, &run, "auth.1");
}
