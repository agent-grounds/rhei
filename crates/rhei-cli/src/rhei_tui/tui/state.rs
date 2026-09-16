//! The shared run model the render thread maintains: host-supplied plan rows and
//! machine, overlaid with runtime state from the event stream, plus the
//! keyboard-driven UI state of the Flow surface. §FS-rhei-run-tui.1.5

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::rhei_viz_model::VizModel;
pub(super) use crate::rhei_viz_model::{Machine, TaskRow};

use crate::rhei_tui::dashboard::{GateTransitionSink, InterveneSink, PlanLoader};
use crate::rhei_tui::event::{
    summarize_usage_summaries, AccountingRunSummary, AgentStream, MessageLevel, RunEvent, Slot,
    TaskOutcome, UsageSummary,
};

use super::text::sanitize_terminal_text;
use super::theme::{category, Category, Theme};
use super::{JOURNAL_BUFFER, SLOT_TRAFFIC_BUFFER};

/// The terminal views (§FS-rhei-run-tui.1.5.4). Flow leads.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum View {
    Flow,
    Machine,
    Cost,
    Journal,
}

impl View {
    pub(super) const ORDER: [View; 4] = [View::Flow, View::Machine, View::Cost, View::Journal];

    pub(super) fn index(self) -> usize {
        Self::ORDER.iter().position(|v| *v == self).unwrap_or(0)
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            View::Flow => "Flow",
            View::Machine => "Machine",
            View::Cost => "Cost",
            View::Journal => "Journal",
        }
    }
}

/// In Flow, focus toggles between the plan outline and the surroundings
/// inspector (§FS-rhei-run-tui.1.5.2).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FlowFocus {
    Outline,
    Inspector,
}

/// Cost grouping, cyclable with `g` (§FS-rhei-run-tui.1.5.2).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CostGroup {
    Task,
    Agent,
    Model,
    State,
}

impl CostGroup {
    pub(super) fn next(self) -> Self {
        match self {
            CostGroup::Task => CostGroup::Agent,
            CostGroup::Agent => CostGroup::Model,
            CostGroup::Model => CostGroup::State,
            CostGroup::State => CostGroup::Task,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            CostGroup::Task => "task",
            CostGroup::Agent => "agent",
            CostGroup::Model => "model",
            CostGroup::State => "state",
        }
    }
}

/// Journal severity/kind filter, cyclable with `f` (§FS-rhei-run-tui.1.5.2).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum JournalFilter {
    All,
    Warnings,
    Errors,
}

impl JournalFilter {
    pub(super) fn next(self) -> Self {
        match self {
            JournalFilter::All => JournalFilter::Warnings,
            JournalFilter::Warnings => JournalFilter::Errors,
            JournalFilter::Errors => JournalFilter::All,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            JournalFilter::All => "all",
            JournalFilter::Warnings => "warn+",
            JournalFilter::Errors => "error",
        }
    }

    fn admits(self, level: MessageLevel) -> bool {
        match self {
            JournalFilter::All => true,
            JournalFilter::Warnings => matches!(level, MessageLevel::Warn | MessageLevel::Error),
            JournalFilter::Errors => matches!(level, MessageLevel::Error),
        }
    }
}

/// One captured agent output line, retained per slot in a bounded ring buffer.
#[derive(Clone)]
pub(super) struct TrafficLine {
    pub(super) stream: AgentStream,
    pub(super) text: String,
}

/// The runtime overlay for one worker slot.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ProcessKind {
    Agent,
    Program,
}

#[derive(Clone, Default)]
pub(super) struct SlotState {
    pub(super) active: bool,
    pub(super) task: Option<String>,
    pub(super) agent: Option<String>,
    pub(super) state: Option<String>,
    pub(super) started_at: Option<Instant>,
    pub(super) log_path: Option<PathBuf>,
    pub(super) traffic: VecDeque<TrafficLine>,
    pub(super) usage: Option<UsageSummary>,
}

