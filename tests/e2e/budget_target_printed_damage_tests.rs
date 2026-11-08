//! The budget lines printed for a witness that ends mid-receipt and for a program
//! state's refused start, beside the cases of `budget_target_printed_tests`, and
//! taken the same way: `rhei list` takes the printed target, then the line runs
//! with only its placeholders filled in (`agent-grounds/rhei#577`).

// §FS-rhei-budgets.10.1 §FS-rhei-budgets.5.4.3 §FS-rhei-budgets.9 §FS-rhei-errors.1.2.1

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use super::budget_program_verification_support::drop_last_receipt;
use super::budget_support::home_for;
use super::budget_target_support::*;
use super::*;

const PLAN: &str = "my project/plan.rhei.md";

/// A lone plan with an account, under a path that needs quoting.
fn established(prefix: &str) -> World {
    let world = world(prefix);
    world.plain("my project");
    assert_ne!(shell_quote(PLAN), PLAN, "the case is only a case while the path needs quoting");
    world.init(PLAN);
    world
}

/// The journal of `world`'s one account, and the witness the machine keeps of it.
fn account_files(world: &World) -> (PathBuf, PathBuf) {
    let accounts = world.dir.join("my project/.agent-grounds/rhei/budgets");
    let account =
        fs::read_dir(accounts).expect("the accounts").next().expect("one").expect("entry");
    let witness = home_for(&world.dir)
        .join("state/rhei/budget-authority")
        .join(account.file_name())
        .join("history.jsonl");
    (account.path().join("journal.jsonl"), witness)
}

/// End `witness` mid-receipt: bytes past its last newline.
fn tear(witness: &std::path::Path) {
    let mut file = fs::OpenOptions::new().append(true).open(witness).expect("open the witness");
    file.write_all(b"{\"cut\":\"short").expect("write the fragment");
}

/// `torn_refusal`, which core raises with `forget` beside the trim: the journal is
/// wholly absent, so a runnable line retires the root. §FS-rhei-budgets.5.4.3
#[test]
fn budget_target_printed_the_torn_refusal_forget_runs() {
    let world = established("budget-printed-torn-refusal");
    let (journal, witness) = account_files(&world);
    tear(&witness);
    fs::remove_file(&journal).expect("lose the journal");

    let refused = world.run(&["budget", "adjust", PLAN, "--invocations", "3", "--reason", "x"]);

    assert_eq!(refused.status.code(), Some(1), "a torn witness refuses:\n{}", said_raw(&refused));
    assert!(raw_stderr(&refused).contains("ends mid-receipt"), "{}", raw_stderr(&refused));
    let printed = printed_command(&raw_stderr(&refused), "rhei budget forget ");
    world.assert_printed_target_is_listable(&printed);
    let run = filled_in(&printed);
    let retired = world.paste(&run);
    assert!(retired.status.success(), "`{run}` should run:\n{}", retired.stderr);
    assert!(retired.stdout.contains("retired "), "`{run}` retires:\n{}", retired.stdout);
}

/// `rhei remove` lists an account it cannot read as evidence, in core's words: the
/// one surface that prints a refusal rather than failing on it. §FS-rhei-remove.2
#[test]
fn budget_target_printed_the_remove_evidence_forget_runs() {
    let world = established("budget-printed-remove");
    let (journal, witness) = account_files(&world);
    tear(&witness);
    fs::remove_file(&journal).expect("lose the journal");

    let refused = world.run(&["remove", PLAN, "--task", "1"]);

    let said = said_raw(&refused);
    assert!(said.contains("ends mid-receipt"), "the evidence names the damage:\n{said}");
    let printed = printed_command(&said, "rhei budget forget ");
    world.assert_printed_target_is_listable(&printed);
    let run = filled_in(&printed);
    let retired = world.paste(&run);
    assert!(retired.status.success(), "`{run}` should run:\n{}", retired.stderr);
}

/// The report's "then run `rhei budget show` again", which used to name no target,
/// so from any other directory it discovered something else or failed.
#[test]
fn budget_target_printed_the_torn_reports_show_again_runs() {
    let world = established("budget-printed-torn-again");
    let (_, witness) = account_files(&world);
    tear(&witness);

    let out = world.run(&["budget", "show", PLAN]);

    assert_eq!(out.status.code(), Some(1), "a torn witness is damaged:\n{}", said_raw(&out));
    let line = printed_command(&stdout(&out), "rhei budget show ");
    let printed = line.split('`').next().expect("the command's closing backtick");
    world.assert_printed_target_is_listable(printed);
    let shown = world.paste(printed);
    assert_eq!(shown.status.code(), Some(1), "still damaged:\n{}", shown.stderr);
    assert!(
        shown.stdout.contains("witness_torn"),
        "`{printed}`:\n{}{}",
        shown.stdout,
        shown.stderr
    );
}

