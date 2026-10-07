//! Codex refusals park uncharged across sequential/parallel restarts.
//! §FS-rhei-agents.2.3 §FS-rhei-run.3.3

use super::provider_limit_support::*;
use super::*;

const AGENT: &str = include_str!("fixtures/provider_limit_codex/refusal.py");

fn fixture(name: &str, transport: &str, parallel: bool) -> ProviderFixture {
    let machine = SIMPLE_MACHINE
        .replace("target: codex:openai:alpha", "target: cdx:openai:alpha")
        .replace(
            "attempts: 1",
            &format!(
                "concurrent: {parallel}\n    attempts: 2\n    outputs:\n      - name: finding\n        path: runtime/exports/{{task_id}}/finding.md"
            ),
        );
    let body = format!("TRANSPORT = {transport:?}\n{AGENT}");
    let parallel_tasks = [
        SINGLE_TASK[0],
        (
            "02.md",
            "### Task 2: Independent helper\n**State:** working\n**Target:** helper:openai:alpha\n",
        ),
    ];
    let tasks = if parallel { parallel_tasks.as_slice() } else { SINGLE_TASK };
    let fixture = ProviderFixture::new(name, tasks, &machine, &body);
    let helper = write_python_agent(
        &fixture.dir,
        "helper.py",
        "root = pathlib.Path(env('RHEI_ROOT'))\nwrite(root / 'runtime' / 'exports' / env('RHEI_TASK_ID') / 'finding.md', 'helper finding')\nresult('Independent helper completed.\\n')\n",
    );
    fs::write(
        fixture.root.join(".agent-grounds/rhei/settings.json"),
        serde_json::json!({"agents": {"cdx": {
            "family": "codex",
            "command": serde_json::from_str::<serde_json::Value>(&fixture_command(&fixture.agent)).unwrap(),
            "stdin_prompt": true,
            "timeout": "30s"
        }, "helper": {
            "family": "codex",
            "command": serde_json::from_str::<serde_json::Value>(&fixture_command(&helper)).unwrap(),
            "stdin_prompt": true,
            "timeout": "30s"
        }}}).to_string(),
    ).unwrap();
    fixture
}

