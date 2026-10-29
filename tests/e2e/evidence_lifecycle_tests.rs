//! What each lifecycle command does to a run's evidence: a full reset deletes
//! it, a narrowed one keeps it and writes a reset event, a retry and a recover
//! leave it alone, an interrupted run still ends its records and manifest, a
//! killed one is finalized by the next run, and a live intervention is
//! recorded. §FS-rhei-evidence.10 §FS-rhei-evidence.9.3 §FS-rhei-evidence.5.4

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::evidence_support::{
    assert_store_exists, assert_valid, end_for, every_file, evidence_workspace, one_start, records,
    run_dirs, sha256_hex, EvidenceWorkspace, Profile,
};
use super::{rhei_command, run_cli, write_fixture_file, write_python_agent, CliRun};

/// How long any wait in this file may take before it fails the test.
const BOUND: Duration = Duration::from_secs(60);

/// A stub agent that sleeps while `slow` exists beside it, then finishes; it
/// fails without a result while `fail` exists.
const GATED_AGENT: &str = r#"here = pathlib.Path(__file__).resolve().parent
print('stub agent started', flush=True)
deadline = time.time() + 30
while (here / 'slow').exists() and time.time() < deadline:
    time.sleep(0.2)
if (here / 'fail').exists():
    sys.exit(1)
print('stub agent ran', flush=True)
result('## Result\n\nStub agent finished.\n')
"#;

/// `rhei <args…>` as an operator at a shell: no enclosing run's identity.
fn operator(ws_root: &Path, args: &[&str]) -> CliRun {
    let mut cmd = operator_command(ws_root, args);
    CliRun::from(&cmd.output().expect("rhei runs"))
}

