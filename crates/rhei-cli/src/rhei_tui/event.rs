use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

/// Slot index assigned to a running task invocation.
///
/// The engine allocates one slot per concurrent agent/program and releases the
/// slot when the invocation exits. The renderer uses the slot to update the
/// correct tile without reconciling task ids on every frame. The type is
/// wider than a byte so callers with very large `--parallel` values cannot
/// silently collide into slot 255.
pub type Slot = u16;

/// Outcome of a released slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskOutcome {
    Completed,
    Failed(String),
    /// The invocation's selected transition was its state's poll self-loop, so
    /// the machine itself called this exit "not done yet": the attempt is a
    /// handled wait, neither a failure nor a finished state, whatever exit code
    /// matched the edge. It carries no reason the way [`Self::Failed`] does —
    /// the exit that matched is already on the same record. §FS-rhei-states.2.2
    Waiting,
    /// A supported provider refused the invocation until a durable UTC
    /// deadline. This is a wait, not a failure. §FS-rhei-run-tui.1.1
    ProviderLimited {
        provider: String,
        next_attempt_at: String,
    },
    Cancelled,
    TimedOut,
    /// The engine ended the invocation because the run was interrupted. The
    /// ticket keeps its state and no transition fires, so this is neither a
    /// failure nor a timeout. §FS-rhei-run.3.2
    Interrupted,
    /// An `outcome` read off a `--json` stream that this build has no variant
    /// for, held under the name the stream spelled it with.
    ///
    /// The vocabulary is additive under one `schema`, so a reader meeting a
    /// value a newer run emitted carries it through — prints it, stores it,
    /// passes it on — rather than deciding it must be one of the words it does
    /// know (§FS-rhei-run-json.2.2). No run emits this variant; only
    /// `decode_outcome` produces one.
    Unrecognized(String),
}

/// Aggregate statistics emitted with `RunFinished`.
#[derive(Debug, Clone, Default)]
pub struct RunSummary {
    pub agents_spawned: u32,
    pub programs_spawned: u32,
    pub terminal_tasks: usize,
    pub total_tasks: usize,
    /// What *this run* spent: the rollup over the invocation records that name
    /// it. §FS-rhei-run-report.2.1
    pub accounting: Option<AccountingRunSummary>,
    /// `workspace_total`: what the workspace has spent over its whole life.
    /// Carried beside the run's own so neither is read as the other.
    ///
    /// Boxed because `RunEvent` is sized by its largest variant, and a second
    /// inline rollup here makes `RunFinished` half again the size of every
    /// other event on the channel.
    // §FS-rhei-cost-accounting.6
    pub workspace_accounting: Option<Box<AccountingRunSummary>>,
    /// Why the run stopped, present exactly when `--until-idle` was selected.
    /// An unselected run leaves it `None` and its stream is byte-identical to
    /// the one it emitted before this field existed.
    // §FS-rhei-run-json.2.1
    pub stop: Option<crate::rhei_tui::run_stop::RunStop>,
}

/// Severity of an engine log message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageLevel {
    Info,
    Warn,
    Error,
}

/// Agent subprocess stream that produced a live output line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DimensionStatus {
    Measured,
    Partial,
    Unsupported,
    Omitted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DimensionSummary {
    pub value: Option<u64>,
    pub status: DimensionStatus,
    pub missing_count: u64,
    pub measured_count: u64,
}

