#[derive(Clone, Copy, Debug, Default)]
struct ExtractedUsage {
    total: Option<u64>,
    total_source: Option<&'static str>,
    input_total: Option<u64>,
    input_cached_read: Option<u64>,
    input_cache_write: Option<u64>,
    output_total: Option<u64>,
    output_cached_read: Option<u64>,
    output_cache_write: Option<u64>,
}

impl ExtractedUsage {
    fn merge(&mut self, other: ExtractedUsage) {
        merge_usage_value(&mut self.total, other.total);
        merge_usage_value(&mut self.input_total, other.input_total);
        merge_usage_value(&mut self.input_cached_read, other.input_cached_read);
        merge_usage_value(&mut self.input_cache_write, other.input_cache_write);
        merge_usage_value(&mut self.output_total, other.output_total);
        merge_usage_value(&mut self.output_cached_read, other.output_cached_read);
        merge_usage_value(&mut self.output_cache_write, other.output_cache_write);
    }

    fn has_total(&self) -> bool {
        self.total.is_some() || self.input_total.is_some() || self.output_total.is_some()
    }
}

fn merge_usage_value(target: &mut Option<u64>, value: Option<u64>) {
    if let Some(value) = value {
        *target = Some(target.unwrap_or(0).saturating_add(value));
    }
}

enum ExtractedUsageStatus {
    Measured(ExtractedUsage),
    NoUsageEmitted,
    ExtractorUnavailable,
    ExtractorFailed,
}

/// A capture's existing status plus the reasons retained when that status is
/// `extractor-failed`. Keeping the status enum unchanged preserves every
/// consumer that only needs accounting coverage. §FS-rhei-cost-accounting.4
struct ExtractedUsageResult {
    status: ExtractedUsageStatus,
    extraction_diagnostics: Vec<String>,
}

const MISSING_CLAUDE_RESULT_DIAGNOSTIC: &str =
    "claude-code result envelope is missing required `result` text";
const HISTORICAL_EXTRACTION_FAILURE_DIAGNOSTIC: &str =
    "usage capture reported extractor-failed without a diagnostic";
const INVALID_USAGE_CAPTURE_DIAGNOSTIC: &str = "usage capture contains invalid JSON";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgentUsageExtractor {
    Claude,
    Codex,
    Pi,
}

#[derive(Debug, Deserialize)]
struct ClaudeResultEnvelope {
    #[serde(rename = "type")]
    event_type: String,
    result: String,
    #[serde(default)]
    usage: Option<ClaudeUsage>,
    #[serde(default, rename = "modelUsage")]
    model_usage: Option<BTreeMap<String, ClaudeModelUsage>>,
}

#[derive(Debug, Deserialize)]
struct ClaudeUsage {
    input_tokens: u64,
    cache_read_input_tokens: u64,
    cache_creation_input_tokens: u64,
    output_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct ClaudeModelUsage {
    #[serde(rename = "inputTokens")]
    input_tokens: u64,
    #[serde(rename = "cacheReadInputTokens")]
    cache_read_input_tokens: u64,
    #[serde(rename = "cacheCreationInputTokens")]
    cache_creation_input_tokens: u64,
    #[serde(rename = "outputTokens")]
    output_tokens: u64,
}

#[derive(Debug)]
struct ClaudeResult {
    text: String,
    usage: ExtractedUsage,
}

enum ClaudeResultLine {
    Result(ClaudeResult),
    Unrelated,
    Malformed,
}

enum OutputUsage {
    Measured(ExtractedUsage),
    Ignored,
    Failed,
}

#[derive(Clone, Debug)]
struct AgentUsageCapture {
    extractor: AgentUsageExtractor,
    replace_usage_capture: bool,
    path: PathBuf,
    invocation_id: String,
    task_id: String,
    state: String,
    agent: String,
    provider: Option<String>,
    model: Option<String>,
    price_book: PriceBook,
    slot: rhei_tui::Slot,
    cli_session: Arc<Mutex<Option<AccountingCliSession>>>,
}

struct AgentAccountingInvocation<'a> {
    workspace_root: &'a Path,
    task: &'a rhei_core::ast::Task,
    state: &'a str,
    resolved: &'a ResolvedAgent,
    visit: u64,
    started_at: std::time::SystemTime,
    ended_at: std::time::SystemTime,
    slot: Option<rhei_tui::Slot>,
    usage_capture_path: Option<&'a Path>,
    cli_session: Option<&'a AccountingCliSession>,
    log_path: Option<&'a Path>,
    price_book: &'a PriceBook,
    sink: &'a Arc<dyn rhei_tui::EventSink>,
}

