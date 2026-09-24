// End-of-run console summary: after `rhei run` exits and the TUI restores the
// terminal, print a compact, scan-first view — result line, distribution bar,
// counts, attention, and a source-order task tree — without opening a file.

// §FS-rhei-run-report.3: the renderer here is pure; `SummarySink` collects the
// per-task data during the run.

// `HashMap` and `Mutex` are already imported at the crate root (this file is
// `include!`-ed), so they are referenced unqualified without a local `use`.

/// Per-task activity accumulated from the run event stream. The tree shows the
/// driver and timing of the work that advanced each task. §FS-rhei-run-report.3.2
#[derive(Debug, Clone, Default)]
struct TaskActivity {
    /// `"agent"` or `"program"` — the driver of the last invocation for the task.
    driver: Option<&'static str>,
    /// Number of invocations spawned for the task (fan-out targets count > 1).
    invocations: u32,
    /// Duration of the last invocation, milliseconds.
    last_duration_ms: u64,
    /// Direct accounting for usage reported against this task during the run.
    accounting: Option<rhei_tui::AccountingRunSummary>,
    /// Required artifacts the last worker left unwritten, rendered as
    /// `name (path)`, paired with the state it left them in. The halt
    /// classification uses them only while the ticket is still in that state,
    /// and a fresh spawn clears them, so an old stall never explains a new one.
    // §FS-rhei-run-report.3.1
    missing_outputs: Option<(String, Vec<String>)>,
    /// The code the task's last released invocation exited with, `None` before
    /// one has been released or when the run has no code for it. Paired with
    /// `missing_outputs` by the same spawn, which clears both, so a halt row
    /// never names one attempt's artifacts beside another's exit.
    // §FS-rhei-run-report.3.1 §FS-rhei-programs.3.2
    last_exit_code: Option<i32>,
}

/// One spawned transition from the run event stream, rendered into the report's
/// ledger and invocations (agent/program only; callback and terminal-at-start
/// rows are synthesized at build time). §FS-rhei-run-report.4 §FS-rhei-run-report.7
#[derive(Debug, Clone)]
struct LedgerRecord {
    task: String,
    from: String,
    to: String,
    /// `"agent"` or `"program"`.
    driver: &'static str,
    log_path: std::path::PathBuf,
    exit_code: Option<i32>,
    duration_ms: u64,
    outcome: LedgerOutcome,
}

/// The terminal disposition of a spawned invocation, mirrored from
/// [`rhei_tui::TaskOutcome`] so the renderer does not depend on the TUI enum.
#[derive(Debug, Clone)]
enum LedgerOutcome {
    Completed,
    Failed(String),
    /// The invocation selected its state's poll self-loop, so it neither failed
    /// nor finished the state: it waited. §FS-rhei-states.2.2
    Waiting,
    ProviderLimited { provider: String, next_attempt_at: String },
    Cancelled,
    TimedOut,
    /// The run was interrupted and the engine ended the invocation; no
    /// transition was selected. §FS-rhei-run.3.2
    Interrupted,
    /// An outcome word this build has no variant for, kept as it was read.
    /// Reachable only from a decoded `--json` stream — a run's own events never
    /// carry one. §FS-rhei-run-json.2.2
    Unrecognized(String),
}

/// `EventSink` recording per-task driver/duration for the console task tree and
/// the spawned-transition ledger for the durable report; teed alongside the
/// journal/frontend sinks and read post-run. §FS-rhei-run-report.3.2 §FS-rhei-run-report.8
pub struct SummarySink {
    inner: Mutex<SummaryState>,
}

#[derive(Default)]
struct SummaryState {
    /// Driver of each in-flight slot, keyed by slot index, set on `SlotAssigned`.
    inflight: HashMap<u16, &'static str>,
    /// Finalized per-task activity, keyed by task id.
    tasks: HashMap<String, TaskActivity>,
    /// Spawned transitions in chronological order, for the durable ledger.
    ledger: Vec<LedgerRecord>,
    /// Usage reported during the run, used before `RunFinished` publishes the
    /// authoritative rollup or on an early-error fallback. §FS-rhei-cost-accounting.7
    /// One record per invocation id: the source of every task row, and of the
    /// run-level strip whenever `RunFinished` publishes none of its own.
    /// §FS-rhei-cost-accounting.9
    usages: UsageLedger,
    /// The finalized run rollup from `RunFinished`, when available.
    accounting: Option<rhei_tui::AccountingRunSummary>,
    /// `workspace_total` — the workspace's lifetime, kept apart from the run's
    /// own so neither can stand in for the other.
    // §FS-rhei-cost-accounting.6 §FS-rhei-run-report.2.1
    workspace_accounting: Option<rhei_tui::AccountingRunSummary>,
    /// Which of the strip's two sources produced what it is about to show.
    // §FS-rhei-run-report.2.1
    source: AccountingSource,
    /// The first and the latest bound snapshot of the run, keyed by dimension.
    ///
    /// Both, because the record answers "what was this bounded by" and "what
    /// did it spend" — one number cannot do both. §FS-rhei-run-report.3.1
    bounds_first: BTreeMap<String, rhei_tui::BoundReport>,
    bounds_last: BTreeMap<String, rhei_tui::BoundReport>,
    /// Every dimension that refused an admission this run, so the record says
    /// what stopped it without the reader reconstructing it from rows.
    /// §FS-rhei-budgets.8
    bound_halts: Vec<(String, rhei_tui::BoundReport, Option<String>, String)>,
}

impl SummarySink {
    pub fn new() -> Self {
        Self { inner: Mutex::new(SummaryState::default()) }
    }

    /// Snapshot the accumulated activity for rendering after the run. A poisoned
    /// lock (a worker panicked mid-run) degrades to empty rather than panicking
    /// the report — a partial report still beats none.
    fn snapshot(&self) -> HashMap<String, TaskActivity> {
        self.inner.lock().map(|state| state.tasks.clone()).unwrap_or_default()
    }

    /// The spawned-transition ledger in chronological order; empty on a poisoned
    /// lock, for the same best-effort reason as [`snapshot`](Self::snapshot).
    fn ledger(&self) -> Vec<LedgerRecord> {
        self.inner.lock().map(|state| state.ledger.clone()).unwrap_or_default()
    }

    /// The run's bounds: where each count stood when the run started, where it
    /// stood when it ended, and every dimension that refused an admission.
    /// §FS-rhei-run-report.3.1
    fn bounds(&self) -> BoundsSection {
        self.inner
            .lock()
            .map(|state| BoundsSection {
                first: state.bounds_first.clone(),
                last: state.bounds_last.clone(),
                halts: state.bound_halts.clone(),
            })
            .unwrap_or_default()
    }

    /// Run-level accounting, preferring the finalized `RunFinished` rollup and
    /// falling back to accumulated usage events for aborted runs.
    ///
    /// The fallback is where `source` earns its place: a run that never reached
    /// `RunFinished` lands here with `state.source` still at its
    /// `RunEvents` default, so the strip it renders is labelled as the events
    /// summary it is. `Rollup` is written in exactly one place — the
    /// `RunFinished` arm below — and only together with the rollup itself.
    // §FS-rhei-run-report.2.1
    fn accounting(&self) -> RunAccountingStrip {
        self.inner
            .lock()
            .map(|state| RunAccountingStrip {
                run: state.accounting.clone().or_else(|| state.usages.run_rollup()),
                workspace: state.workspace_accounting.clone(),
                source: state.source,
            })
            .unwrap_or_default()
    }
}

impl Default for SummarySink {
    fn default() -> Self {
        Self::new()
    }
}