fn operator_command(ws_root: &Path, args: &[&str]) -> Command {
    let mut cmd = rhei_command(ws_root.join(".home"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("RHEI_") {
            cmd.env_remove(key);
        }
    }
    cmd.current_dir(ws_root).args(args);
    cmd
}

/// A workspace whose agent is [`GATED_AGENT`].
fn gated_workspace(prefix: &str, profile: Profile) -> EvidenceWorkspace {
    let ws = evidence_workspace(prefix, profile);
    write_python_agent(&ws.root, "stub-agent.py", GATED_AGENT);
    ws
}

/// Poll until `condition` holds, failing after [`BOUND`].
fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let start = Instant::now();
    while !condition() {
        assert!(start.elapsed() < BOUND, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Wait for `child` to exit within [`BOUND`], killing it otherwise.
fn wait_exit(child: &mut Child, what: &str) {
    let start = Instant::now();
    while child.try_wait().expect("poll child").is_none() {
        if start.elapsed() > BOUND {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{what} did not exit within {}s", BOUND.as_secs());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Spawn `rhei run` on the workspace with output going to files.
fn spawn_run(ws: &EvidenceWorkspace, extra: &[&str], name: &str) -> Child {
    let plan = ws.plan.display().to_string();
    let machine = ws.machine.display().to_string();
    let mut args = vec!["--state-machine", machine.as_str(), "run", plan.as_str(), "--no-tui"];
    args.extend_from_slice(extra);
    let mut cmd = operator_command(&ws.root, &args);
    cmd.stdin(Stdio::null())
        .stdout(fs::File::create(ws.root.join(format!("{name}.stdout"))).expect("stdout file"))
        .stderr(fs::File::create(ws.root.join(format!("{name}.stderr"))).expect("stderr file"));
    cmd.spawn().expect("rhei run starts")
}

/// The agent's log path, which exists today as soon as its spawn opens.
fn agent_log(ws: &EvidenceWorkspace) -> PathBuf {
    ws.root.join("runtime/logs/task-plan.1-work.log")
}

/// §FS-rhei-reset.2 step 4: the store lives under `runtime/`, so a full reset
/// deletes it with the rest. §FS-rhei-evidence.10
#[test]
fn a_full_reset_removes_the_evidence_store() {
    let ws = evidence_workspace("evidence-reset-full", Profile::default());
    ws.run();
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);

    let plan = ws.plan.display().to_string();
    let machine = ws.machine.display().to_string();
    let reset = operator(&ws.root, &["--state-machine", &machine, "reset", &plan, "--yes"]);
    assert!(reset.status.success(), "{}\n{}", reset.stdout, reset.stderr);
    assert!(!evidence.exists(), "a full reset deletes runtime/evidence/");
}

/// A Panta project of one member rhei, `auth`, whose one ticket runs the
/// probe's program and agent; the run root is the project root.
fn panta_workspace(prefix: &str) -> EvidenceWorkspace {
    let ws = evidence_workspace(prefix, Profile::default());
    fs::remove_file(&ws.plan).expect("drop the lone plan");
    write_fixture_file(&ws.root, "index.panta.md", "# Panta: Evidence\n");
    let member = ws.root.join("auth");
    fs::create_dir_all(member.join("tasks")).expect("member tasks");
    write_fixture_file(&member, "index.rhei.md", "# Rhei: Auth\n");
    write_fixture_file(
        &member.join("tasks"),
        "01-probe.md",
        "### Task 1: Probe\n**State:** measure\n",
    );
    EvidenceWorkspace { plan: ws.root.clone(), ..ws }
}

/// A narrowed reset keeps the store, says so, and writes one reset event
/// naming the earlier run and every log it removed.
/// §FS-rhei-evidence.10.1 §FS-rhei-reset.2.1 §FS-rhei-reset.4
#[test]
#[ignore = "red until #520: a narrowed reset writes no reset event and keeps no store; implement removes this"]
fn a_narrowed_reset_keeps_the_store_and_writes_a_reset_event() {
    let ws = panta_workspace("evidence-reset-narrow");
    ws.run();
    let logs: Vec<String> = fs::read_dir(ws.root.join("runtime/logs"))
        .expect("fixture: the run wrote logs")
        .flatten()
        .map(|entry| format!("runtime/logs/{}", entry.file_name().to_string_lossy()))
        .filter(|path| path.contains("task-auth.1-"))
        .collect();
    assert!(!logs.is_empty(), "fixture: the ticket's logs");
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);
    let run_ids: Vec<String> = run_dirs(&evidence)
        .iter()
        .map(|dir| dir.file_name().expect("run id").to_string_lossy().to_string())
        .collect();

    let machine = ws.machine.display().to_string();
    let reset =
        operator(&ws.root, &["--state-machine", &machine, "reset", ".", "--rhei", "auth", "--yes"]);
    assert!(reset.status.success(), "{}\n{}", reset.stdout, reset.stderr);
    assert!(
        reset.stdout.contains("Kept run evidence in runtime/evidence/"),
        "a narrowed reset says it kept the evidence:\n{}",
        reset.stdout
    );
    assert_eq!(run_dirs(&evidence).len(), run_ids.len(), "the run directories are kept");

    let events: Vec<PathBuf> = fs::read_dir(evidence.join("events"))
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    assert_eq!(events.len(), 1, "one reset event: {events:?}");
    let event: serde_json::Value =
        serde_json::from_slice(&fs::read(&events[0]).expect("read event")).expect("event JSON");
    assert_valid("rhei.evidence.event.v1", &event);
    assert_eq!(event["kind"], "reset");
    assert_eq!(event["rheis"], serde_json::json!(["auth"]));
    assert_eq!(event["runs"], serde_json::json!(run_ids));
    let removed: Vec<&str> =
        event["removed"].as_array().expect("removed").iter().filter_map(|p| p.as_str()).collect();
    for log in &logs {
        assert!(removed.contains(&log.as_str()), "{log} is named as removed: {removed:?}");
    }
}

/// An exhausted `attempts: 1` visit, a `rhei retry`, then a rerun: the granted
/// attempt's start record carries the spawn record's grants.
/// §FS-rhei-evidence.3.1 §FS-rhei-retry.2
#[test]
fn a_retried_attempts_start_record_carries_the_grant() {
    let ws = gated_workspace("evidence-retry", Profile::default());
    fs::write(ws.root.join("fail"), "").expect("fail the first attempt");
    let failed = run_cli("run", &ws.plan, &ws.machine, &["--no-tui", "--no-callbacks"]);
    assert!(!failed.status.success(), "fixture: the first attempt fails");

    let machine = ws.machine.display().to_string();
    let plan = ws.plan.display().to_string();
    let retry = operator(&ws.root, &["--state-machine", &machine, "retry", &plan, "--task", "1"]);
    assert!(retry.status.success(), "fixture retry:\n{}\n{}", retry.stdout, retry.stderr);
    fs::remove_file(ws.root.join("fail")).expect("let the next attempt finish");
    ws.run();

    let spawn: serde_json::Value = serde_json::from_slice(
        &fs::read(ws.root.join("runtime/spawns/task-plan.1-work.json")).expect("spawn record"),
    )
    .expect("spawn JSON");
    assert_eq!(spawn["grants"].as_array().map(Vec::len), Some(1), "fixture: {spawn:#}");

    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);
    let granted = records(&evidence, "start")
        .into_iter()
        .find(|record| {
            record["kind"] == "agent"
                && record["invocation_id"].as_str().is_some_and(|id| id.ends_with("::attempt-2"))
        })
        .unwrap_or_else(|| panic!("no start record for attempt 2"));
    assert_eq!(granted["grants"], spawn["grants"], "the granted attempt carries the grant");
}

/// With nothing to recover, `rhei recover` still leaves every evidence byte
/// as it was. §FS-rhei-recover.3 §FS-rhei-evidence.10
#[test]
fn rhei_recover_leaves_every_evidence_byte_unchanged() {
    let ws = evidence_workspace("evidence-recover", Profile::default());
    ws.run();
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);
    let before = every_file(&evidence);

    let root = ws.root.display().to_string();
    let recover = operator(&ws.root, &["recover", &root]);
    assert!(recover.status.success(), "{}\n{}", recover.stdout, recover.stderr);
    assert_eq!(every_file(&evidence), before);
}

/// A detached run stopped while its agent works: the agent's end record says
/// `interrupted`, and the manifest is written with the run's status.
/// §FS-rhei-run.3.2 §FS-rhei-evidence.4 §FS-rhei-evidence.9.1
// `--headless` and `rhei stop` are Unix-only in this suite. §FS-rhei-run-headless.1.3
#[cfg(unix)]
#[test]
fn an_interrupted_run_ends_its_records_and_writes_its_manifest() {
    let ws = gated_workspace("evidence-interrupt", Profile::default());
    fs::write(ws.root.join("slow"), "").expect("hold the agent");
    let plan = ws.plan.display().to_string();
    let machine = ws.machine.display().to_string();
    let launch = operator(
        &ws.root,
        &["--state-machine", &machine, "run", &plan, "--headless", "--no-callbacks"],
    );
    assert!(launch.status.success(), "fixture launch:\n{}\n{}", launch.stdout, launch.stderr);
    let log = agent_log(&ws);
    wait_for("the agent's log", || {
        fs::read_to_string(&log).is_ok_and(|text| text.contains("stub agent started"))
    });
    let stop = operator(&ws.root, &["stop", "--wait"]);
    assert!(stop.status.success(), "fixture stop:\n{}\n{}", stop.stdout, stop.stderr);

    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);
    let agent = one_start(&evidence, "agent");
    assert_eq!(end_for(&evidence, &agent)["ending"], "interrupted");
    let dirs = run_dirs(&evidence);
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(dirs[0].join("manifest.json")).expect("the interrupted run wrote its manifest"),
    )
    .expect("manifest JSON");
    assert_eq!(manifest["status"], "interrupted");
}