/// The identity facts fixed by the shared spawn plan before the process starts.
/// The durable `run_id` and the id suffix therefore cannot drift apart.
// §FS-rhei-cost-accounting.3.7
#[derive(Clone, Debug)]
struct AccountingAttemptIdentity {
    invocation_id: String,
    run_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum CostGroup {
    Agent,
    Model,
    State,
    Node,
    /// §FS-rhei-cost-accounting.8.3: one group per run, plus the never-omitted
    /// group for the records that name none.
    Run,
    /// §FS-rhei-cost-accounting.8.3: the UTC calendar day of `started_at`.
    Day,
}

/// Finish accounting with the identity the spawn plan created before launch.
// §FS-rhei-cost-accounting.3.7
fn record_agent_accounting_attempt(
    invocation: AgentAccountingInvocation<'_>,
    identity: &AccountingAttemptIdentity,
) -> MietteResult<Option<rhei_tui::UsageSummary>> {
    // §FS-rhei-cost-accounting.3.2: Built-ins must not silently omit records.
    if !agent_has_accounting_extractor(invocation.resolved.agent.id()) {
        return Ok(None);
    }

    // §FS-rhei-cost-accounting.2: Accounting files live under runtime/accounting/.
    let accounting_root = invocation.workspace_root.join("runtime/accounting");
    write_price_book(&accounting_root, invocation.price_book)?;

    // §FS-rhei-cost-accounting.11: Extraction failures affect coverage only.
    let extraction = extract_usage_with_diagnostics(
        invocation.usage_capture_path,
        invocation.log_path,
        invocation.resolved.agent.id(),
    );
    let (tokens, extraction_status) = match extraction.status {
        ExtractedUsageStatus::Measured(usage) => (tokens_from_usage(usage), "measured"),
        ExtractedUsageStatus::NoUsageEmitted => {
            (AccountingTokens::default(), "no-usage-emitted")
        }
        ExtractedUsageStatus::ExtractorUnavailable => {
            (AccountingTokens::default(), "extractor-unavailable")
        }
        ExtractedUsageStatus::ExtractorFailed => {
            (AccountingTokens::default(), "extractor-failed")
        }
    };
    let provider = invocation.resolved.model_provider.clone();
    let model =
        invocation.resolved.model_name.clone().or_else(|| invocation.resolved.model.clone());
    let pricing =
        price_tokens(invocation.price_book, provider.as_deref(), model.as_deref(), &tokens);
    let target_slug = resolved_agent_target_slug(invocation.resolved);
    let record = AccountingInvocationRecord {
        schema: ACCOUNTING_INVOCATION_SCHEMA.to_string(),
        // §FS-rhei-cost-accounting.3.7: use the identity created by the
        // persisted spawn plan rather than recomputing a visit-level key.
        invocation_id: identity.invocation_id.clone(),
        // §FS-rhei-cost-accounting.3.5: every record written inside a run names
        // it, so attributing spend to a run is a fact rather than an inference.
        run_id: identity.run_id.clone(),
        task_id: invocation.task.id.to_string(),
        state: invocation.state.to_string(),
        visit: invocation.visit,
        target_slug,
        agent: invocation.resolved.agent.id().to_string(),
        provider,
        model,
        started_at: format_iso8601_utc(invocation.started_at),
        ended_at: format_iso8601_utc(invocation.ended_at),
        // §FS-rhei-cost-accounting.3.4: New records carry elapsed wall time.
        duration_ms: Some(accounting_duration_ms(invocation.started_at, invocation.ended_at)),
        cli_session: invocation.cli_session.cloned(),
        extraction_status: extraction_status.to_string(),
        // A current failed writer always received either a retained parser
        // reason or the historical-event fallback. §FS-rhei-cost-accounting.4
        extraction_diagnostics: extraction.extraction_diagnostics,
        scope: "aggregate-agent-process".to_string(),
        // §FS-rhei-cost-accounting.3.6: every record Rhei writes says which
        // convention its dimensions follow, so a reader of a mixed archive
        // never has to infer it.
        token_convention: Some(TOKEN_CONVENTION_INCLUDES_CACHE.to_string()),
        tokens,
        pricing,
    };

    write_invocation_record(&accounting_root, &record)?;
    let usage =
        usage_summary_from_record(&record, &ReachablePriceBooks::with_selected(invocation.price_book));
    // §FS-rhei-cost-accounting.7: Emit UsageReported after durable write.
    // §FS-rhei-cost-accounting.7.1: that one report is the invocation's `Final`.
    invocation.sink.emit(rhei_tui::RunEvent::UsageReported {
        slot: invocation.slot,
        task: invocation.task.id.to_string(),
        invocation_id: identity.invocation_id.clone(),
        report: rhei_tui::UsageReport::Final,
        usage: usage.clone(),
    });
    Ok(Some(usage))
}

/// Compatibility entry point for record-writing unit fixtures that exercise
/// extraction independently from the scheduler-owned spawn plan.
// §FS-rhei-cost-accounting.3.7
#[cfg(test)]
fn record_agent_accounting_invocation(
    invocation: AgentAccountingInvocation<'_>,
) -> MietteResult<Option<rhei_tui::UsageSummary>> {
    let identity = AccountingAttemptIdentity {
        invocation_id: accounting_invocation_id(
            &invocation.task.id.to_string(),
            invocation.state,
            invocation.resolved,
            invocation.visit,
        ),
        run_id: current_run_id(),
    };
    record_agent_accounting_attempt(invocation, &identity)
}

/// What one run spent, told apart from what its workspace has ever spent.
///
/// Two different numbers that were one number until now: the strip an operator
/// reads after a run was the whole workspace's lifetime total, presented as
/// what the run had just cost.
// §FS-rhei-run-report.2.1 §FS-rhei-cost-accounting.6
#[derive(Clone, Debug)]
struct RunAccountingRollup {
    /// The records naming this run. Zero-valued when it spawned no agent,
    /// because zero is what such a run spent.
    run: rhei_tui::AccountingRunSummary,
    /// `workspace_total` — the workspace's whole accounting history.
    workspace: Option<rhei_tui::AccountingRunSummary>,
}

/// What a run that spawned no agent spent: nothing, said as a number rather
/// than left blank for the lifetime total to fill.
// §FS-rhei-run-report.2.1
fn zero_run_accounting() -> rhei_tui::AccountingRunSummary {
    let none_spent = rhei_tui::DimensionSummary {
        value: Some(0),
        status: rhei_tui::DimensionStatus::Measured,
        measured_count: 0,
        missing_count: 0,
    };
    rhei_tui::AccountingRunSummary {
        total: none_spent.clone(),
        input_total: none_spent.clone(),
        input_cached_read: none_spent.clone(),
        input_cache_write: none_spent.clone(),
        output_total: none_spent.clone(),
        output_cached_read: none_spent.clone(),
        output_cache_write: none_spent,
        cost_micro: Some(0),
        priced_cost_micro: Some(0),
        currency: Some("USD".to_string()),
        coverage: rhei_tui::UsageCoverage::Complete,
        pricing_status: rhei_tui::PricingStatus::Priced,
        invocation_count: 0,
        measured_invocation_count: 0,
        missing_invocation_count: 0,
    }
}

/// This run's own rollup, over the records that name it.
///
/// It is a run selection like any other, so §FS-rhei-cost-accounting.6.2 holds
/// here too: while the workspace carries records that name no run, one of them
/// may be this run's, and the reading cannot claim to be complete.
// §FS-rhei-run-report.2.1 §FS-rhei-cost-accounting.6.2
fn this_runs_rollup(inspection: &CostInspection) -> rhei_tui::AccountingRunSummary {
    let Some(run_id) = current_run_id() else {
        // Nothing published an identity for this process, so nothing on disk
        // can be attributed to it — and it cannot be sure of that either.
        return demote_if(zero_run_accounting(), true);
    };
    let selection = CostSelection::resolve(Some(&run_id), None, None).unwrap_or_default();
    let selected = selection.apply(inspection.scoped(), inspection.unreadable_root);
    selected
        .summary()
        .unwrap_or_else(|| demote_if(zero_run_accounting(), !selected.unattributed.is_empty()))
}

fn regenerate_accounting_indexes(
    workspace_root: &Path,
    rhei: &rhei_core::ast::Rhei,
) -> MietteResult<Option<RunAccountingRollup>> {
    // §FS-rhei-cost-accounting.6: Task and run rollups are derived indexes.
    let accounting_root = workspace_root.join("runtime/accounting");
    let inspection = read_cost_inspection(&accounting_root);
    if inspection.invocations.is_empty() {
        return Ok(None);
    }
    let task_dir = accounting_root.join("tasks");
    fs::create_dir_all(&task_dir).map_err(|err| {
        file_io_report(&task_dir, "failed to create accounting task index directory", err)
    })?;

    let tasks = flatten_tasks(rhei);
    for task in tasks {
        let task_id = task.id.to_string();
        let direct = summarize_records(
            inspection.scoped().filter(|held| held.record.task_id == task_id),
        );
        let subtree = summarize_records(inspection.scoped().filter(|held| {
            held.record.task_id == task_id || is_descendant_id(&held.record.task_id, &task_id)
        }));
        if direct.is_some() || subtree.is_some() {
            let payload = serde_json::json!({
                "schema": "rhei.accounting.task.v1",
                "task_id": task_id,
                "direct": direct,
                "subtree": subtree,
            });
            let path = task_dir.join(format!("{}.json", safe_accounting_file_segment(&task.id.to_string())));
            write_json_atomic(&path, &payload)?;
        }
    }

    if let Some(summary) = inspection.summary.as_ref() {
        let payload = serde_json::json!({
            "schema": "rhei.accounting.summary.v1",
            "summary": summary,
        });
        write_json_atomic(&accounting_root.join("summary.json"), &payload)?;
    }
    // §FS-rhei-run-report.2.1: the run's own total and the workspace lifetime
    // total travel together, and named apart, so neither can stand in for the
    // other on the strip an operator reads.
    Ok(Some(RunAccountingRollup {
        run: this_runs_rollup(&inspection),
        workspace: inspection.summary,
    }))
}

fn summarize_records<'a>(
    records: impl IntoIterator<Item = ScopedRecord<'a>>,
) -> Option<rhei_tui::AccountingRunSummary> {
    // §FS-rhei-cost-accounting.6: Rollups summarize invocation records. Each is
    // read against its own root's books. §FS-rhei-cost-accounting.5.2
    let readings: Vec<RecordReading> = records
        .into_iter()
        .map(|held| read_stored_record(held.record, held.books))
        .collect();
    let usages: Vec<rhei_tui::UsageSummary> =
        readings.iter().map(usage_summary_from_reading).collect();
    let summary = rhei_tui::summarize_usage_summaries(usages.iter())?;
    // A record whose convention could not be established is a doubt the
    // aggregate carries, the way an unattributed record is.
    // §FS-rhei-cost-accounting.6.2
    Some(demote_if(summary, readings.iter().any(|reading| reading.convention_unknown)))
}