impl rhei_tui::EventSink for SummarySink {
    fn emit(&self, event: rhei_tui::RunEvent) {
        let mut state = match self.inner.lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        match event {
            // §FS-rhei-run-report.3.1 §FS-rhei-budgets.9
            rhei_tui::RunEvent::BudgetSnapshot { bounds } => {
                for bound in bounds {
                    state
                        .bounds_first
                        .entry(bound.dimension.clone())
                        .or_insert_with(|| bound.clone());
                    state.bounds_last.insert(bound.dimension.clone(), bound);
                }
            }
            rhei_tui::RunEvent::BudgetHalt { task, bound, renews_at, remedy, .. } => {
                state.bound_halts.push((task, bound, renews_at, remedy));
            }
            // `agent` is `Some` for agent-backed work, `None` for programs.
            rhei_tui::RunEvent::SlotAssigned { slot, task, agent, .. } => {
                let driver = if agent.is_some() { "agent" } else { "program" };
                state.inflight.insert(slot, driver);
                // A fresh attempt supersedes what the last one left unwritten,
                // and the exit that went with it. §FS-rhei-run-report.3.1
                let entry = state.tasks.entry(task).or_default();
                entry.missing_outputs = None;
                entry.last_exit_code = None;
            }
            rhei_tui::RunEvent::SlotReleased {
                slot,
                task,
                from,
                to,
                log_path,
                outcome,
                exit_code,
                duration_ms,
                ..
            } => {
                let driver = state.inflight.remove(&slot).unwrap_or("program");
                let entry = state.tasks.entry(task.clone()).or_default();
                entry.driver = Some(driver);
                entry.invocations += 1;
                entry.last_duration_ms = duration_ms;
                // The release is the only event carrying the code, and it
                // follows the missing-artifact event of the same attempt.
                // §FS-rhei-run-report.3.1
                entry.last_exit_code = exit_code;
                let outcome = match outcome {
                    rhei_tui::TaskOutcome::Completed => LedgerOutcome::Completed,
                    rhei_tui::TaskOutcome::Failed(msg) => LedgerOutcome::Failed(msg),
                    rhei_tui::TaskOutcome::Waiting => LedgerOutcome::Waiting,
                    rhei_tui::TaskOutcome::ProviderLimited {
                        provider,
                        next_attempt_at,
                    } => LedgerOutcome::ProviderLimited { provider, next_attempt_at },
                    rhei_tui::TaskOutcome::Cancelled => LedgerOutcome::Cancelled,
                    rhei_tui::TaskOutcome::TimedOut => LedgerOutcome::TimedOut,
                    rhei_tui::TaskOutcome::Interrupted => LedgerOutcome::Interrupted,
                    rhei_tui::TaskOutcome::Unrecognized(name) => {
                        LedgerOutcome::Unrecognized(name)
                    }
                };
                state.ledger.push(LedgerRecord {
                    task,
                    from,
                    to,
                    driver,
                    log_path,
                    exit_code,
                    duration_ms,
                    outcome,
                });
            }
            // Upsert by invocation id, then rebuild the rows the report touched:
            // the task it now names, and the one it left if it moved.
            // §FS-rhei-cost-accounting.9
            rhei_tui::RunEvent::UsageReported { task, invocation_id, usage, .. } => {
                if let Some(left) = state.usages.report(&task, &invocation_id, usage) {
                    let accounting = state.usages.task_rollup(&left);
                    state.tasks.entry(left).or_default().accounting = accounting;
                }
                let accounting = state.usages.task_rollup(&task);
                state.tasks.entry(task).or_default().accounting = accounting;
            }
            // The classification needs the names, not the sentence; a later
            // invocation replaces the list, so it is the last attempt's, and it
            // is kept with the state it belongs to. §FS-rhei-run-report.3.1
            rhei_tui::RunEvent::TaskOutputsMissing { task, state: stalled_in, entries } => {
                state.tasks.entry(task).or_default().missing_outputs =
                    Some((stalled_in, entries));
            }
            // The only place the strip's source is decided; a rollup and the
            // lifetime total arrive together or not at all.
            // §FS-rhei-run-report.2.1
            rhei_tui::RunEvent::RunFinished { summary } => match summary.accounting.clone() {
                Some(rollup) => {
                    state.accounting = Some(rollup);
                    state.workspace_accounting =
                        summary.workspace_accounting.as_deref().cloned();
                    state.source = AccountingSource::Rollup;
                }
                None => {
                    state.accounting = state.usages.run_rollup();
                    state.workspace_accounting = None;
                    state.source = AccountingSource::RunEvents;
                }
            },
            _ => {}
        }
    }
}

/// Build the report, write the durable Markdown files, then print the run's
/// end-of-run surface: rich console summary on a TTY, else a `Report:` pointer.
/// Best-effort — a load or write failure must not mask the result. §FS-rhei-run-report.1 §FS-rhei-run-report.3
fn emit_run_report(
    input: &std::path::Path,
    machines: &rhei_validator::MachineSet,
    summary: &SummarySink,
    runtime_dir: &std::path::Path,
    stats: RunStats,
) {
    use std::io::IsTerminal;
    let Ok(loaded) = load_plan(input) else {
        return;
    };
    // A dry run is a side-effect-free preview: render the console summary but
    // never touch the durable report on disk. §FS-rhei-run-report.3.5
    let dry_run = stats.dry_run;
    // The commands the report suggests carry the plan, so they run from
    // wherever the operator is reading it. §FS-rhei-errors.2
    let plan_arg = plan_arg_for_help(input);
    // The run's own per-ticket execution roots, beside the run's own workspace
    // root in `stats`: the report explains a halted ticket under exactly the
    // roots its ready-set scan looked under. §AR-rhei-panta.5
    let task_roots = &loaded.task_roots;
    let mut report =
        RunSummaryReport::build(&loaded.rhei, machines, summary, stats, &plan_arg, task_roots);
    // Write the durable report even when stdout is piped, so CI runs leave the
    // artifact; a dry run writes nothing, leaving `report_path` unset so no pointer
    // prints below. §FS-rhei-run-report.1 §FS-rhei-run-report.3.5
    if !dry_run {
        if let Err(err) = report.write_to_runtime(runtime_dir) {
            eprintln!("warning: could not write run report: {err}");
        }
    }
    if std::io::stdout().is_terminal() {
        // Honor NO_COLOR for users who disable ANSI globally.
        let color = std::env::var_os("NO_COLOR").is_none();
        print!("{}", report.render_tty(color));
    } else if let Some(report_path) = &report.report_path {
        // The pointer is for a person, so under `--json` it takes the channel
        // people read; stdout is records to its last byte. §FS-rhei-run-json.1
        if stdout_carries_json_records() {
            eprintln!("Report: {report_path}");
        } else {
            println!("Report: {report_path}");
        }
    }
}

/// A short, stable run identifier derived from the run's wall-clock start. FNV-1a
/// over the start nanoseconds folded to six hex digits — enough to disambiguate
/// history entries without a random-number dependency. §FS-rhei-run-report.2
fn short_run_id(started_at: std::time::SystemTime) -> String {
    let nanos =
        started_at.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in nanos.to_le_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:06x}", hash & 0xff_ffff)
}

/// The relative path to the frozen dashboard artifact when one was written this
/// run, for the report's Dashboard pointer. Gated on `enabled_this_run` so a
/// stale `dashboard.html` left by an earlier run is never linked. §FS-rhei-run-report.2
fn frozen_dashboard_relative_path(
    enabled_this_run: bool,
    runtime_dir: &std::path::Path,
    workspace_root: &std::path::Path,
) -> Option<String> {
    if !enabled_this_run {
        return None;
    }
    let path = runtime_dir.join("dashboard.html");
    path.exists().then(|| relativize(&path, workspace_root))
}

/// The current process command line with `argv[0]` normalized to `rhei`, so the
/// report header records the real flags the operator ran. §FS-rhei-run-report.2
fn current_command_line() -> String {
    let mut args: Vec<String> = std::env::args().collect();
    if let Some(first) = args.first_mut() {
        *first = "rhei".to_string();
    }
    args.join(" ")
}

/// Snapshot each task's normalized state at run start, keyed by task id, so the
/// report can mark terminal-at-start tasks and reconcile callback advances that
/// emit no slot events. §FS-rhei-run-report.8
fn collect_initial_states(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
) -> HashMap<String, String> {
    fn walk(
        tasks: &[rhei_core::ast::Task],
        machines: &rhei_validator::MachineSet,
        out: &mut HashMap<String, String>,
    ) {
        for task in tasks {
            out.insert(
                task.id.to_string(),
                normalized_state_name(task.state.as_str(), machines.for_task(&task.id)),
            );
            walk(&task.children, machines, out);
        }
    }
    let mut out = HashMap::new();
    walk(&rhei.tasks, machines, &mut out);
    out
}

/// Writes a best-effort report if `rhei run` returns early with an error.
/// Declared before the frontend so it drops after the terminal is restored; the
/// happy path disarms it after the full report is written. §FS-rhei-run-report.1
struct RunReportGuard<'a> {
    input: &'a std::path::Path,
    // Owned so live Panta admission can replace the active machine set while
    // this fallback remains armed. §FS-rhei-run.3 §AR-rhei-panta.4
    machines: rhei_validator::MachineSet,
    runtime_dir: std::path::PathBuf,
    run_started: std::time::Instant,
    run_started_wall: std::time::SystemTime,
    run_id: String,
    workspace_root: std::path::PathBuf,
    command: String,
    parallel: usize,
    mode: &'static str,
    initial_states: HashMap<String, String>,
    /// A dry run is side-effect-free, so the fallback writes nothing on an early
    /// error either. §FS-rhei-run-report.3.5
    dry_run: bool,
    /// Set once the frontend exists; without it there is nothing to report from.
    summary: Option<std::sync::Arc<SummarySink>>,
    /// Cleared by the happy path after the authoritative report is written.
    armed: bool,
}

