fn accounting_test_record() -> AccountingInvocationRecord {
    AccountingInvocationRecord {
        schema: ACCOUNTING_INVOCATION_SCHEMA.to_string(),
        invocation_id: "1::work::codex::visit-1".to_string(),
        run_id: None,
        task_id: "1".to_string(),
        state: "work".to_string(),
        visit: 1,
        target_slug: None,
        agent: "codex".to_string(),
        provider: Some("openai".to_string()),
        model: Some("gpt-test".to_string()),
        started_at: "2026-05-20T10:00:00Z".to_string(),
        ended_at: "2026-05-20T10:00:00Z".to_string(),
        duration_ms: None,
        cli_session: None,
        extraction_status: "measured".to_string(),
        extraction_diagnostics: Vec::new(),
        scope: "aggregate-agent-process".to_string(),
        token_convention: None,
        tokens: AccountingTokens::default(),
        pricing: AccountingPricing {
            status: "unpriced".to_string(),
            currency: Some("USD".to_string()),
            amount_micro: None,
            priced_amount_micro: None,
            price_book_id: Some(PRICE_BOOK_ID.to_string()),
        },
    }
}

fn accounting_usage(
    coverage: rhei_tui::UsageCoverage,
    pricing_status: rhei_tui::PricingStatus,
    cost_micro: Option<u64>,
    priced_cost_micro: Option<u64>,
) -> rhei_tui::UsageSummary {
    let measured = rhei_tui::DimensionSummary {
        value: Some(1),
        status: rhei_tui::DimensionStatus::Measured,
        missing_count: 0,
        measured_count: 1,
    };
    rhei_tui::UsageSummary {
        invocation_id: format!("{pricing_status:?}-{coverage:?}"),
        state: "work".to_string(),
        agent: "codex".to_string(),
        provider: Some("openai".to_string()),
        model: Some("gpt-test".to_string()),
        total: measured.clone(),
        input_total: measured.clone(),
        input_cached_read: measured.clone(),
        input_cache_write: measured.clone(),
        output_total: measured.clone(),
        output_cached_read: measured.clone(),
        output_cache_write: measured,
        cost_micro,
        priced_cost_micro,
        currency: Some("USD".to_string()),
        coverage,
        status: rhei_tui::UsageStatus::Measured,
        pricing_status,
    }
}

#[test]
fn accounting_invocation_file_ids_are_unique_for_fast_reruns() {
    let record = accounting_test_record();

    assert_ne!(invocation_file_id(&record), invocation_file_id(&record));
}

#[test]
fn accounting_task_file_segments_do_not_collapse_valid_task_ids() {
    assert_eq!(safe_accounting_file_segment("build.api"), "build.api");
    assert_eq!(safe_accounting_file_segment("build_api"), "build_api");
    assert_ne!(
        safe_accounting_file_segment("build.api"),
        safe_accounting_file_segment("build_api")
    );
    assert_eq!(safe_accounting_file_segment("build/api"), "build%2Fapi");
}

#[test]
fn accounting_mixed_priced_and_unpriced_rollup_is_partial() {
    let priced = accounting_usage(
        rhei_tui::UsageCoverage::Complete,
        rhei_tui::PricingStatus::Priced,
        Some(100),
        Some(100),
    );
    let unpriced = accounting_usage(
        rhei_tui::UsageCoverage::Unpriced,
        rhei_tui::PricingStatus::Unpriced,
        None,
        None,
    );

    let summary = rhei_tui::summarize_usage_summaries([&priced, &unpriced]).expect("summary");

    assert_eq!(summary.coverage, rhei_tui::UsageCoverage::Partial);
    assert_eq!(summary.pricing_status, rhei_tui::PricingStatus::PartialPrice);
    assert_eq!(summary.cost_micro, None);
    assert_eq!(summary.priced_cost_micro, Some(100));
}

#[test]
fn accounting_capture_env_is_declared_before_spawn() {
    let path = std::path::PathBuf::from("/tmp/rhei-usage.jsonl");
    let mut command = std::process::Command::new("agent");

    configure_accounting_capture(&mut command, Some(&path));

    let env: std::collections::BTreeMap<String, String> = command
        .get_envs()
        .filter_map(|(key, value)| {
            value.map(|value| {
                (
                    key.to_string_lossy().into_owned(),
                    value.to_string_lossy().into_owned(),
                )
            })
        })
        .collect();
    assert_eq!(
        env.get("RHEI_ACCOUNTING_USAGE_PATH").map(String::as_str),
        Some("/tmp/rhei-usage.jsonl")
    );
    assert_eq!(
        env.get("RHEI_ACCOUNTING_USAGE_SCHEMA").map(String::as_str),
        Some(ACCOUNTING_USAGE_EVENT_SCHEMA)
    );
}