/// A run killed outright leaves no manifest; the next run finalizes it as
/// `incomplete`, naming itself, with the killed process not completed.
/// §FS-rhei-evidence.9.3
#[test]
fn a_killed_run_is_finalized_as_incomplete_by_the_next_run() {
    let ws = gated_workspace("evidence-killed", Profile::default());
    fs::write(ws.root.join("slow"), "").expect("hold the agent");
    let mut killed = spawn_run(&ws, &["--no-callbacks"], "killed");
    let log = agent_log(&ws);
    let started = Instant::now();
    while !fs::read_to_string(&log).is_ok_and(|text| text.contains("stub agent started")) {
        if started.elapsed() > BOUND {
            let _ = killed.kill();
            let _ = killed.wait();
            panic!("fixture: the agent never started");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    killed.kill().expect("kill the run outright");
    killed.wait().expect("reap the killed run");
    fs::remove_file(ws.root.join("slow")).expect("let the next attempt finish");

    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);
    let dead = run_dirs(&evidence);
    assert_eq!(dead.len(), 1, "the killed run's directory");
    assert!(!dead[0].join("manifest.json").exists(), "a killed run writes no manifest");
    let dead_id = dead[0].file_name().expect("run id").to_string_lossy().to_string();

    let mut next = spawn_run(&ws, &["--no-callbacks"], "next");
    wait_exit(&mut next, "the next run");
    let descriptor: serde_json::Value =
        serde_json::from_slice(&fs::read(ws.root.join("runtime/run.json")).expect("run.json"))
            .expect("run.json JSON");
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(dead[0].join("manifest.json")).expect("the next run finalized the dead one"),
    )
    .expect("manifest JSON");
    assert_valid("rhei.evidence.manifest.v1", &manifest);
    assert_eq!(manifest["run_id"], dead_id);
    assert_eq!(manifest["status"], "incomplete");
    assert_eq!(manifest["finalized_by"], descriptor["id"]);
    let agent = manifest["invocations"]
        .as_array()
        .expect("invocations")
        .iter()
        .find(|entry| entry["kind"] == "agent")
        .unwrap_or_else(|| panic!("the killed agent is listed: {manifest:#}"))
        .clone();
    assert_eq!(agent["completed"], false);
}

