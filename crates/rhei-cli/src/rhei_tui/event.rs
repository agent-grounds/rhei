use std::path::PathBuf;
use std::time::{Instant, SystemTime};

pub use super::event_bounds::{bound_journal_line, BoundCeiling, BoundReport};
pub use super::event_provenance::{CheckoutHead, RheiBuild, RunProvenance};
pub use super::event_sink::{EventSink, NullSink, Tee};
pub use super::event_usage::{
    summarize_usage_summaries, AccountingRunSummary, DimensionStatus, DimensionSummary,
    PricingStatus, UsageCoverage, UsageReport, UsageStatus, UsageSummary,
};

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
    /// A recognized provider unavailability; the wait it wrote, `None` when it
    /// spent the visit's last attempt and wrote none. §FS-rhei-run-json.2.1
    ProviderUnavailable {
        provider: Option<String>,
        next_attempt_at: Option<String>,
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
    /// `{path, sha256}` of the run's evidence manifest, carried at the top of
    /// the `run_finished` record. §FS-rhei-evidence.9.2
    pub evidence_manifest: Option<serde_json::Value>,
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
        /// The build and the checkout HEAD at start. §FS-rhei-run-json.2.1
        provenance: RunProvenance,
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
        /// The distinct Codex configuration fallbacks this invocation's own
        /// output carried, in the order first seen; empty for every other
        /// invocation and never an earlier one's. §FS-rhei-run-tui.1.1
        configuration_fallbacks: Vec<String>,
        /// `<file>:<line>` where the worker's edit broke the plan, when the run
        /// reverted it. §FS-rhei-run.3.7.7
        reverted: Option<String>,
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
    /// Where both count bounds stand.
    ///
    /// Emitted once before the first scheduling event, so the bounds are on
    /// screen *before* any capacity is spent, and again as receipts are written
    /// rather than only at exhaustion. A bound nobody can see until it stops
    /// something is not visible.
    // §FS-rhei-budgets.9 §FS-rhei-run-tui.1.1
    BudgetSnapshot {
        bounds: Vec<BoundReport>,
    },
    /// One admission a bound refused.
    ///
    /// The human-readable halt still arrives as an `error`-level
    /// [`RunEvent::Message`], so a frontend that renders neither record still
    /// says why the run stopped; this carries the same facts in a form a reader
    /// can route on. §FS-rhei-budgets.8 §FS-rhei-run-json.2.1
    BudgetHalt {
        task: String,
        reason_code: String,
        bound: BoundReport,
        /// The instant the window renews, present exactly when the window
        /// contract is what limited. §FS-rhei-budgets.8
        renews_at: Option<String>,
        /// The single remedy that raises the limiter that actually stopped the
        /// work — never an inner value the machine ceiling would clamp.
        remedy: String,
    },
    /// A strict run refused a ticket because a sealed input of its rhei changed.
    /// The prose arrives as an `error` [`RunEvent::Message`]; this carries the
    /// facts. §FS-rhei-seal.8.4 §FS-rhei-run-json.2.1
    SealHalt {
        task: String,
        rhei: String,
        state: String,
        generation: u32,
        /// Each changed path with its kind, sorted by path.
        changes: Vec<(String, String)>,
        remedy: String,
    },
}
