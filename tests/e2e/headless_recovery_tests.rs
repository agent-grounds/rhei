//! What a detached run leaves behind, and what survives reading it back.
//!
//! Every case here is a way the run and the process asking about it disagree:
//! the run ended, the run was killed, its registry entry cannot be read, two
//! launchers raced. The unit tests pin the classification; these prove the real
//! binary acts on it.
// §FS-rhei-run-headless.2 §FS-rhei-run-headless.3 §FS-rhei-run-headless.5

use std::fs;
#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use super::headless_support::wait_until;
use super::headless_support::{parse_records, Workspace};
use super::{stderr, stdout};

/// A run reference's registry entry, read straight from the isolated state
/// directory the workspace pins.
// Only the detached-run cases use it, and those are Unix-only.
// §FS-rhei-run-headless.1.3
#[cfg(unix)]
fn registry_entry(ws: &Workspace, id: &str) -> Option<serde_json::Value> {
    let path = registry_path(ws, id);
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn registry_path(ws: &Workspace, id: &str) -> std::path::PathBuf {
    ws.home.join("state").join("rhei").join("runs").join(format!("{id}.json"))
}

/// Launch, then wait for the run to record its own end.
// Only the detached-run cases use it, and those are Unix-only.
// §FS-rhei-run-headless.1.3
#[cfg(unix)]
fn run_to_completion(ws: &Workspace) -> String {
    let id = ws.launch_headless();
    wait_until("the detached run to finish", Duration::from_secs(60), || {
        ws.descriptor().is_some_and(|d| d["status"] == "finished" || d["status"] == "failed")
    });
    id
}

// ---------------------------------------------------------------------------
// §FS-rhei-run-headless.2: the entry outlives the run
// ---------------------------------------------------------------------------

/// The CI shape of §FS-rhei-run-headless.5.3, run in the order CI actually
/// runs it: the answer is asked for after it exists.
// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[cfg(unix)]
#[test]
fn attach_json_wait_replays_a_run_that_has_already_ended() {
    let ws = Workspace::new("headless-replay", 0);
    let id = run_to_completion(&ws);

    let out = ws.rhei(&["attach", "--json", "--wait", &id]);
    assert!(out.status.success(), "the run exited 0, so --wait does too: {}", stderr(&out));
    let records = parse_records(&stdout(&out));
    assert!(records.len() > 3, "the whole log replays after the run is gone");
    assert_eq!(records[0]["event"], "run_started");
    assert!(
        records.iter().any(|record| record["event"] == "run_finished"),
        "the terminator is in the replay"
    );

    // And the entry is what made the bare id resolve at all.
    let entry = registry_entry(&ws, &id).expect("the entry outlives the run");
    assert_eq!(entry["status"], "finished");
    assert_eq!(entry["exit_code"], 0);
}

/// A replay must re-emit the instant the run recorded, not the instant it was
/// read back. §FS-rhei-run-json.2
// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[cfg(unix)]
#[test]
fn attach_json_preserves_each_records_original_timestamp() {
    let ws = Workspace::new("headless-ts", 0);
    let id = run_to_completion(&ws);

    let logged = fs::read_to_string(ws.root.join("runtime/events.jsonl")).expect("event log");
    let written: Vec<String> = parse_records(&logged)
        .iter()
        .map(|record| record["ts"].as_str().expect("ts").to_string())
        .collect();
    let replayed: Vec<String> = parse_records(&stdout(&ws.rhei(&["attach", "--json", &id])))
        .iter()
        .map(|record| record["ts"].as_str().expect("ts").to_string())
        .collect();
    assert_eq!(written, replayed, "a replay restamped every record with the replay instant");
}

/// Three ended runs and one live one. A prefix they all share must reach the
/// live run, or a two-character prefix stops working as ended entries pile up.
// §FS-rhei-run-headless.3
// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[cfg(unix)]
#[test]
fn a_shared_prefix_resolves_to_the_live_run_not_an_ended_one() {
    let ws = Workspace::new("headless-prefix", 20);
    let live = ws.launch_headless();
    let prefix = &live[..2];

    // Ended runs whose ids share the live run's prefix by construction: a real
    // run's id is time-derived, so a collision cannot be arranged any other way.
    for index in 1..=3 {
        let id = format!("{prefix}zzz{index}");
        let ended = ws.root.join(format!("ended{index}"));
        fs::create_dir_all(ended.join("runtime")).expect("ended workspace");
        let descriptor = serde_json::json!({
            "id": id,
            "pid": 1,
            "status": "finished",
            "workspace": ended,
            "plan": ended.join("plan.rhei.md"),
            "started_at": format!("2026-08-22T0{index}:00:00Z"),
            "headless": true,
            "parallel": 1,
            "events": ended.join("runtime/events.jsonl"),
            "exit_code": 0,
        });
        let body = serde_json::to_string_pretty(&descriptor).expect("descriptor");
        fs::write(ended.join("runtime/run.json"), &body).expect("workspace descriptor");
        fs::create_dir_all(registry_path(&ws, &id).parent().expect("dir")).expect("registry");
        fs::write(registry_path(&ws, &id), &body).expect("registry entry");
    }

    // `stop` names the run it resolved, which is what makes this observable —
    // and stops the run the test started, which is what makes it tidy.
    let out = ws.rhei(&["stop", prefix, "--wait"]);
    assert!(out.status.success(), "stopping is not a failure: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains(&format!("Asked run {live}")), "the live run wins the prefix: {text}");

    // The ended entries are still reachable in full, which is the point of
    // keeping them.
    let ended = ws.rhei(&["attach", &format!("{prefix}zzz2")]);
    assert!(stdout(&ended).contains("has ended"), "got: {}", stdout(&ended));
}

/// A run nothing could record an exit code for did not succeed, and `--wait`
/// must not say it did. §FS-rhei-run-headless.5.3
// Unix-only: the state under test is reached by `kill(pid, SIGKILL)` — a signal
// the run cannot catch, so it dies with nothing written.
#[cfg(unix)]
#[test]
fn attach_wait_fails_for_a_run_that_recorded_no_exit_status() {
    let ws = Workspace::new("headless-killed", 30);
    let id = ws.launch_headless();
    let pid = ws.descriptor().expect("descriptor")["pid"].as_u64().expect("pid") as i32;
    unsafe { libc_kill(pid, 9) };
    wait_until("the run to be gone", Duration::from_secs(30), || {
        !stdout(&ws.rhei(&["runs"])).contains(&id)
    });

    let out = ws.rhei(&["attach", "--wait", &id]);
    assert!(!out.status.success(), "a killed run is not a passing run");
    let message = format!("{}{}", stdout(&out), stderr(&out));
    assert!(message.contains("recorded no exit status"), "got: {message}");
}

#[cfg(unix)]
unsafe fn libc_kill(pid: i32, signal: i32) {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    kill(pid, signal);
}

/// The workspace is gone, so nothing can make the pointer meaningful again.
/// This is one of the only two conditions that prune. §FS-rhei-run-headless.2
// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[cfg(unix)]
#[test]
fn a_deleted_workspace_prunes_the_entry() {
    let ws = Workspace::new("headless-pruned", 0);
    let id = run_to_completion(&ws);
    assert!(registry_path(&ws, &id).is_file());

    fs::remove_dir_all(ws.root.join("runtime")).expect("delete the run's runtime directory");
    assert!(ws.rhei(&["runs"]).status.success());
    assert!(!registry_path(&ws, &id).exists(), "an entry whose workspace forgot it is pruned");
}

// ---------------------------------------------------------------------------
// §FS-rhei-run-headless.3: unknown is not dead
// ---------------------------------------------------------------------------

/// A `chmod 000` is a transient accident. Listing must neither hide the run nor
/// destroy its entry. §FS-rhei-run-headless.3
// Unix-only: a Unix file mode is what makes the directory unreadable here.
#[cfg(unix)]
#[test]
fn an_unreadable_workspace_keeps_its_entry_and_says_so() {
    use std::os::unix::fs::PermissionsExt;
    let ws = Workspace::new("headless-chmod", 20);
    let id = ws.launch_headless();
    let rhei_dir = ws.root.join(".rhei");

    fs::set_permissions(&rhei_dir, fs::Permissions::from_mode(0o000)).expect("chmod 000");
    let blind = ws.rhei(&["runs"]);
    // The outage is the run's own death sentence: an unreadable `.rhei` fails
    // the engine's recovery-marker check, which ends the run §FS-rhei-recover.4.
    // Hold the window open until it has, so what follows is pinned, not raced.
    wait_until("the outage to end the run it covers", Duration::from_secs(60), || {
        ws.descriptor().is_some_and(|d| d["status"] == "failed" || d["status"] == "finished")
    });
    fs::set_permissions(&rhei_dir, fs::Permissions::from_mode(0o755)).expect("chmod 755");

    let text = stdout(&blind);
    assert!(text.contains("could not be checked"), "an unknown entry is listed: {text}");
    assert!(text.contains(&id), "and named: {text}");
    assert!(registry_path(&ws, &id).is_file(), "and kept");

    // Readable again, live again — nothing was lost to the outage.
    assert!(stdout(&ws.rhei(&["runs"])).contains(&format!("{id}  running")));
}

/// An older binary reading a newer one's registry must not destroy it.
// §FS-rhei-run-headless.3
#[test]
fn an_unparseable_entry_is_kept_rather_than_deleted() {
    let ws = Workspace::new("headless-unparseable", 0);
    let entry = registry_path(&ws, "future");
    fs::create_dir_all(entry.parent().expect("dir")).expect("registry dir");
    fs::write(&entry, "{\"written_by\": \"a newer rhei\"}\n").expect("entry");

    let out = ws.rhei(&["runs"]);
    assert!(out.status.success());
    assert!(entry.is_file(), "reading a listing must not delete what it cannot parse");
    assert!(stdout(&out).contains("could not be checked"), "got: {}", stdout(&out));
}

// ---------------------------------------------------------------------------
// §FS-rhei-run-headless.1.1: two launchers
// ---------------------------------------------------------------------------

/// Two launches at once: one run starts, the other says so at once, and no
/// second run appears later. §FS-rhei-run-headless.1.1
// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[cfg(unix)]
#[test]
fn simultaneous_headless_launches_start_exactly_one_run() {
    let ws = Workspace::new("headless-race", 20);
    let plan = ws.plan().display().to_string();
    let machine = ws.machine().display().to_string();
    let args = ["run", "--state-machine", machine.as_str(), plan.as_str(), "--headless"];

    let started = Instant::now();
    let (first, second) = std::thread::scope(|scope| {
        let left = scope.spawn(|| ws.rhei(&args));
        let right = scope.spawn(|| ws.rhei(&args));
        (left.join().expect("first launcher"), right.join().expect("second launcher"))
    });
    assert!(
        started.elapsed() < Duration::from_secs(25),
        "the loser must fail fast, not wait out the handshake"
    );

    let (winner, loser) = if first.status.success() { (first, second) } else { (second, first) };
    assert!(winner.status.success(), "exactly one launch succeeds");
    assert!(!loser.status.success(), "and exactly one fails");
    let complaint = format!("{}{}", stdout(&loser), stderr(&loser));
    assert!(
        complaint.contains("already live") || complaint.contains("already starting a run"),
        "the loser names the conflict rather than timing out: {complaint}"
    );

    // Not merely deferred: no second run starts once the first is gone.
    let listed = stdout(&ws.rhei(&["runs"]));
    let summaries =
        listed.lines().filter(|line| line.contains("pid ") && line.contains("parallel ")).count();
    assert_eq!(summaries, 1, "one live run, not two: {listed}");
    ws.stop_quietly();
    assert!(stdout(&ws.rhei(&["runs"])).contains("No runs are live"));
}

/// The launch lock covers one workspace; the run lock covers every execution
/// root a run touches, which is the case a per-workspace lock cannot see. The
/// child must refuse it immediately rather than block while its launcher's
/// handshake times out.
// §FS-rhei-run.2.6 §FS-rhei-run-headless.1.1
// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[cfg(unix)]
#[test]
fn a_child_that_loses_the_run_lock_fails_fast_with_the_lock_diagnostic() {
    let ws = Workspace::new("headless-childlock", 20);
    ws.launch_headless();
    // Hiding the descriptor is what makes the launcher's own pre-check blind,
    // leaving the refusal to the child — which is exactly the shape of two
    // launches on different member plans that share a root.
    let descriptor = ws.root.join("runtime/run.json");
    let hidden = ws.root.join("run.json.hidden");
    fs::rename(&descriptor, &hidden).expect("hide the descriptor");

    let started = Instant::now();
    let second = ws.run(&["--headless"]);
    let elapsed = started.elapsed();
    fs::rename(&hidden, &descriptor).expect("restore the descriptor");

    assert!(!second.status.success(), "a second run must not start");
    assert!(elapsed < Duration::from_secs(20), "it refused in {elapsed:?}, not fast enough");
    let message = format!("{}{}", stdout(&second), stderr(&second));
    assert!(message.contains("already live"), "got: {message}");
    assert!(message.contains("run.lock"), "the run lock is what refused it: {message}");
    // The refused root is named inside the refusal, and the name arrives whole:
    // a writer sharing this console used to cut the path mid-token, leaving
    // `already live` standing and the rest overwritten. Search only between the
    // two matches above, because the launcher's own `help:` line names the
    // console log under this same root — over the whole message the check would
    // pass on exactly the cut it guards. The basename, not the path, because
    // macOS canonicalises the temporary directory's `/var` to `/private/var`.
    let name = ws.root.file_name().expect("the workspace root has a name");
    let name = name.to_str().expect("the workspace root's name is utf-8");
    let refusal = message.find("already live").and_then(|start| {
        message[start..].find("run.lock").map(|cut| &message[start..start + cut])
    });
    assert!(
        refusal.is_some_and(|refusal| refusal.contains(name)),
        "the refused root's name was cut: {message}"
    );
}

// ---------------------------------------------------------------------------
// §FS-rhei-run-json: the stream and the log agree
// ---------------------------------------------------------------------------

/// A dry run is side-effect-free, but the frontend the caller asked for is
/// still the frontend it gets. §FS-rhei-run-json.4
#[test]
fn a_json_dry_run_puts_records_on_stdout_and_writes_no_event_log() {
    let ws = Workspace::new("headless-dry", 0);
    let out = ws.run(&["--json", "--dry-run"]);
    assert!(out.status.success(), "dry run failed: {}", stderr(&out));

    let records = parse_records(&stdout(&out));
    assert_eq!(records.first().map(|r| r["event"].clone()), Some("run_started".into()));
    assert_eq!(records.last().map(|r| r["event"].clone()), Some("run_finished".into()));
    assert!(!ws.root.join("runtime/events.jsonl").exists(), "a dry run writes no event log");
    assert!(!ws.root.join("runtime/run.json").exists(), "and publishes no descriptor");
}

/// A one-ticket workspace driven by a fake agent that prints, so there is real
/// `agent_output` traffic to inline. The program-driven fixture next door never
/// produces any: only an agent's stdout becomes `agent_output` records.
fn chatty_agent_workspace(
    prefix: &str,
) -> (super::TestDir, std::path::PathBuf, std::path::PathBuf) {
    use super::{unique_temp_dir, write_fixture_file};

    let dir = unique_temp_dir(prefix);
    let workspace = dir.join("workspace");
    fs::create_dir_all(workspace.join(".agent-grounds/rhei")).expect("workspace");
    fs::write(
        workspace.join("plan.rhei.md"),
        "# Rhei: Chatty\n\n## Tasks\n\n### Task 1: Work\n**State:** work\n",
    )
    .expect("plan");

    let agent = super::write_python_agent(
        &dir,
        "chatty-agent.py",
        r#"print('agent line one', flush=True)
print('agent line two', flush=True)
result('## Result\n\nDone.\n')
"#,
    );
    let command = super::fixture_command(&agent);
    fs::write(
        workspace.join(".agent-grounds/rhei/settings.json"),
        format!(
            "{{\n  \"defaults\": {{ \"agent\": \"mock\" }},\n  \"agents\": \
             {{ \"mock\": {{ \"command\": {command}, \"stdin_prompt\": true, \"timeout\": \"2m\" }} }}\n}}"
        ),
    )
    .expect("settings");

    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        "name: chatty\nversion: 1\nstates:\n  work:\n    initial: true\n    \
         description: Do it\n    agent: mock\n    agent_timeout: 2m\n  completed:\n    \
         final: true\n    description: Done\ntransitions:\n  - from: work\n    \
         to: completed\n",
    );
    (dir, workspace, machine)
}

/// Inlined agent output must not renumber the structural records, or `--since`
/// on the durable log skips records the stdout stream already numbered past.
// §FS-rhei-run-json.2 §FS-rhei-run-json.2.3
#[test]
fn inlined_agent_output_leaves_the_structural_sequence_alone() {
    let (_dir, workspace, machine) = chatty_agent_workspace("headless-agent-output");
    let home = workspace.join(".home");
    fs::create_dir_all(home.join("state")).expect("isolated home");
    let out = super::rhei_process()
        .current_dir(&workspace)
        .env("HOME", &home)
        .env("XDG_STATE_HOME", home.join("state"))
        .args([
            "run",
            "--state-machine",
            &machine.display().to_string(),
            &workspace.join("plan.rhei.md").display().to_string(),
            "--json",
            "--json-agent-output",
        ])
        .output()
        .expect("rhei runs");
    assert!(out.status.success(), "run failed: {}", stderr(&out));

    let streamed = parse_records(&stdout(&out));
    assert!(
        streamed.iter().any(|record| record["event"] == "agent_output"),
        "the flag asked for the traffic and got it"
    );
    for record in &streamed {
        if record["event"] == "agent_output" {
            assert!(record["seq"].is_null(), "agent output is not a cursor point: {record}");
        }
    }

    let structural: Vec<u64> =
        streamed.iter().filter_map(|record| record["seq"].as_u64()).collect();
    let logged = fs::read_to_string(workspace.join("runtime/events.jsonl")).expect("event log");
    let durable: Vec<u64> =
        parse_records(&logged).iter().filter_map(|record| record["seq"].as_u64()).collect();
    assert_eq!(structural, durable, "one run has one structural sequence, not two");
    assert_eq!(structural, (1..=structural.len() as u64).collect::<Vec<_>>(), "gap-free from 1");
}

/// The marker that says "you are the detached child" describes the process, not
/// its work: an agent or program must inherit a clean environment.
// §FS-rhei-run-headless.1.2
// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[cfg(unix)]
#[test]
fn the_detached_child_marker_does_not_reach_supervised_work() {
    let ws = Workspace::new("headless-marker", 0);
    run_to_completion(&ws);

    let log = ws.root.join("runtime/logs/task-plan.1-pending.log");
    let text = fs::read_to_string(&log).unwrap_or_else(|err| panic!("{}: {err}", log.display()));
    assert!(
        text.contains("headless-marker=unset"),
        "the detached-child marker leaked into the program: {text}"
    );
}

// ---------------------------------------------------------------------------
// §FS-rhei-run-headless.5: what attach records and refuses
// ---------------------------------------------------------------------------

/// A surface that resolved a different state machine draws states the run
/// cannot be in. §FS-rhei-run-headless.5
// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[cfg(unix)]
#[test]
fn the_descriptor_records_the_state_machine_the_run_resolved() {
    let ws = Workspace::new("headless-machine", 0);
    run_to_completion(&ws);

    let recorded = ws.descriptor().expect("descriptor")["state_machine"]
        .as_str()
        .map(std::path::PathBuf::from)
        .expect("an explicit --state-machine is recorded");
    assert!(recorded.is_absolute(), "recorded as {}", recorded.display());
    assert_eq!(
        recorded.canonicalize().ok(),
        ws.machine().canonicalize().ok(),
        "the run's own machine, not whatever the default resolves to"
    );
}

/// The named-file diagnostic §FS-rhei-run-headless.8 promises belongs to the
/// record stream as much as to the terminal surface — which followed a file
/// nothing was writing, forever.
// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[cfg(unix)]
#[test]
fn attach_json_refuses_a_live_run_with_no_event_log() {
    let ws = Workspace::new("headless-nolog", 20);
    let id = ws.launch_headless();
    let events = ws.root.join("runtime/events.jsonl");
    fs::remove_file(&events).expect("remove the event log");

    let started = Instant::now();
    let out = ws.rhei(&["attach", "--json", &id]);
    assert!(started.elapsed() < Duration::from_secs(20), "it must refuse, not follow forever");
    assert!(!out.status.success(), "there is nothing to follow");
    let message = format!("{}{}", stdout(&out), stderr(&out));
    assert!(message.contains("no event log"), "got: {message}");
    assert!(message.contains(&path_fragment(&events)), "the diagnostic names the file: {message}");
}

// Only the detached-run cases use it, and those are Unix-only.
// §FS-rhei-run-headless.1.3
#[cfg(unix)]
fn path_fragment(path: &Path) -> String {
    path.file_name().expect("file name").to_string_lossy().into_owned()
}
