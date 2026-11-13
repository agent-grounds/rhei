//! A `rhei run` killed outright mid-visit, run again (issue #661).
//!
//! `SIGTERM` is caught and its visit resumes; `SIGKILL` - an OOM kill, a
//! container stopped after its grace, the session that started the run dying -
//! runs no code of rhei's, so no spawn record is written. The next run resumes
//! an agent visit from the killed run's evidence once its group is gone, and
//! refuses a program visit and a group still running.
//!
//! Unix-only: the worker is a shell script and the kill is `SIGKILL`. The
//! Windows check (pid and creation time) is pinned by unit tests.
//! §REQ-cross-platform.4

// §FS-rhei-agents.8.1.1 §FS-rhei-agents.3.2.3 §FS-rhei-run.3.2

use super::killed_run_support::*;

/// The issue's reproducer: kill the run while `cover` runs, then run again. The
/// ticket finishes, `prepare` is not repeated, the killed transcript is kept
/// byte for byte, and the resumed attempt is `-attempt2` of a visit the kill
/// charged nothing.
// §FS-rhei-agents.8.1.1 §FS-rhei-agents.8.4 §FS-rhei-agents.3.2.1
#[test]
#[ignore = "red: the fixture's finalize program writes no result, so no run of this machine reaches done"]
fn a_run_killed_during_an_agent_visit_resumes_it_as_its_next_attempt() {
    let fx = Fixture::new("killed-run-agent");
    fx.hold("cover");
    let mut first = fx.spawn_run("first");
    let pgid = fx.wait_for_hold("cover", 1, &mut first, "first");
    fx.kill_run_and_settle(first, pgid, "first");
    assert_eq!(
        fx.logs_of("cover"),
        ["task-workspace.1-cover.log"],
        "fixture: the killed visit's log"
    );
    let killed_log = fx.log_bytes("task-workspace.1-cover.log");

    fx.release("cover");
    let (status, output) = fx.run_to_end("second");
    assert!(status.success(), "the rerun should finish the ticket\n{output}");
    fx.assert_state("done");
    assert_eq!(fx.starts("prepare"), 1, "the states before the kill are not repeated");
    assert_eq!(fx.starts("cover"), 2, "the killed visit runs once more");
    assert_eq!(
        fx.logs_of("cover"),
        ["task-workspace.1-cover-attempt2.log", "task-workspace.1-cover.log"],
        "the visit resumes as its next attempt, beside the kept log"
    );
    assert_eq!(
        fx.log_bytes("task-workspace.1-cover.log"),
        killed_log,
        "the killed log is kept as it was"
    );
    assert!(
        output.contains(
            "attempt 1 of 2; the previous attempt was killed together with its run (previous log:"
        ),
        "the re-spawn names the kill and charges nothing\n{output}"
    );

    let record = fx.seen_record("cover", 2);
    assert_eq!(record["ending"], "killed", "the killed attempt's record\n{record}");
    assert_eq!(record["killed"], 1, "the visit's first kill\n{record}");
    assert_eq!(record["attempt_charged"], false, "the first kill is free\n{record}");
    assert_eq!(record["attempt"], 1, "{record}");
    assert!(record.get("code").is_none_or(|code| code.is_null()), "no exit code\n{record}");
    assert!(
        record["log"].as_str().is_some_and(|log| log.ends_with("task-workspace.1-cover.log")),
        "the record names the kept log\n{record}"
    );
}

/// The first kill of a visit is free and every later one is charged, so under
/// `attempts: 2` a worker killed every time runs three times and the next run
/// halts the visit, naming `rhei retry`.
// §FS-rhei-agents.3.2.3 §FS-rhei-agents.8.4
#[test]
fn a_visit_killed_again_is_charged_and_halts_within_its_budget() {
    let fx = Fixture::new("killed-run-charged");
    fx.hold("cover");
    for (n, name) in [(1, "first"), (2, "second"), (3, "third")] {
        let mut run = fx.spawn_run(name);
        let pgid = fx.wait_for_hold("cover", n, &mut run, name);
        fx.kill_run_and_settle(run, pgid, name);
    }
    let second = fx.seen_record("cover", 2);
    assert_eq!(second["killed"], 1, "{second}");
    assert_eq!(second["attempt_charged"], false, "the first kill is free\n{second}");
    let third = fx.seen_record("cover", 3);
    assert_eq!(third["ending"], "killed", "{third}");
    assert_eq!(third["attempt"], 2, "{third}");
    assert_eq!(third["killed"], 2, "{third}");
    assert_eq!(third["attempt_charged"], true, "the second kill is charged\n{third}");

    let (status, output) = fx.run_to_end("fourth");
    assert!(!status.success(), "the spent visit halts\n{output}");
    assert!(output.contains("rhei retry"), "the halt names the attended remedy\n{output}");
    assert!(!output.contains("no spawn record accounts for it"), "{output}");
    assert_eq!(fx.starts("cover"), 3, "no fourth spawn");
    assert_eq!(fx.starts("prepare"), 1, "nothing before the visit repeats");
    fx.assert_state("cover");
}

