//! Provider classification preserves completion side effects and precedence.
//! §FS-rhei-run.3.3 §FS-rhei-agents.5.2.1 §FS-rhei-agents.5.2.2

use super::provider_limit_support::*;
use super::*;

fn snapshots(fixture: &ProviderFixture) -> serde_json::Value {
    let result = super::snapshot_tests::run_snapshot_command(
        &fixture.root,
        &fixture.machine,
        &[
            "list",
            "--plan",
            fixture.root.to_str().unwrap(),
            "--format",
            "json",
            "--produced-by",
            "all",
        ],
    );
    assert_success(&result);
    serde_json::from_str(&result.stdout).unwrap()
}

/// R1-07: enabled callbacks and usable session snapshots stay silent on a
/// refusal, then demonstrably fire on successful resumption in both paths.
/// §FS-rhei-run.3.3 §FS-rhei-agents.5.2.1 §FS-rhei-agents.5.2.2
#[test]
fn provider_limit_parking_suppresses_enabled_completion_side_effects() {
    for parallel in [1, 2] {
        for continue_on_error in [false, true] {
            let tasks = [
                ("01.md", "### Task 1: First\n**State:** working\n"),
                ("02.md", "### Task 2: Second\n**State:** working\n"),
            ];
            let fixture = ProviderFixture::new(
                "provider-side-effects",
                &tasks[..parallel],
                SIMPLE_MACHINE,
                &format!(
                    r#"if sys.argv[-1] == 'callback':
    append(pathlib.Path(__file__).parent/'callbacks', 'fired\n')
    print('{{"success":true}}')
    raise SystemExit(0)
root = pathlib.Path(env('RHEI_ROOT'))
local = env('RHEI_TASK_ID_LOCAL')
if '--session-dir' in sys.argv:
    session_dir = pathlib.Path(sys.argv[sys.argv.index('--session-dir') + 1])
    write(session_dir/(local + '.jsonl'), '{{"session":{{"provider":"openai","model":"alpha"}}}}\n')
    write(root/('session-' + local), 'created')
write(root/('start-' + local), 'started')
marker = root/('refused-' + local)
if not marker.exists():
    write(marker, 'yes')
    if {parallel} == 2:
        await_marker(root/('start-' + ('2' if local == '1' else '1')))
    print({LIMIT_SIGNAL:?}, file=sys.stderr, flush=True)
    raise SystemExit(1)
result('## Result\n\nDone.\n')
"#
                ),
            );
            let settings = fixture.root.join(".agent-grounds/rhei/settings.json");
            let mut profile: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
            profile["agents"]["codex"]["session"] = serde_json::json!({
                "resume": {"flag": "--resume"}, "session_dir_flag": "--session-dir",
                "layout": {"kind": "FlatById", "ext": "jsonl"}
            });
            fs::write(settings, profile.to_string()).unwrap();
            let machine = SIMPLE_MACHINE
                .replace("    initial: true", "    initial: true\n    concurrent: true")
                .replace("    attempts: 1", "    attempts: 1\n    snapshot:\n      emit:\n        name: provider-control\n        on: always")
                + &format!("    on_leave: 'cli:{} callback'\n", fixture_command_line(&fixture.agent));
            fs::write(&fixture.machine, machine).unwrap();
            let parallel_arg = parallel.to_string();
            let mut args = vec!["--parallel", &parallel_arg];
            if continue_on_error {
                args.push("--continue-on-error");
            }
            let mut run = fixture.start(&args);
            fixture.parked(&mut run);
            wait_for("all refusals to release their slots", || {
                fixture.events().iter().filter(|e| e["outcome"] == "provider_limited").count()
                    == parallel
            });
            assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
            assert!(!fixture.dir.join("callbacks").exists());
            assert_eq!(snapshots(&fixture), serde_json::json!([]));
            assert!(fs::read_to_string(fixture.root.join("runtime/state-transitions.log"))
                .unwrap_or_default()
                .trim()
                .is_empty());
            for number in 1..=parallel {
                assert!(fixture.root.join(format!("session-{number}")).exists());
            }
            expire_provider_deadlines(&fixture.root);
            fixture.finish_success(&mut run);
            assert_eq!(
                fs::read_to_string(fixture.dir.join("callbacks")).unwrap().lines().count(),
                parallel
            );
            assert!(
                snapshots(&fixture)
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|s| s["snapshot_name"] == "provider-control"),
                "successful control must actually emit"
            );
        }
    }
}