impl RunReportGuard<'_> {
    /// The run wrote its own report; suppress the best-effort fallback.
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for RunReportGuard<'_> {
    fn drop(&mut self) {
        // A dry run never writes a report, even when it aborts. §FS-rhei-run-report.3.5
        if !self.armed || self.dry_run {
            return;
        }
        let Some(summary) = self.summary.clone() else {
            return;
        };
        // Best-effort from the data captured before the failure: spawn counts come
        // from the ledger, callbacks and dashboard are unknown on an aborted run.
        let ledger = summary.ledger();
        let agents = ledger.iter().filter(|r| r.driver == "agent").count() as u32;
        let programs = ledger.iter().filter(|r| r.driver == "program").count() as u32;
        emit_run_report(
            self.input,
            &self.machines,
            &summary,
            &self.runtime_dir,
            RunStats {
                agents_spawned: agents,
                programs_spawned: programs,
                callback_only: 0,
                duration: Some(self.run_started.elapsed()),
                dashboard: None,
                run_id: self.run_id.clone(),
                started_at: Some(self.run_started_wall),
                workspace_root: self.workspace_root.clone(),
                command: self.command.clone(),
                parallel: self.parallel,
                mode: self.mode,
                initial_states: self.initial_states.clone(),
                dry_run: false,
                // The fallback fires while the run is failing, so there is no
                // captured reading to use: ask the token now. §FS-rhei-run.3.2
                interrupted: interrupted_by_signal(),
                // A run tearing down never reached a stopping decision, and an
                // idle return is one. §FS-rhei-run.3
                stop: None,
            },
        );
    }
}

/// The scan glyph for a task's final state. Color and the state label remain the
/// primary signal; the marker degrades to an ASCII fallback. §FS-rhei-run-report.3.2
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marker {
    /// Terminal-success state.
    Done,
    /// Gating state awaiting a human.
    Gate,
    /// Blocked or failed — needs attention.
    Attention,
    /// Cancelled.
    Cancelled,
    /// Terminal at the start of the run — no work was attempted. §FS-rhei-run-report.3.2
    TerminalAtStart,
}

impl Marker {
    fn glyph(self) -> char {
        match self {
            Marker::Done => '✓',
            Marker::Gate => '⏸',
            Marker::Attention => '!',
            Marker::Cancelled => '⊘',
            Marker::TerminalAtStart => '·',
        }
    }

    /// ANSI color for this marker class. Only `Attention` and `Gate` are
    /// saturated; success, cancelled, and terminal-at-start rows stay calm.
    /// §FS-rhei-viz-ux.3
    fn color(self) -> &'static str {
        match self {
            Marker::Done => GREEN,
            Marker::Gate => YELLOW,
            Marker::Attention => RED,
            Marker::Cancelled => DIM,
            Marker::TerminalAtStart => DIM,
        }
    }

    /// Whether this marker represents a task a human must still act on.
    fn needs_attention(self) -> bool {
        matches!(self, Marker::Gate | Marker::Attention)
    }
}

/// State names that read as failure whatever the machine says about them.
fn state_is_failure(state: &str) -> bool {
    matches!(state, "blocked" | "failed")
}

/// Classify a state into a marker. Failure/cancel state names win over the
/// `gating` flag: a machine may park a `blocked` task in a gating state, but it
/// still reads as attention, not a calm gate. §FS-rhei-run-report.3.2
fn classify_marker(state: &str, machine: &rhei_validator::StateMachine) -> Marker {
    match state {
        // §FS-rhei-states.1.4: the reserved cancel name, in either spelling.
        _ if machine.is_cancellation(state) => return Marker::Cancelled,
        _ if state_is_failure(state) => return Marker::Attention,
        _ => {}
    }
    let def = machine.states.get(state);
    if def.map(|d| d.gating).unwrap_or(false) {
        Marker::Gate
    } else if def.map(|d| d.terminal).unwrap_or(false) {
        Marker::Done
    } else {
        Marker::Attention
    }
}

/// The marker for one task row, with the run's own halt classification allowed
/// to overrule the state-based reading.
///
/// A parent held open only by its own subtree is the eligibility rule working,
/// not something wrong — and since every ancestor of one gated leaf is halted
/// this way, classifying by state alone painted a whole spine of the tree red.
/// It reads as a deliberate pause instead, exactly like the gate that is
/// really holding it. A parent that is itself `blocked` or `failed` keeps its
/// own attention marker: that is wrong independently of its children.
// §FS-rhei-run-report.3.2
fn marker_for_task(
    id: &str,
    state: &str,
    machine: &rhei_validator::StateMachine,
    halt_causes: &HashMap<String, HaltCause>,
) -> Marker {
    // A held descendant is a deliberate pause, not work to act on: its
    // supervisor is the ticket that is owed a visit, and it takes the
    // Attention row. §FS-rhei-supervision.3.4
    if matches!(
        halt_causes.get(id),
        Some(
            HaltCause::HeldBySupervisor { .. }
                | HaltCause::WaitingOnPerson { .. }
                | HaltCause::ProviderLimited { .. }
        )
    ) && !state_is_failure(state)
    {
        return Marker::Gate;
    }
    if is_calm_parent(id, state, machine, halt_causes) {
        return Marker::Gate;
    }
    classify_marker(state, machine)
}

/// Whether this ticket is a parent held open *only* by its own subtree: the run
/// classified it as waiting on descendants, and its own state neither reads as
/// failure nor is pending a decision of its own.
///
/// Such a parent is not halted work — the open descendant is, and it reports
/// for itself. It therefore takes no Attention row, no `N gated` tally, no
/// `could not advance` count, and no blocked ledger entry; one gated leaf under
/// three ancestors otherwise produced four of each, with the topmost parent's
/// reason text repeating the whole transitive subtree. It keeps its calm marker
/// and its `waiting on open descendant …` detail in the task tree, which is
/// where the structure is worth showing.
///
/// A parent that is itself gating, `blocked`, or `failed` is excluded: those
/// are things to act on independently of what its children are doing, even
/// though the open subtree outranks them in the classification order.
// §FS-rhei-run-report.3.1 §FS-rhei-run-report.3.2 §FS-rhei-plan-language.3
fn is_calm_parent(
    id: &str,
    state: &str,
    machine: &rhei_validator::StateMachine,
    halt_causes: &HashMap<String, HaltCause>,
) -> bool {
    classify_marker(state, machine) == Marker::Attention
        && !state_is_failure(state)
        && matches!(halt_causes.get(id), Some(HaltCause::WaitingOnDescendants { .. }))
}

/// One row of the source-order task tree.
struct TaskRow {
    depth: usize,
    id: String,
    state: String,
    marker: Marker,
    /// Driver + timing for advanced tasks, or a short reason for halted ones.
    detail: Option<String>,
}

/// A halted task surfaced in the Attention group, with its proven blocker and
/// the next action. §FS-rhei-run-report.3.1
struct AttentionRow {
    id: String,
    state: String,
    reason: String,
    next: String,
    /// True for a deliberate pause — a gating state awaiting a human, or a
    /// parent held open by its own subtree; false for a blocked/failed task.
    /// It splits the `N gated · M blocked` header. §FS-rhei-run-report.3.1
    is_gate: bool,
    /// True for a poll waiting on a person. The Waiting group holds two unlike
    /// things — a subtree a supervisor is holding, and a wait on a human — and
    /// its count line names each rather than calling both "held".
    // §FS-rhei-states.2.5 §FS-rhei-run-report.3.1
    waits_on_person: bool,
    /// True for a durable provider wait, counted separately from supervision
    /// holds and human polling. §FS-rhei-run-report.3.1
    provider_limited: bool,
}

/// Run-level facts the summary needs beyond the plan itself. §FS-rhei-run-report.8
pub struct RunStats {
    pub agents_spawned: u32,
    pub programs_spawned: u32,
    pub callback_only: u32,
    pub duration: Option<std::time::Duration>,
    pub dashboard: Option<String>,
    /// Short run identifier shown in the header and history filename.
    pub run_id: String,
    /// Wall-clock start, rendered in the report header. `None` falls back to
    /// the run id alone.
    pub started_at: Option<std::time::SystemTime>,
    /// Workspace root, used to render relative artifact links. §FS-rhei-run-report.1
    pub workspace_root: std::path::PathBuf,
    /// The command label shown in the header (`rhei run …`).
    pub command: String,
    /// Worker parallelism for the run.
    pub parallel: usize,
    /// `"agent"` or `"callback"` execution mode.
    pub mode: &'static str,
    /// Task id → normalized state at run start, for terminal-at-start detection
    /// and reconciling callback advances that emit no slot events.
    /// §FS-rhei-run-report.8
    pub initial_states: HashMap<String, String>,
    /// True under `--dry-run`: the report records a simulated run that applied no
    /// changes, so its result line and counts read as a preview. §FS-rhei-run-report.3.5
    pub dry_run: bool,
    /// True when the run's own loop was cut short by a signal, captured where
    /// that loop ends: a run already finished when the signal arrived — parked
    /// on the TUI's finished screen — has a result of its own to report.
    // §FS-rhei-run.3.2
    pub interrupted: bool,
    /// How a run that selected `--until-idle` stopped, `None` for every run
    /// that did not. It is what moves the waiting tickets out of Attention and
    /// what gives the header its idle phrase, so both stay selected-only.
    // §FS-rhei-run-report.3.1
    pub stop: Option<rhei_tui::RunStop>,
}

