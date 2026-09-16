//! Provider waits compose with authored polling without spending poll attempts.
//! §FS-rhei-run.3.3 §FS-rhei-run.5.1

use super::provider_limit_support::*;
use super::*;

const POLL_MACHINE: &str = r#"name: provider-poll-controls
version: 1
states:
  working:
    initial: true
    concurrent: true
    target: codex:openai:alpha
    attempts: 3
    poll:
      interval: 1s
      max_attempts: 2
  completed:
    final: true
transitions:
  - from: working
    to: working
    condition: pollAttempts < pollMaxAttempts
  - from: working
    to: completed
    condition: pollAttempts >= pollMaxAttempts
"#;

/// R1-09: a refusal is uncharged and leaves poll counters untouched; the next
/// two ordinary invocations still self-loop once and take the exhaustion edge.
/// §FS-rhei-run.5.1 §FS-rhei-run.3.3
#[test]
fn provider_limit_refusal_preserves_poll_accounting_and_exhaustion() {
    for continue_on_error in [false, true] {
        let fixture = ProviderFixture::new(
            "provider-poll-accounting",
            SINGLE_TASK,
            POLL_MACHINE,
            &format!(
                r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root/'starts', env('RHEI_ATTEMPT') + '\n')
marker = root/'refused'
if not marker.exists():
    write(marker, 'yes')
    print({LIMIT_SIGNAL:?}, file=sys.stderr, flush=True)
    raise SystemExit(1)
result('## Result\n\nPoll done.\n')
"#
            ),
        );
        let args = if continue_on_error { vec!["--continue-on-error"] } else { vec![] };
        let mut run = fixture.start(&args);
        fixture.parked(&mut run);
        let parked = metadata(&fixture.root);
        let task = &parked["metadata"]["tasks"]["1"];
        assert!(task["pollNextAttemptAt"].is_null());
        assert!(task["stateVisits"].is_null());
        let records = fixture.records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["attempt_charged"], false);
        assert_eq!(records[0]["charged"], 0);
        expire_provider_deadlines(&fixture.root);
        fixture.finish_success(&mut run);
        let events = fixture.events();
        assert_eq!(events.iter().filter(|e| e["event"] == "slot_released").count(), 3);
        assert_eq!(events.iter().filter(|e| e["outcome"] == "provider_limited").count(), 1);
        assert_eq!(events.iter().filter(|e| e["outcome"] == "waiting").count(), 1);
        let final_metadata = metadata(&fixture.root);
        let final_task = &final_metadata["metadata"]["tasks"]["1"];
        assert!(final_task["providerLimits"].is_null());
        assert!(final_task["pollNextAttemptAt"]["working"].is_null());
        assert!(final_task["stateVisits"]["working"].is_null());
        let ledger =
            fs::read_to_string(fixture.root.join("runtime/state-transitions.log")).unwrap();
        assert_eq!(ledger.matches("working@working").count(), 1);
        assert_eq!(ledger.matches("working@completed").count(), 1);
    }
}

/// R1-09: task 1's poll is later than the shared provider deadline; task 2's
/// poll is earlier. The run selects task 2 at the provider boundary, then task
/// 1 only after its separate poll boundary. Both are at their exhaustion visit.
/// §FS-rhei-run.5.1 §FS-rhei-run.3.3
#[test]
fn provider_limit_poll_orderings_choose_earliest_effective_task() {
    let fixture = ProviderFixture::new(
        "provider-poll-deadlines",
        &[
            ("01.md", "### Task 1: Later poll\n**State:** working\n"),
            ("02.md", "### Task 2: Later provider\n**State:** working\n"),
        ],
        POLL_MACHINE,
        r#"root = pathlib.Path(env('RHEI_ROOT'))
local = env('RHEI_TASK_ID_LOCAL')
append(root/'start-order', local + '\n')
write(root/('started-' + local), str(int(time.time())))
result('## Result\n\nPoll done.\n')
"#,
    );
    // Seed a persisted poll visit and shared wait far enough ahead that CLI
    // setup cannot spend the deadlines. Short clock inputs are installed live.
    edit_metadata(&fixture.root, |m| {
        m["metadata"] = serde_json::json!({"tasks": {
            "1": {"providerLimits": {"working": limit_record(epoch_now() + 1200)},
                  "pollNextAttemptAt": {"working": epoch_now() + 1800},
                  "stateVisits": {"working": 2}},
            "2": {"providerLimits": {"working": limit_record(epoch_now() + 1200)},
                  "pollNextAttemptAt": {"working": epoch_now() + 600},
                  "stateVisits": {"working": 2}}
        }});
    });
    let mut run = fixture.start(&["--parallel", "2"]);
    assert_stays_parked(&fixture, &mut run);
    assert!(!fixture.root.join("start-order").exists());
    let provider_at = epoch_now() + 4;
    let earlier_poll = provider_at - 2;
    edit_metadata(&fixture.root, |m| {
        for task in ["1", "2"] {
            m["metadata"]["tasks"][task]["providerLimits"]["working"]["nextAttemptAt"] =
                utc_at(provider_at).into();
        }
        m["metadata"]["tasks"]["2"]["pollNextAttemptAt"]["working"] = earlier_poll.into();
    });
    wait_for("earlier poll boundary", || epoch_now() >= earlier_poll);
    assert!(
        !fixture.root.join("start-order").exists(),
        "provider deadline still blocks both tasks"
    );
    wait_for("task 2's provider eligibility", || {
        fixture.events().iter().any(|e| e["event"] == "slot_released" && e["task"] == "workspace.2")
    });
    assert_eq!(fs::read_to_string(fixture.root.join("start-order")).unwrap(), "2\n");
    let started_two: u64 =
        fs::read_to_string(fixture.root.join("started-2")).unwrap().parse().unwrap();
    assert!(started_two >= provider_at);
    assert!(!fixture.root.join("started-1").exists(), "task 1's later poll still blocks it");
    assert!(run.child().try_wait().unwrap().is_none());
    let later_poll = epoch_now() + 2;
    edit_metadata(&fixture.root, |m| {
        m["metadata"]["tasks"]["1"]["pollNextAttemptAt"]["working"] = later_poll.into();
    });
    fixture.finish_success(&mut run);
    assert_eq!(fs::read_to_string(fixture.root.join("start-order")).unwrap(), "2\n1\n");
    let started_one: u64 =
        fs::read_to_string(fixture.root.join("started-1")).unwrap().parse().unwrap();
    assert!(started_one >= later_poll);
    let events = fixture.events();
    assert_eq!(events.iter().filter(|e| e["event"] == "slot_released").count(), 2);
    assert!(
        !events.iter().any(|e| e["outcome"] == "waiting"),
        "both seeded visits exhaust instead of self-looping"
    );
    assert!(!markdown_text(&fixture.root).contains("providerLimits:"));
}
