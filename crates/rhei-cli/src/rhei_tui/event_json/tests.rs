//! Round-trip and shape tests for the record contract. §FS-rhei-run-json.2

use super::*;
use crate::rhei_tui::event::{RunSummary, Slot};
use crate::rhei_tui::run_stop::{IdleBlocker, RunStop, StopReason};
use std::time::Duration;

fn at() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_755_864_202)
}

fn assigned(slot: Slot) -> RunEvent {
    RunEvent::SlotAssigned {
        slot,
        task: "auth.1".to_string(),
        from: "pending".to_string(),
        to: "implement".to_string(),
        agent: Some("claude-code".to_string()),
        template_context: None,
        log_path: PathBuf::from("runtime/logs/auth.1-implement.log"),
        started_at: Instant::now(),
        wall_clock: at(),
    }
}

/// Every variant must encode: the payload match is exhaustive, so this test is
/// what makes a *new* variant fail loudly rather than encode as `{}`.
fn every_variant() -> Vec<RunEvent> {
    vec![
        RunEvent::RunStarted {
            run_id: "3f9a2c".to_string(),
            workspace: PathBuf::from("/w"),
            parallel: 2,
            total_tasks: 7,
        },
        RunEvent::PassStarted { pass: 1, ready: vec!["auth.1".to_string()] },
        assigned(0),
        RunEvent::SlotReleased {
            slot: 0,
            task: "auth.1".to_string(),
            from: "pending".to_string(),
            to: "review".to_string(),
            log_path: PathBuf::from("runtime/logs/auth.1-implement.log"),
            outcome: TaskOutcome::Completed,
            finished_at: Instant::now(),
            wall_clock: at(),
            exit_code: Some(0),
            duration_ms: 3_490,
        },
        RunEvent::PassEnded { pass: 1, progressed: true },
        RunEvent::TasksDeferred { pass: 1, tasks: vec!["auth.2".to_string()] },
        RunEvent::TaskOutputsMissing {
            task: "auth.1".to_string(),
            state: "implement".to_string(),
            entries: vec!["result (runtime/results/auth.1.md)".to_string()],
        },
        RunEvent::Message { level: MessageLevel::Warn, text: "heads up".to_string() },
        RunEvent::RunLink { label: "Dashboard".to_string(), url: "http://127.0.0.1:1".to_string() },
        RunEvent::AgentOutput {
            slot: 0,
            task: "auth.1".to_string(),
            stream: AgentStream::Stderr,
            line: "building".to_string(),
            wall_clock: at(),
        },
        RunEvent::RunFinished {
            summary: RunSummary { total_tasks: 7, terminal_tasks: 7, ..RunSummary::default() },
        },
    ]
}

#[test]
fn every_variant_encodes_with_a_distinct_kind() {
    let mut kinds = Vec::new();
    for (index, event) in every_variant().iter().enumerate() {
        let record = encode(Some(index as u64 + 1), event, at(), None);
        let kind = record["event"].as_str().expect("event discriminator").to_string();
        assert_eq!(record["seq"], index as u64 + 1);
        assert_eq!(record["ts"], "2025-08-22T12:03:22Z");
        assert!(!kinds.contains(&kind), "duplicate record kind {kind}");
        kinds.push(kind);
    }
    assert_eq!(kinds.len(), 11, "a RunEvent variant is missing from the contract test");
}

#[test]
fn every_variant_round_trips() {
    for (index, event) in every_variant().iter().enumerate() {
        let line = encode(Some(index as u64 + 1), event, at(), None).to_string();
        let decoded = decode(&line).unwrap_or_else(|| panic!("decode failed for {line}"));
        assert_eq!(decoded.seq, Some(index as u64 + 1));
        assert_eq!(decoded.ts, at(), "a replay re-emits the instant the run recorded");
        // Re-encoding the decoded event must produce the same record: that is
        // what "the attach client sees what the run emitted" means.
        let again = encode(decoded.seq, &decoded.event, at(), None).to_string();
        assert_eq!(line, again);
    }
}