impl Default for DimensionSummary {
    fn default() -> Self {
        Self { value: None, status: DimensionStatus::Unknown, missing_count: 0, measured_count: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UsageCoverage {
    Complete,
    Partial,
    Unpriced,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UsageStatus {
    Measured,
    UnsupportedAgent,
    ExtractorUnavailable,
    ExtractorFailed,
    NoUsageEmitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PricingStatus {
    Priced,
    PartialPrice,
    Unpriced,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UsageSummary {
    pub invocation_id: String,
    pub state: String,
    pub agent: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub total: DimensionSummary,
    pub input_total: DimensionSummary,
    pub input_cached_read: DimensionSummary,
    pub input_cache_write: DimensionSummary,
    pub output_total: DimensionSummary,
    pub output_cached_read: DimensionSummary,
    pub output_cache_write: DimensionSummary,
    pub cost_micro: Option<u64>,
    pub priced_cost_micro: Option<u64>,
    pub currency: Option<String>,
    pub coverage: UsageCoverage,
    pub status: UsageStatus,
    pub pricing_status: PricingStatus,
}

/// Which of an invocation's usage reports an event carries.
///
/// One invocation emits several: one `Streamed` running total per turn a
/// streaming extractor measures, then one `Final` after its durable record is
/// written. A frontend that keys a view by invocation id upserts both; one that
/// appends lines prints the `Final` only, because a written line cannot be
/// revised into the next running total. §FS-rhei-cost-accounting.7.1
///
/// An enum rather than a `bool` because `final` is a Rust keyword and because
/// the JSON record contract names both values (§FS-rhei-run-json.2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UsageReport {
    /// A running total for an invocation still in progress, re-summed from its
    /// whole capture stream, so it reads low until the last turn is in.
    Streamed,
    /// The one report that follows the invocation's durable record and carries
    /// what that record says it cost.
    Final,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AccountingRunSummary {
    pub total: DimensionSummary,
    pub input_total: DimensionSummary,
    pub input_cached_read: DimensionSummary,
    pub input_cache_write: DimensionSummary,
    pub output_total: DimensionSummary,
    pub output_cached_read: DimensionSummary,
    pub output_cache_write: DimensionSummary,
    pub cost_micro: Option<u64>,
    pub priced_cost_micro: Option<u64>,
    pub currency: Option<String>,
    pub coverage: UsageCoverage,
    pub pricing_status: PricingStatus,
    pub invocation_count: u64,
    pub measured_invocation_count: u64,
    pub missing_invocation_count: u64,
}

/// Summarize invocation usage records into the shared run/task accounting shape.
/// §FS-rhei-cost-accounting.6: rollups summarize invocation records consistently.
pub fn summarize_usage_summaries<'a>(
    usages: impl IntoIterator<Item = &'a UsageSummary>,
) -> Option<AccountingRunSummary> {
    let usages: Vec<&UsageSummary> = usages.into_iter().collect();
    if usages.is_empty() {
        return None;
    }

    let measured_invocation_count =
        usages.iter().filter(|usage| usage.status == UsageStatus::Measured).count() as u64;
    let missing_invocation_count = usages.len() as u64 - measured_invocation_count;
    let priced_cost_micro = sum_options(usages.iter().map(|usage| usage.priced_cost_micro));
    let cost_micro = if usages.iter().all(|usage| usage.cost_micro.is_some()) {
        Some(usages.iter().filter_map(|usage| usage.cost_micro).sum())
    } else {
        None
    };
    let currency = usages.iter().find_map(|usage| usage.currency.clone());
    let pricing_status = summarize_pricing_status(&usages);
    let coverage = summarize_coverage(&usages, cost_micro, priced_cost_micro);

    Some(AccountingRunSummary {
        total: summarize_dimension(usages.iter().map(|usage| &usage.total)),
        input_total: summarize_dimension(usages.iter().map(|usage| &usage.input_total)),
        input_cached_read: summarize_dimension(usages.iter().map(|usage| &usage.input_cached_read)),
        input_cache_write: summarize_dimension(usages.iter().map(|usage| &usage.input_cache_write)),
        output_total: summarize_dimension(usages.iter().map(|usage| &usage.output_total)),
        output_cached_read: summarize_dimension(
            usages.iter().map(|usage| &usage.output_cached_read),
        ),
        output_cache_write: summarize_dimension(
            usages.iter().map(|usage| &usage.output_cache_write),
        ),
        cost_micro,
        priced_cost_micro,
        currency,
        coverage,
        pricing_status,
        invocation_count: usages.len() as u64,
        measured_invocation_count,
        missing_invocation_count,
    })
}

fn summarize_dimension<'a>(
    dimensions: impl IntoIterator<Item = &'a DimensionSummary>,
) -> DimensionSummary {
    let mut value = 0u64;
    let mut saw_value = false;
    let mut missing_count = 0u64;
    let mut measured_count = 0u64;
    let mut unavailable_status = None;

    for dimension in dimensions {
        if let Some(v) = dimension.value {
            value = value.saturating_add(v);
            saw_value = true;
        }
        measured_count = measured_count.saturating_add(dimension.measured_count);
        missing_count = missing_count.saturating_add(dimension.missing_count);
        if dimension.status != DimensionStatus::Measured {
            unavailable_status = Some(dimension.status);
        }
    }

    let status = if saw_value && missing_count == 0 {
        DimensionStatus::Measured
    } else if saw_value {
        DimensionStatus::Partial
    } else {
        unavailable_status.unwrap_or(DimensionStatus::Unknown)
    };

    DimensionSummary { value: saw_value.then_some(value), status, missing_count, measured_count }
}

fn sum_options(values: impl IntoIterator<Item = Option<u64>>) -> Option<u64> {
    let mut total = 0u64;
    let mut saw = false;
    for value in values.into_iter().flatten() {
        total = total.saturating_add(value);
        saw = true;
    }
    saw.then_some(total)
}

fn summarize_pricing_status(usages: &[&UsageSummary]) -> PricingStatus {
    let mut saw_priced = false;
    let mut saw_partial = false;
    let mut saw_unpriced = false;
    let mut saw_applicable = false;
    for usage in usages {
        match usage.pricing_status {
            PricingStatus::Priced => {
                saw_priced = true;
                saw_applicable = true;
            }
            PricingStatus::PartialPrice => {
                saw_partial = true;
                saw_applicable = true;
            }
            PricingStatus::Unpriced => {
                saw_unpriced = true;
                saw_applicable = true;
            }
            PricingStatus::NotApplicable => {}
        }
    }
    if !saw_applicable {
        PricingStatus::NotApplicable
    } else if saw_partial || (saw_priced && saw_unpriced) {
        PricingStatus::PartialPrice
    } else if saw_priced {
        PricingStatus::Priced
    } else {
        PricingStatus::Unpriced
    }
}

fn summarize_coverage(
    usages: &[&UsageSummary],
    cost_micro: Option<u64>,
    priced_cost_micro: Option<u64>,
) -> UsageCoverage {
    if usages.iter().all(|usage| usage.coverage == UsageCoverage::None) {
        return UsageCoverage::None;
    }
    if usages.iter().any(|usage| {
        matches!(usage.coverage, UsageCoverage::Partial) || usage.status != UsageStatus::Measured
    }) {
        return UsageCoverage::Partial;
    }
    if cost_micro.is_some() {
        UsageCoverage::Complete
    } else if priced_cost_micro.is_some() {
        UsageCoverage::Partial
    } else if usages.iter().any(|usage| usage.coverage == UsageCoverage::Unpriced) {
        UsageCoverage::Unpriced
    } else {
        UsageCoverage::None
    }
}

/// Events emitted by the execution engine.
///
/// The shape follows the TUI event surface. `Message` is an additional variant
/// used while the stdout path still emits humanized strings; a TUI frontend can
/// surface these in its journal pane.
// §FS-rhei-run-tui.1.1: Run event surface.
#[derive(Debug, Clone)]
pub enum RunEvent {
    RunStarted {
        /// The id this run is named by everywhere — the report, the descriptor,
        /// and `rhei attach <id>`. Carried on the event so the record stream is
        /// self-describing. §FS-rhei-run.2.7 §FS-rhei-run-json.2.1
        run_id: String,
        workspace: PathBuf,
        parallel: u16,
        total_tasks: usize,
    },
    PassStarted {
        pass: u32,
        ready: Vec<String>,
    },
    /// A worker has been assigned to a task.
    ///
    /// `from` is the task's persisted state at the moment of claim; `to` is
    /// the state the worker is operating in. When `from == to`, the worker
    /// is running an *autonomous* state that the engine did not transition
    /// into as part of the claim — it is "starting work in `to`," not
    /// "moving from `from` to `to`." Renderers must distinguish the two
    /// cases so the UI does not show a phantom `state→state` self-loop.
    SlotAssigned {
        slot: Slot,
        task: String,
        from: String,
        to: String,
        agent: Option<String>,
        template_context: Option<crate::rhei_viz_model::TemplateContext>,
        log_path: PathBuf,
        started_at: Instant,
        wall_clock: SystemTime,
    },
    /// A worker slot has been released.
    ///
    /// `from` is the state at assignment; `to` is the state the task ended
    /// up in. When `from == to`, the worker exited without changing state
    /// (typical for autonomous states that hand control back to the run loop
    /// for re-evaluation) — render as "ended in `to`," not as a transition.
    SlotReleased {
        slot: Slot,
        task: String,
        from: String,
        to: String,
        log_path: PathBuf,
        outcome: TaskOutcome,
        finished_at: Instant,
        wall_clock: SystemTime,
        exit_code: Option<i32>,
        duration_ms: u64,
    },
    PassEnded {
        pass: u32,
        progressed: bool,
    },
    /// Tasks that were eligible this pass but yielded their slot to a same-state
    /// claimant (non-`concurrent` state). They are reconsidered next pass.
    TasksDeferred {
        pass: u32,
        tasks: Vec<String>,
    },
    RunFinished {
        summary: RunSummary,
    },
    Message {
        level: MessageLevel,
        text: String,
    },
    RunLink {
        label: String,
        url: String,
    },
    AgentOutput {
        slot: Slot,
        task: String,
        stream: AgentStream,
        line: String,
        wall_clock: SystemTime,
    },
    /// Accounting event emitted once per measured turn while the agent runs,
    /// and once more after the durable invocation record is written. `report`
    /// says which of the two this is, and only `Final` carries the
    /// invocation's settled cost.
    /// §FS-rhei-cost-accounting.7 §FS-rhei-cost-accounting.7.1
    UsageReported {
        slot: Option<Slot>,
        task: String,
        invocation_id: String,
        report: UsageReport,
        usage: UsageSummary,
    },
    /// A worker exited `0`, or took a declared route out of a non-zero exit,
    /// and left required artifacts unwritten, so the ticket did not advance.
    /// `entries` are already rendered as `name (path)`, the ticket's terminal
    /// result included under the name `result`.
    ///
    /// The human-readable warning still arrives as a [`RunEvent::Message`]; this
    /// carries the same facts in a form the run report can classify with,
    /// instead of re-deriving them from prose.
    // §FS-rhei-run-report.3.1 §FS-rhei-agents.3.2.1
    TaskOutputsMissing {
        task: String,
        state: String,
        entries: Vec<String>,
    },
}

/// Sink that consumes `RunEvent`s. Implementations must be cheap to clone and
/// safe to share across threads (the engine spawns parallel workers).
pub trait EventSink: Send + Sync {
    fn emit(&self, event: RunEvent);
}

/// Composite sink that forwards every event to each inner sink in order.
#[derive(Clone)]
pub struct Tee {
    inners: Arc<Vec<Arc<dyn EventSink>>>,
}

impl Tee {
    pub fn new(sinks: Vec<Arc<dyn EventSink>>) -> Self {
        Self { inners: Arc::new(sinks) }
    }
}

impl EventSink for Tee {
    fn emit(&self, event: RunEvent) {
        for sink in self.inners.iter() {
            sink.emit(event.clone());
        }
    }
}

/// Sink that discards every event. Useful as the default frontend when the
/// engine is responsible for producing stdout (backward-compatible mode).
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: RunEvent) {}
}