/// A program killed with its run may already have done its work, so it is not
/// run again on rhei's own: the rerun refuses with the program message and
/// lists the `rhei transition` that moves the ticket on.
// §FS-rhei-agents.8.1.1
#[test]
fn a_run_killed_during_a_program_visit_refuses_and_leaves_it_to_a_person() {
    let fx = Fixture::new("killed-run-program");
    fx.hold("finalize");
    let mut first = fx.spawn_run("first");
    let pgid = fx.wait_for_hold("finalize", 1, &mut first, "first");
    fx.kill_run_and_settle(first, pgid, "first");
    fx.release("finalize");

    let (status, output) = fx.run_to_end("second");
    assert!(!status.success(), "the rerun refuses\n{output}");
    assert!(
        output.contains(
            "belongs to a program whose run was killed while it ran; it may have finished"
        ),
        "the refusal names the killed program\n{output}"
    );
    assert!(
        output.contains("rhei transition")
            && (output.contains("--to=done") || output.contains("--to done")),
        "the help lists the state's edges as runnable commands\n{output}"
    );
    assert!(!output.contains("<state>"), "no placeholder for a state\n{output}");
    assert_eq!(fx.starts("finalize"), 1, "the program is not started again");
    fx.assert_state("finalize");
}

/// What the killed agent left in its group keeps the visit refused, and the
/// refusal lists the processes rather than printing a `kill` line; once the
/// group is gone, the next run resumes.
// §FS-rhei-agents.8.1.1 §DA-supervised-process-groups
#[test]
#[ignore = "red: the fixture's finalize program writes no result, so no run of this machine reaches done"]
fn a_killed_agent_whose_group_still_runs_is_refused_until_it_is_stopped() {
    let fx = Fixture::new("killed-run-straggler");
    fx.hold("cover");
    std::fs::write(fx.path("straggler"), "").expect("write straggler flag");
    let mut first = fx.spawn_run("first");
    let pgid = fx.wait_for_hold("cover", 1, &mut first, "first");
    first.0.kill().expect("SIGKILL rhei run");
    first.0.wait().expect("reap rhei run");
    let straggler = std::fs::read_to_string(fx.path("pids/straggler")).expect("straggler pid");
    let straggler = straggler.trim().to_string();
    // The held leader may die of the parent-death signal; the straggler does not.
    assert!(group_alive(pgid), "fixture: the straggler keeps the group alive");

    fx.release("cover");
    let (status, output) = fx.run_to_end("second");
    assert!(!status.success(), "a live group refuses\n{output}");
    assert!(
        output.contains(
            "belongs to a worker whose run was killed, and its process group still has processes"
        ),
        "the refusal names the live group\n{output}"
    );
    assert!(output.contains(&straggler), "it lists the straggler's pid\n{output}");
    assert!(output.contains("sleep 300"), "and its command line\n{output}");
    assert!(!output.contains("kill -TERM"), "it prints no kill command\n{output}");
    assert!(!output.contains("rhei reset"), "a reset would start a second worker\n{output}");
    assert_eq!(fx.starts("cover"), 1, "no second worker beside the first");
    fx.assert_state("cover");

    kill_group(pgid);
    assert!(wait_group_gone(pgid, PATIENCE), "group {pgid} never emptied");
    let (status, output) = fx.run_to_end("third");
    assert!(status.success(), "once the group is gone the visit resumes\n{output}");
    fx.assert_state("done");
    assert_eq!(
        fx.logs_of("cover"),
        ["task-workspace.1-cover-attempt2.log", "task-workspace.1-cover.log"]
    );
}