/// One stored record as a reading: every surface that reports a stored record —
/// `rhei cost`, `rhei summary`, the run report, the rollups, the dashboard —
/// comes through here, so this is where a record written before the convention
/// existed is read into it. §FS-rhei-cost-accounting.5.2
fn usage_summary_from_record(
    record: &AccountingInvocationRecord,
    books: &ReachablePriceBooks,
) -> rhei_tui::UsageSummary {
    usage_summary_from_reading(&read_stored_record(record, books))
}

fn usage_summary_from_reading(reading: &RecordReading<'_>) -> rhei_tui::UsageSummary {
    let record = reading.record;
    let tokens = &reading.tokens;
    let pricing = &reading.pricing;
    // §FS-rhei-cost-accounting.7: UsageSummary mirrors invocation data.
    let status = match record.extraction_status.as_str() {
        "measured" => rhei_tui::UsageStatus::Measured,
        "unsupported-agent" => rhei_tui::UsageStatus::UnsupportedAgent,
        "extractor-unavailable" => rhei_tui::UsageStatus::ExtractorUnavailable,
        "extractor-failed" => rhei_tui::UsageStatus::ExtractorFailed,
        _ => rhei_tui::UsageStatus::NoUsageEmitted,
    };
    let pricing_status = match pricing.status.as_str() {
        "priced" => rhei_tui::PricingStatus::Priced,
        "partial-price" => rhei_tui::PricingStatus::PartialPrice,
        "unpriced" => rhei_tui::PricingStatus::Unpriced,
        _ => rhei_tui::PricingStatus::NotApplicable,
    };
    let coverage = usage_coverage(status, pricing_status);
    rhei_tui::UsageSummary {
        invocation_id: record.invocation_id.clone(),
        state: record.state.clone(),
        agent: record.agent.clone(),
        provider: record.provider.clone(),
        model: record.model.clone(),
        total: dimension_summary(&tokens.total),
        input_total: dimension_summary(&tokens.input.total),
        input_cached_read: dimension_summary(&tokens.input.cached_read),
        input_cache_write: dimension_summary(&tokens.input.cache_write),
        output_total: dimension_summary(&tokens.output.total),
        output_cached_read: dimension_summary(&tokens.output.cached_read),
        output_cache_write: dimension_summary(&tokens.output.cache_write),
        cost_micro: pricing.amount_micro,
        priced_cost_micro: pricing.priced_amount_micro.or(pricing.amount_micro),
        currency: pricing.currency.clone(),
        coverage,
        status,
        pricing_status,
    }
}

fn usage_summary_from_extracted_usage(
    invocation_id: &str,
    state: &str,
    agent: &str,
    provider: Option<String>,
    model: Option<String>,
    usage: ExtractedUsage,
    price_book: &PriceBook,
) -> rhei_tui::UsageSummary {
    let tokens = tokens_from_usage(usage);
    let pricing = price_tokens(price_book, provider.as_deref(), model.as_deref(), &tokens);
    let pricing_status = match pricing.status.as_str() {
        "priced" => rhei_tui::PricingStatus::Priced,
        "partial-price" => rhei_tui::PricingStatus::PartialPrice,
        "unpriced" => rhei_tui::PricingStatus::Unpriced,
        _ => rhei_tui::PricingStatus::NotApplicable,
    };
    let status = rhei_tui::UsageStatus::Measured;
    rhei_tui::UsageSummary {
        invocation_id: invocation_id.to_string(),
        state: state.to_string(),
        agent: agent.to_string(),
        provider,
        model,
        total: dimension_summary(&tokens.total),
        input_total: dimension_summary(&tokens.input.total),
        input_cached_read: dimension_summary(&tokens.input.cached_read),
        input_cache_write: dimension_summary(&tokens.input.cache_write),
        output_total: dimension_summary(&tokens.output.total),
        output_cached_read: dimension_summary(&tokens.output.cached_read),
        output_cache_write: dimension_summary(&tokens.output.cache_write),
        cost_micro: pricing.amount_micro,
        priced_cost_micro: pricing.priced_amount_micro.or(pricing.amount_micro),
        currency: pricing.currency,
        coverage: usage_coverage(status, pricing_status),
        status,
        pricing_status,
    }
}

fn usage_coverage(
    status: rhei_tui::UsageStatus,
    pricing_status: rhei_tui::PricingStatus,
) -> rhei_tui::UsageCoverage {
    if status != rhei_tui::UsageStatus::Measured {
        return rhei_tui::UsageCoverage::None;
    }
    match pricing_status {
        rhei_tui::PricingStatus::Priced => rhei_tui::UsageCoverage::Complete,
        rhei_tui::PricingStatus::PartialPrice => rhei_tui::UsageCoverage::Partial,
        rhei_tui::PricingStatus::Unpriced => rhei_tui::UsageCoverage::Unpriced,
        rhei_tui::PricingStatus::NotApplicable => rhei_tui::UsageCoverage::None,
    }
}

fn dimension_summary(dimension: &AccountingTokenDimension) -> rhei_tui::DimensionSummary {
    // §FS-rhei-cost-accounting.3.1: Dimension status distinguishes absence.
    if let Some(value) = dimension.value {
        return rhei_tui::DimensionSummary {
            value: Some(value),
            status: rhei_tui::DimensionStatus::Measured,
            measured_count: 1,
            missing_count: 0,
        };
    }
    let status = match dimension.status.as_deref() {
        Some("unsupported") => rhei_tui::DimensionStatus::Unsupported,
        Some("omitted") => rhei_tui::DimensionStatus::Omitted,
        _ => rhei_tui::DimensionStatus::Unknown,
    };
    rhei_tui::DimensionSummary { value: None, status, measured_count: 0, missing_count: 1 }
}

fn agent_has_accounting_extractor(agent: &str) -> bool {
    // §FS-rhei-cost-accounting.4: v1 supports claude-code, codex, and pi.
    agent_usage_extractor(agent).is_some()
}

fn agent_usage_extractor(agent: &str) -> Option<AgentUsageExtractor> {
    match agent {
        "codex" => Some(AgentUsageExtractor::Codex),
        "pi" => Some(AgentUsageExtractor::Pi),
        "claude-code" => Some(AgentUsageExtractor::Claude),
        _ => None,
    }
}

fn accounting_capture_path_for_spawn(
    runtime_dir: &Path,
    task_id: &str,
    state_name: &str,
    resolved: &ResolvedAgent,
) -> Option<PathBuf> {
    if !agent_has_accounting_extractor(resolved.agent.id()) {
        return None;
    }
    let target = resolved_agent_target_slug(resolved).unwrap_or_else(|| resolved.agent.id().to_string());
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let sequence = ACCOUNTING_INVOCATION_FILE_SEQUENCE
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Some(runtime_dir.join("accounting/captures").join(format!(
        "{}-{}-{}-{}-{}.jsonl",
        safe_accounting_file_segment(task_id),
        safe_accounting_file_segment(state_name),
        safe_accounting_file_segment(&target),
        millis,
        sequence
    )))
}

fn accounting_invocation_id(
    task_id: &str,
    state: &str,
    resolved: &ResolvedAgent,
    visit: u64,
) -> String {
    let target_slug = resolved_agent_target_slug(resolved);
    format!(
        "{}::{}::{}::visit-{}",
        safe_accounting_file_segment(task_id),
        safe_accounting_file_segment(state),
        safe_accounting_file_segment(target_slug.as_deref().unwrap_or(resolved.agent.id())),
        visit
    )
}