impl SlotState {
    pub(super) fn process_kind(&self) -> Option<ProcessKind> {
        if !self.active {
            return None;
        }
        if self.agent.is_some() {
            Some(ProcessKind::Agent)
        } else {
            Some(ProcessKind::Program)
        }
    }
}

/// A durably written invocation accounting record, mirrored for the Cost view.
#[derive(Clone)]
pub(super) struct UsageRecord {
    pub(super) task: String,
    pub(super) usage: UsageSummary,
}

/// One journal line carrying its severity so the Journal view can filter it.
#[derive(Clone)]
pub(super) struct JournalEntry {
    pub(super) level: MessageLevel,
    pub(super) text: String,
}

/// A run-emitted or workspace link, surfaced in the Journal view.
#[derive(Clone)]
pub(super) struct LinkEntry {
    pub(super) label: String,
    pub(super) url: String,
}

/// What the one-line composer is collecting (§FS-rhei-run-tui.1.5.5). Both
/// modes are the same widget and the same keys; only the destination of the
/// typed line differs, so the gate reuses it rather than growing a second one.
pub(super) enum ComposerKind {
    /// A message for a live agent's stdin.
    Intervene,
    /// The result message carried by a human-gate release. `terminal` is what
    /// the composer says about a blank line: on an edge into a `final: true`
    /// state the server will refuse it, and the operator should know that
    /// before pressing Enter, not after.
    // §FS-rhei-states.3.3
    GateResult { from: String, to: String, terminal: bool },
}

/// The one-line composer (§FS-rhei-run-tui.1.5.5).
pub(super) struct Composer {
    pub(super) task: String,
    pub(super) slot: Option<Slot>,
    pub(super) input: String,
    pub(super) kind: ComposerKind,
}

/// The full UI + run model owned by the render thread. The engine never touches
/// it; events arrive over a channel and are applied here.
pub(super) struct UiState {
    pub(super) workspace: PathBuf,
    pub(super) parallel: u16,
    pub(super) total_tasks: usize,
    pub(super) finished: bool,
    pub(super) dashboard_url: Option<String>,
    pub(super) theme: Theme,

    /// Last-good plan model. A failed reload keeps this rather than blanking.
    pub(super) plan: VizModel,
    plan_loader: Option<PlanLoader>,

    pub(super) slots: Vec<SlotState>,
    pub(super) invocations: Vec<UsageRecord>,
    pub(super) accounting: Option<AccountingRunSummary>,
    pub(super) deferred: HashSet<String>,
    pub(super) pass: u32,
    pub(super) journal: VecDeque<JournalEntry>,
    pub(super) links: Vec<LinkEntry>,

    pub(super) view: View,
    pub(super) selected: Option<String>,
    auto_selected: bool,
    pub(super) flow_focus: FlowFocus,
    pub(super) inspector_section: usize,
    pub(super) inspector_item: Option<usize>,
    pub(super) machine_focus: usize,
    pub(super) cost_group: CostGroup,
    pub(super) journal_filter: JournalFilter,
    pub(super) filter: Option<String>,
    pub(super) filter_editing: bool,
    pub(super) composer: Option<Composer>,
    pub(super) gate_active: bool,
    pub(super) help: bool,
    pub(super) inspector_scroll: u16,
    pub(super) journal_scroll: u16,
    pub(super) cost_cursor: usize,
    pub(super) spinner: u64,

    pub(super) intervene: Option<Arc<dyn InterveneSink>>,
    pub(super) gate: Option<Arc<dyn GateTransitionSink>>,
    /// True when this surface watches a run it does not drive, which changes
    /// what `q` and `Ctrl+C` mean and what the chrome says.
    // §FS-rhei-run-headless.5.1
    pub(super) attached: bool,
}

const SPINNER_FRAMES: [char; 4] = ['◐', '◓', '◑', '◒'];

