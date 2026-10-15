//! What every command does when it cannot decide whether a run is alive.
//!
//! The workspace's `.rhei/run.lock` is made unreadable while the run is
//! demonstrably working — the outage the liveness section names verbatim. The
//! run is untouched by it: it holds its lock through an open descriptor, so
//! only the processes *asking about* it are blinded. Every case here failed by
//! reading that silence as "the run has ended".

// §FS-rhei-run-headless.3 §FS-rhei-run-headless.5.3 §FS-rhei-run-headless.7

// Unix-only: the outage every case here builds is a Unix file mode — `chmod 000`
// on the lock file — and there is no Windows equivalent that blinds a reader
// without also disturbing the run.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

use super::headless_support::{parse_records, Workspace};
use super::{stderr, stdout};

/// The lock file a liveness probe opens. Making *this* unreadable — rather than
/// the whole `.rhei` directory — leaves `runtime/run.json` readable, which is
/// what lets the run's own recorded end still be observed.
fn run_lock(ws: &Workspace) -> std::path::PathBuf {
    ws.root.join(".rhei").join("run.lock")
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .unwrap_or_else(|err| panic!("chmod {mode:o} {}: {err}", path.display()));
}

/// Launch a run and blind every later probe of its liveness.
fn launch_unreadable(ws: &Workspace) -> String {
    let id = ws.launch_headless();
    set_mode(&run_lock(ws), 0o000);
    id
}

fn readable_again(ws: &Workspace) {
    set_mode(&run_lock(ws), 0o644);
}

fn descriptor_pid(ws: &Workspace) -> i32 {
    ws.descriptor().expect("descriptor")["pid"].as_u64().expect("pid") as i32
}

/// `kill(pid, 0)`: the question "is this pid still there?" without sending
/// anything.
fn is_alive(pid: i32) -> bool {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    // SAFETY: signal 0 delivers nothing; it only reports whether the pid exists.
    unsafe { kill(pid, 0) == 0 }
}

// ---------------------------------------------------------------------------
// Resolution: an unchecked run is still a run you can name
// ---------------------------------------------------------------------------

/// `rhei runs` lists the id as unchecked, and both commands that act on a run
/// must still reach it by that id. Resolving only live entries left the
/// operator reading an id off one command that the next two refused to accept.
// §FS-rhei-run-headless.3
#[test]
fn an_unchecked_run_resolves_for_both_attach_and_stop() {
    let ws = Workspace::new("undecided-resolve", 20);
    let id = launch_unreadable(&ws);

    let listed = stdout(&ws.rhei(&["runs"]));
    assert!(listed.contains("could not be checked"), "the entry is undecided: {listed}");
    assert!(listed.contains(&id), "and named: {listed}");

    // Neither command may answer "no run matches"; what they do *after*
    // resolving is what the rest of this file is about.
    let attached = ws.rhei(&["attach", "--wait", &id]);
    let attach_message = format!("{}{}", stdout(&attached), stderr(&attached));
    assert!(!attach_message.contains("no run matches"), "attach lost the id: {attach_message}");

    let stopped = ws.rhei(&["stop", &id]);
    let stop_message = format!("{}{}", stdout(&stopped), stderr(&stopped));
    assert!(!stop_message.contains("no run matches"), "stop lost the id: {stop_message}");

    readable_again(&ws);
    ws.stop_quietly();
}

// ---------------------------------------------------------------------------
// §FS-rhei-run-headless.7: stop
// ---------------------------------------------------------------------------