/// The opaque identity of one attempt, built from the run and the persisted
/// spawn-plan facts rather than a second retry counter.
// §FS-rhei-cost-accounting.3.7 §FS-rhei-cost-accounting.8.1
fn accounting_attempt_invocation_id(
    task_id: &str,
    state: &str,
    resolved: &ResolvedAgent,
    visit: u64,
    run_id: &str,
    moves: u64,
    attempt: u64,
) -> String {
    format!(
        "{}::run-{}::move-{moves}::attempt-{attempt}",
        accounting_invocation_id(task_id, state, resolved, visit),
        safe_accounting_file_segment(run_id),
    )
}

/// Configure streamed usage with the spawn plan's already-created identity.
// §FS-rhei-cost-accounting.3.7 §FS-rhei-cost-accounting.7.1
fn usage_capture_for_attempt(
    resolved: &ResolvedAgent,
    capture_path: Option<&Path>,
    identity: &AccountingAttemptIdentity,
    task_id: &str,
    state: &str,
    slot: rhei_tui::Slot,
    price_book: &PriceBook,
) -> Option<AgentUsageCapture> {
    let extractor = agent_usage_extractor(resolved.agent.id())?;
    Some(AgentUsageCapture {
        extractor,
        replace_usage_capture: resolved.agent.id() == "claude-code"
            && agent_stdin_format(resolved) == AgentStdinFormat::ClaudeCodeStreamJson,
        path: capture_path?.to_path_buf(),
        // §FS-rhei-cost-accounting.3.7: streamed usage keeps the identity
        // created before spawn unchanged.
        invocation_id: identity.invocation_id.clone(),
        task_id: task_id.to_string(),
        state: state.to_string(),
        agent: resolved.agent.id().to_string(),
        provider: resolved.model_provider.clone(),
        model: resolved.model_name.clone().or_else(|| resolved.model.clone()),
        price_book: price_book.clone(),
        slot,
        cli_session: Arc::new(Mutex::new(None)),
    })
}

#[cfg(test)]
// §FS-rhei-cost-accounting.3.7
fn usage_capture_for_spawn(
    resolved: &ResolvedAgent,
    capture_path: Option<&Path>,
    task_id: &str,
    state: &str,
    visit: u64,
    slot: rhei_tui::Slot,
    price_book: &PriceBook,
) -> Option<AgentUsageCapture> {
    let identity = AccountingAttemptIdentity {
        invocation_id: accounting_invocation_id(task_id, state, resolved, visit),
        run_id: current_run_id(),
    };
    usage_capture_for_attempt(
        resolved,
        capture_path,
        &identity,
        task_id,
        state,
        slot,
        price_book,
    )
}

fn configure_agent_accounting_args(cmd: &mut std::process::Command, resolved: &ResolvedAgent) {
    match agent_usage_extractor(resolved.agent.id()) {
        // §FS-rhei-cost-accounting.4: Ordinary Claude output is a typed JSON result.
        Some(AgentUsageExtractor::Claude)
            if agent_stdin_format(resolved) != AgentStdinFormat::ClaudeCodeStreamJson =>
        {
            cmd.args(["--output-format", "json"]);
        }
        Some(AgentUsageExtractor::Codex) => {
            cmd.arg("--json");
        }
        Some(AgentUsageExtractor::Pi) => {
            cmd.args(["--mode", "json"]);
        }
        _ => {}
    }
}

fn configure_accounting_capture(cmd: &mut std::process::Command, capture_path: Option<&Path>) {
    if let Some(path) = capture_path {
        // §FS-rhei-cost-accounting.4: Declare the structured usage capture path before spawn.
        cmd.env("RHEI_ACCOUNTING_USAGE_PATH", path);
        cmd.env("RHEI_ACCOUNTING_USAGE_SCHEMA", ACCOUNTING_USAGE_EVENT_SCHEMA);
    }
}

fn capture_agent_output_usage(
    capture: Option<&AgentUsageCapture>,
    stream: rhei_tui::AgentStream,
    line: &str,
    sink: &Arc<dyn rhei_tui::EventSink>,
) {
    let Some(capture) = capture else { return };
    if stream != rhei_tui::AgentStream::Stdout {
        return;
    }
    capture_cli_session_from_output(capture, line);
    let usage = match extract_usage_from_output_line(capture.extractor, line) {
        OutputUsage::Measured(usage) => usage,
        OutputUsage::Ignored => return,
        OutputUsage::Failed => {
            let diagnostic = extraction_failure_diagnostic(capture.extractor, line);
            let _ = append_extractor_failure_event_with_diagnostic(&capture.path, diagnostic);
            return;
        }
    };
    if append_usage_capture_event(&capture.path, usage, capture.replace_usage_capture).is_err() {
        return;
    }
    if let ExtractedUsageStatus::Measured(aggregate) = extract_usage_from_capture(Some(&capture.path))
    {
        let usage = usage_summary_from_extracted_usage(
            &capture.invocation_id,
            &capture.state,
            &capture.agent,
            capture.provider.clone(),
            capture.model.clone(),
            aggregate,
            &capture.price_book,
        );
        // §FS-rhei-cost-accounting.7.1: a running total re-summed from the whole
        // capture while the agent still runs, so it is `Streamed`, not the cost.
        sink.emit(rhei_tui::RunEvent::UsageReported {
            slot: Some(capture.slot),
            task: capture.task_id.clone(),
            invocation_id: capture.invocation_id.clone(),
            report: rhei_tui::UsageReport::Streamed,
            usage,
        });
    }
}

enum AgentOutputLine {
    Passthrough,
    Replace(String),
    Suppress,
}

fn display_agent_output_line(
    capture: Option<&AgentUsageCapture>,
    stream: rhei_tui::AgentStream,
    line: &str,
) -> Option<String> {
    if stream == rhei_tui::AgentStream::Stdout {
        if let Some(capture) = capture {
            return match display_output_line(capture.extractor, line) {
                AgentOutputLine::Passthrough => Some(line.to_string()),
                AgentOutputLine::Replace(display) => Some(display),
                AgentOutputLine::Suppress => None,
            };
        }
    }
    Some(line.to_string())
}

// §FS-rhei-run-tui.1.2: decoded Claude results remain line-oriented live
// traffic, including internal blank lines.
fn agent_output_lines(line: String, split_logical_lines: bool) -> Vec<String> {
    if !split_logical_lines {
        return vec![line];
    }
    match line.is_empty() {
        true => vec![String::new()],
        false => line.lines().map(str::to_owned).collect(),
    }
}

fn extract_usage_from_output_line(
    extractor: AgentUsageExtractor,
    line: &str,
) -> OutputUsage {
    match extractor {
        AgentUsageExtractor::Claude => match parse_claude_result_line(line) {
            ClaudeResultLine::Result(ClaudeResult { usage, .. }) => OutputUsage::Measured(usage),
            ClaudeResultLine::Unrelated => OutputUsage::Ignored,
            ClaudeResultLine::Malformed => OutputUsage::Failed,
        },
        AgentUsageExtractor::Codex => extract_codex_json_usage(line)
            .map(OutputUsage::Measured)
            .unwrap_or(OutputUsage::Ignored),
        AgentUsageExtractor::Pi => extract_pi_json_usage(line)
            .map(OutputUsage::Measured)
            .unwrap_or(OutputUsage::Ignored),
    }
}

/// Explain a structured parser rejection without copying the rejected output.
/// The fixed vocabulary is stable, single-line, and safely below the durable
/// 240-character bound. §FS-rhei-cost-accounting.4
fn extraction_failure_diagnostic(extractor: AgentUsageExtractor, line: &str) -> &'static str {
    match extractor {
        AgentUsageExtractor::Claude => {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                return "claude-code output line is not valid JSON";
            };
            if value.get("result").and_then(serde_json::Value::as_str).is_none() {
                MISSING_CLAUDE_RESULT_DIAGNOSTIC
            } else {
                "claude-code result envelope has invalid or missing usage"
            }
        }
        AgentUsageExtractor::Codex => "codex structured usage event could not be parsed",
        AgentUsageExtractor::Pi => "pi structured usage event could not be parsed",
    }
}