fn start(
    fixture: &ProviderFixture,
    name: &str,
    child_tz: Option<&str>,
    parallel: bool,
) -> RunningChild {
    let mut command = rhei_command(fixture.root.join(".home"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("RHEI_") {
            command.env_remove(key);
        }
    }
    // Only the child gets TZ; Windows exercises its native OS-local zone.
    // §FS-rhei-run.3.3 §REQ-test-isolation
    if let Some(zone) = child_tz {
        command.env("TZ", zone);
    }
    command.arg("--state-machine").arg(&fixture.machine).arg("run").arg(&fixture.root);
    command.args(["--no-tui", "--no-dashboard", "--json", "--continue-on-error", "--until-idle"]);
    command.args(["--parallel", if parallel { "2" } else { "1" }]);
    RunningChild::spawn(&mut command, &fixture.dir, name)
}

fn assert_parks(transport: &str, parallel: bool, child_tz: Option<&str>) {
    let fixture = fixture("codex-limit-restart", transport, parallel);
    let mut runs = Vec::new();
    let mut statuses = Vec::new();
    let mut snapshots = Vec::new();
    for name in ["first", "restarted", "third"] {
        let mut run = start(&fixture, name, child_tz, parallel);
        statuses.push(run.wait_for_exit("Codex until-idle refusal/restart").code());
        snapshots.push((refusal_records(&fixture), metadata(&fixture.root)));
        runs.push(run.output());
    }
    if parallel {
        assert!(
            runs[0].contains("(parallel)"),
            "must exercise worker-pool completion: {}",
            runs[0]
        );
    }
    let records = refusal_records(&fixture);
    let last = records.last().expect("the fake Codex agent ran");
    assert_eq!(
        last["ending"], "provider_limited",
        "Codex {transport} refusal must park without spending attempts (parallel={parallel}); statuses={statuses:?}; records={records:#?}; {}",
        runs.join("\n")
    );
    let expected: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(fixture.root.join("runtime/expected.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(statuses, vec![Some(3); 3], "{runs:#?}");
    assert_eq!(records.len(), 1, "{records:#?}");
    assert_eq!(last["charged"], 0, "{last:#}");
    assert_eq!(last["attempt_charged"], false, "{last:#}");
    assert_eq!(last["code"], 1, "{last:#}");
    let wait = &snapshots[0].1["metadata"]["tasks"]["1"]["providerLimits"]["working"];
    assert_eq!(wait["nextAttemptAt"], expected["deadline"], "{wait:#}");
    assert_eq!(wait["signal"], expected["signal"], "{wait:#}");
    assert_eq!(wait["identity"], serde_json::json!({"agent": "cdx", "provider": "openai"}));
    assert!(wait["observedAt"].as_str().is_some(), "{wait:#}");
    assert!(wait["nextAttemptAt"].as_str().unwrap() > utc_at(epoch_now()).as_str());
    for snapshot in &snapshots[1..] {
        assert_eq!(snapshot, &snapshots[0], "restart retains records and durable wait");
    }
    let task = fs::read_to_string(fixture.root.join("tasks/01.md")).unwrap();
    assert!(task.contains("**State:** working"), "{task}");
    assert_eq!(fs::read_to_string(fixture.root.join("runtime/starts.txt")).unwrap(), "1\n");
    for output in runs {
        assert!(!output.contains("Re-spawning"), "{output}");
        assert!(!output.contains("attempts spent on this visit"), "{output}");
        assert!(!output.contains("halting Task"), "{output}");
    }
}

fn refusal_records(fixture: &ProviderFixture) -> Vec<serde_json::Value> {
    fixture.records().into_iter().filter(|record| record["worker"] == "cdx").collect()
}

#[test]
fn codex_json_refusal_parks_sequential_across_three_runs() {
    assert_parks("json", false, None);
}

#[test]
fn codex_json_refusal_parks_parallel_across_three_runs() {
    assert_parks("json", true, None);
}

#[test]
fn codex_bare_refusal_parks_sequential_across_three_runs() {
    assert_parks("bare", false, None);
}

#[test]
fn codex_bare_refusal_parks_parallel_across_three_runs() {
    assert_parks("bare", true, None);
}

#[test]
fn codex_plain_stderr_refusal_parks_sequential_across_three_runs() {
    assert_parks("stderr", false, None);
}

#[test]
fn codex_old_wording_control_parks_sequential_across_three_runs() {
    assert_parks("control", false, None);
}

#[test]
fn codex_old_wording_control_parks_parallel_across_three_runs() {
    assert_parks("control", true, None);
}

// Unix honors TZ; the ungated tests above exercise native local wiring on all OSes.
// §FS-rhei-run.3.3
#[cfg(unix)]
#[test]
fn codex_json_refusal_uses_child_non_utc_local_zone() {
    assert_parks("json", false, Some("Europe/Zurich"));
}

/// Exercise native selection in actual child processes, beyond injected-zone
/// classification. Only the child receives TZ. §FS-rhei-run.3.3
#[cfg(unix)]
fn assert_native_date(text: &str, zone: &str, deadline: Option<&str>) {
    let fixture = fixture("codex-native-edge", "json", false);
    let signal = format!(
        "You've hit your usage limit. Visit https://chatgpt.com/codex/settings/usage to purchase more credits or try again at {text}."
    );
    fs::create_dir_all(fixture.root.join("runtime")).unwrap();
    fs::write(
        fixture.root.join("runtime/expected.json"),
        serde_json::json!({
            "signal": signal, "deadline": deadline,
        })
        .to_string(),
    )
    .unwrap();
    let mut baseline = None;
    for name in ["first", "restart", "third"] {
        let mut run = start(&fixture, name, Some(zone), false);
        let status = run.wait_for_exit("native Codex date classification").code();
        let records = refusal_records(&fixture);
        let meta = metadata(&fixture.root);
        let wait = &meta["metadata"]["tasks"]["1"]["providerLimits"]["working"];
        let record = records.last().expect("native refusal spawned");
        if let Some(deadline) = deadline {
            assert_eq!(status, Some(3), "{text}: {}", run.output());
            assert_eq!(records.len(), 1);
            assert_eq!(record["ending"], "provider_limited");
            assert_eq!(record["charged"], 0);
            assert_eq!(record["attempt_charged"], false);
            assert_eq!(wait["nextAttemptAt"], deadline);
            let snapshot = (records, meta);
            if let Some(baseline) = &baseline {
                assert_eq!(&snapshot, baseline);
            } else {
                baseline = Some(snapshot);
            }
            assert_eq!(fs::read_to_string(fixture.root.join("runtime/starts.txt")).unwrap(), "1\n");
        } else {
            assert_eq!(status, Some(1), "{text}: {}", run.output());
            assert_eq!(record["ending"], "exited", "{text}: {record:#}");
            assert!(record["charged"].as_u64().unwrap() > 0);
            assert_eq!(record["attempt_charged"], true);
            assert_eq!(record["code"], 1);
            assert!(wait.is_null(), "ordinary result must not persist a wait: {wait:#}");
        }
    }
}

#[cfg(unix)]
#[test]
fn codex_native_spring_gap_in_either_minute_remains_ordinary() {
    for text in ["Mar 28th, 2032 1:59 AM", "Mar 28th, 2032 2:00 AM"] {
        assert_native_date(text, "Europe/Zurich", None);
    }
}

#[cfg(unix)]
#[test]
fn codex_native_autumn_first_unique_minute_parks_across_restarts() {
    assert_native_date("Oct 31st, 2032 3:00 AM", "Europe/Zurich", Some("2032-10-31T02:01:00Z"));
}

#[cfg(unix)]
#[test]
fn codex_native_upper_year_rejects_unreadable_waits_and_retains_valid_waits() {
    assert_native_date("Dec 31st, 9999 11:59 PM", "UTC0", None);
    assert_native_date("Dec 31st, 9999 6:59 PM", "XST5", None);
    assert_native_date("Dec 31st, 9999 11:58 PM", "UTC0", Some("9999-12-31T23:59:00Z"));
}