/// A stub that takes its prompt as an argument, keeps stdin open for one live
/// message, prints it, and finishes. No fixture prelude: it would read stdin to
/// its end before the body runs.
const INTERVENE_AGENT: &str = r#"import pathlib
import re
import sys
sys.stdin.reconfigure(encoding='utf-8', newline='')
prompt = next(arg for arg in sys.argv[1:] if arg.startswith('# Task '))
print('stub agent started', flush=True)
line = sys.stdin.readline()
print('got: ' + line.strip(), flush=True)
section = prompt.split('\n## Result\n', 1)[1]
path = pathlib.Path(re.search(r'^- `([^`]+)`$', section, re.MULTILINE).group(1))
if not path.is_absolute():
    root = re.search(r'^- This rhei: `([^`]+)`', prompt, re.MULTILINE)
    path = pathlib.Path(root.group(1) if root else '.') / path
path.parent.mkdir(parents=True, exist_ok=True)
path.write_text('## Result\n\nIntervened.\n', encoding='utf-8')
"#;

/// A live message delivered with `rhei intervene` is recorded in the agent's
/// end record: its hash, size and outcome. §FS-rhei-evidence.5.4 §FS-rhei-viz.5
#[test]
fn a_live_intervention_is_recorded_in_the_end_record() {
    const MESSAGE: &str = "evidence intervention probe";
    let profile = Profile {
        fields: serde_json::json!({
            "prompt_flag": "--prompt",
            "intervene_stdin": true,
            "timeout": "60s",
        }),
        ..Profile::default()
    };
    let ws = evidence_workspace("evidence-intervene", profile);
    fs::write(ws.root.join("stub-agent.py"), INTERVENE_AGENT).expect("intervene stub");
    let mut run = spawn_run(&ws, &["--no-callbacks", "--dashboard"], "intervene");
    let log = agent_log(&ws);
    wait_for("the agent and the dashboard", || {
        ws.root.join("runtime/dashboard.json").is_file()
            && fs::read_to_string(&log).is_ok_and(|text| text.contains("stub agent started"))
    });
    let plan = ws.plan.display().to_string();
    let sent =
        operator(&ws.root, &["intervene", "--plan", &plan, "--task", "plan.1", "-m", MESSAGE]);
    if !sent.status.success() {
        let _ = run.kill();
        let _ = run.wait();
        panic!("fixture intervene:\n{}\n{}", sent.stdout, sent.stderr);
    }
    wait_exit(&mut run, "the intervened run");
    assert!(
        fs::read_to_string(&log).is_ok_and(|text| text.contains(MESSAGE)),
        "fixture: the agent got the message"
    );

    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);
    let agent = one_start(&evidence, "agent");
    let end = end_for(&evidence, &agent);
    let interventions = end["interventions"].as_array().expect("interventions");
    assert_eq!(interventions.len(), 1, "{end:#}");
    let delivered = &interventions[0];
    // The message as sent, without the newline rhei appends on stdin. §FS-rhei-evidence.5.4
    assert_eq!(delivered["sha256"], sha256_hex(MESSAGE.as_bytes()), "{delivered:#}");
    assert_eq!(delivered["bytes"], MESSAGE.len(), "{delivered:#}");
    assert!(delivered["outcome"].as_str().is_some_and(|o| !o.is_empty()), "{delivered:#}");
}