fn display_output_line(extractor: AgentUsageExtractor, line: &str) -> AgentOutputLine {
    match extractor {
        AgentUsageExtractor::Claude => match parse_claude_result_line(line) {
            ClaudeResultLine::Result(ClaudeResult { text, .. }) => AgentOutputLine::Replace(text),
            ClaudeResultLine::Unrelated | ClaudeResultLine::Malformed => {
                AgentOutputLine::Passthrough
            }
        },
        AgentUsageExtractor::Codex => display_codex_json_line(line)
            .map(AgentOutputLine::Replace)
            .unwrap_or(AgentOutputLine::Passthrough),
        AgentUsageExtractor::Pi => display_pi_json_line(line),
    }
}

fn parse_claude_result_line(line: &str) -> ClaudeResultLine {
    let value = match serde_json::from_str::<serde_json::Value>(line) {
        Ok(value) => value,
        Err(_) => return ClaudeResultLine::Malformed,
    };
    let Some(object) = value.as_object() else {
        return ClaudeResultLine::Unrelated;
    };
    if object.get("type").and_then(serde_json::Value::as_str) != Some("result") {
        return ClaudeResultLine::Unrelated;
    }
    let envelope = match serde_json::from_value::<ClaudeResultEnvelope>(value) {
        Ok(envelope) if envelope.event_type == "result" => envelope,
        _ => return ClaudeResultLine::Malformed,
    };
    let usage = envelope
        .usage
        .as_ref()
        .map(claude_usage_from_usage)
        .or_else(|| envelope.model_usage.as_ref().and_then(claude_usage_from_models));
    let Some(usage) = usage else {
        return ClaudeResultLine::Malformed;
    };
    ClaudeResultLine::Result(ClaudeResult {
        text: envelope.result,
        usage,
    })
}

/// Fold a provider's cache dimensions into `input.total`, for the providers
/// that report an input count beside them rather than around them.
///
/// Anthropic's `input_tokens` and Pi's `input` exclude their cache dimensions;
/// OpenAI's `input_tokens` already contains them. The record states one
/// convention whatever the provider, and the conversion belongs here, at
/// extraction, so nothing downstream has to know which agent wrote it.
// §FS-rhei-cost-accounting.3.1 §FS-rhei-cost-accounting.4
fn include_cache_in_input_total(usage: &mut ExtractedUsage) {
    let Some(input_total) = usage.input_total else {
        return;
    };
    usage.input_total = Some(
        input_total
            .saturating_add(usage.input_cached_read.unwrap_or(0))
            .saturating_add(usage.input_cache_write.unwrap_or(0)),
    );
}

fn claude_usage_from_usage(usage: &ClaudeUsage) -> ExtractedUsage {
    let mut extracted = ExtractedUsage {
        input_total: Some(usage.input_tokens),
        input_cached_read: Some(usage.cache_read_input_tokens),
        input_cache_write: Some(usage.cache_creation_input_tokens),
        output_total: Some(usage.output_tokens),
        ..ExtractedUsage::default()
    };
    include_cache_in_input_total(&mut extracted);
    extracted
}

fn claude_usage_from_models(
    models: &BTreeMap<String, ClaudeModelUsage>,
) -> Option<ExtractedUsage> {
    let mut extracted = ExtractedUsage::default();
    for usage in models.values() {
        extracted.merge(ExtractedUsage {
            input_total: Some(usage.input_tokens),
            input_cached_read: Some(usage.cache_read_input_tokens),
            input_cache_write: Some(usage.cache_creation_input_tokens),
            output_total: Some(usage.output_tokens),
            ..ExtractedUsage::default()
        });
    }
    // Summing per model and converting once is the same arithmetic as
    // converting each model's usage and summing. §FS-rhei-cost-accounting.3.1
    include_cache_in_input_total(&mut extracted);
    extracted.has_total().then_some(extracted)
}

fn extract_codex_json_usage(line: &str) -> Option<ExtractedUsage> {
    let value = serde_json::from_str::<serde_json::Value>(line).ok()?;
    let object = value.as_object()?;
    if object.get("type").and_then(serde_json::Value::as_str) != Some("turn.completed") {
        return None;
    }
    object.get("usage").and_then(usage_from_json_payload)
}

fn extract_pi_json_usage(line: &str) -> Option<ExtractedUsage> {
    let value = serde_json::from_str::<serde_json::Value>(line).ok()?;
    extract_pi_usage_from_value(&value)
}

fn extract_pi_usage_from_value(value: &serde_json::Value) -> Option<ExtractedUsage> {
    let object = value.as_object()?;
    if object.get("type").and_then(serde_json::Value::as_str) != Some("message_end") {
        return None;
    }
    let message = object.get("message")?.as_object()?;
    if message.get("role").and_then(serde_json::Value::as_str) != Some("assistant") {
        return None;
    }
    let usage = message.get("usage")?.as_object()?;
    let mut extracted = ExtractedUsage {
        // Pi's own aggregate is `input + cacheRead + cacheWrite + output`, so
        // it already counts every token and is taken as reported; its `input`
        // is what has to grow. §FS-rhei-cost-accounting.3.6
        total: dimension_u64(usage.get("totalTokens")),
        input_total: dimension_u64(usage.get("input")),
        input_cached_read: dimension_u64(usage.get("cacheRead")),
        input_cache_write: dimension_u64(usage.get("cacheWrite")),
        output_total: dimension_u64(usage.get("output")),
        ..ExtractedUsage::default()
    };
    include_cache_in_input_total(&mut extracted);
    extracted.has_total().then_some(extracted)
}

fn display_pi_json_line(line: &str) -> AgentOutputLine {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return AgentOutputLine::Passthrough;
    };
    let Some(object) = value.as_object() else {
        return AgentOutputLine::Passthrough;
    };
    let Some(event_type) = object.get("type").and_then(serde_json::Value::as_str) else {
        return AgentOutputLine::Passthrough;
    };
    match event_type {
        "session" => object
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(|id| AgentOutputLine::Replace(format!("pi session started: {id}")))
            .unwrap_or(AgentOutputLine::Suppress),
        "agent_start" => AgentOutputLine::Replace("pi agent started".to_string()),
        "agent_end" => AgentOutputLine::Replace("pi agent completed".to_string()),
        "message_end" => display_pi_message_end(&value),
        _ => AgentOutputLine::Suppress,
    }
}

fn display_pi_message_end(value: &serde_json::Value) -> AgentOutputLine {
    let Some(message) = value
        .get("message")
        .and_then(serde_json::Value::as_object)
    else {
        return AgentOutputLine::Suppress;
    };
    if message.get("role").and_then(serde_json::Value::as_str) != Some("assistant") {
        return AgentOutputLine::Suppress;
    }
    let mut display = Vec::new();
    if let Some(content) = message.get("content").and_then(serde_json::Value::as_array) {
        let text = content
            .iter()
            .filter_map(serde_json::Value::as_object)
            .filter(|item| item.get("type").and_then(serde_json::Value::as_str) == Some("text"))
            .filter_map(|item| item.get("text").and_then(serde_json::Value::as_str))
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            display.push(text);
        }
    }
    if let Some(error) = message.get("errorMessage").and_then(serde_json::Value::as_str) {
        display.push(format!("pi error: {error}"));
    }
    if let Some(usage) = extract_pi_usage_from_value(value) {
        display.push(format!(
            "pi message completed: total={} input={} cached_input={} cache_write_input={} output={}",
            usage
                .total
                .or_else(|| sum_optional_pair(usage.input_total, usage.output_total))
                .map(format_plain_u64)
                .unwrap_or_else(|| "-".to_string()),
            usage.input_total.map(format_plain_u64).unwrap_or_else(|| "-".to_string()),
            usage
                .input_cached_read
                .map(format_plain_u64)
                .unwrap_or_else(|| "-".to_string()),
            usage
                .input_cache_write
                .map(format_plain_u64)
                .unwrap_or_else(|| "-".to_string()),
            usage.output_total.map(format_plain_u64).unwrap_or_else(|| "-".to_string()),
        ));
    }
    if display.is_empty() {
        AgentOutputLine::Suppress
    } else {
        AgentOutputLine::Replace(display.join("\n"))
    }
}

