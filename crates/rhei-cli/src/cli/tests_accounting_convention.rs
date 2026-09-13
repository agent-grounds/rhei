/// One turn, told in three provider dialects. 1,000 input tokens of which 700
/// were served from cache, and 50 output tokens. Whatever the agent, the record
/// that comes out has to say the same thing about it. §FS-rhei-cost-accounting.3.1
const ONE_TURN_PER_AGENT: [(AgentUsageExtractor, &str); 3] = [
    (
        AgentUsageExtractor::Claude,
        r#"{"type":"result","subtype":"success","result":"done","usage":{"input_tokens":300,"cache_read_input_tokens":700,"cache_creation_input_tokens":0,"output_tokens":50}}"#,
    ),
    (
        AgentUsageExtractor::Codex,
        r#"{"type":"turn.completed","usage":{"input_tokens":1000,"cached_input_tokens":700,"output_tokens":50}}"#,
    ),
    (
        AgentUsageExtractor::Pi,
        r#"{"type":"message_end","message":{"role":"assistant","usage":{"input":300,"cacheRead":700,"cacheWrite":0,"output":50,"totalTokens":1050}}}"#,
    ),
];

/// The price book the reproduction of `agent-grounds/rhei#166` used, cut down to the
/// two models it priced. Real rates, so the amounts below are the ticket's own.
fn repro_price_book() -> PriceBook {
    PriceBook {
        schema: ACCOUNTING_PRICES_SCHEMA.to_string(),
        price_book_id: "foundation-site-2026-09-02".to_string(),
        currency: "USD".to_string(),
        entries: vec![
            PriceBookEntry {
                provider: "openai".to_string(),
                model: "gpt-5.6-sol".to_string(),
                effective_at: "2026-09-02T00:00:00Z".to_string(),
                unit: "1m_tokens".to_string(),
                input_total_micro: 4_000_000,
                input_cached_read_micro: 400_000,
                input_cache_write_micro: 4_000_000,
                output_total_micro: 20_000_000,
                extensions: BTreeMap::new(),
            },
            PriceBookEntry {
                provider: "anthropic".to_string(),
                model: "claude-sonnet-5".to_string(),
                effective_at: "2026-09-02T00:00:00Z".to_string(),
                unit: "1m_tokens".to_string(),
                input_total_micro: 2_000_000,
                input_cached_read_micro: 200_000,
                input_cache_write_micro: 2_500_000,
                output_total_micro: 10_000_000,
                extensions: BTreeMap::new(),
            },
        ],
        extensions: BTreeMap::new(),
    }
}

fn measured_usage(line: &str, extractor: AgentUsageExtractor) -> ExtractedUsage {
    match extract_usage_from_output_line(extractor, line) {
        OutputUsage::Measured(usage) => usage,
        _ => panic!("{extractor:?} should measure its own dialect: {line}"),
    }
}

/// The ticket itself: `input.total` meant one thing for `codex` and another for
/// `claude-code`, and both records claimed to be measured.
// §FS-rhei-cost-accounting.3.1
#[test]
fn every_built_in_extractor_reports_one_input_total_for_the_same_turn() {
    for (extractor, line) in ONE_TURN_PER_AGENT {
        let tokens = tokens_from_usage(measured_usage(line, extractor));

        assert_eq!(tokens.input.total.value, Some(1_000), "{extractor:?} input.total");
        assert_eq!(tokens.input.cached_read.value, Some(700), "{extractor:?} cached_read");
        assert_eq!(tokens.input.cache_write.value.unwrap_or(0), 0, "{extractor:?} cache_write");
        assert_eq!(tokens.output.total.value, Some(50), "{extractor:?} output.total");
        assert_eq!(tokens.total.value, Some(1_050), "{extractor:?} total");
    }
}

/// The assertion that would have caught the defect, stated without naming an
/// agent: the parts sit inside the whole.
// §FS-rhei-cost-accounting.3.1
#[test]
fn cache_dimensions_are_parts_of_input_total_not_additions_to_it() {
    for (extractor, line) in ONE_TURN_PER_AGENT {
        let tokens = tokens_from_usage(measured_usage(line, extractor));
        let input_total = tokens.input.total.value.expect("input.total measured");
        let cached_read = tokens.input.cached_read.value.unwrap_or(0);
        let cache_write = tokens.input.cache_write.value.unwrap_or(0);

        assert!(cached_read <= input_total, "{extractor:?}: {cached_read} cached of {input_total}");
        assert!(cache_write <= input_total, "{extractor:?}: {cache_write} written of {input_total}");
        assert!(
            cached_read + cache_write <= input_total,
            "{extractor:?}: {cached_read} + {cache_write} cache tokens of {input_total} input"
        );
    }
}