#[test]
fn run_started_carries_the_schema_version() {
    let record = encode(Some(1), &every_variant()[0], at(), None);
    assert_eq!(record["schema"], SCHEMA_VERSION);
    // No other record carries it: a consumer pins once, at the head.
    assert!(encode(Some(2), &every_variant()[1], at(), None).get("schema").is_none());
}

#[test]
fn slot_assigned_names_the_log_and_the_agent() {
    let record = encode(Some(3), &assigned(2), at(), None);
    assert_eq!(record["slot"], 2);
    assert_eq!(record["agent"], "claude-code");
    assert_eq!(record["log_path"], "runtime/logs/auth.1-implement.log");
}

#[test]
fn a_program_slot_reports_a_null_agent_rather_than_omitting_it() {
    let mut event = assigned(0);
    if let RunEvent::SlotAssigned { agent, .. } = &mut event {
        *agent = None;
    }
    let record = encode(Some(1), &event, at(), None);
    assert!(record["agent"].is_null(), "agent must be present-and-null, not absent");
}

#[test]
fn a_failed_outcome_carries_its_reason() {
    let event = RunEvent::SlotReleased {
        slot: 0,
        task: "auth.1".to_string(),
        from: "a".to_string(),
        to: "a".to_string(),
        log_path: PathBuf::from("l"),
        outcome: TaskOutcome::Failed("exit 2".to_string()),
        finished_at: Instant::now(),
        wall_clock: at(),
        exit_code: Some(2),
        duration_ms: 1,
    };
    let record = encode(Some(1), &event, at(), None);
    assert_eq!(record["outcome"], "failed");
    assert_eq!(record["reason"], "exit 2");
    let decoded = decode(&record.to_string()).expect("decode");
    match decoded.event {
        RunEvent::SlotReleased { outcome: TaskOutcome::Failed(reason), .. } => {
            assert_eq!(reason, "exit 2");
        }
        other => panic!("expected a failed release, got {other:?}"),
    }
}

/// Startup validation warnings use the ordinary message vocabulary, so JSON
/// can preserve the exact text without contaminating its record stream.
// §FS-rhei-run-json.1 §FS-rhei-run-tui.1.1
#[test]
fn a_consumes_advisory_encodes_as_an_exact_warning_message() {
    let text = "warning: **Consumes:** declares export data-flow for prompt injection, not filesystem visibility. Workers can read undeclared sibling exports under runtime/exports/. For a blind round, schedule participants concurrently and brief them not to inspect sibling exports; neither measure enforces blindness once an export exists.";
    let record = encode(
        Some(2),
        &RunEvent::Message { level: MessageLevel::Warn, text: text.to_string() },
        at(),
        None,
    );

    assert_eq!(record["event"], "message");
    assert_eq!(record["level"], "warn");
    assert_eq!(record["text"], text);
}

/// The value this vocabulary gained. A wait must survive the stream in both
/// directions, or `rhei attach --json` shows an attached reader a completion
/// where the run itself wrote a wait.
// §FS-rhei-run-json.2.1 §FS-rhei-states.2.2
#[test]
fn a_waiting_outcome_round_trips_and_carries_no_reason() {
    let event = RunEvent::SlotReleased {
        slot: 0,
        task: "auth.1".to_string(),
        from: "ci-wait".to_string(),
        to: "ci-wait".to_string(),
        log_path: PathBuf::from("l"),
        outcome: TaskOutcome::Waiting,
        finished_at: Instant::now(),
        wall_clock: at(),
        exit_code: Some(75),
        duration_ms: 1,
    };
    let record = encode(Some(1), &event, at(), None);
    assert_eq!(record["outcome"], "waiting");
    assert_eq!(record["exit_code"], 75, "the exit that matched the self-loop stays on the record");
    assert!(record["reason"].is_null(), "a wait's cause is its exit code, not a reason string");
    let decoded = decode(&record.to_string()).expect("decode");
    match decoded.event {
        RunEvent::SlotReleased { outcome, .. } => assert_eq!(outcome, TaskOutcome::Waiting),
        other => panic!("expected a released slot, got {other:?}"),
    }
}

