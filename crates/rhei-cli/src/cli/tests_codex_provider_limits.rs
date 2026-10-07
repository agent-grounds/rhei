mod provider_limit_codex_contract {
    use super::*;

    const PREFIX: &str = "You've hit your usage limit. Visit https://chatgpt.com/codex/settings/usage to purchase more credits or try again at ";
    const OLD: &str = "You've hit your weekly limit · resets 11:19pm (Europe/Zurich)";

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

    fn resolved() -> ResolvedAgent {
        let settings = RheiSettings { agents: built_in_agents(), ..RheiSettings::default() };
        resolve_target_agent("codex:openai:alpha", None, &settings).unwrap()
    }

    fn observed() -> std::time::SystemTime {
        Utc.with_ymd_and_hms(2026, 10, 7, 0, 0, 0).unwrap().into()
    }

    fn signal(date: &str) -> String {
        format!("{PREFIX}{date}.")
    }

    fn classify(lines: &[String]) -> Option<ProviderLimit> {
        classify_provider_limit(&resolved(), status(1), false, false, lines, observed())
    }

    /// Error decoding is independent of the dated parser. §FS-rhei-agents.2.3
    #[test]
    fn codex_error_message_exposes_logical_stdout_lines() {
        let text = format!("context\n{OLD}\nmore context");
        let event = serde_json::json!({"type": "error", "message": text}).to_string();
        let lines = provider_limit_output_lines(
            "codex", &[(rhei_tui::AgentStream::Stdout, event)],
        );
        assert_eq!(lines, vec!["context", OLD, "more context"], "decode only error.message");
        assert!(classify(&lines).is_some(), "decoded old wording remains recognizable");
    }

    /// Neither event copies nor arbitrary fields expose signals. §FS-rhei-agents.2.3
    #[test]
    fn codex_decoding_excludes_wrong_family_stream_event_and_field() {
        use rhei_tui::AgentStream::{Stderr, Stdout};
        let error = serde_json::json!({"type": "error", "message": OLD}).to_string();
        let cases = [
            ("claude-code", Stdout, error.clone()),
            ("custom", Stdout, error.clone()),
            ("codex", Stderr, error),
            ("codex", Stdout, serde_json::json!({"type": "turn.failed", "error": {"message": OLD}}).to_string()),
            ("codex", Stdout, serde_json::json!({"type": "item.completed", "item": {"type": "agent_message", "text": OLD}}).to_string()),
            ("codex", Stdout, serde_json::json!({"type": "result", "result": OLD}).to_string()),
            ("codex", Stdout, serde_json::json!({"type": "error", "other": OLD}).to_string()),
            ("codex", Stdout, serde_json::json!({"type": "error", "message": {"text": OLD}}).to_string()),
            ("codex", Stdout, format!("{{\"type\":\"error\",\"message\":\"{OLD}\"")),
        ];
        for (family, stream, raw) in cases {
            let lines = provider_limit_output_lines(family, &[(stream, raw.clone())]);
            assert_eq!(lines, vec![raw.clone()], "{family}: {raw}");
            assert!(classify(&lines).is_none(), "{family}: {raw}");
        }
    }

    /// Fixed clock, future calendar examples, and native local-zone arithmetic.
    /// The injected-zone matrix needs the seam named in the contract export.
    /// §FS-rhei-agents.2.3 §FS-rhei-run.3.3
    #[test]
    fn codex_absolute_dates_validate_ordinals_calendar_and_safe_minute() {
        let cases = [
            ("Jan 1st, 2032 12:00 AM", (2032, 1, 1, 0, 1)),
            ("Jan 2nd, 2032 12:00 PM", (2032, 1, 2, 12, 1)),
            ("Jan 3rd, 2032 1:59 AM", (2032, 1, 3, 2, 0)),
            ("Jan 11th, 2032 11:19 PM", (2032, 1, 11, 23, 20)),
            ("Jan 12th, 2032 11:19 PM", (2032, 1, 12, 23, 20)),
            ("Jan 13th, 2032 11:19 PM", (2032, 1, 13, 23, 20)),
            ("Jan 21st, 2032 11:19 PM", (2032, 1, 21, 23, 20)),
            ("Jan 22nd, 2032 11:19 PM", (2032, 1, 22, 23, 20)),
            ("Jan 23rd, 2032 11:19 PM", (2032, 1, 23, 23, 20)),
            ("Feb 29th, 2032 11:59 PM", (2032, 3, 1, 0, 0)),
            ("Dec 31st, 2032 11:59 PM", (2033, 1, 1, 0, 0)),
        ];
        for (text, (year, month, day, hour, minute)) in cases {
            let expected = chrono::Local.with_ymd_and_hms(year, month, day, hour, minute, 0)
                .single().expect("fixture boundary is unique in the native zone")
                .with_timezone(&Utc).to_rfc3339_opts(SecondsFormat::Secs, true);
            let refusal = format!(" \x1b[31m{}\x1b[0m ", signal(text));
            let result = classify(&[refusal]).unwrap_or_else(|| panic!("valid dated refusal must park: {text}"));
            assert_eq!(result.next_attempt_at, expected, "{text}");
            assert_eq!(result.signal, signal(text), "ANSI removal and outer trimming");
        }
    }

    /// These are negative controls; passing before recognition exists is not
    /// evidence that the new grammar works. §FS-rhei-agents.2.3
    #[test]
    fn codex_dated_lookalikes_remain_ordinary() {
        for text in [
            "Jan 01st, 2032 11:19 PM", "Jan 1st, 2032 01:19 PM",
            "Jan 11st, 2032 11:19 PM", "Jan 12nd, 2032 11:19 PM",
            "Jan 13rd, 2032 11:19 PM", "Jan 21th, 2032 11:19 PM",
            "Feb 29th, 2031 11:19 PM", "Feb 29th, 2100 11:19 PM",
            "Apr 31st, 2032 11:19 PM", "Jan 0th, 2032 11:19 PM",
            "Jan 32nd, 2032 11:19 PM", "Jan 1st, 0000 11:19 PM",
            "Jan 1st, 203 11:19 PM", "Jan 1st, 10000 11:19 PM",
            "Jan 1st, 2032 0:19 PM", "Jan 1st, 2032 13:19 PM",
            "Jan 1st, 2032 11:9 PM", "Jan 1st, 2032 11:60 PM",
            "Jan 1st, 2032 11:19 pm", "jan 1st, 2032 11:19 PM",
        ] {
            assert!(classify(&[signal(text)]).is_none(), "{text}");
        }
        let valid = signal("Jan 1st, 2032 11:19 PM");
        for text in [format!("quoted: {valid}"), format!("{valid} extra"),
            valid.trim_end_matches('.').to_string(), valid.replace("purchase", "buy"),
            valid.replace("https://", "http://"), valid.replace("You've", "you've")] {
            assert!(classify(&[text.clone()]).is_none(), "{text}");
        }
    }

    /// Count both grammars before calendar resolution, without deduplication.
    /// §FS-rhei-agents.2.3
    #[test]
    fn codex_mixed_and_decoded_duplicates_are_ordinary() {
        let dated = signal("Jan 1st, 2032 11:19 PM");
        let invalid_date = signal("Feb 30th, 2032 11:19 PM");
        for text in [dated.clone(), invalid_date] {
            assert!(classify(&[OLD.into(), text.clone()]).is_none(), "mixed grammar: {text}");
        }
        use rhei_tui::AgentStream::{Stderr, Stdout};
        let event = serde_json::json!({"type": "error", "message": OLD}).to_string();
        for captured in [
            vec![(Stdout, event.clone()), (Stdout, event.clone())],
            vec![(Stdout, event.clone()), (Stdout, OLD.into())],
            vec![(Stdout, event), (Stderr, OLD.into())],
            vec![(Stdout, serde_json::json!({"type": "error", "message": format!("{OLD}\n{OLD}")}).to_string())],
        ] {
            let lines = provider_limit_output_lines("codex", &captured);
            assert!(classify(&lines).is_none(), "decoded duplicates: {lines:#?}");
        }
        assert!(classify(&[dated.clone(), dated]).is_none());
    }

    /// Provider and outcome gates precede refusal parsing. §FS-rhei-agents.2.3
    #[test]
    fn codex_usage_provider_and_outcome_boundaries_remain_closed() {
        let lines = [signal("Jan 1st, 2032 11:19 PM")];
        for provider in [None, Some("acme"), Some("OpenAI")] {
            let mut agent = resolved();
            agent.model_provider = provider.map(str::to_owned);
            assert!(classify_provider_limit(&agent, status(1), false, false, &lines, observed()).is_none());
        }
        for (code, timeout, interruption) in [(0, false, false), (1, true, false), (1, false, true)] {
            assert!(classify_provider_limit(&resolved(), status(code), timeout, interruption, &lines, observed()).is_none());
        }
    }

    /// Absolute deadlines do not acquire the old grammar's next-day rollover.
    /// §FS-rhei-run.3.3
    #[test]
    fn codex_expired_and_equal_absolute_boundaries_are_ordinary() {
        let reported = chrono::Local.with_ymd_and_hms(2032, 1, 1, 23, 19, 0).single().unwrap();
        let boundary = reported + TimeDelta::minutes(1);
        let lines = [signal("Jan 1st, 2032 11:19 PM")];
        for at in [boundary, boundary + TimeDelta::seconds(1)] {
            assert!(classify_provider_limit(&resolved(), status(1), false, false, &lines, at.into()).is_none());
        }
    }

    /// The shared safe-minute seam already accepts an injected named zone.
    /// Pin both sides of each transition before its local-zone extension.
    /// §FS-rhei-run.3.3
    #[test]
    fn codex_safe_boundary_requires_both_minutes_to_be_unique() {
        let zone: Tz = "Europe/Zurich".parse().unwrap();
        let gap = NaiveDate::from_ymd_opt(2032, 3, 28).unwrap();
        let overlap = NaiveDate::from_ymd_opt(2032, 10, 31).unwrap();
        for date in [gap, overlap] {
            assert!(unique_safe_boundary(zone, date, 1, 59).is_none(), "boundary: {date}");
            assert!(unique_safe_boundary(zone, date, 2, 20).is_none(), "reported: {date}");
        }
        assert_eq!(
            unique_safe_boundary(zone, NaiveDate::from_ymd_opt(2032, 2, 29).unwrap(), 23, 59)
                .unwrap().to_rfc3339_opts(SecondsFormat::Secs, true),
            "2032-02-29T23:00:00Z"
        );
        assert_eq!(
            unique_safe_boundary(zone, NaiveDate::from_ymd_opt(2026, 10, 9).unwrap(), 23, 19)
                .unwrap().to_rfc3339_opts(SecondsFormat::Secs, true),
            "2026-10-09T21:20:00Z"
        );
    }
}