#[test]
fn accounting_extractor_ignores_arbitrary_json_without_schema() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("usage.jsonl");
    std::fs::write(
        &path,
        r#"{"metrics":{"input_tokens":123,"output_tokens":456}}"#,
    )
    .expect("write capture");

    match extract_usage_from_capture(Some(&path)) {
        ExtractedUsageStatus::NoUsageEmitted => {}
        _ => panic!("arbitrary JSON must not be treated as usage"),
    }
}

#[test]
fn accounting_extractor_accepts_structured_usage_event() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("usage.jsonl");
    std::fs::write(
        &path,
        format!(
            r#"{{"schema":"{}","usage":{{"input_tokens":123,"output_tokens":456}}}}"#,
            ACCOUNTING_USAGE_EVENT_SCHEMA
        ),
    )
    .expect("write capture");

    match extract_usage_from_capture(Some(&path)) {
        ExtractedUsageStatus::Measured(usage) => {
            assert_eq!(usage.input_total, Some(123));
            assert_eq!(usage.output_total, Some(456));
        }
        _ => panic!("structured usage event should be measured"),
    }
}

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

#[test]
fn fake_claude_json_spawn_records_measured_rollup_and_cost() {
    // §FS-rhei-cost-accounting.4 §FS-rhei-cost-accounting.6: exercise the
    // subprocess output, invocation record, and derived run summary together.
    let dir = tempfile::tempdir().expect("tempdir");
    let command = python_fixture_command(
        dir.path(),
        "claude-result-agent",
        r#"import json

print(json.dumps({
    'type': 'result',
    'subtype': 'success',
    'is_error': False,
    'result': 'plain Claude response',
    'usage': {
        'input_tokens': 123000,
        'cache_read_input_tokens': 456000,
        'cache_creation_input_tokens': 78000,
        'output_tokens': 90000,
    },
}), flush=True)
"#,
    );
    let mut profile = built_in_agents().remove("claude-code").expect("claude-code");
    profile.command = command;
    let resolved = ResolvedAgent {
        agent: AgentConfig::from("claude-code"),
        profile,
        mode: None,
        target: None,
        model: Some("impl-fast".to_string()),
        model_provider: Some("anthropic".to_string()),
        model_name: Some("claude-sonnet-4-6".to_string()),
        timeout_secs: Some(10),
        autonomous_args: Vec::new(),
    };
    let plan = rhei_core::parse(
        "# Rhei: Claude Accounting\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n",
    )
    .expect("parse plan");
    let log_path = dir.path().join("agent.log");
    let sink = Arc::new(RecordingSink::default());
    let sink_trait: Arc<dyn rhei_tui::EventSink> = sink.clone();
    let tooling = ResolvedTooling::default();
    let outcome = spawn_and_wait_agent(
        &resolved,
        &builtin_price_book(),
        "prompt",
        dir.path(),
        dir.path(),
        None,
        &dir.path().join("plan.rhei.md"),
        None,
        "1",
        "pending",
        1,
        &tooling,
        &log_path,
        dir.path(),
        None,
        0,
        sink.clone(),
        None,
        &spawn_plan_for_test(&log_path),
        None,
    )
    .expect("fake Claude agent runs");
    assert!(outcome.status.success());
    let log = std::fs::read_to_string(&log_path).expect("read Claude log");
    assert!(log.contains("\nplain Claude response\n"));
    assert!(!log.contains("\"type\": \"result\""), "raw envelope leaked: {log}");
    assert!(sink.events.lock().expect("events").iter().any(|event| matches!(
        event,
        rhei_tui::RunEvent::AgentOutput { line, .. } if line == "plain Claude response"
    )));

    let usage = record_agent_accounting_invocation(AgentAccountingInvocation {
        workspace_root: dir.path(),
        task: &plan.tasks[0],
        state: "pending",
        resolved: &resolved,
        visit: 1,
        started_at: std::time::SystemTime::now(),
        ended_at: std::time::SystemTime::now(),
        slot: Some(0),
        usage_capture_path: outcome.usage_capture_path.as_deref(),
        cli_session: outcome.cli_session.as_ref(),
        log_path: Some(&log_path),
        price_book: &builtin_price_book(),
        sink: &sink_trait,
    })
    .expect("record accounting")
    .expect("Claude usage is present");
    assert_eq!(usage.status, rhei_tui::UsageStatus::Measured);
    // The tokens grow, because they now count the cached reads and cache
    // writes that were silently outside `input.total`. The money does not: the
    // same tokens are charged at the same rates under either convention.

    // §FS-rhei-cost-accounting.3.1 §FS-rhei-cost-accounting.5
    assert_eq!(usage.input_total.value, Some(657_000));
    assert_eq!(usage.input_cached_read.value, Some(456000));
    assert_eq!(usage.input_cache_write.value, Some(78000));
    assert_eq!(usage.output_total.value, Some(90000));
    assert_eq!(usage.cost_micro, Some(2_148_300));

    // The workspace lifetime total, which is what this rollup has always been;
    // the run's own share travels beside it. §FS-rhei-cost-accounting.6
    let summary = regenerate_accounting_indexes(dir.path(), &plan)
        .expect("regenerate rollups")
        .expect("run summary")
        .workspace
        .expect("workspace rollup");
    assert_eq!(summary.invocation_count, 1);
    assert_eq!(summary.measured_invocation_count, 1);
    assert_eq!(summary.missing_invocation_count, 0);
    assert_eq!(summary.cost_micro, Some(2_148_300));
}