/// Provider parking is an additive typed release with enough information for
/// headless and attached clients to schedule and explain it. §FS-rhei-run-json.2.1
#[test]
fn a_provider_limited_outcome_round_trips_with_its_deadline() {
    let event = RunEvent::SlotReleased {
        slot: 0,
        task: "auth.1".to_string(),
        from: "working".to_string(),
        to: "working".to_string(),
        log_path: PathBuf::from("l"),
        outcome: TaskOutcome::ProviderLimited {
            provider: "openai".to_string(),
            next_attempt_at: "2026-09-16T20:21:00Z".to_string(),
        },
        finished_at: Instant::now(),
        wall_clock: at(),
        exit_code: Some(1),
        duration_ms: 1,
    };
    let record = encode(Some(1), &event, at(), None);
    assert_eq!(record["outcome"], "provider_limited");
    assert_eq!(record["provider"], "openai");
    assert_eq!(record["next_attempt_at"], "2026-09-16T20:21:00Z");
    let decoded = decode(&record.to_string()).expect("decode");
    match decoded.event {
        RunEvent::SlotReleased { outcome, .. } => assert_eq!(
            outcome,
            TaskOutcome::ProviderLimited {
                provider: "openai".to_string(),
                next_attempt_at: "2026-09-16T20:21:00Z".to_string(),
            }
        ),
        other => panic!("expected a released slot, got {other:?}"),
    }
}

/// The rule that lets the vocabulary grow without moving `schema`: a reader
/// that meets a word it does not know passes the record on wearing it, rather
/// than rewriting the field to one it does. `rhei attach --json` decodes and
/// re-encodes every record, so a rewrite here would reach the wire — an older
/// rhei attached to a newer run would report its waits as completions, which is
/// the misreport `waiting` was added to remove.
// §FS-rhei-run-json.2.2
#[test]
fn an_outcome_this_build_does_not_know_survives_the_round_trip_unchanged() {
    let record = json!({
        "seq": 1,
        "ts": "2025-08-22T12:03:22Z",
        "event": "slot_released",
        "slot": 0,
        "task": "auth.1",
        "from": "parking",
        "to": "parking",
        "log_path": "runtime/logs/auth.1-parking.log",
        "outcome": "parked",
        "exit_code": 75,
        "duration_ms": 1,
    });
    let decoded = decode(&record.to_string()).expect("decode");
    match &decoded.event {
        RunEvent::SlotReleased { outcome, .. } => {
            assert_eq!(*outcome, TaskOutcome::Unrecognized("parked".to_string()));
        }
        other => panic!("expected a released slot, got {other:?}"),
    }

    let re_encoded = encode(decoded.seq, &decoded.event, at(), None);
    assert_eq!(re_encoded["outcome"], "parked", "the word the run wrote is the word passed on");
    assert_eq!(re_encoded["exit_code"], 75, "every other field of the record is untouched");
    assert_eq!(re_encoded["task"], "auth.1");
    assert_eq!(re_encoded["log_path"], "runtime/logs/auth.1-parking.log");
}

#[test]
fn agent_output_is_the_only_non_structural_event() {
    for event in every_variant() {
        let structural = is_structural(&event);
        assert_eq!(structural, !matches!(event, RunEvent::AgentOutput { .. }));
    }
}