/// `pi` is pinned by evidence rather than assumed to follow one of the other
/// two. It states the answer itself: across 605 usage objects in 70 real `pi`
/// session transcripts under `~/.pi/agent/sessions`, `totalTokens` equals
/// `input + cacheRead + cacheWrite + output` in every one, and equals
/// `input + output` in none of the 514 whose cache dimensions are nonzero. The
/// numbers below are one of those turns, copied out.
// §FS-rhei-cost-accounting.3.6
#[test]
fn pi_input_total_is_converted_because_pis_own_aggregate_excludes_cache() {
    let line = r#"{"type":"message_end","message":{"role":"assistant","usage":{"input":4529,"cacheRead":20992,"cacheWrite":0,"output":541,"reasoning":275,"totalTokens":26062}}}"#;

    let tokens = tokens_from_usage(measured_usage(line, AgentUsageExtractor::Pi));

    assert_eq!(tokens.input.total.value, Some(25_521), "4529 fresh + 20992 cached");
    assert_eq!(tokens.input.cached_read.value, Some(20_992));
    assert_eq!(tokens.output.total.value, Some(541));
    // Pi's own aggregate is the check: converted, the parts add up to it.
    assert_eq!(tokens.total.value, Some(26_062));
    assert_eq!(
        tokens.input.total.value.unwrap() + tokens.output.total.value.unwrap(),
        tokens.total.value.unwrap(),
        "input.total + output.total is what pi reports as totalTokens"
    );
}

/// The second defect in the ticket. `input.total` was charged at the full rate
/// and `cached_read` again at the cache rate, so a `codex` cached read was
/// billed twice. Both amounts here are the reproduction's own.
// §FS-rhei-cost-accounting.5
#[test]
fn a_cached_read_is_charged_once_whichever_agent_reported_it() {
    let book = repro_price_book();

    // 1,000 input of which 700 cached, 50 output: 300 fresh at $4/M, 700 cached
    // at $0.40/M, 50 output at $20/M. The stored figure was 5,280.
    let codex = tokens_from_usage(ExtractedUsage {
        input_total: Some(1_000),
        input_cached_read: Some(700),
        output_total: Some(50),
        ..ExtractedUsage::default()
    });
    let priced = price_tokens(&book, Some("openai"), Some("gpt-5.6-sol"), &codex);
    assert_eq!(priced.amount_micro, Some(2_480), "700 cached reads charged twice");
    assert_eq!(priced.priced_amount_micro, Some(2_480));

    // The same run's `claude-code` record, converted: 800 input of which 700
    // cached, 50 output. Its money does not move, because the same tokens are
    // charged at the same rates under either convention -- the record stored
    // 100 uncached input and was priced at 840 for it.
    let claude = tokens_from_usage(ExtractedUsage {
        input_total: Some(800),
        input_cached_read: Some(700),
        input_cache_write: Some(0),
        output_total: Some(50),
        ..ExtractedUsage::default()
    });
    let priced = price_tokens(&book, Some("anthropic"), Some("claude-sonnet-5"), &claude);
    assert_eq!(priced.amount_micro, Some(840), "anthropic pricing must not move");
}

/// The sharpest case of the same arithmetic: nothing is left to charge at the
/// full rate.
// §FS-rhei-cost-accounting.5
#[test]
fn a_fully_cached_prompt_costs_the_cache_rate_once() {
    let tokens = tokens_from_usage(ExtractedUsage {
        input_total: Some(1_000_000),
        input_cached_read: Some(1_000_000),
        output_total: Some(0),
        ..ExtractedUsage::default()
    });

    let priced = price_tokens(&repro_price_book(), Some("openai"), Some("gpt-5.6-sol"), &tokens);

    assert_eq!(priced.amount_micro, Some(400_000), "$0.40/M once, not $4.40/M");
}

/// A provider that reports parts larger than the whole must not underflow the
/// remainder into a charge for 18 quintillion tokens.
// §FS-rhei-cost-accounting.5
#[test]
fn parts_larger_than_the_whole_saturate_instead_of_underflowing() {
    let tokens = tokens_from_usage(ExtractedUsage {
        input_total: Some(100),
        input_cached_read: Some(700),
        output_total: Some(50),
        ..ExtractedUsage::default()
    });

    let priced =
        price_tokens(&repro_price_book(), Some("anthropic"), Some("claude-sonnet-5"), &tokens);

    // Nothing fresh is left: 700 cached at $0.20/M plus 50 output at $10/M.
    assert_eq!(priced.amount_micro, Some(640));
}