fn display_codex_json_line(line: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(line).ok()?;
    let object = value.as_object()?;
    match object.get("type").and_then(serde_json::Value::as_str)? {
        "thread.started" => object
            .get("thread_id")
            .and_then(serde_json::Value::as_str)
            .map(|id| format!("codex thread started: {id}")),
        "turn.started" => Some("codex turn started".to_string()),
        "turn.completed" => extract_codex_json_usage(line).map(|usage| {
            format!(
                "codex turn completed: total={} input={} cached_input={} output={}",
                usage
                    .total
                    .or_else(|| sum_optional_pair(usage.input_total, usage.output_total))
                    .map(format_plain_u64)
                    .unwrap_or_else(|| "-".to_string()),
                usage.input_total.map(format_plain_u64).unwrap_or_else(|| "-".to_string()),
                usage.input_cached_read
                    .map(format_plain_u64)
                    .unwrap_or_else(|| "-".to_string()),
                usage.output_total.map(format_plain_u64).unwrap_or_else(|| "-".to_string()),
            )
        }),
        "item.completed" => object
            .get("item")
            .and_then(serde_json::Value::as_object)
            .and_then(|item| {
                if item.get("type").and_then(serde_json::Value::as_str) == Some("agent_message") {
                    item.get("text").and_then(serde_json::Value::as_str).map(str::to_string)
                } else {
                    None
                }
            }),
        "error" => object
            .get("message")
            .and_then(serde_json::Value::as_str)
            .map(|message| format!("codex error: {message}")),
        _ => None,
    }
}

fn format_plain_u64(value: u64) -> String {
    value.to_string()
}

fn append_usage_capture_event(
    path: &Path,
    usage: ExtractedUsage,
    replace: bool,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut usage_object = serde_json::Map::new();
    if let Some(value) = usage.total {
        usage_object.insert("total_tokens".to_string(), serde_json::Value::from(value));
    }
    if let Some(value) = usage.input_total {
        usage_object.insert("input_tokens".to_string(), serde_json::Value::from(value));
    }
    if let Some(value) = usage.input_cached_read {
        usage_object.insert("cached_input_tokens".to_string(), serde_json::Value::from(value));
    }
    if let Some(value) = usage.input_cache_write {
        usage_object.insert("cache_write_input_tokens".to_string(), serde_json::Value::from(value));
    }
    if let Some(value) = usage.output_total {
        usage_object.insert("output_tokens".to_string(), serde_json::Value::from(value));
    }
    if let Some(value) = usage.output_cached_read {
        usage_object.insert("cached_output_tokens".to_string(), serde_json::Value::from(value));
    }
    if let Some(value) = usage.output_cache_write {
        usage_object.insert("output_cache_write".to_string(), serde_json::Value::from(value));
    }
    let event = serde_json::json!({
        "schema": ACCOUNTING_USAGE_EVENT_SCHEMA,
        "usage": serde_json::Value::Object(usage_object),
    });
    let mut options = fs::OpenOptions::new();
    options.create(true).write(true);
    if replace {
        options.truncate(true);
    } else {
        options.append(true);
    }
    let mut file = options.open(path)?;
    writeln!(file, "{}", event)
}

#[cfg(test)]
fn append_extractor_failure_event(path: &Path) -> std::io::Result<()> {
    append_extractor_failure_event_value(path, None)
}

fn append_extractor_failure_event_with_diagnostic(
    path: &Path,
    diagnostic: &str,
) -> std::io::Result<()> {
    append_extractor_failure_event_value(path, Some(diagnostic))
}

/// Current writers include the parser reason; accepting `None` keeps the
/// historical event shape available to compatibility readers and fixtures.
/// §FS-rhei-cost-accounting.4
fn append_extractor_failure_event_value(
    path: &Path,
    diagnostic: Option<&str>,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut event = serde_json::json!({
        "schema": ACCOUNTING_USAGE_EVENT_SCHEMA,
        "status": "extractor-failed",
    });
    if let Some(diagnostic) = diagnostic {
        event["diagnostic"] = serde_json::Value::String(normalize_extraction_diagnostic(
            Some(diagnostic),
        ));
    }
    let mut file = fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", event)
}

#[cfg(test)]
fn extract_usage(
    capture_path: Option<&Path>,
    log_path: Option<&Path>,
    agent: &str,
) -> ExtractedUsageStatus {
    extract_usage_with_diagnostics(capture_path, log_path, agent).status
}

fn extract_usage_with_diagnostics(
    capture_path: Option<&Path>,
    log_path: Option<&Path>,
    agent: &str,
) -> ExtractedUsageResult {
    let captured = extract_usage_from_capture_with_diagnostics(capture_path);
    match &captured.status {
        ExtractedUsageStatus::NoUsageEmitted => {
            // §FS-rhei-cost-accounting.4: Claude usage comes only from its
            // typed result envelope, never from human-readable log text.
            if agent == "claude-code" {
                captured
            } else {
                ExtractedUsageResult {
                    status: extract_usage_from_agent_log(log_path)
                        .unwrap_or(ExtractedUsageStatus::NoUsageEmitted),
                    extraction_diagnostics: Vec::new(),
                }
            }
        }
        _ => captured,
    }
}

fn extract_usage_from_capture(capture_path: Option<&Path>) -> ExtractedUsageStatus {
    extract_usage_from_capture_with_diagnostics(capture_path).status
}

/// Aggregate usage and retain distinct failure reasons in first-seen order.
/// A failure still outranks measurements, while replacement happens earlier
/// when the cumulative provider event truncates the capture. §FS-rhei-cost-accounting.4
fn extract_usage_from_capture_with_diagnostics(
    capture_path: Option<&Path>,
) -> ExtractedUsageResult {
    // §FS-rhei-cost-accounting.4: Only Rhei-declared structured usage events are accepted.
    let Some(capture_path) = capture_path else {
        return ExtractedUsageResult {
            status: ExtractedUsageStatus::ExtractorUnavailable,
            extraction_diagnostics: Vec::new(),
        };
    };
    if !capture_path.is_file() {
        return ExtractedUsageResult {
            status: ExtractedUsageStatus::NoUsageEmitted,
            extraction_diagnostics: Vec::new(),
        };
    }
    let Ok(text) = fs::read_to_string(capture_path) else {
        return ExtractedUsageResult {
            status: ExtractedUsageStatus::ExtractorUnavailable,
            extraction_diagnostics: Vec::new(),
        };
    };
    let mut aggregate = ExtractedUsage::default();
    let mut saw = false;
    let mut extraction_diagnostics = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            continue;
        }
        let value = match serde_json::from_str::<serde_json::Value>(line) {
            Ok(value) => value,
            Err(_) => {
                push_distinct_extraction_diagnostic(
                    &mut extraction_diagnostics,
                    INVALID_USAGE_CAPTURE_DIAGNOSTIC,
                );
                continue;
            }
        };
        if value.get("schema").and_then(serde_json::Value::as_str)
            == Some(ACCOUNTING_USAGE_EVENT_SCHEMA)
            && value.get("status").and_then(serde_json::Value::as_str)
                == Some("extractor-failed")
        {
            let diagnostic = normalize_extraction_diagnostic(
                value.get("diagnostic").and_then(serde_json::Value::as_str),
            );
            push_distinct_extraction_diagnostic(&mut extraction_diagnostics, &diagnostic);
            continue;
        }
        if let Some(usage) = usage_from_structured_event_value(&value) {
            aggregate.merge(usage);
            saw = true;
        }
    }
    if !extraction_diagnostics.is_empty() {
        return ExtractedUsageResult {
            status: ExtractedUsageStatus::ExtractorFailed,
            extraction_diagnostics,
        };
    }
    let status = if saw && aggregate.has_total() {
        ExtractedUsageStatus::Measured(aggregate)
    } else {
        ExtractedUsageStatus::NoUsageEmitted
    };
    ExtractedUsageResult { status, extraction_diagnostics }
}

