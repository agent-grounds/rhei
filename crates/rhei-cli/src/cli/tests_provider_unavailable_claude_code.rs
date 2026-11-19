mod provider_unavailable_claude_code {
    use super::*;
    use serde_json::json;

    const REFUSED: &str =
        "API Error: Connection refused — a firewall or proxy may be blocking it (ECONNREFUSED)";

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

    fn claude_code() -> ResolvedAgent {
        resolved("claude-code:anthropic:claude-sonnet-4-6")
    }

    fn observed() -> std::time::SystemTime {
        std::time::UNIX_EPOCH + Duration::from_millis(1_789_579_200_500)
    }

    /// A stream Claude Code 2.1.328 printed, kept with the e2e tests.
    fn fixture(name: &str) -> Vec<serde_json::Value> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/e2e/fixtures/provider_unavailable_claude_code")
            .join(format!("{name}.jsonl"));
        let raw = fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        raw.lines().map(|line| serde_json::from_str(line).expect("a JSON event")).collect()
    }

    fn stdout(events: &[serde_json::Value]) -> Vec<(rhei_tui::AgentStream, String)> {
        events.iter().map(|event| (rhei_tui::AgentStream::Stdout, event.to_string())).collect()
    }

    fn classify(resolved: &ResolvedAgent, events: &[serde_json::Value]) -> Option<ProviderUnavailable> {
        classify_provider_unavailable(resolved, status(1), false, false, &stdout(events), observed())
    }

    fn retry(attempt: u64, max_retries: u64, error_status: serde_json::Value) -> serde_json::Value {
        json!({"type": "system", "subtype": "api_retry", "attempt": attempt,
               "max_retries": max_retries, "error_status": error_status, "error": "unknown"})
    }

    fn give_up(text: &str) -> serde_json::Value {
        json!({"type": "assistant", "is_api_error_message": true, "error": "server_error",
               "message": {"model": "<synthetic>", "role": "assistant",
                           "content": [{"type": "text", "text": text}]}})
    }

    fn answer() -> serde_json::Value {
        json!({"type": "assistant", "message": {"model": "claude-sonnet-4-6", "role": "assistant",
               "content": [{"type": "text", "text": "Reading the ticket."}]}})
    }

    /// Every retry used up on `error_status`, then the give-up.
    fn used_up(error_status: serde_json::Value) -> Vec<serde_json::Value> {
        vec![retry(1, 3, error_status.clone()), retry(2, 3, error_status.clone()),
             retry(3, 3, error_status), give_up(REFUSED)]
    }

    /// The captured give-ups whose last retry is a transient class — no HTTP
    /// response, 429, 500 and 529 — are recognized, signal and wait key
    /// included. §FS-rhei-provider-unavailability.2
    #[test]
    fn the_transient_give_ups_claude_code_printed_are_recognized() {
        let refused = classify(&claude_code(), &fixture("refused")).expect("refused");
        assert_eq!((refused.family, refused.signal.as_str(), refused.answered), ("claude-code", REFUSED, false));
        assert_eq!(
            refused.identity,
            ProviderIdentity { agent: "claude-code".to_string(), provider: "anthropic".to_string() }
        );
        for name in ["hang", "status-429", "status-500", "status-529"] {
            let verdict = classify(&claude_code(), &fixture(name)).unwrap_or_else(|| panic!("{name}"));
            assert!(!verdict.answered, "{name}: nothing but <synthetic> was said");
        }
        let hang = classify(&claude_code(), &fixture("hang")).expect("hang");
        assert!(hang.signal.starts_with("Request timed out."), "{}", hang.signal);
    }

    /// `null`, 429 and every 5xx are the transient classes; 401, 403, 408,
    /// 409 and a 4xx not retried at all are not. §FS-rhei-provider-unavailability.2
    #[test]
    fn only_a_transient_status_class_on_the_last_retry_is_recognized() {
        for accepted in [json!(null), json!(429), json!(500), json!(503), json!(529), json!(599)] {
            assert!(classify(&claude_code(), &used_up(accepted.clone())).is_some(), "{accepted}");
        }
        for refused in [401, 403, 408, 409, 400, 600] {
            assert_eq!(classify(&claude_code(), &used_up(json!(refused))), None, "{refused}");
        }
        assert_eq!(classify(&claude_code(), &fixture("status-401")), None, "the captured 401");
        assert_eq!(classify(&claude_code(), &fixture("status-400")), None, "400 is never retried");
    }

    /// The count is the last retry's own: below its `max_retries` is not a
    /// give-up, and a site's lowered count is respected. §FS-rhei-provider-unavailability.2
    #[test]
    fn a_last_retry_under_its_own_count_is_not_recognized() {
        let early = [retry(1, 6, json!(529)), retry(2, 6, json!(529)), give_up("Repeated 529")];
        assert_eq!(classify(&claude_code(), &early), None, "gave up early");
        let lowered = [retry(1, 1, json!(null)), give_up(REFUSED)];
        assert!(classify(&claude_code(), &lowered).is_some(), "CLAUDE_CODE_MAX_RETRIES=1");
        let recovered = [retry(3, 3, json!(null)), answer()];
        assert_eq!(classify(&claude_code(), &recovered), None, "the model answered after the last retry");
        assert_eq!(classify(&claude_code(), &[retry(3, 3, json!(null))]), None, "no give-up event");
        assert_eq!(classify(&claude_code(), &[give_up(REFUSED)]), None, "no api_retry");
    }

    /// A `<synthetic>` message is Claude Code's, never the provider's answer;
    /// an answer before the retries is still recognized, as answered.
    // §FS-rhei-provider-unavailability.3
    #[test]
    fn a_synthetic_message_is_never_an_answer() {
        let mut synthetic = vec![give_up("an earlier synthetic note")];
        synthetic.extend(used_up(json!(null)));
        assert!(!classify(&claude_code(), &synthetic).expect("recognized").answered);
        let mut answered = vec![answer()];
        answered.extend(used_up(json!(null)));
        assert!(classify(&claude_code(), &answered).expect("still recognized").answered);
    }

    /// Exit 0, a timeout, an interruption, no provider, another family, and
    /// stderr are not Claude Code giving up. §FS-rhei-provider-unavailability.2
    #[test]
    fn only_a_claude_code_non_zero_exit_with_a_provider_is_recognized() {
        let lines = stdout(&fixture("refused"));
        let at = observed();
        let classify_with = |resolved: &ResolvedAgent, code, timed_out, interrupted| {
            classify_provider_unavailable(resolved, status(code), timed_out, interrupted, &lines, at)
        };
        assert_eq!(classify_with(&claude_code(), 0, false, false), None, "exit 0");
        assert_eq!(classify_with(&claude_code(), 143, true, false), None, "timed out");
        assert_eq!(classify_with(&claude_code(), 130, false, true), None, "interrupted");
        assert_eq!(classify_with(&resolved("claude-code:claude-sonnet-4-6"), 1, false, false), None, "no provider");
        let codex = super::provider_limits::codex_openai();
        assert_eq!(classify_with(&codex, 1, false, false), None, "another family");
        let stderr: Vec<_> =
            lines.iter().map(|(_, line)| (rhei_tui::AgentStream::Stderr, line.clone())).collect();
        assert_eq!(classify_provider_unavailable(&claude_code(), status(1), false, false, &stderr, at), None);
    }
}