/// One rendered Transition Ledger row. §FS-rhei-run-report.4
struct LedgerEntry {
    task: String,
    from: String,
    /// Destination state, or `-` when no transition was taken.
    to: String,
    /// `agent`, `program`, `callback-only`, `terminal-at-start`, or `blocked`.
    driver: &'static str,
    /// Invocation label + relative log link, or `none`.
    invocation: String,
    reason: String,
}

/// One spawned agent/program for the Invocations section. §FS-rhei-run-report.7
struct InvocationRow {
    driver: &'static str,
    task: String,
    /// `exit 0`, `exit 42`, `cancelled`, `timed out`, or `—`.
    exit: String,
    duration_ms: u64,
    /// Relative log path.
    log: String,
}

/// Direct accounting shown for a task in the end-of-run report.
// §FS-rhei-run-report.2.2
struct TaskAccountingRow {
    task: String,
    cost: String,
    tokens: AccountingTokenPresentation,
    coverage: String,
}

/// The fully resolved run report, ready to render to the console or to Markdown.
pub struct RunSummaryReport {
    title: String,
    result: String,
    duration: Option<std::time::Duration>,
    /// State label, count, and marker class, in canonical count order.
    state_counts: Vec<(String, usize, Marker)>,
    total_tasks: usize,
    work: String,
    accounting: RunAccountingStrip,
    attention: Vec<AttentionRow>,
    /// Tickets nobody has to act on because someone else's turn is what they
    /// are waiting for. Held descendants dilute Attention: a held ticket's own
    /// next action is "nothing to do on this ticket". §FS-rhei-supervision.3.4
    waiting: Vec<AttentionRow>,
    rows: Vec<TaskRow>,
    dashboard: Option<String>,
    // ── Durable-report fields (§FS-rhei-run-report.1, .2, .4, .7) ────────────
    run_id: String,
    started_at: Option<std::time::SystemTime>,
    workspace: String,
    command: String,
    parallel: usize,
    mode: &'static str,
    agents_spawned: u32,
    programs_spawned: u32,
    callback_only: u32,
    terminal_at_start: usize,
    ledger: Vec<LedgerEntry>,
    invocations: Vec<InvocationRow>,
    task_accounting: Vec<TaskAccountingRow>,
    /// Relative paths to the written report files, filled by [`write_to_runtime`].
    report_path: Option<String>,
    history_path: Option<String>,
    /// What the run was bounded by, and what it spent. §FS-rhei-run-report.3.1
    bounds: BoundsSection,
}

/// Each count dimension at the start and at the end of the run, and the halts.
///
/// Both ends, because the record answers "what was this bounded by" and "what
/// did it spend": one number cannot do both, and a reader who has only the
/// second cannot tell a run that finished from one that nearly did not.
/// §FS-rhei-run-report.3.1 §FS-rhei-budgets.9
#[derive(Clone, Default)]
struct BoundsSection {
    first: BTreeMap<String, rhei_tui::BoundReport>,
    last: BTreeMap<String, rhei_tui::BoundReport>,
    halts: Vec<(String, rhei_tui::BoundReport, Option<String>, String)>,
}

impl BoundsSection {
    fn is_empty(&self) -> bool {
        self.last.is_empty()
    }

    /// `80 (built_in)`, or the clamped form naming requester and limiter.
    /// §FS-rhei-budgets.2.3
    fn provenance(bound: &rhei_tui::BoundReport) -> String {
        match &bound.limiting_source {
            Some(limiter) => format!(
                "{} (requested by the {}, limited by {limiter} settings)",
                bound.effective, bound.value_source
            ),
            None => format!("{} ({})", bound.effective, bound.value_source),
        }
    }

    fn render_markdown(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        let mut out = String::from("## Bounds\n\n");
        out.push_str(
            "| Dimension | Bound | Mode | Consumed at start | Consumed at end | Remaining |\n\
             | --- | --- | --- | ---: | ---: | ---: |\n",
        );
        for (dimension, bound) in &self.last {
            let started = self.first.get(dimension).map(|b| b.consumed).unwrap_or(bound.consumed);
            out.push_str(&format!(
                "| {} | {} | {} | {started} | {} | {} |\n",
                md_cell(dimension),
                md_cell(&Self::provenance(bound)),
                md_cell(bound.window.as_deref().unwrap_or(&bound.mode)),
                bound.consumed,
                bound.remaining,
            ));
        }
        out.push('\n');
        for (task, bound, renews_at, remedy) in &self.halts {
            out.push_str(&format!(
                "- `{}` was stopped by its {} bound of {}; {}\n",
                task,
                bound.dimension,
                Self::provenance(bound),
                match renews_at {
                    Some(at) => format!("it renews at {at}"),
                    None => remedy.clone(),
                }
            ));
        }
        if !self.halts.is_empty() {
            out.push('\n');
        }
        out
    }
}

// ANSI codes; emitted only when color is enabled.
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";

/// Width of the static state-distribution bar, in cells.
const BAR_WIDTH: usize = 24;
/// Maximum task rows printed before fully-completed subtrees collapse.
const MAX_TASK_ROWS: usize = 40;
/// Maximum attention rows printed before the rest defer to the report.
const MAX_ATTENTION_ROWS: usize = 5;

impl RunSummaryReport {
    /// Build the report from the on-disk plan, the run's spawn counts, and the
    /// per-task activity captured by [`SummarySink`]. `task_roots` with
    /// `stats.workspace_root` are the roots the run's own ready-set scan
    /// resolved artifacts under, so the report explains a halted ticket exactly
    /// as the halt message did.
    // §FS-rhei-run-report.8 §AR-rhei-panta.5
    pub fn build(
        rhei: &rhei_core::ast::Rhei,
        machines: &rhei_validator::MachineSet,
        summary: &SummarySink,
        stats: RunStats,
        plan_arg: &str,
        task_roots: &std::collections::HashMap<String, std::path::PathBuf>,
    ) -> Self {
        let activity = summary.snapshot();
        // Read once: the ledger answers both "why is this ticket halted" below
        // and the report's own Transition Ledger further down.
        let ledger_records = summary.ledger();
        let ledger = &ledger_records;

        // Why each halted ticket is halted, resolved once against the whole
        // plan. The table below needs the plan's priors and claims, which a
        // per-task walk cannot see. §FS-rhei-run-report.3.1
        let halt_causes: HashMap<String, HaltCause> = classify_halted_tasks(
            rhei,
            machines,
            &None,
            &|id| activity.contains_key(id),
            // §FS-rhei-run-report.3.1: what the ticket's last worker left
            // unwritten and the code it exited with, captured live rather
            // than re-read from prose, while the ticket sits in the state it stalled in.
            &|id, state| {
                activity.get(id).and_then(|entry| {
                    entry
                        .missing_outputs
                        .as_ref()
                        .filter(|(stalled_in, entries)| stalled_in == state && !entries.is_empty())
                        .map(|(_, entries)| UnwrittenOutputs {
                            entries: entries.clone(),
                            exit_code: entry.last_exit_code,
                        })
                })
            },
            // Only the ticket's *last* invocation explains where it is, and
            // only for a run the operator stopped: a failing run ends its
            // workers the same way. §FS-rhei-run-report.3.1 §FS-rhei-run.3.2
            &|id| {
                stats.interrupted
                    && matches!(
                        ledger
                            .iter()
                            .rev()
                            .find(|record| record.task == id)
                            .map(|record| &record.outcome),
                        Some(LedgerOutcome::Interrupted)
                    )
            },
            plan_arg,
            // The run's own roots, so every surface of one run agrees. §AR-rhei-panta.5
            &ReadySetRoots { workspace_root: &stats.workspace_root, task_roots },
        )
        .into_iter()
        .map(|(task, cause)| (task.id.to_string(), cause))
        .collect();

        // Source-order walk that preserves hierarchy depth.
        let mut rows = Vec::new();
        let mut attention = Vec::new();
        let mut waiting = Vec::new();
        let mut counts: std::collections::BTreeMap<String, (usize, Marker)> =
            std::collections::BTreeMap::new();
        collect_rows(
            &rhei.tasks,
            0,
            machines,
            &activity,
            &halt_causes,
            &mut rows,
            &mut attention,
            &mut waiting,
            &mut counts,
        );

        // An idle return is a run with nothing for anyone to act on: every
        // ticket it left behind is waiting on somebody else's turn, and a
        // non-empty Attention would say the opposite. Both groups are filled
        // from one classification, so this moves rows rather than
        // reclassifying them, and only a selected run reaches it at all.

        // §FS-rhei-run-report.3.1: Attention is empty on every idle return.
        let idle = stats
            .stop
            .as_ref()
            .filter(|stop| stop.reason == rhei_tui::StopReason::Idle)
            .cloned();
        if idle.is_some() {
            waiting.append(&mut attention);
        }

        // Terminal-at-start: same terminal state at run start as now, so no work
        // was attempted. The row keeps its state count but flips to the calm `·`
        // marker so it reads apart from work that just ran. §FS-rhei-run-report.3.2
        let mut terminal_at_start = 0usize;
        for row in &mut rows {
            let was = stats.initial_states.get(&row.id).map(String::as_str);
            let unchanged_terminal = was == Some(row.state.as_str())
                && is_terminal_state(&row.state, machines.for_task(&parse_task_id(&row.id)));
            if unchanged_terminal {
                terminal_at_start += 1;
                // A success state flips to the calm `·` marker; a cancelled task
                // keeps its own `⊘` marker but still counts as terminal-at-start.
                if row.marker == Marker::Done {
                    row.marker = Marker::TerminalAtStart;
                    row.detail = Some("terminal at start".to_string());
                }
            }
        }

        let total_tasks = rows.len();

        // Counts in canonical order: success, gate, attention, cancelled.
        let mut state_counts: Vec<(String, usize, Marker)> =
            counts.into_iter().map(|(state, (n, marker))| (state, n, marker)).collect();
        state_counts.sort_by_key(|(_, _, marker)| marker_order(*marker));

        let no_work = stats.agents_spawned == 0 && stats.programs_spawned == 0;
        let advanced_without_work = rows.iter().any(|r| {
            r.marker == Marker::Done
                && stats.initial_states.get(&r.id).map(String::as_str) != Some(r.state.as_str())
        });
        // A dry run simulated transitions but applied nothing, so its result
        // reads as a preview rather than an outcome. §FS-rhei-run-report.3.5
        let result = if stats.dry_run {
            "dry run — no changes applied".to_string()
        } else {
            // Why the loop ended, as the caller read it when it ended (see
            // `result_phrase`). §FS-rhei-run.3.2 §FS-rhei-run-report.3.1
            result_phrase(
                &attention,
                &rows,
                no_work,
                advanced_without_work,
                stats.interrupted,
                idle.as_ref(),
            )
        };
        let work = format_work(stats.agents_spawned, stats.programs_spawned, stats.callback_only);
        let accounting = summary.accounting();
        let task_accounting = build_task_accounting_rows(&rows, &activity);

        let ledger_rows = build_ledger(
            &rows,
            &attention,
            &waiting,
            &halt_causes,
            ledger,
            &stats.initial_states,
            machines,
            &stats.workspace_root,
        );
        let invocations = build_invocations(ledger, &stats.workspace_root);

        Self {
            title: rhei.title.clone(),
            result,
            bounds: summary.bounds(),
            duration: stats.duration,
            state_counts,
            total_tasks,
            work,
            accounting,
            attention,
            waiting,
            rows,
            dashboard: stats.dashboard,
            run_id: stats.run_id,
            started_at: stats.started_at,
            workspace: stats.workspace_root.display().to_string(),
            command: stats.command,
            parallel: stats.parallel,
            mode: stats.mode,
            agents_spawned: stats.agents_spawned,
            programs_spawned: stats.programs_spawned,
            callback_only: stats.callback_only,
            terminal_at_start,
            ledger: ledger_rows,
            invocations,
            task_accounting,
            report_path: None,
            history_path: None,
        }
    }