fn push_distinct_extraction_diagnostic(diagnostics: &mut Vec<String>, diagnostic: &str) {
    if !diagnostics.iter().any(|retained| retained == diagnostic) {
        diagnostics.push(diagnostic.to_string());
    }
}

/// Constrain a retained event reason to the published safe string shape. An
/// absent or empty historical reason receives one stable factual fallback.
/// §FS-rhei-cost-accounting.4
fn normalize_extraction_diagnostic(diagnostic: Option<&str>) -> String {
    let diagnostic = diagnostic.unwrap_or(HISTORICAL_EXTRACTION_FAILURE_DIAGNOSTIC);
    let single_line = diagnostic.split_whitespace().collect::<Vec<_>>().join(" ");
    let source = if single_line.is_empty() {
        HISTORICAL_EXTRACTION_FAILURE_DIAGNOSTIC
    } else {
        &single_line
    };
    source.chars().take(240).collect()
}

fn extract_usage_from_agent_log(log_path: Option<&Path>) -> Option<ExtractedUsageStatus> {
    let log_path = log_path?;
    let text = fs::read_to_string(log_path).ok()?;
    parse_codex_total_tokens_from_log(&text).map(|total| {
        ExtractedUsageStatus::Measured(ExtractedUsage {
            total: Some(total),
            total_source: Some("agent-log-total"),
            ..ExtractedUsage::default()
        })
    })
}

fn parse_codex_total_tokens_from_log(text: &str) -> Option<u64> {
    let mut lines = text.lines();
    let mut last_total = None;
    while let Some(line) = lines.next() {
        if line.trim().eq_ignore_ascii_case("tokens used") {
            if let Some(value_line) = lines.next() {
                if let Some(value) = parse_token_count(value_line.trim()) {
                    last_total = Some(value);
                }
            }
        }
    }
    last_total
}

fn parse_token_count(text: &str) -> Option<u64> {
    let compact: String = text.chars().filter(|ch| ch.is_ascii_digit()).collect();
    if compact.is_empty() {
        None
    } else {
        compact.parse().ok()
    }
}

fn usage_from_structured_event_value(value: &serde_json::Value) -> Option<ExtractedUsage> {
    let object = value.as_object()?;
    let schema = object.get("schema").and_then(serde_json::Value::as_str)?;
    if schema != ACCOUNTING_USAGE_EVENT_SCHEMA {
        return None;
    }
    object
        .get("usage")
        .and_then(usage_from_json_payload)
        .or_else(|| usage_from_json_payload(value))
}

/// The shapes whose `input_tokens` already counts the cache dimensions inside
/// it: OpenAI's `turn.completed.usage`, and Rhei's own
/// `rhei.accounting.usage.v1` capture event, which states its dimensions in
/// §FS-rhei-cost-accounting.3.1's convention. Both paths meet here, and neither
/// is converted — converting a capture event would count its cached reads
/// twice. §FS-rhei-cost-accounting.4
fn usage_from_json_payload(value: &serde_json::Value) -> Option<ExtractedUsage> {
    let object = value.as_object()?;
    for key in ["usage", "token_usage", "tokens", "metrics"] {
        if let Some(nested) = object.get(key).and_then(usage_from_json_payload) {
            return Some(nested);
        }
    }

    let mut usage = ExtractedUsage {
        total: first_u64(object, &["total_tokens", "tokens_used", "total"]),
        total_source: None,
        input_total: first_u64(
            object,
            &[
                "input_tokens",
                "prompt_tokens",
                "input_total",
                "total_input_tokens",
            ],
        ),
        output_total: first_u64(
            object,
            &[
                "output_tokens",
                "completion_tokens",
                "output_total",
                "total_output_tokens",
            ],
        ),
        input_cached_read: first_u64(
            object,
            &[
                "cache_read_input_tokens",
                "cached_input_tokens",
                "input_cached_read",
            ],
        ),
        input_cache_write: first_u64(
            object,
            &[
                "cache_creation_input_tokens",
                "cache_write_input_tokens",
                "input_cache_write",
            ],
        ),
        output_cached_read: first_u64(
            object,
            &["output_cached_read", "cached_output_tokens"],
        ),
        output_cache_write: first_u64(object, &["output_cache_write"]),
    };

    if let Some(input) = object.get("input").and_then(serde_json::Value::as_object) {
        usage.input_total = usage.input_total.or_else(|| dimension_u64(input.get("total")));
        usage.input_cached_read =
            usage.input_cached_read.or_else(|| dimension_u64(input.get("cached_read")));
        usage.input_cache_write =
            usage.input_cache_write.or_else(|| dimension_u64(input.get("cache_write")));
    }
    if let Some(output) = object.get("output").and_then(serde_json::Value::as_object) {
        usage.output_total = usage.output_total.or_else(|| dimension_u64(output.get("total")));
        usage.output_cached_read =
            usage.output_cached_read.or_else(|| dimension_u64(output.get("cached_read")));
        usage.output_cache_write =
            usage.output_cache_write.or_else(|| dimension_u64(output.get("cache_write")));
    }

    usage.has_total().then_some(usage)
}

