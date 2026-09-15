/// A valid final Claude cumulative result replaces an earlier extraction
/// failure instead of retaining it as precedence evidence.
// §FS-rhei-cost-accounting.4
#[test]
fn later_valid_claude_result_replaces_earlier_extraction_diagnostic() {
    let dir = tempfile::tempdir().expect("tempdir");
    let capture = AgentUsageCapture {
        extractor: AgentUsageExtractor::Claude,
        replace_usage_capture: true,
        path: dir.path().join("usage.jsonl"),
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
        r#"{"type":"result","usage":{"input_tokens":123}}"#,
        &sink,
    );
    capture_agent_output_usage(
        Some(&capture),
        rhei_tui::AgentStream::Stdout,
        r#"{"type":"result","result":"valid final result","usage":{"input_tokens":20,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"output_tokens":8}}"#,
        &sink,
    );

    let capture_text = std::fs::read_to_string(&capture.path).expect("capture");
    assert_eq!(capture_text.lines().count(), 1, "valid result replaces failure");
    assert!(!capture_text.contains("extractor-failed"));
    match extract_usage_from_capture(Some(&capture.path)) {
        ExtractedUsageStatus::Measured(usage) => {
            assert_eq!(usage.input_total, Some(20));
            assert_eq!(usage.output_total, Some(8));
        }
        _ => panic!("latest valid cumulative result should be measured"),
    }
}