    /// Render the rich, colored summary for an interactive terminal.
    /// §FS-rhei-run-report.3.1
    pub fn render_tty(&self, color: bool) -> String {
        let c = Palette::new(color);
        let mut out = String::new();

        // Header: title + duration, then the result line.
        let dur = self.duration.map(format_duration_long).unwrap_or_default();
        out.push_str(&format!(
            "\n{}Run Report{}  {}{}{}",
            c.bold, c.reset, c.bold, self.title, c.reset
        ));
        if !dur.is_empty() {
            out.push_str(&format!("   {}{}{}", c.dim, dur, c.reset));
        }
        out.push('\n');
        out.push_str(&format!("  {}{}{}\n\n", c.result_color(&self.result), self.result, c.reset));

        // Counts: distribution bar + labeled states, then work.
        out.push_str("  States    ");
        out.push_str(&self.render_bar(&c));
        out.push_str("   ");
        out.push_str(&self.render_state_labels(&c));
        out.push('\n');
        out.push_str(&format!("  Work      {}\n", self.work));
        out.push_str(&self.accounting.render_console());

        // Attention.
        if !self.attention.is_empty() {
            let gated = self.attention.iter().filter(|a| a.is_gate).count();
            let blocked = self.attention.len() - gated;
            out.push_str(&format!(
                "\n{}Attention{}  {} gated · {} blocked\n",
                c.bold, c.reset, gated, blocked
            ));
            for row in self.attention.iter().take(MAX_ATTENTION_ROWS) {
                out.push_str(&format!(
                    "  {}!{} {:<26} {}{:<11}{} {}\n",
                    c.red, c.reset, row.id, c.dim, row.state, c.reset, row.reason
                ));
                out.push_str(&format!("        {}→ {}{}\n", c.dim, row.next, c.reset));
            }
            if self.attention.len() > MAX_ATTENTION_ROWS {
                out.push_str(&format!(
                    "  {}… {} more in the report{}\n",
                    c.dim,
                    self.attention.len() - MAX_ATTENTION_ROWS,
                    c.reset
                ));
            }
        }

        // Waiting — held tickets and person-waiting polls, neither of which is
        // anyone's action item. §FS-rhei-supervision.3.4 §FS-rhei-states.2.5
        if !self.waiting.is_empty() {
            out.push_str(&format!(
                "\n{}Waiting{}    {}\n",
                c.bold,
                c.reset,
                waiting_tally(&self.waiting)
            ));
            for row in self.waiting.iter().take(MAX_ATTENTION_ROWS) {
                out.push_str(&format!(
                    "  {}\u{23f8}{} {:<26} {}{:<11}{} {}\n",
                    c.dim, c.reset, row.id, c.dim, row.state, c.reset, row.reason
                ));
            }
            if self.waiting.len() > MAX_ATTENTION_ROWS {
                out.push_str(&format!(
                    "  {}\u{2026} {} more in the report{}\n",
                    c.dim,
                    self.waiting.len() - MAX_ATTENTION_ROWS,
                    c.reset
                ));
            }
        }

        // Task tree.
        out.push_str(&format!(
            "\n{}Tasks{}   {} tasks · source order\n",
            c.bold, c.reset, self.total_tasks
        ));
        out.push_str(&self.render_tree(&c));

        // Pointers: the durable report is the at-a-glance summary's companion;
        // the console points at it for the full forensic read. §FS-rhei-run-report.3.1
        out.push('\n');
        if let Some(report) = &self.report_path {
            out.push_str(&format!("Report     {report}\n"));
        }
        if let Some(history) = &self.history_path {
            out.push_str(&format!("History    {history}\n"));
        }
        if let Some(dashboard) = &self.dashboard {
            out.push_str(&format!("Dashboard  {dashboard}\n"));
        }
        // Drop trailing spaces left by empty detail columns; keep the final newline.
        let trailing_newline = out.ends_with('\n');
        let mut trimmed = out.lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
        if trailing_newline {
            trimmed.push('\n');
        }
        trimmed
    }

