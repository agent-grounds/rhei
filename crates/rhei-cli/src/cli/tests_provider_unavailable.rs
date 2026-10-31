mod provider_unavailable {
    use super::*;
    use serde_json::json;

    const SIGNAL: &str = "fetch failed";

    fn status(code: i32) -> std::process::ExitStatus {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(code << 8)
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(code as u32)
        }
    }

    fn resolved(target: &str) -> ResolvedAgent {
        let settings = RheiSettings { agents: built_in_agents(), ..RheiSettings::default() };
        resolve_target_agent(target, None, &settings).expect("built-in target")
    }

    fn pi() -> ResolvedAgent {
        resolved("pi:openai-codex:gpt-6.1-sol")
    }

    fn observed() -> std::time::SystemTime {
        std::time::UNIX_EPOCH + Duration::from_millis(1_789_579_200_500)
    }

    /// An assistant `message_end`, with pi's `errorMessage` when it carries one.
    fn turn(stop_reason: &str, error: Option<&str>) -> serde_json::Value {
        let mut message = json!({"role": "assistant", "stopReason": stop_reason});
        if let Some(error) = error {
            message["errorMessage"] = error.into();
        }
        json!({"type": "message_end", "message": message})
    }

    fn failed(error: &str) -> serde_json::Value {
        turn("error", Some(error))
    }

    fn start(attempt: u64) -> serde_json::Value {
        json!({"type": "auto_retry_start", "attempt": attempt, "maxAttempts": 3})
    }

    fn end(success: bool, attempt: u64, final_error: Option<&str>) -> serde_json::Value {
        let mut event = json!({"type": "auto_retry_end", "success": success, "attempt": attempt});
        if let Some(error) = final_error {
            event["finalError"] = error.into();
        }
        event
    }

    /// Every retry pi allows spent on `last`, the error of the final turn.
    fn gave_up(first: &str, last: &str) -> Vec<serde_json::Value> {
        let mut events = Vec::new();
        for attempt in 1..=3 {
            events.extend([failed(first), start(attempt)]);
        }
        events.extend([failed(last), end(false, 3, Some(last))]);
        events
    }

    fn stdout(events: &[serde_json::Value]) -> Vec<(rhei_tui::AgentStream, String)> {
        events.iter().map(|event| (rhei_tui::AgentStream::Stdout, event.to_string())).collect()
    }

    fn classify(
        resolved: &ResolvedAgent,
        lines: &[(rhei_tui::AgentStream, String)],
    ) -> Option<ProviderUnavailable> {
        classify_provider_unavailable(resolved, status(0), false, false, lines, observed())
    }

    /// The incident, and the two edges that reach pi's count without being
    /// transient: a non-retried error on the last retry, and an abort during the
    /// last backoff. All three are pi's verdict. §FS-rhei-agents.2.5
    #[test]
    fn pi_giving_up_its_retries_is_recognized_with_its_final_error() {
        let incident = classify(&pi(), &stdout(&gave_up(SIGNAL, SIGNAL))).expect("the incident");
        assert_eq!(incident.signal, SIGNAL);
        assert!(!incident.answered, "the provider never answered");
        assert_eq!(
            incident.identity,
            ProviderIdentity { agent: "pi".to_string(), provider: "openai-codex".to_string() }
        );

        // The spelling that keys the wait and keeps pi's own model reference.
        let spelled = resolved("pi:openai-codex:openai-codex/gpt-6.1-sol");
        let spelled = classify(&spelled, &stdout(&gave_up(SIGNAL, SIGNAL))).expect("recognized");
        assert_eq!(spelled.identity.provider, "openai-codex");

        let last_retry = classify(&pi(), &stdout(&gave_up("503", "400 bad request")));
        assert_eq!(last_retry.expect("non-retried on the last retry").signal, "400 bad request");

        let mut cancelled = gave_up(SIGNAL, SIGNAL);
        cancelled.truncate(cancelled.len() - 2);
        cancelled.push(end(false, 3, Some("Retry cancelled")));
        let cancelled = classify(&pi(), &stdout(&cancelled)).expect("an abort in the last backoff");
        assert_eq!(cancelled.signal, "Retry cancelled");
    }

    /// A session one assistant turn of which ended without error was answered:
    /// still recognized, and marked so the ending is charged. §FS-rhei-agents.3.2.3
    #[test]
    fn a_session_the_provider_answered_is_recognized_as_answered() {
        let mut events = vec![turn("toolUse", None)];
        events.extend(gave_up(SIGNAL, SIGNAL));
        assert!(classify(&pi(), &stdout(&events)).expect("still recognized").answered);
    }

    /// Without `finalError` the signal is the last turn's error, and without
    /// either it says there was none. §FS-rhei-agents.8.4
    #[test]
    fn the_signal_falls_back_to_the_last_turns_error() {
        let only_try = json!({"type": "auto_retry_start", "attempt": 1, "maxAttempts": 1});
        let signal = |last: serde_json::Value| {
            let events = [failed("503"), only_try.clone(), last, end(false, 1, None)];
            classify(&pi(), &stdout(&events)).map(|verdict| verdict.signal)
        };
        assert_eq!(signal(failed("socket hang up")).as_deref(), Some("socket hang up"));
        assert_eq!(signal(turn("error", None)).as_deref(), Some("no error message"));
    }

    /// Every stream that is not pi's retries giving up, and every session that
    /// is not a pi exit `0` with a provider to key the wait. §FS-rhei-agents.2.5
    #[test]
    fn every_other_ending_is_not_recognized() {
        let incident = stdout(&gave_up(SIGNAL, SIGNAL));
        let not_recognized = |events: &[serde_json::Value], why: &str| {
            assert_eq!(classify(&pi(), &stdout(events)), None, "{why}");
        };
        not_recognized(&[failed("400 bad request")], "an error pi does not retry");
        not_recognized(&[failed("503"), start(1), turn("stop", None), end(true, 1, None)], "recovered");
        not_recognized(
            &[failed("503"), start(1), failed("400"), end(false, 1, Some("400"))],
            "gave up early: an error pi does not retry, below maxAttempts",
        );
        let mut answered_after = gave_up(SIGNAL, SIGNAL);
        answered_after.push(turn("stop", None));
        not_recognized(&answered_after, "an assistant turn after the end");
        not_recognized(&[failed("503"), end(false, 3, Some("503"))], "no auto_retry_start");
        not_recognized(&[start(3), turn("stop", None), end(false, 3, None)], "a last turn without error");
        not_recognized(&[], "an empty stream");

        let prose = vec![(rhei_tui::AgentStream::Stdout, SIGNAL.to_string())];
        assert_eq!(classify(&pi(), &prose), None, "prose");
        let stderr: Vec<_> =
            incident.iter().map(|(_, line)| (rhei_tui::AgentStream::Stderr, line.clone())).collect();
        assert_eq!(classify(&pi(), &stderr), None, "stderr contributes nothing");

        let codex = super::provider_limits::codex_openai();
        assert_eq!(classify(&codex, &incident), None, "another family");
        let unkeyed = resolved("pi:openai-codex/gpt-6.1-sol");
        assert_eq!(classify(&unkeyed, &incident), None, "no resolved provider");
        let at = observed();
        assert_eq!(classify_provider_unavailable(&pi(), status(1), false, false, &incident, at), None);
        assert_eq!(classify_provider_unavailable(&pi(), status(0), true, false, &incident, at), None);
        assert_eq!(classify_provider_unavailable(&pi(), status(0), false, true, &incident, at), None);
    }

    /// Endings 1 to 8 of a visit wait 1, 2, 4, 8, 16, 32, 60 and 60 minutes
    /// after the whole second they were observed in. §FS-rhei-run.3.3
    #[test]
    fn the_backoff_doubles_to_thirty_two_minutes_then_holds_an_hour() {
        let verdict = classify(&pi(), &stdout(&gave_up(SIGNAL, SIGNAL))).expect("the incident");
        for (n, minutes) in (1..=8).zip([1, 2, 4, 8, 16, 32, 60, 60]) {
            let wait = verdict.wait(n).expect("a dated wait");
            assert_eq!(wait.kind, ProviderWaitKind::Unavailable);
            assert_eq!(wait.signal, SIGNAL);
            assert_eq!(wait.observed_at, "2026-09-16T17:20:00Z");
            let at = |instant: &str| DateTime::parse_from_rfc3339(instant).unwrap().timestamp();
            let waited = at(&wait.next_attempt_at) - at(&wait.observed_at);
            assert_eq!(waited, minutes * 60, "ending {n}");
        }
    }

    /// A record an older rhei wrote has no `kind` and reads as a limit; a
    /// limit writes none, and an unavailability's survives the round trip.
    // §FS-rhei-run.3.3
    #[test]
    fn a_provider_record_without_kind_reads_as_a_limit() {
        let verdict = classify(&pi(), &stdout(&gave_up(SIGNAL, SIGNAL))).expect("the incident");
        let unavailable = verdict.wait(1).expect("a dated wait");
        let written = provider_limit_value(&unavailable);
        assert_eq!(written["kind"].as_str(), Some("unavailable"));
        assert_eq!(provider_limit_from_value(&written), Some(unavailable.clone()));

        let limit = ProviderLimit { kind: ProviderWaitKind::Limit, ..unavailable };
        let written = provider_limit_value(&limit);
        assert!(written.get("kind").is_none(), "a limit's record is unchanged: {written:?}");
        assert_eq!(provider_limit_from_value(&written).map(|read| read.kind), Some(limit.kind));
    }

    fn record(ending: &str, signal: Option<&str>) -> SpawnRecord {
        let mut raw = json!({
            "task": "plan.1", "state": "working", "moves": 0, "attempt": 1, "charged": 1,
            "attempt_charged": true, "kind": "agent", "worker": "pi", "log": "l",
            "started": "s", "ended": "e", "duration": "1s", "code": 0, "ending": ending
        });
        if let Some(signal) = signal {
            raw["signal"] = signal.into();
        }
        serde_json::from_value(raw).expect("a spawn record")
    }

    /// A record without `unavailable` reads as `0`, and `0` is not written, so
    /// every record an older rhei wrote reads as the one it was. §FS-rhei-agents.8.4
    #[test]
    fn a_spawn_record_without_unavailable_reads_as_none() {
        let old = record("exited", None);
        assert_eq!((old.unavailable, old.signal.as_deref()), (0, None));
        let written = serde_json::to_value(&old).unwrap();
        assert!(written.get("unavailable").is_none() && written.get("signal").is_none());
    }

    /// The record's `ending` decides before its code: a `provider_unavailable`
    /// record exited `0` and never reads so, and proves no successful work.
    // §FS-rhei-agents.3.2.1 §FS-rhei-agents.8.4
    #[test]
    fn an_unavailable_record_reads_as_its_signal_never_as_exited_0() {
        let ending = record(PROVIDER_UNAVAILABLE_ENDING, Some(SIGNAL));
        assert_eq!(ending.ending_sentence(), "found its provider unavailable (fetch failed)");
        assert!(!ending.exited_zero());
        assert!(!ending.proves_successful_work("plan.1", "working", 0));
        let silent = record(PROVIDER_UNAVAILABLE_ENDING, None);
        assert_eq!(silent.ending_sentence(), "found its provider unavailable (no error message)");
    }

    fn write_record(dir: &Path, record: &SpawnRecord) -> PathBuf {
        let path = dir.join("runtime/spawns/task-plan.1-working.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, serde_json::to_string_pretty(record).unwrap()).unwrap();
        path
    }

    fn verdict(answered: bool) -> ProviderUnavailable {
        ProviderUnavailable {
            identity: ProviderIdentity { agent: "pi".into(), provider: "openai-codex".into() },
            signal: SIGNAL.to_string(),
            answered,
            observed: observed(),
        }
    }

    /// The output check rewrites the record in place: a free ending gives its
    /// charge back; an answered one or the seventh keeps it; a record that
    /// cannot be read is left alone. §FS-rhei-agents.8.4 §FS-rhei-agents.3.2.3
    #[test]
    fn the_rewrite_gives_back_only_a_free_endings_charge() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_record(dir.path(), &record("exited", None));
        let free = record_provider_unavailable(&path, &verdict(false)).expect("rewritten");
        assert_eq!((free.charged, free.attempt_charged, free.unavailable), (0, false, 1));
        assert_eq!((free.ending.as_str(), free.signal.as_deref()), ("provider_unavailable", Some(SIGNAL)));
        assert_eq!(unavailable_charge(&free, &verdict(false)), "no attempt charged (free 1 of 6 on this visit)");

        let path = write_record(dir.path(), &record("exited", None));
        let answered = record_provider_unavailable(&path, &verdict(true)).expect("rewritten");
        assert_eq!((answered.charged, answered.attempt_charged, answered.unavailable), (1, true, 1));
        let charge = unavailable_charge(&answered, &verdict(true));
        assert_eq!(charge, "attempt charged, because the provider answered this session");

        let path = write_record(dir.path(), &SpawnRecord { unavailable: 6, ..record("exited", None) });
        let seventh = record_provider_unavailable(&path, &verdict(false)).expect("rewritten");
        assert_eq!((seventh.charged, seventh.attempt_charged, seventh.unavailable), (1, true, 7));
        let charge = unavailable_charge(&seventh, &verdict(false));
        assert_eq!(charge, "attempt charged, because this visit's 6 free endings are spent");

        fs::write(&path, "{").unwrap();
        assert!(record_provider_unavailable(&path, &verdict(false)).is_none());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{", "left exited and charged");
        let absent = dir.path().join("runtime/spawns/none.json");
        assert!(record_provider_unavailable(&absent, &verdict(false)).is_none());
    }

    /// The metrics note written at the exit is amended to the record's ending,
    /// and only its own line changes. §FS-rhei-agents.8.4 §FS-rhei-metrics.2
    #[test]
    fn the_metrics_note_takes_the_rewritten_ending() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = dir.path().join("runtime");
        let (other, own) = (runtime.join("logs/other.log"), runtime.join("logs/own.log"));
        for log in [&other, &own] {
            note_metric_pending_session(&runtime, "plan.1", "working", 1, 0, 1, log, Some(0), "exited");
        }
        amend_metric_pending_ending(&runtime, &own, PROVIDER_UNAVAILABLE_ENDING);
        let notes = read_pending_sessions(&metric_pending_path(&runtime));
        let endings: Vec<_> = notes.iter().map(|note| note.ending.as_str()).collect();
        assert_eq!(endings, ["exited", "provider_unavailable"]);

        let undeclared = dir.path().join("undeclared/runtime");
        amend_metric_pending_ending(&undeclared, &own, PROVIDER_UNAVAILABLE_ENDING);
        assert!(!metric_pending_path(&undeclared).exists(), "no note, nothing to amend");
    }

    fn released(outcome: rhei_tui::TaskOutcome) -> rhei_tui::RunEvent {
        rhei_tui::RunEvent::SlotReleased {
            slot: 0,
            task: "plan.1".to_string(),
            from: "working".to_string(),
            to: "working".to_string(),
            log_path: PathBuf::from("runtime/logs/plan.1-working.log"),
            outcome,
            finished_at: std::time::Instant::now(),
            wall_clock: observed(),
            exit_code: Some(0),
            duration_ms: 1,
            configuration_fallbacks: Vec::new(),
            reverted: None,
        }
    }

    const DEADLINE: &str = "2026-09-16T17:21:00Z";

    fn waited() -> rhei_tui::TaskOutcome {
        rhei_tui::TaskOutcome::ProviderUnavailable {
            provider: Some("openai-codex".to_string()),
            next_attempt_at: Some(DEADLINE.to_string()),
        }
    }

    fn halted() -> rhei_tui::TaskOutcome {
        rhei_tui::TaskOutcome::ProviderUnavailable { provider: None, next_attempt_at: None }
    }

    /// `slot_released` names the outcome and, only while a wait was written,
    /// its provider and deadline; both read back as written. §FS-rhei-run-json.2.1
    #[test]
    fn run_json_carries_the_wait_only_when_one_was_written() {
        let record = rhei_tui::encode_event(Some(1), &released(waited()), observed(), None);
        assert_eq!(record["outcome"], "provider_unavailable");
        assert_eq!((record["provider"].as_str(), record["next_attempt_at"].as_str()), (Some("openai-codex"), Some(DEADLINE)));
        let halt = rhei_tui::encode_event(Some(2), &released(halted()), observed(), None);
        assert_eq!(halt["outcome"], "provider_unavailable");
        assert!(halt.get("provider").is_none() && halt.get("next_attempt_at").is_none(), "{halt}");
        for (record, outcome) in [(record, waited()), (halt, halted())] {
            match rhei_tui::decode_event(&record.to_string()).expect("decoded").event {
                rhei_tui::RunEvent::SlotReleased { outcome: read, .. } => assert_eq!(read, outcome),
                other => panic!("expected a released slot, got {other:?}"),
            }
        }
        let blocker = rhei_tui::IdleBlocker::ProviderUnavailable;
        assert_eq!(blocker.name(), "provider_unavailable");
        assert_eq!(rhei_tui::IdleBlocker::from_name("provider_unavailable"), Some(blocker));
    }

    /// The journal spells the outcome as the stream does, with the wait beside
    /// it only when one was written. §FS-rhei-run-tui.1.7
    #[test]
    fn the_journal_records_the_outcome_and_its_wait() {
        let dir = tempfile::tempdir().unwrap();
        let sink = rhei_tui::JournalSink::open(dir.path()).unwrap();
        rhei_tui::EventSink::emit(&sink, released(waited()));
        rhei_tui::EventSink::emit(&sink, released(halted()));
        let journal = fs::read_to_string(dir.path().join("runtime/transitions.log")).unwrap();
        let lines: Vec<_> = journal.lines().collect();
        let wait = format!("outcome=provider_unavailable,provider=openai-codex,next_attempt_at={DEADLINE}");
        assert!(lines[0].ends_with(&wait), "{journal}");
        assert!(lines[1].ends_with("outcome=provider_unavailable"), "{journal}");
    }

    /// The report: the idle result, the Waiting row and its tally, and the
    /// ledger's reason with no transition. §FS-rhei-run-report.3.1 §FS-rhei-run-report.4
    #[test]
    fn the_report_reads_an_unavailable_provider_as_a_wait() {
        let blocker = rhei_tui::IdleBlocker::ProviderUnavailable;
        assert_eq!(blocker.result_phrase(None), "idle \u{2014} waiting on an unavailable provider");

        let cause = HaltCause::ProviderLimited {
            provider: "openai-codex".to_string(),
            next_attempt_at: DEADLINE.to_string(),
            kind: ProviderWaitKind::Unavailable,
        };
        let (reason, next) = cause.describe("plan.1", "working");
        assert_eq!(reason, format!("provider openai-codex unavailable until {DEADLINE}"));
        assert_eq!(next, "nothing to do on Task plan.1; rhei resumes it automatically");
        let row = AttentionRow {
            id: "plan.1".to_string(),
            state: "working".to_string(),
            reason,
            next,
            is_gate: true,
            waits_on_person: false,
            provider_limited: false,
            provider_unavailable: true,
        };
        assert_eq!(waiting_tally(&[row]), "1 provider-unavailable");

        let ledger = LedgerOutcome::from(waited());
        assert!(ledger.unrouted(), "no transition was taken");
        let reason = ledger_outcome_reason(&ledger, Some(0));
        assert_eq!(reason, format!("provider openai-codex unavailable until {DEADLINE}"));
        let spent = LedgerOutcome::from(halted());
        assert_eq!(ledger_outcome_reason(&spent, Some(0)), "provider unavailable");
    }
}