#[test]
fn fake_claude_incomplete_json_is_extractor_failed_and_unmeasured() {
    // §FS-rhei-cost-accounting.4: a malformed result envelope must remain
    // unmeasured through subprocess capture, durable accounting, and rollup.
    let dir = tempfile::tempdir().expect("tempdir");
    let command = python_fixture_command(
        dir.path(),
        "claude-incomplete-result-agent",
        r#"import json

print(json.dumps({
    'type': 'result',
    'usage': {'input_tokens': 123},
}), flush=True)
"#,
    );
    let mut profile = built_in_agents().remove("claude-code").expect("claude-code");
    profile.command = command;
    let resolved = ResolvedAgent {
        agent: AgentConfig::from("claude-code"),
        profile,
        mode: None,
        target: None,
        model: Some("impl-fast".to_string()),
        model_provider: Some("anthropic".to_string()),
        model_name: Some("claude-sonnet-4-6".to_string()),
        timeout_secs: Some(10),
        autonomous_args: Vec::new(),
    };
    let plan = rhei_core::parse(
        "# Rhei: Claude Accounting\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n",
    )
    .expect("parse plan");
    let log_path = dir.path().join("agent.log");
    let sink = Arc::new(RecordingSink::default());
    let sink_trait: Arc<dyn rhei_tui::EventSink> = sink.clone();
    let tooling = ResolvedTooling::default();
    let outcome = spawn_and_wait_agent(
        &resolved,
        &builtin_price_book(),
        "prompt",
        dir.path(),
        dir.path(),
        None,
        &dir.path().join("plan.rhei.md"),
        None,
        "1",
        "pending",
        1,
        &tooling,
        &log_path,
        dir.path(),
        None,
        0,
        sink.clone(),
        None,
        &spawn_plan_for_test(&log_path),
        None,
    )
    .expect("fake Claude agent runs");
    assert!(outcome.status.success());

    let usage = record_agent_accounting_invocation(AgentAccountingInvocation {
        workspace_root: dir.path(),
        task: &plan.tasks[0],
        state: "pending",
        resolved: &resolved,
        visit: 1,
        started_at: std::time::SystemTime::now(),
        ended_at: std::time::SystemTime::now(),
        slot: Some(0),
        usage_capture_path: outcome.usage_capture_path.as_deref(),
        cli_session: outcome.cli_session.as_ref(),
        log_path: Some(&log_path),
        price_book: &builtin_price_book(),
        sink: &sink_trait,
    })
    .expect("record accounting")
    .expect("Claude accounting record is present");
    assert_eq!(usage.status, rhei_tui::UsageStatus::ExtractorFailed);
    assert_eq!(usage.total.value, None);
    assert_eq!(usage.input_total.value, None);
    assert_eq!(usage.output_total.value, None);
    assert_eq!(usage.cost_micro, None);

    // The workspace lifetime total, which is what this rollup has always been;
    // the run's own share travels beside it. §FS-rhei-cost-accounting.6
    let summary = regenerate_accounting_indexes(dir.path(), &plan)
        .expect("regenerate rollups")
        .expect("run summary")
        .workspace
        .expect("workspace rollup");
    assert_eq!(summary.invocation_count, 1);
    assert_eq!(summary.measured_invocation_count, 0);
    assert_eq!(summary.missing_invocation_count, 1);
    assert_eq!(summary.cost_micro, None);

    let log = std::fs::read_to_string(&log_path).expect("read agent log");
    assert!(log.contains("input_tokens"));
    assert!(!log.trim().is_empty());
}