/// A kept log whose bytes changed after the killed run was finalized is no
/// longer the attempt's, so it is refused exactly as today. The edit has to
/// land after finalization, which hashes the log as found: a rerun that halts
/// on the still-running group is the run that finalizes.
// §FS-rhei-agents.8.1.1 §FS-rhei-evidence.9.3
#[test]
fn an_edited_killed_log_keeps_todays_refusal() {
    let fx = Fixture::new("killed-run-edited");
    fx.hold("cover");
    std::fs::write(fx.path("straggler"), "").expect("write straggler flag");
    let mut first = fx.spawn_run("first");
    let pgid = fx.wait_for_hold("cover", 1, &mut first, "first");
    first.0.kill().expect("SIGKILL rhei run");
    first.0.wait().expect("reap rhei run");
    fx.release("cover");
    let (status, output) = fx.run_to_end("finalizing");
    assert!(
        !status.success() && output.contains("its process group still has processes"),
        "fixture: the rerun finalizes the killed run and refuses the live group\n{output}"
    );

    kill_group(pgid);
    assert!(wait_group_gone(pgid, PATIENCE), "group {pgid} never emptied");
    let log = fx.workspace.join("runtime/logs/task-workspace.1-cover.log");
    let mut bytes = std::fs::read(&log).expect("read log");
    bytes.extend_from_slice(b"planted\n");
    std::fs::write(&log, bytes).expect("edit log");

    let (status, output) = fx.run_to_end("edited");
    assert!(!status.success(), "an edited log is not the attempt's\n{output}");
    assert!(
        output.contains("exists and no spawn record accounts for it"),
        "the refusal is today's\n{output}"
    );
    assert!(!output.contains("cannot tell whether"), "the group record is there\n{output}");
    assert_eq!(fx.starts("cover"), 1, "nothing ran over it");
    fx.assert_state("cover");
}

/// A killed run with no group record cannot say whether its agent has
/// stopped, so it keeps today's refusal with one sentence more.
// §FS-rhei-agents.8.1.1 §FS-rhei-evidence.3.4
#[test]
fn a_killed_agent_with_no_group_record_keeps_the_refusal_and_says_why() {
    let fx = Fixture::new("killed-run-no-group");
    fx.hold("cover");
    let mut first = fx.spawn_run("first");
    let pgid = fx.wait_for_hold("cover", 1, &mut first, "first");
    fx.kill_run_and_settle(first, pgid, "first");
    let groups = fx.group_records();
    assert_eq!(groups.len(), 2, "a group record per spawn - prepare and cover: {groups:?}");
    for group in groups {
        std::fs::remove_file(group).expect("remove group record");
    }

    fx.release("cover");
    let (status, output) = fx.run_to_end("second");
    assert!(!status.success(), "{output}");
    assert!(output.contains("exists and no spawn record accounts for it"), "{output}");
    assert!(
        output.contains("its run was killed, but rhei cannot tell whether that agent has stopped"),
        "the refusal says why it cannot resume\n{output}"
    );
    assert_eq!(fx.starts("cover"), 1);
}

/// In a poll state, which `attempts:` does not bound, the second kill of a
/// visit halts it and names `rhei retry`.
// §FS-rhei-agents.3.2.3 §FS-rhei-retry.2
#[test]
fn a_poll_visit_killed_twice_halts_naming_rhei_retry() {
    let fx = Fixture::poll("killed-run-poll");
    fx.hold("waiting");
    for (n, name) in [(1, "first"), (2, "second")] {
        let mut run = fx.spawn_run(name);
        let pgid = fx.wait_for_hold("waiting", n, &mut run, name);
        fx.kill_run_and_settle(run, pgid, name);
    }
    let (status, output) = fx.run_to_end("third");
    assert!(!status.success(), "the second kill halts the poll visit\n{output}");
    assert!(output.contains("killed together with its run 2 times on this visit"), "{output}");
    assert!(output.contains("rhei retry"), "the halt names the attended remedy\n{output}");
    assert_eq!(fx.starts("waiting"), 2, "no third spawn");
    fx.assert_state("waiting");
}

/// A killed attempt's `ended` is when the next run learned of the kill, so a
/// summary taken while the resumed attempt runs shows no duration for it
/// rather than counting the gap as run time.
// §FS-rhei-summary.2.2 §FS-rhei-agents.8.4
#[test]
fn a_summary_counts_no_time_for_a_killed_attempt() {
    let fx = Fixture::new("killed-run-summary");
    fx.hold("cover");
    let mut first = fx.spawn_run("first");
    let pgid = fx.wait_for_hold("cover", 1, &mut first, "first");
    fx.kill_run_and_settle(first, pgid, "first");
    // A gap a summary would print as seconds of run time.
    std::thread::sleep(std::time::Duration::from_secs(3));

    let mut second = fx.spawn_run("second");
    fx.wait_for_hold("cover", 2, &mut second, "second");
    let summary = summary_details(&fx);
    drop(second);
    let killed: Vec<&str> =
        summary.lines().filter(|line| line.contains("cover — mock —")).collect();
    assert_eq!(killed.len(), 1, "one entry for the killed attempt\n{summary}");
    assert!(
        killed[0].ends_with("cover — mock — no accounting record"),
        "a killed attempt has no duration to print\n{summary}"
    );
}