/// The operator asked to make sure the run is not running. A probe that could
/// not answer is not an answer, so the signal goes anyway — and `--wait` waits
/// for the run to actually go rather than returning on the same silence.
// §FS-rhei-run-headless.7 §FS-rhei-run-headless.3
#[cfg(not(target_os = "linux"))]
#[test]
fn stop_wait_signals_an_unchecked_run_and_waits_for_it_to_really_end() {
    let ws = Workspace::new("undecided-stop", 20);
    let id = launch_unreadable(&ws);
    let pid = descriptor_pid(&ws);
    assert!(is_alive(pid), "the run is working when the lock becomes unreadable");

    let out = ws.rhei(&["stop", "--wait", &id]);
    readable_again(&ws);

    assert!(out.status.success(), "stopping is not a failure: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains(&format!("Asked run {id}")), "the signal was delivered: {text}");
    assert!(
        stderr(&out).contains("could not confirm"),
        "and said what it could not check: {}",
        stderr(&out)
    );
    // The two facts that separate "waited" from "returned on an unreadable
    // lock": the process is gone, and the run recorded the status it exits
    // with. Neither was true when `stop` returned, before this fix.

    // §FS-rhei-run.3.2
    assert!(!is_alive(pid), "`--wait` returned while pid {pid} was still running");
    assert!(text.contains("It exited 130"), "it waited for the recorded status: {text}");
}

/// Linux may recover exact ownership through a pidfd even when the pathname is
/// unreadable. It signals only in that case; otherwise it refuses rather than
/// treating numeric pid existence as authorization.
// §FS-rhei-run-headless.7
#[cfg(target_os = "linux")]
#[test]
fn stop_wait_handles_an_unchecked_run_only_with_proven_ownership() {
    let ws = Workspace::new("undecided-stop", 20);
    let id = launch_unreadable(&ws);
    let pid = descriptor_pid(&ws);
    assert!(is_alive(pid), "the run is working when the lock becomes unreadable");

    let out = ws.rhei(&["stop", "--wait", &id]);
    readable_again(&ws);

    if out.status.success() {
        let text = stdout(&out);
        assert!(text.contains(&format!("Asked run {id}")), "the signal was delivered: {text}");
        assert!(!is_alive(pid), "`--wait` returned while pid {pid} was still running");
        assert!(text.contains("It exited 130"), "it waited for the recorded status: {text}");
    } else {
        assert!(
            stderr(&out).contains("refusing to stop")
                && stderr(&out).contains("lock ownership could not be"),
            "the refusal explains the missing proof: {}",
            stderr(&out)
        );
        assert!(is_alive(pid), "the refused stop signalled pid {pid}");
        ws.stop_quietly();
    }
}

// ---------------------------------------------------------------------------
// §FS-rhei-run-headless.5.3: the two waits
// ---------------------------------------------------------------------------

/// The whole of the `--wait` contract, under an outage: keep waiting, then
/// report the run's own result. Reporting "has ended / recorded no exit status"
/// and exiting non-zero — which is what an unreadable lock used to produce —
/// fails the CI step of §FS-rhei-run-headless.5.3 for a run that passed.
// §FS-rhei-run-headless.5.3
#[test]
fn attach_wait_waits_out_the_outage_and_reports_the_runs_own_result() {
    let ws = Workspace::new("undecided-wait-ok", 1);
    let id = launch_unreadable(&ws);

    let started = Instant::now();
    let out = ws.rhei(&["attach", "--wait", &id]);
    let waited = started.elapsed();
    readable_again(&ws);

    let text = stdout(&out);
    assert!(out.status.success(), "the run exited 0:\n{text}\n{}", stderr(&out));
    assert!(text.contains("It exited 0."), "it reported the run's own status: {text}");
    assert!(waited > Duration::from_millis(500), "it returned in {waited:?} without waiting");
}

/// And when the outage never clears, the wait ends the only honest way: saying
/// what it could not check, and failing. What it must never do is print the
/// finished-run block for a run it never saw finish.
// §FS-rhei-run-headless.5.3 §FS-rhei-run-headless.3
#[test]
fn attach_wait_gives_up_loudly_rather_than_reporting_a_running_run_as_ended() {
    let ws = Workspace::new("undecided-wait-stuck", 30);
    let id = launch_unreadable(&ws);
    let pid = descriptor_pid(&ws);

    let started = Instant::now();
    let out = ws.rhei(&["attach", "--wait", &id]);
    let waited = started.elapsed();

    assert!(is_alive(pid), "the run is still working, which is the point of the case");
    readable_again(&ws);
    ws.stop_quietly();

    assert!(!out.status.success(), "a run it could not check is not a run that passed");
    assert!(!stdout(&out).contains("has ended"), "got: {}", stdout(&out));
    assert!(
        stderr(&out).contains("could not tell whether run"),
        "it names what it could not check: {}",
        stderr(&out)
    );
    assert!(waited > Duration::from_secs(1), "it gave up in {waited:?}, without waiting at all");
    assert!(waited < Duration::from_secs(25), "and it is bounded, not a hang");
}

/// A record stream that stops early says the run was interrupted
/// (§FS-rhei-run-json.2.1). Ending one at exit `0` with an empty stderr
/// therefore states an outcome that did not happen.
// §FS-rhei-run-headless.5.3 §FS-rhei-run-json.2.1
#[test]
fn attach_json_never_ends_a_truncated_stream_at_zero() {
    let ws = Workspace::new("undecided-json", 30);
    let id = launch_unreadable(&ws);

    let started = Instant::now();
    let out = ws.rhei(&["attach", "--json", &id]);
    let followed = started.elapsed();
    readable_again(&ws);
    ws.stop_quietly();

    let records = parse_records(&stdout(&out));
    assert!(!records.is_empty(), "it followed the run it could not check");
    assert!(
        !records.iter().any(|record| record["event"] == "run_finished"),
        "the run had not finished, so the stream is genuinely incomplete"
    );
    assert!(!out.status.success(), "an incomplete stream must not exit 0");
    assert!(!stderr(&out).is_empty(), "and must say why on stderr");
    assert!(followed > Duration::from_secs(1), "it stopped following after {followed:?}");
    assert!(followed < Duration::from_secs(25), "and it is bounded, not a hang");
}

// ---------------------------------------------------------------------------
// §FS-rhei-run-headless.8: a run with no event log
// ---------------------------------------------------------------------------

/// `--wait` without `--json` opens no surface and reads no records, so the
/// named-file refusal of §FS-rhei-run-headless.8 does not belong to it: a run that failed to write
/// its event log is still a run whose exit status is worth waiting for.
// §FS-rhei-run-headless.5.3 §FS-rhei-run-headless.8
#[test]
fn attach_wait_works_for_a_run_with_no_event_log() {
    let ws = Workspace::new("undecided-nolog-wait", 1);
    let id = ws.launch_headless();
    let events = ws.root.join("runtime/events.jsonl");
    fs::remove_file(&events).expect("remove the event log");

    let out = ws.rhei(&["attach", "--wait", &id]);
    let message = format!("{}{}", stdout(&out), stderr(&out));
    assert!(!message.contains("no event log"), "the quiet wait reads no records: {message}");
    assert!(out.status.success(), "it waited and reported the run's own status: {message}");
    assert!(!events.exists(), "the run never rewrote the log it could not write");
}

// ---------------------------------------------------------------------------
// §FS-rhei-run-headless.2: the descriptor's shape
// ---------------------------------------------------------------------------

/// The documented object always carries `exit_code`. Omitting it while it is
/// unknown makes a consumer tell "still running" from "this build dropped the
/// field", which is not a distinction the descriptor offers any way to make.
// §FS-rhei-run-headless.2
#[test]
fn a_live_descriptor_carries_a_null_exit_code() {
    let ws = Workspace::new("undecided-exitcode", 20);
    ws.launch_headless();

    let descriptor = ws.descriptor().expect("descriptor");
    let exit_code = descriptor.get("exit_code").expect("exit_code is always present");
    assert!(exit_code.is_null(), "it is null while unknown, not absent: {descriptor}");

    let listed = stdout(&ws.rhei(&["runs", "--json"]));
    let runs: serde_json::Value = serde_json::from_str(&listed).expect("the listing is JSON");
    let first = runs.get(0).expect("one live run");
    assert!(first.get("exit_code").expect("present in the listing too").is_null());
    ws.stop_quietly();
}

// ---------------------------------------------------------------------------
// §FS-rhei-run-json.2.1: paths are named as the journal names them
// ---------------------------------------------------------------------------

/// One run wrote the relative path into `transitions.log` and the absolute path
/// into `events.jsonl`, so a consumer reading both saw two names for one file.
/// A replay re-emits what the log holds, unchanged.
// §FS-rhei-run-json.2.1
#[test]
fn the_event_log_names_paths_the_way_the_journal_does() {
    let ws = Workspace::new("undecided-relpath", 0);
    let id = ws.launch_headless();
    super::headless_support::wait_until("the run to finish", Duration::from_secs(60), || {
        ws.descriptor().is_some_and(|d| d["status"] == "finished" || d["status"] == "failed")
    });

    let logged = fs::read_to_string(ws.root.join("runtime/events.jsonl")).expect("event log");
    let paths: Vec<String> = parse_records(&logged)
        .iter()
        .filter_map(|record| record["log_path"].as_str().map(str::to_string))
        .collect();
    assert!(!paths.is_empty(), "the run assigned at least one slot");
    for path in &paths {
        assert!(!path.starts_with('/'), "recorded absolute: {path}");
        assert!(path.starts_with("runtime/logs/"), "recorded as {path}");
    }

    let journal = fs::read_to_string(ws.root.join("runtime/transitions.log")).expect("journal");
    for path in &paths {
        assert!(journal.contains(path.as_str()), "the journal names it differently: {path}");
    }

    // The replay is the same contract read back: a relative path stays relative
    // rather than being re-absolutized against whoever is reading.
    let replayed: Vec<String> = parse_records(&stdout(&ws.rhei(&["attach", "--json", &id])))
        .iter()
        .filter_map(|record| record["log_path"].as_str().map(str::to_string))
        .collect();
    assert_eq!(paths, replayed, "a replay renamed the run's logs");
}