/// A machine whose one state runs a program, and a plan waiting at it, with the
/// account's journal one receipt short, so that start is refused. §FS-rhei-budgets.9
fn program_refused(prefix: &str) -> World {
    let world = established(prefix);
    world.write(PLAN, "# Rhei: P\n\n## Tasks\n\n### Task 1: A\n**State:** claim\n");
    world.write(
        "states.yaml",
        "name: refused-program\nversion: 1\nstates:\n  claim:\n    initial: true\n    \
         description: Take the claim\n    program:\n      command: never started\n    \
         program_timeout: 30s\n  completed:\n    description: Done\n    final: true\n\
         transitions:\n  - { from: claim, to: completed }\n",
    );
    drop_last_receipt(&world.dir.join("my project"), &home_for(&world.dir));
    world
}

/// `verify_program_account`'s remedy, on both program dispatch paths.
#[test]
fn budget_target_printed_the_program_refusals_show_runs() {
    for parallel in ["1", "2"] {
        let world = program_refused(&format!("budget-printed-program-{parallel}"));

        let run = ["--state-machine", "states.yaml", "run", PLAN, "--no-tui", "--no-callbacks"];
        let refused = world.run(&[&run[..], &["--parallel", parallel]].concat());

        assert!(!refused.status.success(), "the start is refused:\n{}", said_raw(&refused));
        let printed = printed_command(&raw_stderr(&refused), "rhei budget show ");
        world.assert_printed_target_is_listable(&printed);
        let shown = world.paste(&printed);
        assert_eq!(shown.status.code(), Some(1), "the account is damaged:\n{}", shown.stderr);
        assert!(shown.stdout.contains("fewer receipts"), "`{printed}`:\n{}", shown.stdout);
    }
}

/// A detached run's log is read later and from anywhere, so the remedy it holds runs
/// from another directory. §FS-rhei-errors.1.2.1
#[cfg(unix)]
#[test]
fn budget_target_printed_the_program_refusals_show_runs_from_a_detached_log() {
    let world = program_refused("budget-printed-program-detached");
    let machine = ["--state-machine", "states.yaml"];
    let launched =
        world.run(&[&machine[..], &["run", "--headless", PLAN, "--no-callbacks"]].concat());
    let refusal = "budget journal is untrustworthy";
    let damage = "does not match the project journal";
    // Either startup observation is valid; other launch failures are not. §FS-rhei-run-headless.1.1
    if !launched.status.success() {
        let said = raw_stderr(&launched);
        assert_eq!(launched.status.code(), Some(1), "the account refuses:\n{said}");
        assert!(
            said.contains("the run exited before it started (exit status 1)")
                && said.contains(refusal)
                && said.contains(damage),
            "only the damaged-account early exit is expected:\n{said}"
        );
    }

    let log = world.dir.join("my project/runtime/run.log");
    let report = world.dir.join("my project/runtime/run-report.md");
    let descriptor = world.dir.join("my project/runtime/run.json");
    let plan = fs::canonicalize(&*world.dir).expect("absolute fixture directory").join(PLAN);
    let expected = format!("rhei budget show {}", shell_quote(&plan.display().to_string()));
    // Wait for the complete remedy, not just its prefix. §FS-rhei-errors.1.2.1
    super::headless_support::wait_until(
        "the detached run's complete refusal and remedy",
        std::time::Duration::from_secs(30),
        || {
            let said = raw_stderr_from_file(&log);
            said.contains(refusal)
                && said.contains(damage)
                && said.contains(&format!("{expected}\n"))
        },
    );
    // The run must fail by itself and retain its diagnosis. §FS-rhei-budgets.9
    super::headless_support::wait_until(
        "the detached run's failed outcome and durable report",
        std::time::Duration::from_secs(30),
        || {
            let ended = fs::read(&descriptor)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .is_some_and(|d| d["status"] == "failed" && d["exit_code"] == 1);
            let report = fs::read_to_string(&report).unwrap_or_default();
            ended && report.contains(refusal) && report.contains(damage)
        },
    );

    let printed = printed_command(&raw_stderr_from_file(&log), "rhei budget show ");
    assert_eq!(printed, expected, "the log names the exact absolute plan, quoted as one word");
    let elsewhere = world.write("elsewhere/.keep", "").parent().expect("a parent").to_path_buf();
    assert_printed_target_is_listable_with(&world.rhei(), &printed, &elsewhere);
    let shown = paste_with(&world.rhei(), &printed, &elsewhere);
    assert_eq!(shown.status.code(), Some(1), "the account is damaged:\n{}", shown.stderr);
    assert!(shown.stdout.contains("fewer receipts"), "`{printed}`:\n{}", shown.stdout);
}