#[test]
fn unknown_and_torn_lines_are_skipped_not_fatal() {
    // A newer rhei's record kind, a half-written final line, and blank input.
    assert!(decode(r#"{"seq":1,"ts":"2025-08-22T12:03:22Z","event":"future_thing"}"#).is_none());
    assert!(decode(r#"{"seq":1,"ts":"2025-08-22T12:0"#).is_none());
    assert!(decode("").is_none());
    assert!(decode(r#"{"seq":"one","ts":"2025-08-22T12:03:22Z","event":"pass_ended"}"#).is_none());
}

#[test]
fn timestamps_round_trip_through_the_wire_format() {
    for secs in [0u64, 1_000_000_000, 1_755_864_202, 4_102_444_800] {
        let stamp = UNIX_EPOCH + Duration::from_secs(secs);
        let text = format_rfc3339(stamp);
        assert_eq!(parse_rfc3339(&text), Some(stamp), "{text} did not round-trip");
    }
    assert_eq!(parse_rfc3339("2025-08-22T12:03:22"), None, "a missing Z must not parse");
    assert_eq!(parse_rfc3339("not a timestamp"), None);
}

#[test]
fn an_event_with_its_own_wall_clock_is_stamped_from_it() {
    assert_eq!(event_wall_clock(&assigned(0)), Some(at()));
    assert_eq!(event_wall_clock(&RunEvent::PassEnded { pass: 1, progressed: true }), None);
}

/// `agent_output` is the one record with no cursor: numbering it would make
/// the stdout stream and `runtime/events.jsonl` disagree about every `seq`
/// after the first agent line. §FS-rhei-run-json.2 §FS-rhei-run-json.2.3
#[test]
fn a_record_with_no_sequence_number_still_decodes() {
    let output = RunEvent::AgentOutput {
        slot: 1,
        task: "auth.1".to_string(),
        stream: AgentStream::Stdout,
        line: "building".to_string(),
        wall_clock: at(),
    };
    let record = encode(None, &output, at(), None);
    assert!(record.get("seq").is_none(), "agent_output carries no cursor");
    let decoded = decode(&record.to_string()).expect("decode");
    assert_eq!(decoded.seq, None);
    assert_eq!(decoded.ts, at());
}

/// The `stop` payload survives a round trip, and adding it moves nothing.
///
/// Two claims in one test because they are two halves of the same promise:
/// `run_finished.summary` gains an object whose every field is present — with
/// null where it does not apply — and `schema` stays `1` because an addition
/// is an addition. The unselected half is the other half of it: a run that did
/// not ask for the option emits no `stop` key at all, not a null one, so its
/// stream is byte-identical to the one this build's predecessor wrote.
/// §FS-rhei-run-json.2.1 §FS-rhei-run-json.2.2
#[test]
fn the_stop_payload_round_trips_without_moving_the_schema() {
    let stop = RunStop {
        reason: StopReason::Idle,
        idle_blocker: Some(IdleBlocker::Poll),
        next_attempt_at: Some("2030-01-01T00:00:00Z".to_string()),
        exit_code: 3,
    };
    let finished = RunEvent::RunFinished {
        summary: RunSummary {
            total_tasks: 5,
            terminal_tasks: 2,
            stop: Some(stop.clone()),
            ..RunSummary::default()
        },
    };

    let record = encode(Some(1), &finished, at(), None);
    let payload = &record["summary"]["stop"];
    assert_eq!(payload["reason"], "idle");
    assert_eq!(payload["idle_blocker"], "poll");
    assert_eq!(payload["next_attempt_at"], "2030-01-01T00:00:00Z");
    assert_eq!(payload["exit_code"], 3);

    let decoded = decode(&record.to_string()).expect("decode");
    let RunEvent::RunFinished { summary } = decoded.event else {
        panic!("run_finished decodes as itself");
    };
    assert_eq!(summary.stop, Some(stop), "every field survives the round trip");

    let started = encode(Some(0), &every_variant()[0], at(), None);
    assert_eq!(started["schema"], 1, "an additive payload does not move `schema`");

    let unselected = encode(
        Some(2),
        &RunEvent::RunFinished { summary: RunSummary { total_tasks: 5, ..RunSummary::default() } },
        at(),
        None,
    );
    assert!(
        unselected["summary"].get("stop").is_none(),
        "an unselected run's stream carries no stop key at all; got:\n{}",
        unselected["summary"]
    );
}
