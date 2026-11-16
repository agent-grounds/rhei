//! A provider record on one arm must not change a future poll's classification
//! when another invocation still to be spawned is free (agent-grounds/rhei#735).
//! The held/control fixture ports tool-reports/runtime/repro/rhei.132/run.sh.
//! §FS-rhei-run.5.1 §FS-rhei-run-report.3.1 §FS-rhei-run-json.2.1

use super::provider_limit_support::*;
use super::until_idle_support::{assert_exit, bounded, records_of, stop_payload, EXIT_IDLE};
use super::*;

const MACHINE: &str = r#"name: poll-fanout-classification
version: 1
states:
  fix:
    initial: true
    all_targets: ["pi:anthropic:claude-opus-5-5", "pi:openai:gpt-6"]
    attempts: 1
    instructions: Fix it.
    poll: { interval: 1h, max_attempts: 5 }
  completed: { final: true }
transitions:
  - { from: fix, to: fix, condition: pollAttempts < pollMaxAttempts }
  - { from: fix, to: completed }
"#;

/// Both target strings go through this stand-in; a scheduling regression must
/// leave evidence rather than ever invoking a real provider.
const LOG_START: &str = r#"append(pathlib.Path(env('RHEI_ROOT')) / 'starts', 'unexpected spawn\n')
raise SystemExit(1)
"#;

struct Observation {
    console: String,
    stop: serde_json::Value,
    plain_report: String,
    json_report: String,
}

/// Separate workspaces for each invocation keep the plain and JSON reports
/// independent. The sole difference between held and control is the record.
fn observe(case: &str, near: u64, far: u64) -> Observation {
    let mut console = String::new();
    let mut stop = serde_json::Value::Null;
    let mut reports = Vec::new();
    for json in [false, true] {
        let fixture = ProviderFixture::new(
            &format!("rhei-735-{case}-{json}"),
            &[("01.md", "### Task 1: Fix it\n**State:** fix\n")],
            MACHINE,
            LOG_START,
        );
        let settings_path = fixture.root.join(".agent-grounds/rhei/settings.json");
        let mut settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        settings["agents"]["pi"] = settings["agents"]["codex"].clone();
        fs::write(settings_path, settings.to_string()).unwrap();
        edit_metadata(&fixture.root, |m| {
            m["metadata"] = serde_json::json!({"tasks": {"1": {
                "pollNextAttemptAt": {"fix": near}
            }}});
            if case == "held" {
                let mut record = limit_record(far);
                record["identity"] = serde_json::json!({"agent": "pi", "provider": "anthropic"});
                m["metadata"]["tasks"]["1"]["providerLimits"] = serde_json::json!({"fix": record});
            }
        });

        let home = fixture.root.join(".home");
        let mut command = rhei_command(&home);
        // No outer run, provider credentials or machine settings may decide
        // this fixture. Keep only what the host needs to run the stand-in.
        command.env_clear();
        for key in ["PATH", "SystemRoot", "WINDIR", "COMSPEC", "PATHEXT", "TMPDIR", "TMP", "TEMP"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command.env("HOME", &home).env("USERPROFILE", &home);
        command.env("XDG_CONFIG_HOME", home.join("config"));
        command.env("XDG_STATE_HOME", home.join("state"));
        command.current_dir(&fixture.root);
        command.arg("--state-machine").arg(&fixture.machine).arg("run").arg(&fixture.root);
        command.args(["--until-idle", "--no-tui", "--no-dashboard"]);
        if json {
            command.arg("--json");
        }
        let result = bounded(command, "poll fan-out held/control", &fixture.dir);
        assert_exit(&result, EXIT_IDLE, &format!("{case}, json={json}: poll holds the ticket"));
        assert!(
            !fixture.root.join("starts").exists(),
            "{case}, json={json}: no agent may spawn; {}{}",
            result.stdout,
            result.stderr
        );
        if json {
            assert!(records_of(&result, "slot_assigned").is_empty(), "{}", result.stdout);
            let finished = records_of(&result, "run_finished");
            assert_eq!(finished.len(), 1, "one stop record: {}", result.stdout);
            assert_eq!(finished[0]["summary"]["agents_spawned"], 0);
            stop = stop_payload(&result);
            assert_eq!(stop["reason"], "idle");
            assert_eq!(stop["exit_code"], EXIT_IDLE);
            assert_eq!(stop["next_attempt_at"], utc_at(near), "{case}: deadline stays NEAR");
        } else {
            console = format!("{}{}", result.stdout, result.stderr);
            assert!(console.contains("Run idle:"), "{case}: {console}");
            assert!(console.contains(&format!("next attempt {}", utc_at(near))), "{console}");
        }
        reports.push(fs::read_to_string(fixture.root.join("runtime/run-report.md")).unwrap());
    }
    Observation { console, stop, plain_report: reports.remove(0), json_report: reports.remove(0) }
}

/// Every classification surface agrees with the control. Collect mismatches
/// so the red run demonstrates the count, blocker and false report rows, while
/// setup, no-spawn and already-correct deadline checks fail independently.
/// §FS-rhei-run.5.1 §FS-rhei-run-report.3.1 §FS-rhei-run-json.2.1
#[test]
fn until_idle_classifies_a_polling_fan_out_with_a_free_arm_like_its_control() {
    let now = epoch_now();
    let near = now + 3600;
    let far = now + 7200;
    let control = observe("control", near, far);
    assert!(control.console.contains("1 waiting on poll"), "{}", control.console);
    assert_eq!(control.stop["idle_blocker"], "poll", "invalid control: {}", control.stop);
    let held = observe("held", near, far);
    let mut failures = Vec::new();
    if !held.console.contains("1 waiting on poll") {
        failures.push(format!("held console must count one poll wait:\n{}", held.console));
    }
    if held.stop != control.stop {
        failures.push(format!(
            "held stop must match control:\nheld={}\ncontrol={}",
            held.stop, control.stop
        ));
    }
    for (case, observation) in [("control", &control), ("held", &held)] {
        for (surface, text) in [
            ("console", &observation.console),
            ("plain report", &observation.plain_report),
            ("JSON report", &observation.json_report),
        ] {
            if text.contains("provider anthropic limited until") {
                failures.push(format!(
                    "{case} {surface} falsely attributes the wait to a provider:\n{text}"
                ));
            }
            if surface != "console"
                && !text.contains(&format!("Result: idle — retry at {}", utc_at(near)))
            {
                failures.push(format!("{case} {surface} must report idle at NEAR:\n{text}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
