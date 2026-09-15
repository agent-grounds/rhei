//! §FS-rhei-run-json.3: the durable log and its follower.

use super::*;
use crate::rhei_tui::event::{AgentStream, MessageLevel};

fn message(text: &str) -> RunEvent {
    RunEvent::Message { level: MessageLevel::Info, text: text.to_string() }
}

#[test]
fn records_are_sequenced_from_one_and_flushed_per_line() {
    let tmp = tempfile::tempdir().unwrap();
    let sink = EventLogSink::create(tmp.path()).unwrap();
    sink.emit(message("first"));
    // Read *while the sink is still open*: a follower must not have to wait
    // for the run to end.
    let contents = fs::read_to_string(sink.path()).unwrap();
    assert_eq!(contents.lines().count(), 1);
    assert!(contents.contains(r#""seq":1"#));

    sink.emit(message("second"));
    let contents = fs::read_to_string(sink.path()).unwrap();
    let seqs: Vec<Option<u64>> =
        contents.lines().filter_map(event_json::decode).map(|r| r.seq).collect();
    assert_eq!(seqs, vec![Some(1), Some(2)]);
    assert_eq!(sink.last_seq(), 2);
}

/// Initial validation advisories are structural messages and therefore remain
/// available to dashboards and attach clients in the durable event log.
// §FS-rhei-run-tui.1.1 §FS-rhei-run-json.3
#[test]
fn the_consumes_advisory_is_retained_in_the_durable_event_log() {
    let tmp = tempfile::tempdir().unwrap();
    let sink = EventLogSink::create(tmp.path()).unwrap();
    let text = "warning: **Consumes:** declares export data-flow for prompt injection, not filesystem visibility. Workers can read undeclared sibling exports under runtime/exports/. For a blind round, schedule participants concurrently and brief them not to inspect sibling exports; neither measure enforces blindness once an export exists.";
    sink.emit(RunEvent::Message { level: MessageLevel::Warn, text: text.to_string() });

    let contents = fs::read_to_string(sink.path()).unwrap();
    let record = event_json::decode(contents.trim()).expect("warning record");
    assert!(matches!(
        record.event,
        RunEvent::Message { level: MessageLevel::Warn, text: actual } if actual == text
    ));
}

#[test]
fn agent_output_stays_out_of_the_durable_log() {
    let tmp = tempfile::tempdir().unwrap();
    let sink = EventLogSink::create(tmp.path()).unwrap();
    sink.emit(RunEvent::AgentOutput {
        slot: 0,
        task: "auth.1".to_string(),
        stream: AgentStream::Stdout,
        line: "noise".to_string(),
        wall_clock: SystemTime::now(),
    });
    sink.emit(message("kept"));
    let contents = fs::read_to_string(sink.path()).unwrap();
    assert_eq!(contents.lines().count(), 1, "only the structural event belongs here");
    // The one record present is still seq 1: a skipped event must not burn a
    // sequence number, or a consumer sees a gap it cannot explain.
    assert!(contents.contains(r#""seq":1"#));
}

#[test]
fn a_new_run_truncates_the_previous_runs_log() {
    let tmp = tempfile::tempdir().unwrap();
    {
        let sink = EventLogSink::create(tmp.path()).unwrap();
        sink.emit(message("old run"));
    }
    let sink = EventLogSink::create(tmp.path()).unwrap();
    sink.emit(message("new run"));
    let contents = fs::read_to_string(sink.path()).unwrap();
    assert_eq!(contents.lines().count(), 1);
    assert!(contents.contains("new run"));
}

#[test]
fn the_reader_follows_appends_and_returns_only_new_records() {
    let tmp = tempfile::tempdir().unwrap();
    let sink = EventLogSink::create(tmp.path()).unwrap();
    let mut reader = EventLogReader::open(sink.path());

    sink.emit(message("one"));
    let first = reader.poll();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].seq, Some(1), "records carry the sequence they were written under");
    assert_eq!(reader.last_seq(), 1);

    // Nothing new: a poll on a quiet run must return nothing, not a replay.
    assert!(reader.poll().is_empty());

    sink.emit(message("two"));
    sink.emit(message("three"));
    let rest = reader.poll();
    assert_eq!(rest.len(), 2);
    assert_eq!(reader.last_seq(), 3);
}

#[test]
fn a_torn_final_line_is_re_read_whole_on_the_next_poll() {
    let tmp = tempfile::tempdir().unwrap();
    let path = event_log_path(tmp.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let whole =
        format!("{}\n", event_json::encode(Some(1), &message("complete"), SystemTime::now(), None));
    let fragment =
        event_json::encode(Some(2), &message("half written"), SystemTime::now(), None).to_string();
    let torn = &fragment[..fragment.len() / 2];
    fs::write(&path, format!("{whole}{torn}")).unwrap();

    let mut reader = EventLogReader::open(&path);
    assert_eq!(reader.poll().len(), 1, "the torn line must not be decoded");
    assert_eq!(reader.last_seq(), 1);

    // The run finishes writing it.
    fs::write(&path, format!("{whole}{fragment}\n")).unwrap();
    let events = reader.poll();
    assert_eq!(events.len(), 1, "the completed line arrives whole");
    assert_eq!(reader.last_seq(), 2);
}

#[test]
fn a_missing_log_polls_empty_rather_than_failing() {
    let tmp = tempfile::tempdir().unwrap();
    let mut reader = EventLogReader::open(tmp.path().join("runtime/events.jsonl"));
    assert!(reader.poll().is_empty());
}

#[test]
fn a_truncating_new_run_resets_a_live_reader() {
    let tmp = tempfile::tempdir().unwrap();
    let sink = EventLogSink::create(tmp.path()).unwrap();
    let mut reader = EventLogReader::open(sink.path());
    sink.emit(message("one"));
    sink.emit(message("two"));
    assert_eq!(reader.poll().len(), 2);

    drop(sink);
    let sink = EventLogSink::create(tmp.path()).unwrap();
    sink.emit(message("fresh"));
    let events = reader.poll();
    assert_eq!(events.len(), 1, "the reader must restart, not read past the new end");
    assert_eq!(reader.last_seq(), 1);
}