/// A program printing the exact supported signal keeps its authored exit-code
/// route, regardless of the agent failure flag. §FS-rhei-run.3.3
#[test]
fn provider_limit_signal_from_program_uses_exit_route() {
    for continue_on_error in [false, true] {
        let fixture = ProviderFixture::new(
            "provider-program-control",
            SINGLE_TASK,
            SIMPLE_MACHINE,
            &format!(
                r#"result('## Result\n\nProgram routed.\n')
print({LIMIT_SIGNAL:?}, file=sys.stderr, flush=True)
raise SystemExit(1)
"#
            ),
        );
        fs::write(
            &fixture.machine,
            SIMPLE_MACHINE
                .replace(
                    "    target: codex:openai:alpha",
                    &format!("    program:\n      command: {}", fixture_command(&fixture.agent)),
                )
                .replace("    to: completed", "    to: completed\n    exit_code: 1"),
        )
        .unwrap();
        let args = if continue_on_error { vec!["--continue-on-error"] } else { vec![] };
        let mut run = fixture.start(&args);
        fixture.finish_success(&mut run);
        assert!(!markdown_text(&fixture.root).contains("providerLimits:"));
        assert!(!fixture.events().iter().any(|e| e["outcome"] == "provider_limited"));
        let records = fixture.records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["kind"], "program");
        assert_eq!(records[0]["code"], 1);
    }
}

fn precedence_fixture(timeout: bool) -> ProviderFixture {
    let machine = if timeout {
        SIMPLE_MACHINE.replace("    attempts: 1", "    attempts: 1\n    agent_timeout: 1s")
    } else {
        SIMPLE_MACHINE.to_string()
    };
    ProviderFixture::new(
        "provider-precedence",
        SINGLE_TASK,
        &machine,
        &format!("print({LIMIT_SIGNAL:?}, file=sys.stderr, flush=True)\ntime.sleep(9)\n"),
    )
}

fn assert_precedence(fixture: &ProviderFixture, ending: &str, charged: bool) {
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
    assert!(!markdown_text(&fixture.root).contains("providerLimits:"));
    assert!(!fixture.events().iter().any(|e| e["outcome"] == "provider_limited"));
    let records = fixture.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["ending"], ending);
    assert_eq!(records[0]["attempt_charged"], charged);
    assert_eq!(records[0]["charged"], u64::from(charged));
    let log = PathBuf::from(records[0]["log"].as_str().unwrap());
    let log = if log.is_absolute() { log } else { fixture.root.join(log) };
    assert!(fs::read_to_string(log).unwrap().contains(ending));
}

/// Timeout wins over captured provider text and still charges the attempt.
/// §FS-rhei-agents.5.2.1 §FS-rhei-run.3.3
#[test]
fn provider_limit_signal_does_not_override_timeout() {
    for continue_on_error in [false, true] {
        let fixture = precedence_fixture(true);
        let args = if continue_on_error { vec!["--continue-on-error"] } else { vec![] };
        let mut run = fixture.start(&args);
        assert_eq!(fixture.finish(&mut run).code(), Some(1), "{}", fixture.output());
        assert_precedence(&fixture, "timed out", true);
    }
}

/// A real foreground signal beats already-captured provider text. Like the
/// repository's other process-signal tests this uses Unix SIGINT.
/// §FS-rhei-run.3.2 §FS-rhei-run.3.3
#[cfg(unix)]
#[test]
fn provider_limit_signal_does_not_override_interruption() {
    for continue_on_error in [false, true] {
        let fixture = precedence_fixture(false);
        // Synchronize on explicitly enabled output events. §FS-rhei-run-json.2.3
        let mut args = vec!["--json-agent-output"];
        if continue_on_error {
            args.push("--continue-on-error");
        }
        let mut run = fixture.start(&args);
        wait_for("captured provider text before interrupt", || {
            fixture
                .events()
                .iter()
                .any(|e| e["event"] == "agent_output" && e["line"] == LIMIT_SIGNAL)
        });
        assert!(std::process::Command::new("kill")
            .args(["-INT", &run.child().id().to_string()])
            .status()
            .unwrap()
            .success());
        assert_eq!(fixture.finish(&mut run).code(), Some(130), "{}", fixture.output());
        assert_precedence(&fixture, "interrupted", false);
    }
}
