// How the Claude result envelope is read: which shapes are accepted as billing
// telemetry, which are refused, and how a cumulative stream's latest result
// replaces the capture rather than adding to it.
//
// Its own part beside the record-writing tests, the same way extraction
// diagnostics and token conventions already are.

// §AR-source-file-size.3 §FS-rhei-cost-accounting.4

#[test]
fn claude_result_json_extracts_typed_usage_dimensions() {
    // §FS-rhei-cost-accounting.4: Claude result usage is normalized without
    // treating unrelated JSON fields as billing telemetry.
    let line = r#"{"type":"result","subtype":"success","is_error":false,"result":"useful response","usage":{"input_tokens":123,"cache_read_input_tokens":456,"cache_creation_input_tokens":78,"output_tokens":90}}"#;

    let usage = match extract_usage_from_output_line(AgentUsageExtractor::Claude, line) {
        OutputUsage::Measured(usage) => usage,
        OutputUsage::Ignored => panic!("Claude result usage should be measured"),
        OutputUsage::Failed => panic!("valid Claude result should not fail extraction"),
    };
    // Anthropic's `input_tokens` excludes its cache dimensions, so the three
    // are added into one `input.total`. §FS-rhei-cost-accounting.3.1
    assert_eq!(usage.input_total, Some(657));
    assert_eq!(usage.input_cached_read, Some(456));
    assert_eq!(usage.input_cache_write, Some(78));
    assert_eq!(usage.output_total, Some(90));
    let tokens = tokens_from_usage(usage);
    assert_eq!(tokens.total.value, Some(747));
    assert_eq!(tokens.input.total.value, Some(657));
    assert_eq!(tokens.input.cached_read.value, Some(456));
    assert_eq!(tokens.input.cache_write.value, Some(78));
    assert_eq!(tokens.output.total.value, Some(90));
    assert!(matches!(
        display_output_line(AgentUsageExtractor::Claude, line),
        AgentOutputLine::Replace(text) if text == "useful response"
    ));
}

#[test]
fn claude_result_json_accepts_typed_model_usage_fallback() {
    let line = r#"{"type":"result","subtype":"success","result":"response","modelUsage":{"claude-sonnet-4-6":{"inputTokens":100,"cacheReadInputTokens":20,"cacheCreationInputTokens":30,"outputTokens":40}}}"#;

    let usage = match extract_usage_from_output_line(AgentUsageExtractor::Claude, line) {
        OutputUsage::Measured(usage) => usage,
        OutputUsage::Ignored => panic!("Claude model usage should be measured"),
        OutputUsage::Failed => panic!("valid Claude model usage should not fail extraction"),
    };
    // §FS-rhei-cost-accounting.3.1: 100 fresh, 20 cached and 30 written are
    // 150 input tokens, in the `modelUsage` shape as in the `usage` one.
    assert_eq!(usage.input_total, Some(150));
    assert_eq!(usage.input_cached_read, Some(20));
    assert_eq!(usage.input_cache_write, Some(30));
    assert_eq!(usage.output_total, Some(40));
}

#[test]
fn claude_result_json_rejects_unrelated_and_malformed_usage() {
    let unrelated = r#"{"metrics":{"input_tokens":123,"output_tokens":456}}"#;
    assert!(matches!(
        extract_usage_from_output_line(AgentUsageExtractor::Claude, unrelated),
        OutputUsage::Ignored
    ));

    let malformed = r#"{"type":"result","result":"response","usage":{"input_tokens":"not-a-number"}}"#;
    assert!(matches!(
        extract_usage_from_output_line(AgentUsageExtractor::Claude, malformed),
        OutputUsage::Failed
    ));
    assert!(matches!(
        extract_usage_from_output_line(AgentUsageExtractor::Claude, "not json"),
        OutputUsage::Failed
    ));

    let dir = tempfile::tempdir().expect("tempdir");
    let log_path = dir.path().join("agent.log");
    std::fs::write(&log_path, "tokens used\n999\n").expect("write log");
    assert!(matches!(
        extract_usage(Some(&dir.path().join("missing.jsonl")), Some(&log_path), "claude-code"),
        ExtractedUsageStatus::NoUsageEmitted
    ));

    let capture_path = dir.path().join("failed.jsonl");
    append_extractor_failure_event(&capture_path).expect("write failure marker");
    assert!(matches!(
        extract_usage_from_capture(Some(&capture_path)),
        ExtractedUsageStatus::ExtractorFailed
    ));
}

#[test]
fn claude_result_json_rejects_incomplete_envelopes() {
    // §FS-rhei-cost-accounting.4: partial Claude envelopes cannot fabricate
    // measured billing telemetry or suppress their raw diagnostic output.
    for incomplete in [
        r#"{"type":"result","usage":{"input_tokens":123}}"#,
        r#"{"type":"result","result":"response","usage":{"input_tokens":123}}"#,
        r#"{"type":"result","result":"response"}"#,
    ] {
        assert!(matches!(
            extract_usage_from_output_line(AgentUsageExtractor::Claude, incomplete),
            OutputUsage::Failed
        ));
        assert!(matches!(
            display_output_line(AgentUsageExtractor::Claude, incomplete),
            AgentOutputLine::Passthrough
        ));
    }
}

#[test]
fn claude_result_stream_usage_keeps_latest_cumulative_capture() {
    // Claude's stream result events are cumulative across intervention turns.
    // §FS-rhei-cost-accounting.4
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("usage.jsonl");
    let capture = AgentUsageCapture {
        extractor: AgentUsageExtractor::Claude,
        replace_usage_capture: true,
        path: path.clone(),
        invocation_id: "1::work::claude-code::visit-1".to_string(),
        task_id: "1".to_string(),
        state: "work".to_string(),
        agent: "claude-code".to_string(),
        provider: Some("anthropic".to_string()),
        model: Some("claude-sonnet-4-6".to_string()),
        price_book: builtin_price_book(),
        slot: 0,
        cli_session: Arc::new(Mutex::new(None)),
    };
    let sink: Arc<dyn rhei_tui::EventSink> = Arc::new(RecordingSink::default());

    capture_agent_output_usage(
        Some(&capture),
        rhei_tui::AgentStream::Stdout,
        r#"{"type":"result","subtype":"success","result":"first","usage":{"input_tokens":10,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"output_tokens":5}}"#,
        &sink,
    );
    capture_agent_output_usage(
        Some(&capture),
        rhei_tui::AgentStream::Stdout,
        r#"{"type":"result","subtype":"success","result":"second","usage":{"input_tokens":20,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"output_tokens":8}}"#,
        &sink,
    );

    match extract_usage_from_capture(Some(&path)) {
        ExtractedUsageStatus::Measured(usage) => {
            assert_eq!(usage.input_total, Some(20));
            assert_eq!(usage.output_total, Some(8));
        }
        _ => panic!("latest Claude stream result should be measured"),
    }
}