impl UiState {
    pub(super) fn with_context(
        workspace: PathBuf,
        parallel: u16,
        total_tasks: usize,
        plan_loader: Option<PlanLoader>,
        intervene: Option<Arc<dyn InterveneSink>>,
        gate: Option<Arc<dyn GateTransitionSink>>,
        attached: bool,
    ) -> Self {
        let parallel = parallel.max(1);
        let mut state = Self {
            workspace,
            parallel,
            total_tasks,
            finished: false,
            dashboard_url: None,
            theme: Theme::from_env(),
            plan: VizModel::default(),
            plan_loader,
            slots: vec![SlotState::default(); parallel as usize],
            invocations: Vec::new(),
            accounting: None,
            deferred: HashSet::new(),
            pass: 0,
            journal: VecDeque::with_capacity(JOURNAL_BUFFER),
            links: Vec::new(),
            view: View::Flow,
            selected: None,
            auto_selected: false,
            flow_focus: FlowFocus::Outline,
            inspector_section: 0,
            inspector_item: None,
            machine_focus: 0,
            cost_group: CostGroup::Task,
            journal_filter: JournalFilter::All,
            filter: None,
            filter_editing: false,
            composer: None,
            gate_active: false,
            help: false,
            inspector_scroll: 0,
            journal_scroll: 0,
            cost_cursor: 0,
            spinner: 0,
            intervene,
            gate,
            attached,
        };
        state.refresh_plan();
        state
    }

    /// Re-read the plan through the host loader, keeping the last-good model on a
    /// transient failure (§FS-rhei-run-tui.1.5.7).
    pub(super) fn refresh_plan(&mut self) {
        if let Some(loader) = &self.plan_loader {
            if let Some(model) = loader() {
                self.plan = model;
            }
        }
        self.ensure_selection();
    }

    /// On load, auto-select the first running task, falling back to the first
    /// state-derived `active` task when no slot is running (§FS-rhei-run-tui.1.5.3).
    fn ensure_selection(&mut self) {
        let still_present = self.selected.as_ref().is_some_and(|id| self.task(id).is_some());
        if still_present {
            return;
        }
        if !self.auto_selected || !still_present {
            if let Some(id) = self.first_running_task().or_else(|| self.first_active_task()) {
                self.selected = Some(id);
                self.auto_selected = true;
                return;
            }
        }
        if self.selected.is_none() {
            self.selected = self.plan.tasks.first().map(|t| t.id.clone());
        }
    }

    fn first_running_task(&self) -> Option<String> {
        self.slots.iter().find(|s| s.active).and_then(|s| s.task.clone())
    }

    fn first_active_task(&self) -> Option<String> {
        self.plan
            .tasks
            .iter()
            .find(|t| category(&self.plan.machine, &t.state) == Category::Active)
            .map(|t| t.id.clone())
    }

    pub(super) fn tick_spinner(&mut self) {
        self.spinner = self.spinner.wrapping_add(1);
    }

    pub(super) fn spinner_glyph(&self) -> char {
        if self.theme.reduced_motion() {
            '•'
        } else {
            SPINNER_FRAMES[(self.spinner as usize) % SPINNER_FRAMES.len()]
        }
    }

    /// The slot currently running `task`, if any.
    pub(super) fn running_slot(&self, task: &str) -> Option<(Slot, &SlotState)> {
        self.slots
            .iter()
            .enumerate()
            .find(|(_, s)| s.active && s.task.as_deref() == Some(task))
            .map(|(i, s)| (i as Slot, s))
    }

    pub(super) fn is_live(&self, task: &str) -> bool {
        self.running_slot(task).is_some()
    }

    pub(super) fn running_process_kind(&self, task: &str) -> Option<ProcessKind> {
        self.running_slot(task).and_then(|(_, slot)| slot.process_kind())
    }

    pub(super) fn task(&self, id: &str) -> Option<&TaskRow> {
        self.plan.tasks.iter().find(|task| task.id == id)
    }

    pub(super) fn selected_task(&self) -> Option<&TaskRow> {
        self.selected.as_deref().and_then(|id| self.task(id))
    }