fn first_u64(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<u64> {
    keys.iter().find_map(|key| dimension_u64(object.get(*key)))
}

fn dimension_u64(value: Option<&serde_json::Value>) -> Option<u64> {
    match value? {
        serde_json::Value::Number(number) => number.as_u64(),
        serde_json::Value::String(text) => text.parse::<u64>().ok(),
        serde_json::Value::Object(object) => first_u64(object, &["value", "tokens"]),
        _ => None,
    }
}

fn tokens_from_usage(usage: ExtractedUsage) -> AccountingTokens {
    // §FS-rhei-cost-accounting.3.1: Missing dimensions remain unavailable.
    let total = usage.total.or_else(|| sum_optional_pair(usage.input_total, usage.output_total));
    let total_source = usage.total_source.unwrap_or("agent-usage-capture");
    AccountingTokens {
        total: total
            .map(|value| AccountingTokenDimension::measured_from(value, total_source))
            .unwrap_or_else(|| AccountingTokenDimension::unavailable("unknown")),
        input: AccountingTokenSide {
            total: usage
                .input_total
                .map(AccountingTokenDimension::measured)
                .unwrap_or_else(|| AccountingTokenDimension::unavailable("unknown")),
            cached_read: usage
                .input_cached_read
                .map(AccountingTokenDimension::measured)
                .unwrap_or_else(|| AccountingTokenDimension::unavailable("unsupported")),
            cache_write: usage
                .input_cache_write
                .map(AccountingTokenDimension::measured)
                .unwrap_or_else(|| AccountingTokenDimension::unavailable("unsupported")),
        },
        output: AccountingTokenSide {
            total: usage
                .output_total
                .map(AccountingTokenDimension::measured)
                .unwrap_or_else(|| AccountingTokenDimension::unavailable("unknown")),
            cached_read: usage
                .output_cached_read
                .map(AccountingTokenDimension::measured)
                .unwrap_or_else(|| AccountingTokenDimension::unavailable("unsupported")),
            cache_write: usage
                .output_cache_write
                .map(AccountingTokenDimension::measured)
                .unwrap_or_else(|| AccountingTokenDimension::unavailable("unsupported")),
        },
    }
}

fn sum_optional_pair(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn price_tokens(
    price_book: &PriceBook,
    provider: Option<&str>,
    model: Option<&str>,
    tokens: &AccountingTokens,
) -> AccountingPricing {
    // §FS-rhei-cost-accounting.5: Pricing is separate from measurement.
    let priceable_measured = [
        tokens.input.total.value,
        tokens.input.cached_read.value,
        tokens.input.cache_write.value,
        tokens.output.total.value,
        tokens.output.cached_read.value,
        tokens.output.cache_write.value,
    ]
    .into_iter()
    .flatten()
    .count();
    if priceable_measured == 0 && tokens.total.value.is_none() {
        return AccountingPricing {
            status: "not-applicable".to_string(),
            currency: None,
            amount_micro: None,
            priced_amount_micro: None,
            price_book_id: None,
        };
    }
    if priceable_measured == 0 {
        return AccountingPricing {
            status: "unpriced".to_string(),
            currency: Some(price_book.currency.clone()),
            amount_micro: None,
            priced_amount_micro: None,
            price_book_id: Some(price_book.price_book_id.clone()),
        };
    }

    let Some(entry) = price_entry(price_book, provider, model) else {
        return AccountingPricing {
            status: "unpriced".to_string(),
            currency: Some(price_book.currency.clone()),
            amount_micro: None,
            priced_amount_micro: None,
            price_book_id: Some(price_book.price_book_id.clone()),
        };
    };
    let mut amount = 0u64;
    amount = amount.saturating_add(price_dimension(
        uncached_input_tokens(tokens),
        entry.input_total_micro,
    ));
    amount = amount.saturating_add(price_dimension(
        tokens.input.cached_read.value,
        entry.input_cached_read_micro,
    ));
    amount = amount.saturating_add(price_dimension(
        tokens.input.cache_write.value,
        entry.input_cache_write_micro,
    ));
    amount = amount.saturating_add(price_dimension(tokens.output.total.value, entry.output_total_micro));
    AccountingPricing {
        status: "priced".to_string(),
        currency: Some(price_book.currency.clone()),
        amount_micro: Some(amount),
        priced_amount_micro: Some(amount),
        price_book_id: Some(price_book.price_book_id.clone()),
    }
}

/// What the full input rate applies to: `input.total` less the two cache
/// dimensions, which are parts of it and each carry a rate of their own.
/// Charging the whole at the full rate and the parts at theirs charges every
/// cached token twice. An unavailable dimension subtracts nothing, and the
/// subtraction saturates rather than underflowing when a provider reports
/// parts larger than the whole. §FS-rhei-cost-accounting.5
fn uncached_input_tokens(tokens: &AccountingTokens) -> Option<u64> {
    let input_total = tokens.input.total.value?;
    Some(
        input_total
            .saturating_sub(tokens.input.cached_read.value.unwrap_or(0))
            .saturating_sub(tokens.input.cache_write.value.unwrap_or(0)),
    )
}

fn price_dimension(tokens: Option<u64>, price_micro: u64) -> u64 {
    let Some(tokens) = tokens else { return 0 };
    // §FS-rhei-cost-accounting.5: Cost uses integer micro-unit arithmetic.
    let amount = (tokens as u128 * price_micro as u128) / PRICE_UNIT_TOKENS as u128;
    u64::try_from(amount).unwrap_or(u64::MAX)
}

fn price_entry<'a>(
    price_book: &'a PriceBook,
    provider: Option<&str>,
    model: Option<&str>,
) -> Option<&'a PriceBookEntry> {
    let provider = provider?;
    let model = model?;
    price_book
        .entries
        .iter()
        .find(|entry| entry.provider == provider && entry.model == model)
}

fn write_invocation_record(
    accounting_root: &Path,
    record: &AccountingInvocationRecord,
) -> MietteResult<PathBuf> {
    // §FS-rhei-cost-accounting.2: File names use path-safe file ids.
    let dir = accounting_root.join("invocations");
    fs::create_dir_all(&dir)
        .map_err(|err| file_io_report(&dir, "failed to create accounting invocation directory", err))?;
    let file_id = invocation_file_id(record);
    let path = dir.join(format!("{file_id}.json"));
    write_json_atomic(&path, record)?;
    Ok(path)
}

fn write_json_atomic(path: &Path, value: &impl serde::Serialize) -> MietteResult<()> {
    // §FS-rhei-cost-accounting.11: Publish accounting artifacts atomically.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| file_io_report(parent, "failed to create accounting directory", err))?;
    }
    let staging = unique_staging_path(path);
    let text = serde_json::to_string_pretty(value)
        .map_err(|err| miette!(
            help = "rhei writes token accounting under runtime/. Check that directory is writable.",
            "failed to serialize accounting artifact '{}': {err}", path.display()
        ))?;
    fs::write(&staging, text)
        .map_err(|err| file_io_report(&staging, "failed to write accounting staging file", err))?;
    fs::rename(&staging, path)
        .map_err(|err| file_io_report(path, "failed to publish accounting artifact", err))
}

fn unique_staging_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().and_then(OsStr::to_str).unwrap_or("artifact.json");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), nanos))
}

fn invocation_file_id(record: &AccountingInvocationRecord) -> String {
    let mut hasher = Sha256::new();
    hasher.update(record.invocation_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(record.started_at.as_bytes());
    hasher.update(b"\0");
    hasher.update(record.ended_at.as_bytes());
    hasher.update(b"\0");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = ACCOUNTING_INVOCATION_FILE_SEQUENCE
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(nanos.to_le_bytes());
    hasher.update(sequence.to_le_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(32);
    for byte in &digest[..16] {
        std::fmt::Write::write_fmt(&mut out, format_args!("{byte:02x}"))
            .expect("writing to String cannot fail");
    }
    out
}

fn safe_accounting_file_segment(value: &str) -> String {
    // §FS-rhei-cost-accounting.2: Task index file ids preserve distinct task ids.
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' => {
                encoded.push(*byte as char);
            }
            other => {
                std::fmt::Write::write_fmt(&mut encoded, format_args!("%{other:02X}"))
                    .expect("writing to String cannot fail");
            }
        }
    }
    if encoded.is_empty() {
        "task".to_string()
    } else {
        encoded
    }
}

fn is_descendant_id(candidate: &str, ancestor: &str) -> bool {
    candidate.starts_with(ancestor) && candidate.as_bytes().get(ancestor.len()) == Some(&b'.')
}

fn format_summary_cost(summary: &rhei_tui::AccountingRunSummary) -> String {
    match summary.cost_micro.or(summary.priced_cost_micro) {
        Some(value) => format_cost_micro(value, summary.currency.as_deref()),
        None => "unpriced".to_string(),
    }
}

fn format_usage_cost(usage: &rhei_tui::UsageSummary) -> String {
    match usage.cost_micro.or(usage.priced_cost_micro) {
        Some(value) => format_cost_micro(value, usage.currency.as_deref()),
        None => "unpriced".to_string(),
    }
}

fn format_cost_micro(value: u64, currency: Option<&str>) -> String {
    let units = value / 1_000_000;
    let cents = (value % 1_000_000) / 10_000;
    match currency {
        Some("USD") | None => format!("${units}.{cents:02}"),
        Some(currency) => format!("{units}.{cents:02} {currency}"),
    }
}

fn format_dimension_value(summary: &rhei_tui::DimensionSummary) -> String {
    let Some(value) = summary.value else {
        return "-".to_string();
    };
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

fn summary_sort_cost(summary: &rhei_tui::AccountingRunSummary) -> u64 {
    summary.cost_micro.or(summary.priced_cost_micro).unwrap_or(0)
}