    /// Render the durable Markdown report — header, outcome strip, attention,
    /// ledger, task final states, invocations: the commit-friendly explanation
    /// an operator can read without the dashboard. §FS-rhei-run-report.1 §FS-rhei-run-report.2
    pub fn render_markdown(&self) -> String {
        let mut out = String::new();

        // 1. Header.
        out.push_str(&format!("# Run Report: {}\n\n", self.title));
        let when = self
            .started_at
            .map(format_iso8601_utc)
            .map(|ts| format!("{ts} / {}", self.run_id))
            .unwrap_or_else(|| self.run_id.clone());
        out.push_str(&format!("Run: {when}\n"));
        out.push_str(&format!("Workspace: {}\n", self.workspace));
        out.push_str(&format!("Command: {}\n", self.command));
        out.push_str(&format!("Mode: {} · parallel {}\n", self.mode, self.parallel));
        if let Some(dur) = self.duration {
            out.push_str(&format!("Duration: {}\n", format_duration_long(dur)));
        }
        out.push_str(&format!("Result: {}\n", self.result));
        if let Some(dashboard) = &self.dashboard {
            out.push_str(&format!("Dashboard: {dashboard}\n"));
        }
        out.push('\n');

        // 2. Outcome strip — final states and run activity. The reuse/blocked
        // signal sits at the top of the report, never below a fold.
        out.push_str("| Final states | Count |\n| --- | ---: |\n");
        for (state, n, _) in &self.state_counts {
            out.push_str(&format!("| {state} | {n} |\n"));
        }
        out.push('\n');
        let could_not_advance = self.attention.len();
        out.push_str("| Activity | Count |\n| --- | ---: |\n");
        out.push_str(&format!("| agent invocations | {} |\n", self.agents_spawned));
        out.push_str(&format!("| program invocations | {} |\n", self.programs_spawned));
        out.push_str(&format!("| callback-only transitions | {} |\n", self.callback_only));
        out.push_str(&format!("| terminal at start | {} |\n", self.terminal_at_start));
        out.push_str(&format!("| could not advance | {could_not_advance} |\n"));
        out.push('\n');
        out.push_str(&self.accounting.render_markdown());
        if self.agents_spawned == 0 && self.programs_spawned == 0 {
            out.push_str(
                "> No agent or program ran this run. Any task that advanced did so through \
                 callbacks, transition rules, or outputs that already existed — inspect the \
                 ledger below before assuming work was performed.\n\n",
            );
        }

        // 3. Attention.
        if !self.attention.is_empty() {
            out.push_str("## Attention\n\n");
            out.push_str("| Task | State | Reason | Next action |\n| --- | --- | --- | --- |\n");
            for a in &self.attention {
                out.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    md_cell(&a.id),
                    md_cell(&a.state),
                    md_cell(&a.reason),
                    md_cell(&a.next),
                ));
            }
            out.push('\n');
        }

        // 3a. Bounds — what the run was bounded by and what it spent, so the
        // record answers "what stopped it" without reconstructing it from rows.
        // §FS-rhei-run-report.3.1
        out.push_str(&self.bounds.render_markdown());

        // 3b. Waiting — held tickets, kept out of Attention so the rows a
        // person must act on stay undiluted. §FS-rhei-supervision.3.4
        if !self.waiting.is_empty() {
            out.push_str("## Waiting\n\n");
            out.push_str("| Task | State | Reason | Next action |\n| --- | --- | --- | --- |\n");
            for row in &self.waiting {
                out.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    md_cell(&row.id),
                    md_cell(&row.state),
                    md_cell(&row.reason),
                    md_cell(&row.next),
                ));
            }
            out.push('\n');
        }

        // 4. Transition ledger.
        out.push_str("## Transition Ledger\n\n");
        out.push_str(
            "| Task | From | To | Driver | Invocation | Reason |\n\
             | --- | --- | --- | --- | --- | --- |\n",
        );
        for e in &self.ledger {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                e.task,
                md_cell(&e.from),
                md_cell(&e.to),
                e.driver,
                md_link_or_text(&e.invocation),
                md_cell(&e.reason),
            ));
        }
        out.push('\n');

        // 5. Task final states.
        out.push_str("## Task Final States\n\n");
        for row in &self.rows {
            let indent = "  ".repeat(row.depth);
            let detail = row.detail.as_deref().unwrap_or("");
            let detail = if detail.is_empty() {
                String::new()
            } else {
                format!(" — {detail}")
            };
            out.push_str(&format!(
                "{indent}- {} `{}` ({}){detail}\n",
                row.marker.glyph(),
                row.id,
                row.state,
            ));
        }
        out.push('\n');

        if !self.task_accounting.is_empty() {
            // The shared projection fixes both column order and cache-part
            // visibility for every direct task rollup. §FS-rhei-run-report.2.2
            out.push_str("## Task Costs\n\n");
            out.push_str("| Task | Cost");
            for dimension in ACCOUNTING_TOKEN_DIMENSIONS {
                out.push_str(&format!(" | {}", dimension.column_heading()));
            }
            out.push_str(" | Coverage |\n| --- | ---:");
            for _ in ACCOUNTING_TOKEN_DIMENSIONS {
                out.push_str(" | ---:");
            }
            out.push_str(" | --- |\n");
            for row in &self.task_accounting {
                out.push_str(&format!("| {} | {}", md_cell(&row.task), row.cost));
                for value in row.tokens.values() {
                    out.push_str(&format!(" | {value}"));
                }
                out.push_str(&format!(" | {} |\n", row.coverage));
            }
            out.push('\n');
        }

        // 6. Invocations.
        if !self.invocations.is_empty() {
            out.push_str("## Invocations\n\n");
            out.push_str(
                "| Task | Driver | Exit | Duration | Log |\n| --- | --- | --- | --- | --- |\n",
            );
            for inv in &self.invocations {
                out.push_str(&format!(
                    "| {} | {} | {} | {} | [{}]({}) |\n",
                    inv.task,
                    inv.driver,
                    inv.exit,
                    format_duration_short(inv.duration_ms),
                    inv.log,
                    inv.log,
                ));
            }
            out.push('\n');
        }

        out
    }

    /// Write the durable report to `runtime/run-report.md` and a timestamped
    /// history entry, recording the relative paths for the console pointer.
    /// Best-effort. §FS-rhei-run-report.1
    pub fn write_to_runtime(&mut self, runtime_dir: &std::path::Path) -> std::io::Result<()> {
        let body = self.render_markdown();
        let latest = runtime_dir.join("run-report.md");
        let history_dir = runtime_dir.join("run-reports");
        std::fs::create_dir_all(&history_dir)?;
        let stamp = self
            .started_at
            .map(format_iso8601_utc)
            .map(|ts| ts.replace(':', "-"))
            .unwrap_or_else(|| "unknown".to_string());
        let history = history_dir.join(format!("{stamp}-{}.md", self.run_id));
        std::fs::write(&latest, &body)?;
        std::fs::write(&history, &body)?;
        self.report_path = Some(relativize(&latest, &self.workspace_root_path()));
        self.history_path = Some(relativize(&history, &self.workspace_root_path()));
        Ok(())
    }

    /// The workspace root reconstructed from its display string, for link bases.
    fn workspace_root_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.workspace)
    }

    /// The static state-distribution bar, sized by count and colored by class.
    /// Drawn once; never animates. §FS-rhei-run-report.3.1 §FS-rhei-viz-ux.4
    fn render_bar(&self, c: &Palette) -> String {
        if self.total_tasks == 0 {
            return String::new();
        }
        // Proportional widths, with at least one cell per non-empty state.
        let mut widths: Vec<usize> = self
            .state_counts
            .iter()
            .map(|(_, n, _)| {
                let w = (*n * BAR_WIDTH) / self.total_tasks;
                if *n > 0 {
                    w.max(1)
                } else {
                    0
                }
            })
            .collect();
        // Trim overflow from the largest segment so total == BAR_WIDTH.
        let mut total: usize = widths.iter().sum();
        while total > BAR_WIDTH {
            if let Some((idx, _)) =
                widths.iter().enumerate().filter(|(_, w)| **w > 1).max_by_key(|(_, w)| **w)
            {
                widths[idx] -= 1;
                total -= 1;
            } else {
                break;
            }
        }
        let mut bar = String::new();
        for ((_, _, marker), w) in self.state_counts.iter().zip(widths) {
            if w == 0 {
                continue;
            }
            bar.push_str(c.color(marker.color()));
            bar.push_str(&"█".repeat(w));
            bar.push_str(c.reset);
        }
        bar
    }

    fn render_state_labels(&self, c: &Palette) -> String {
        self.state_counts
            .iter()
            .map(|(state, n, marker)| {
                format!("{}{} {}{}", c.color(marker.color()), n, state, c.reset)
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }

    fn render_tree(&self, c: &Palette) -> String {
        let mut out = String::new();
        let mut collapsed = 0usize;
        let mut shown = 0usize;
        for row in &self.rows {
            // Collapse calm completed leaf rows once the tree grows long, but
            // never hide anything that needs a human. §FS-rhei-run-report.3.2
            if shown >= MAX_TASK_ROWS && row.marker == Marker::Done {
                collapsed += 1;
                continue;
            }
            shown += 1;
            let gutter = if row.depth > 0 { "│ ".repeat(row.depth) } else { String::new() };
            let detail = row.detail.as_deref().unwrap_or("");
            // Pad the state column *outside* the color codes so that empty-detail
            // rows can have their trailing padding trimmed away.
            let state_cell = c.colored(row.marker.color(), &row.state);
            let state_pad = " ".repeat(11usize.saturating_sub(row.state.chars().count()));
            out.push_str(&format!(
                "  {}{}{}{} {:<width$} {}{} {}\n",
                c.dim,
                gutter,
                c.reset,
                c.colored(row.marker.color(), &row.marker.glyph().to_string()),
                row.id,
                state_cell,
                state_pad,
                detail,
                width = 26usize.saturating_sub(row.depth * 2),
            ));
        }
        if collapsed > 0 {
            out.push_str(&format!(
                "  {}… {collapsed} completed tasks collapsed{}\n",
                c.dim, c.reset
            ));
        }
        out
    }
}

/// Recursive source-order walk capturing depth, markers, detail, counts, and
/// the attention list.
#[allow(clippy::too_many_arguments)]
fn collect_rows(
    tasks: &[rhei_core::ast::Task],
    depth: usize,
    machines: &rhei_validator::MachineSet,
    activity: &HashMap<String, TaskActivity>,
    halt_causes: &HashMap<String, HaltCause>,
    rows: &mut Vec<TaskRow>,
    attention: &mut Vec<AttentionRow>,
    waiting: &mut Vec<AttentionRow>,
    counts: &mut std::collections::BTreeMap<String, (usize, Marker)>,
) {
    for task in tasks {
        let machine = machines.for_task(&task.id);
        let state = normalized_state_name(task.state.as_str(), machine);
        let id = task.id.to_string();
        let marker = marker_for_task(&id, &state, machine, halt_causes);

        let entry = counts.entry(state.clone()).or_insert((0, marker));
        entry.0 += 1;

        let detail = task_detail(&id, &state, marker, halt_causes, activity);
        // §FS-rhei-run-report.3.1: a parent held open by its own subtree is not
        // halted work, so it is counted nowhere the descendant is already
        // counted — see [`is_calm_parent`].
        if marker.needs_attention() && !is_calm_parent(&id, &state, machine, halt_causes) {
            let (reason, next) = attention_reason(marker, &id, &state, halt_causes);
            let row = AttentionRow {
                id: id.clone(),
                state: state.clone(),
                reason,
                next,
                is_gate: marker == Marker::Gate,
                waits_on_person: matches!(
                    halt_causes.get(&id),
                    Some(HaltCause::WaitingOnPerson { .. })
                ),
                provider_limited: matches!(
                    halt_causes.get(&id),
                    Some(HaltCause::ProviderLimited { .. })
                ),
            };
            // A held subtree and a person wait are both somebody else's turn:
            // Waiting holds them, undiluting the rows a human must act on.
            // §FS-rhei-supervision.3.4 §FS-rhei-states.2.5
            if matches!(
                halt_causes.get(&id),
                Some(
                    HaltCause::HeldBySupervisor { .. }
                        | HaltCause::WaitingOnPerson { .. }
                        | HaltCause::ProviderLimited { .. }
                )
            ) {
                waiting.push(row);
            } else {
                attention.push(row);
            }
        }

        rows.push(TaskRow { depth, id, state, marker, detail });
        collect_rows(
            &task.children,
            depth + 1,
            machines,
            activity,
            halt_causes,
            rows,
            attention,
            waiting,
            counts,
        );
    }
}

/// Build the detail column for a task row: driver + timing when the run spawned
/// work, otherwise a short reason for halted tasks. §FS-rhei-run-report.3.2
fn task_detail(
    id: &str,
    state: &str,
    marker: Marker,
    halt_causes: &HashMap<String, HaltCause>,
    activity: &HashMap<String, TaskActivity>,
) -> Option<String> {
    if let Some(act) = activity.get(id) {
        let cost = act
            .accounting
            .as_ref()
            .map(|accounting| format!(" · {}", format_summary_cost(accounting)))
            .unwrap_or_default();
        if let Some(driver) = act.driver {
            let label = if act.invocations > 1 {
                format!("{driver}×{}", act.invocations)
            } else {
                driver.to_string()
            };
            return Some(format!(
                "{label}  {}{}",
                format_duration_short(act.last_duration_ms),
                cost
            ));
        }
        if !cost.is_empty() {
            return Some(cost.trim_start_matches(" · ").to_string());
        }
    }
    match marker {
        Marker::Gate | Marker::Attention => {
            Some(attention_reason(marker, id, state, halt_causes).0)
        }
        _ => None,
    }
}

/// Project each task's direct rollup into the symmetric token columns.
/// §FS-rhei-run-report.2.2
fn build_task_accounting_rows(
    rows: &[TaskRow],
    activity: &HashMap<String, TaskActivity>,
) -> Vec<TaskAccountingRow> {
    rows.iter()
        .filter_map(|row| {
            let accounting = activity.get(&row.id)?.accounting.as_ref()?;
            Some(TaskAccountingRow {
                task: row.id.clone(),
                cost: format_summary_cost(accounting),
                tokens: AccountingTokenPresentation::new(accounting),
                coverage: format!("{:?}", accounting.coverage),
            })
        })
        .collect()
}

/// The reason and next action for a halted task.
///
/// The plan-wide classification knows whether the
/// ticket is claimed, waiting on a prior, or manual-only, and names the command
/// that clears each. Reporting all three as "stalled in non-terminal state <s>"
/// and advising "inspect logs or mark the task cancelled" told an operator to
/// cancel work that only needed a claim released, and pointed at logs a run
/// that spawned nothing never wrote. The generic pair remains the fallback for
/// a ticket the classifier does not reach.
// §FS-rhei-run-report.3.1
fn attention_reason(
    marker: Marker,
    id: &str,
    state: &str,
    halt_causes: &HashMap<String, HaltCause>,
) -> (String, String) {
    if let Some(cause) = halt_causes.get(id) {
        return cause.describe(id, state);
    }
    match marker {
        Marker::Gate => HaltCause::Gate.describe(id, state),
        _ => HaltCause::Stalled.describe(id, state),
    }
}

/// The run's one-line outcome.
///
/// `interrupted` outranks everything else: the operator stopped the run, so
/// whatever the plan looks like now is a snapshot of work in progress and not a
/// verdict on it. Reading it as "stopped for human attention" told the operator
/// to go and act on tickets whose only problem was that they were interrupted.
///
/// The caller passes the *signal* reading of the stop token, not the bare one:
/// a run unwinding from an error raises it too, on its way to tearing down the
/// groups it still owned, and that run has a verdict of its own. It passes the
/// reading taken where its loop ended, not one taken here: a run that had
/// already finished when the signal arrived was not cut short by it.
// §FS-rhei-run-report.3.1 §FS-rhei-run.3.2
fn result_phrase(
    attention: &[AttentionRow],
    rows: &[TaskRow],
    no_work: bool,
    advanced_without_work: bool,
    // Named for the reading, not for the function that takes it: spelling this
    // `interrupted_by_signal` put the free function of that name in scope
    // beside a parameter shadowing it, and made "ask the token here" — the one
    // thing the paragraph above forbids — a one-character edit that compiles.
    cut_short_by_signal: bool,
    // The idle stop, when this run selected `--until-idle` and reached one. It
    // ranks below both readings above it: an interrupt is a run the operator
    // stopped, and one actionable ticket defeats idle. §FS-rhei-run-report.3.1
    idle: Option<&rhei_tui::RunStop>,
) -> String {
    let all_terminal_success =
        rows.iter().all(|r| matches!(r.marker, Marker::Done | Marker::TerminalAtStart));
    if cut_short_by_signal {
        "interrupted — re-run to continue".to_string()
    } else if !attention.is_empty() {
        // Gated and blocked tasks both halt the run for a human; the report and
        // tree carry the per-task distinction. §FS-rhei-run-report.6
        "stopped for human attention".to_string()
    } else if let Some((kind, stop)) =
        idle.and_then(|stop| stop.idle_blocker.map(|kind| (kind, stop)))
    {
        kind.result_phrase(stop.next_attempt_at.as_deref())
    } else if all_terminal_success && no_work && advanced_without_work {
        // A run that advanced tasks while spawning nothing must not read like a
        // fast successful run — name the absence of work. §FS-rhei-run-report.3.3
        "completed — no work spawned".to_string()
    } else if all_terminal_success {
        "completed".to_string()
    } else {
        "finished".to_string()
    }
}

/// What the Waiting group holds, named by kind: `2 held`, `1 waiting on a
/// person`, or both joined. A group of held tickets alone reads exactly as it
/// did before the second kind joined it.
// §FS-rhei-run-report.3.1 §FS-rhei-states.2.5
fn waiting_tally(waiting: &[AttentionRow]) -> String {
    let people = waiting.iter().filter(|row| row.waits_on_person).count();
    let provider = waiting.iter().filter(|row| row.provider_limited).count();
    let held = waiting.len() - people - provider;
    let mut parts = Vec::new();
    if held > 0 {
        parts.push(format!("{held} held"));
    }
    if people > 0 {
        parts.push(format!("{people} waiting on a person"));
    }
    if provider > 0 {
        parts.push(format!("{provider} provider-limited"));
    }
    parts.join(" \u{b7} ")
}

/// Escape a value for a Markdown table cell: pipes would split the column and
/// newlines would break the row, so both are neutralized.
fn md_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

/// Render an invocation cell. `"<driver> / <log>"` becomes `<driver> / [log](log)`
/// so the log is a relative link; anything else (notably `none`) is escaped text.
/// §FS-rhei-run-report.7
fn md_link_or_text(value: &str) -> String {
    match value.split_once(" / ") {
        Some((label, path)) => format!("{} / [{}]({})", md_cell(label), path, path),
        None => md_cell(value),
    }
}

/// Render a path relative to the workspace root with forward slashes, so report
/// links survive the workspace being moved, committed, or pasted into an issue.
/// §FS-rhei-run-report.1
fn relativize(path: &std::path::Path, root: &std::path::Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// A short reason string for a spawned invocation, from its outcome and exit.
fn ledger_outcome_reason(outcome: &LedgerOutcome, exit_code: Option<i32>) -> String {
    match outcome {
        LedgerOutcome::Completed => match exit_code {
            Some(0) | None => "exit 0".to_string(),
            Some(code) => format!("exit {code}"),
        },
        LedgerOutcome::Failed(msg) => {
            let msg = msg.lines().next().unwrap_or("").trim();
            match exit_code {
                Some(code) if msg.is_empty() => format!("failed, exit {code}"),
                Some(code) => format!("exit {code}: {msg}"),
                None if msg.is_empty() => "failed".to_string(),
                None => format!("failed: {msg}"),
            }
        }
        // The reason a poll wait gives is the wait, with the exit the machine
        // declared as "come back later" kept beside it. §FS-rhei-states.2.2
        LedgerOutcome::Waiting => match exit_code {
            Some(code) => format!("poll delay, exit {code}"),
            None => "poll delay".to_string(),
        },
        LedgerOutcome::ProviderLimited { provider, next_attempt_at } => {
            format!("provider {provider} limited until {next_attempt_at}")
        }
        LedgerOutcome::Cancelled => "cancelled".to_string(),
        LedgerOutcome::TimedOut => "timed out".to_string(),
        // Not a verdict on the ticket: the run stopped the worker. §FS-rhei-run.3.2
        LedgerOutcome::Interrupted => "interrupted".to_string(),
        // The word the stream used, kept rather than translated into one this
        // build knows. §FS-rhei-run-json.2.2
        LedgerOutcome::Unrecognized(name) => match exit_code {
            Some(code) => format!("{name}, exit {code}"),
            None => name.clone(),
        },
    }
}

/// Assemble the Transition Ledger in source order: spawned rows from the event
/// stream, plus synthesized callback / terminal-at-start / blocked rows for tasks
/// that emit no slot events. §FS-rhei-run-report.4
#[allow(clippy::too_many_arguments)]
fn build_ledger(
    rows: &[TaskRow],
    attention: &[AttentionRow],
    waiting: &[AttentionRow],
    halt_causes: &HashMap<String, HaltCause>,
    records: &[LedgerRecord],
    initial_states: &HashMap<String, String>,
    machines: &rhei_validator::MachineSet,
    workspace_root: &std::path::Path,
) -> Vec<LedgerEntry> {
    // Both groups, because a ticket's blocked row must give the reason its
    // summary row gave — a person-waiting poll's is the label, not a fallback
    // that calls the wait a stall. §FS-rhei-run-report.4
    let attention_by_id: HashMap<&str, &AttentionRow> =
        attention.iter().chain(waiting).map(|a| (a.id.as_str(), a)).collect();
    let mut ledger = Vec::new();
    for row in rows {
        let task_records: Vec<&LedgerRecord> =
            records.iter().filter(|r| r.task == row.id).collect();
        if !task_records.is_empty() {
            for rec in &task_records {
                let log = relativize(&rec.log_path, workspace_root);
                ledger.push(LedgerEntry {
                    task: row.id.clone(),
                    from: rec.from.clone(),
                    to: if matches!(rec.outcome, LedgerOutcome::ProviderLimited { .. }) {
                        "-".to_string()
                    } else {
                        rec.to.clone()
                    },
                    driver: rec.driver,
                    invocation: format!("{} / {}", rec.driver, log),
                    reason: ledger_outcome_reason(&rec.outcome, rec.exit_code),
                });
            }
            // If the task ended in a terminal-success state past the last spawned
            // transition, a callback or transition rule carried it the rest of the
            // way — record that advance so the ledger reaches the final state.
            let last_to = task_records.last().map(|r| r.to.as_str());
            if matches!(row.marker, Marker::Done | Marker::TerminalAtStart)
                && last_to != Some(row.state.as_str())
            {
                ledger.push(LedgerEntry {
                    task: row.id.clone(),
                    from: last_to.unwrap_or("").to_string(),
                    to: row.state.clone(),
                    driver: "callback-only",
                    invocation: "none".to_string(),
                    reason: "advanced without spawning work".to_string(),
                });
            }
            continue;
        }

        // No invocation ran for this task this run — classify why it sits where
        // it does from the plan and the initial-state snapshot.
        let initial = initial_states.get(&row.id).map(String::as_str);
        if row.marker == Marker::TerminalAtStart {
            ledger.push(LedgerEntry {
                task: row.id.clone(),
                from: row.state.clone(),
                to: "-".to_string(),
                driver: "terminal-at-start",
                invocation: "none".to_string(),
                reason: "already terminal".to_string(),
            });
        } else if matches!(row.marker, Marker::Attention | Marker::Gate)
            // §FS-rhei-run-report.4: the parent is not a blocked row of its own
            // — see [`is_calm_parent`].
            && !is_calm_parent(
                &row.id,
                &row.state,
                machines.for_task(&parse_task_id(&row.id)),
                halt_causes,
            )
        {
            let reason = attention_by_id
                .get(row.id.as_str())
                .map(|a| a.reason.clone())
                .unwrap_or_else(|| format!("stalled in non-terminal state {}", row.state));
            ledger.push(LedgerEntry {
                task: row.id.clone(),
                from: row.state.clone(),
                to: "-".to_string(),
                driver: "blocked",
                invocation: "none".to_string(),
                reason,
            });
        } else if initial != Some(row.state.as_str()) {
            // Advanced to a new state without spawning a subprocess: callbacks,
            // transition rules, or already-present outputs carried it forward.
            ledger.push(LedgerEntry {
                task: row.id.clone(),
                from: initial.unwrap_or("").to_string(),
                to: row.state.clone(),
                driver: "callback-only",
                invocation: "none".to_string(),
                reason: "advanced without spawning work".to_string(),
            });
        } else if is_terminal_state(&row.state, machines.for_task(&parse_task_id(&row.id))) {
            ledger.push(LedgerEntry {
                task: row.id.clone(),
                from: row.state.clone(),
                to: "-".to_string(),
                driver: "terminal-at-start",
                invocation: "none".to_string(),
                reason: "already terminal".to_string(),
            });
        }
    }
    ledger
}

/// Collect spawned agents/programs for the Invocations section. §FS-rhei-run-report.7
fn build_invocations(
    records: &[LedgerRecord],
    workspace_root: &std::path::Path,
) -> Vec<InvocationRow> {
    records
        .iter()
        .map(|rec| InvocationRow {
            driver: rec.driver,
            task: rec.task.clone(),
            exit: match (&rec.outcome, rec.exit_code) {
                (LedgerOutcome::Cancelled, _) => "cancelled".to_string(),
                (LedgerOutcome::TimedOut, _) => "timed out".to_string(),
                (LedgerOutcome::Interrupted, _) => "interrupted".to_string(),
                (_, Some(code)) => format!("exit {code}"),
                (_, None) => "—".to_string(),
            },
            duration_ms: rec.duration_ms,
            log: relativize(&rec.log_path, workspace_root),
        })
        .collect()
}

fn format_work(agents: u32, programs: u32, callback_only: u32) -> String {
    let mut parts = vec![format!("{agents} agents"), format!("{programs} programs")];
    if callback_only > 0 {
        parts.push(format!("{callback_only} callback-only"));
    }
    parts.join(" · ")
}

fn marker_order(marker: Marker) -> u8 {
    match marker {
        Marker::Done => 0,
        Marker::Gate => 1,
        Marker::Attention => 2,
        Marker::Cancelled => 3,
        Marker::TerminalAtStart => 4,
    }
}

fn format_duration_short(ms: u64) -> String {
    if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}m{:02}s", ms / 60_000, (ms % 60_000) / 1000)
    }
}