    pub(super) fn machine_state(&self, name: &str) -> Option<&crate::rhei_viz_model::MachineState> {
        self.plan.machine.states.iter().find(|state| state.name == name)
    }

    pub(super) fn task_ready(&self, task: &TaskRow) -> &'static str {
        if self.is_live(&task.id) {
            return "running";
        }
        if self.deferred.contains(&task.id) {
            return "deferred";
        }
        if self.unresolved_priors(task).is_empty() {
            "ready"
        } else {
            "blocked"
        }
    }

    pub(super) fn unresolved_priors(&self, task: &TaskRow) -> Vec<String> {
        task.prior
            .iter()
            .filter(|prior| {
                self.task(prior)
                    .map(|prior_task| !self.dependency_is_satisfied(&prior_task.state))
                    .unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    fn dependency_is_satisfied(&self, task_state: &str) -> bool {
        task_state != "cancelled"
            && self.machine_state(task_state).map(|state| state.terminal).unwrap_or(false)
    }

    pub(super) fn push_journal(&mut self, level: MessageLevel, text: String) {
        if self.journal.len() == JOURNAL_BUFFER {
            self.journal.pop_front();
        }
        self.journal.push_back(JournalEntry { level, text });
    }

    pub(super) fn filtered_journal(&self) -> Vec<&JournalEntry> {
        self.journal
            .iter()
            .filter(|e| self.journal_filter.admits(e.level))
            .filter(|e| self.text_matches_filter(&e.text))
            .collect()
    }

    pub(super) fn apply(&mut self, event: &RunEvent) {
        match event {
            RunEvent::RunStarted { workspace, parallel, total_tasks, .. } => {
                self.workspace = workspace.clone();
                self.parallel = (*parallel).max(1);
                self.total_tasks = *total_tasks;
                self.slots = vec![SlotState::default(); self.parallel as usize];
                self.invocations.clear();
                self.accounting = None;
                self.dashboard_url = None;
                self.push_journal(
                    MessageLevel::Info,
                    format!("run started — parallel={} total={}", self.parallel, self.total_tasks),
                );
            }
            RunEvent::PassStarted { pass, ready } => {
                self.pass = *pass;
                self.deferred.clear();
                self.push_journal(
                    MessageLevel::Info,
                    format!("pass {pass}: {} ready", ready.len()),
                );
            }
            RunEvent::SlotAssigned {
                slot, task, from, to, agent, log_path, started_at, ..
            } => {
                let same_state = from == to;
                if let Some(s) = self.slot_mut(*slot) {
                    s.active = true;
                    s.task = Some(task.clone());
                    s.agent = agent.clone();
                    s.state = Some(to.clone());
                    s.started_at = Some(*started_at);
                    s.log_path = Some(log_path.clone());
                    s.usage = None;
                    s.traffic.clear();
                }
                let line = if same_state {
                    format!("▶ slot {slot}: {task} started in {to}")
                } else {
                    format!("▶ slot {slot}: {task} {from}→{to}")
                };
                self.push_journal(MessageLevel::Info, line);
            }
            RunEvent::AgentOutput { slot, stream, line, .. } => {
                let line = sanitize_terminal_text(line);
                if let Some(s) = self.slot_mut(*slot) {
                    if s.traffic.len() == SLOT_TRAFFIC_BUFFER {
                        s.traffic.pop_front();
                    }
                    s.traffic.push_back(TrafficLine { stream: *stream, text: line });
                }
            }
            RunEvent::SlotReleased { slot, task, outcome, duration_ms, .. } => {
                let sym = match outcome {
                    TaskOutcome::Completed => "✓",
                    TaskOutcome::Failed(_) => "✗",
                    // The state is not done and nothing went wrong with it: the
                    // machine asked for another attempt. §FS-rhei-states.2.2
                    TaskOutcome::Waiting => "⏳",
                    TaskOutcome::ProviderLimited { .. } => "⏳",
                    TaskOutcome::Cancelled => "⊘",
                    TaskOutcome::TimedOut => "⏱",
                    // Distinct from cancelled's `⊘`: nothing about the ticket
                    // is wrong, the run simply stopped. §FS-rhei-run.3.2
                    TaskOutcome::Interrupted => "⏹",
                    // A word from a newer run than this reader.
                    // §FS-rhei-run-json.2.2
                    TaskOutcome::Unrecognized(_) => "?",
                };
                // A handled wait claims no attention: it reads at the level a
                // completion does, not a failure's. §FS-rhei-states.2.2
                let level = match outcome {
                    TaskOutcome::Completed
                    | TaskOutcome::Waiting
                    | TaskOutcome::ProviderLimited { .. } => MessageLevel::Info,
                    _ => MessageLevel::Warn,
                };
                // Carried through rather than rewritten, so the line says what
                // the run said even where the symbol cannot.
                // §FS-rhei-run-json.2.2
                let unknown = match outcome {
                    TaskOutcome::ProviderLimited { provider, next_attempt_at } => {
                        format!(" {provider} until {next_attempt_at}")
                    }
                    TaskOutcome::Unrecognized(name) => format!(" {name}"),
                    _ => String::new(),
                };
                if let Some(s) = self.slot_mut(*slot) {
                    *s = SlotState::default();
                }
                self.push_journal(
                    level,
                    format!("{sym} slot {slot}: {task}{unknown} ({}s)", duration_ms / 1000),
                );
            }
            RunEvent::PassEnded { pass, progressed } => {
                self.push_journal(
                    MessageLevel::Info,
                    format!("pass {pass} ended — progressed={progressed}"),
                );
            }
            RunEvent::TasksDeferred { pass, tasks } => {
                for t in tasks {
                    self.deferred.insert(t.clone());
                }
                self.push_journal(
                    MessageLevel::Info,
                    format!("pass {pass} deferred {}: {}", tasks.len(), tasks.join(", ")),
                );
            }
            RunEvent::RunFinished { summary } => {
                self.finished = true;
                self.accounting = summary.accounting.clone().or_else(|| {
                    summarize_usage_summaries(self.invocations.iter().map(|r| &r.usage))
                });
                self.composer = None;
                self.gate_active = false;
                self.push_journal(
                    MessageLevel::Info,
                    format!(
                        "run finished — agents={} programs={} terminal={}/{}",
                        summary.agents_spawned,
                        summary.programs_spawned,
                        summary.terminal_tasks,
                        summary.total_tasks
                    ),
                );
            }
            RunEvent::Message { level, text } => {
                self.push_journal(*level, text.clone());
            }
            RunEvent::RunLink { label, url } => {
                if label == "Dashboard" {
                    self.dashboard_url = Some(url.clone());
                }
                if !self.links.iter().any(|l| l.url == *url) {
                    self.links.push(LinkEntry { label: label.clone(), url: url.clone() });
                }
                self.push_journal(MessageLevel::Info, format!("{label}: {url}"));
            }
            RunEvent::UsageReported { slot, task, invocation_id, usage, .. } => {
                if let Some(existing) = self
                    .invocations
                    .iter_mut()
                    .find(|record| record.usage.invocation_id == *invocation_id)
                {
                    existing.task = task.clone();
                    existing.usage = usage.clone();
                } else {
                    self.invocations.push(UsageRecord { task: task.clone(), usage: usage.clone() });
                }
                self.accounting =
                    summarize_usage_summaries(self.invocations.iter().map(|r| &r.usage));
                if let Some(slot) = slot {
                    if let Some(s) = self.slot_mut(*slot) {
                        s.usage = Some(usage.clone());
                    }
                }
                self.push_journal(
                    MessageLevel::Info,
                    format!("task {task}: usage reported for {}", usage.agent),
                );
            }
            // Data for the run report's halt classification; the operator-facing
            // warning arrives separately as a `Message`, so rendering it here
            // would print the same stall twice. §FS-rhei-run-report.3.1
            RunEvent::TaskOutputsMissing { .. } => {}
        }
    }

    fn slot_mut(&mut self, slot: Slot) -> Option<&mut SlotState> {
        let idx = slot as usize;
        if idx >= self.slots.len() {
            self.slots.resize_with(idx + 1, SlotState::default);
        }
        self.slots.get_mut(idx)
    }

    fn filter_needle(&self) -> Option<String> {
        self.filter
            .as_ref()
            .map(|value| value.trim().to_lowercase())
            .filter(|value| !value.is_empty())
    }

    fn text_matches_filter(&self, text: &str) -> bool {
        self.filter_needle().is_none_or(|needle| text.to_lowercase().contains(&needle))
    }

    /// Whether a task row passes the active `/` filter, matched against its id,
    /// title, or state (§FS-rhei-run-tui.1.5.2).
    fn task_matches_filter(&self, idx: usize) -> bool {
        let Some(needle) = self.filter_needle() else { return true };
        let task = &self.plan.tasks[idx];
        task.id.to_lowercase().contains(&needle)
            || task.title.to_lowercase().contains(&needle)
            || task.state.to_lowercase().contains(&needle)
    }

    /// Plan task indices in source order, after the active filter — the Flow
    /// outline order.
    pub(super) fn visible_task_indices(&self) -> Vec<usize> {
        (0..self.plan.tasks.len()).filter(|i| self.task_matches_filter(*i)).collect()
    }

    /// Machine state indices in declaration order, after the active `/` filter.
    pub(super) fn machine_view_order(&self) -> Vec<usize> {
        (0..self.plan.machine.states.len())
            .filter(|i| self.machine_state_matches_filter(*i))
            .collect()
    }

    fn machine_state_matches_filter(&self, idx: usize) -> bool {
        let Some(needle) = self.filter_needle() else { return true };
        let state = &self.plan.machine.states[idx];
        state.name.to_lowercase().contains(&needle)
            || state
                .description
                .as_ref()
                .is_some_and(|description| description.to_lowercase().contains(&needle))
            || self.plan.tasks.iter().any(|task| {
                task.state == state.name
                    && (task.id.to_lowercase().contains(&needle)
                        || task.title.to_lowercase().contains(&needle))
            })
    }

    /// Keep local cursors on a visible row after the active `/` filter changes.
    pub(super) fn reconcile_filter_focus(&mut self) {
        match self.view {
            View::Machine => {
                let order = self.machine_view_order();
                if !order.contains(&self.machine_focus) {
                    if let Some(first) = order.first() {
                        self.machine_focus = *first;
                    }
                }
            }
            View::Flow | View::Cost => {
                let order = self.visible_task_indices();
                if !order
                    .iter()
                    .any(|i| self.selected.as_deref() == Some(self.plan.tasks[*i].id.as_str()))
                {
                    self.selected = order.first().map(|i| self.plan.tasks[*i].id.clone());
                }
            }
            View::Journal => self.journal_scroll = 0,
        }
    }

    /// Move the global selection through `order`, by `delta` rows, clamped.
    pub(super) fn move_selected_in(&mut self, order: &[usize], delta: isize) {
        if order.is_empty() {
            return;
        }
        let current = self
            .selected
            .as_ref()
            .and_then(|id| order.iter().position(|i| &self.plan.tasks[*i].id == id))
            .unwrap_or(0);
        let next = (current as isize + delta).clamp(0, order.len() as isize - 1) as usize;
        self.selected = Some(self.plan.tasks[order[next]].id.clone());
        self.inspector_section = 0;
        self.inspector_item = None;
        self.inspector_scroll = 0;
    }

    /// Select a task by id if it exists in the plan, returning to the outline.
    pub(super) fn select_task(&mut self, id: &str) -> bool {
        if self.task(id).is_some() {
            self.selected = Some(id.to_string());
            self.inspector_section = 0;
            self.inspector_item = None;
            self.inspector_scroll = 0;
            true
        } else {
            false
        }
    }
}