fn format_duration_long(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        format!("{}m{:02}s", secs / 60, secs % 60)
    }
}

/// ANSI palette gated by a single `color` flag, so the renderer stays one code
/// path for both colored and plain output.
struct Palette {
    color: bool,
    reset: &'static str,
    bold: &'static str,
    dim: &'static str,
    red: &'static str,
}

impl Palette {
    fn new(color: bool) -> Self {
        Self {
            color,
            reset: if color { RESET } else { "" },
            bold: if color { BOLD } else { "" },
            dim: if color { DIM } else { "" },
            red: if color { RED } else { "" },
        }
    }

    fn color(&self, code: &'static str) -> &'static str {
        if self.color {
            code
        } else {
            ""
        }
    }

    fn colored(&self, code: &'static str, text: &str) -> String {
        if self.color {
            format!("{code}{text}{RESET}")
        } else {
            text.to_string()
        }
    }

    fn result_color(&self, result: &str) -> &'static str {
        if !self.color {
            return "";
        }
        if result.starts_with("stopped — ") {
            RED
        } else if result.starts_with("interrupted") {
            // Not red: an interrupted run is a run the operator stopped, not a
            // run that went wrong. §FS-rhei-run-report.3.1
            YELLOW
        } else if result.starts_with("stopped") {
            YELLOW
        } else if result == "completed" {
            GREEN
        } else {
            ""
        }
    }
}

#[cfg(test)]
mod run_summary_tests {
    include!("tests_run_summary.rs");
    include!("tests_run_summary_accounting.rs");
}
